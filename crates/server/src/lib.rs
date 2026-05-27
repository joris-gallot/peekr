use std::convert::Infallible;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::{
  Json, Router,
  extract::{Path, Query, State},
  http::StatusCode,
  middleware::from_fn_with_state,
  response::sse::{Event, Sse},
  routing::{delete, get, post},
};
use bollard::Docker;
use bollard::container::LogOutput;
use bollard::query_parameters::{
  ListContainersOptionsBuilder, LogsOptionsBuilder, StatsOptionsBuilder,
};
use futures_util::StreamExt;
use peekr_common::{Cmd, Resp};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tokio_stream::Stream;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tower_http::cors::CorsLayer;

pub mod agents;
pub mod auth;
pub mod db;

pub use peekr_common::{ContainerInfo, LogLine, StatsSample};

/// Boxed so local (bollard) and remote (agent) branches share one return type.
type EventStream = Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>>;

#[derive(Clone)]
pub struct AppState {
  pub docker: Arc<Docker>,
  pub db: SqlitePool,
  pub secret: Arc<Vec<u8>>,
  pub agents: agents::AgentManager,
}

#[derive(Deserialize)]
struct LogQuery {
  /// unix seconds; set on reconnect to resume instead of re-dumping the tail
  since: Option<i32>,
}

pub async fn connect_docker() -> anyhow::Result<Docker> {
  let docker = Docker::connect_with_local_defaults()?;
  docker.ping().await?;
  Ok(docker)
}

pub fn build_app(state: AppState) -> Router {
  // docker routes require a valid session cookie; scoped per host (local + future agents)
  let protected = Router::new()
    .route("/api/hosts", get(list_hosts).post(add_host))
    .route("/api/hosts/{host}", delete(remove_host))
    .route("/api/hosts/{host}/stats", get(host_stats))
    .route("/api/hosts/{host}/containers", get(list_containers))
    .route("/api/hosts/{host}/containers/{id}/logs", get(stream_logs))
    .route("/api/hosts/{host}/containers/{id}/stats", get(stream_stats))
    .layer(from_fn_with_state(state.clone(), auth::require_auth));

  let public = Router::new()
    .route("/api/healthz", get(healthz))
    .route("/api/agents/connect", get(agents::agent_connect))
    .route("/api/auth/first-run", get(auth::first_run))
    .route("/api/auth/signup", post(auth::signup))
    .route("/api/auth/login", post(auth::login))
    .route("/api/auth/logout", post(auth::logout))
    .route("/api/auth/me", get(auth::me));

  let app = public
    .merge(protected)
    .layer(CorsLayer::permissive())
    .with_state(state);

  #[cfg(feature = "embed-ui")]
  let app = app.fallback(ui::static_handler);

  app
}

async fn healthz() -> &'static str {
  "ok"
}

const LOCAL_HOST: &str = "local";

#[derive(Serialize)]
pub struct HostInfo {
  pub id: String,
  pub name: String,
  pub status: &'static str,
}

async fn list_hosts(State(state): State<AppState>) -> Json<Vec<HostInfo>> {
  let online = state.agents.online_ids().await;
  let mut hosts = vec![HostInfo {
    id: LOCAL_HOST.to_string(),
    name: std::env::var("PEEKR_HOST_NAME").unwrap_or_else(|_| LOCAL_HOST.to_string()),
    status: "online",
  }];
  let rows: Vec<(String, String)> = sqlx::query_as("SELECT id, name FROM hosts ORDER BY name")
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();
  for (id, name) in rows {
    let status = if online.contains(&id) {
      "online"
    } else {
      "offline"
    };
    hosts.push(HostInfo { id, name, status });
  }
  Json(hosts)
}

#[derive(Deserialize)]
struct NewHost {
  name: String,
}

#[derive(Serialize)]
struct CreatedHost {
  id: String,
  name: String,
  /// shown once so the agent can be configured; not retrievable later
  token: String,
}

async fn add_host(
  State(state): State<AppState>,
  Json(body): Json<NewHost>,
) -> Result<Json<CreatedHost>, (StatusCode, String)> {
  if body.name.trim().is_empty() {
    return Err((StatusCode::BAD_REQUEST, "name required".into()));
  }
  let id = rand_hex(6);
  let token = rand_hex(24);
  let created = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map(|d| d.as_secs())
    .unwrap_or(0) as i64;
  sqlx::query("INSERT INTO hosts (id, name, token, created_at) VALUES (?, ?, ?, ?)")
    .bind(&id)
    .bind(&body.name)
    .bind(&token)
    .bind(created)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
  Ok(Json(CreatedHost {
    id,
    name: body.name,
    token,
  }))
}

async fn remove_host(
  Path(host): Path<String>,
  State(state): State<AppState>,
) -> Result<StatusCode, (StatusCode, String)> {
  sqlx::query("DELETE FROM hosts WHERE id = ?")
    .bind(&host)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
  Ok(StatusCode::NO_CONTENT)
}

fn rand_hex(bytes: usize) -> String {
  use rand::RngCore;
  let mut buf = vec![0u8; bytes];
  rand::rng().fill_bytes(&mut buf);
  buf.iter().map(|b| format!("{b:02x}")).collect()
}

/// Cancels the agent stream when the client's SSE connection is dropped.
struct CancelGuard {
  agents: agents::AgentManager,
  host: String,
  id: u64,
}

impl Drop for CancelGuard {
  fn drop(&mut self) {
    let (agents, host, id) = (self.agents.clone(), self.host.clone(), self.id);
    tokio::spawn(async move { agents.cancel(&host, id).await });
  }
}

/// Bridge an agent command's response frames to an SSE stream (logs / stats).
async fn remote_stream(
  agents: agents::AgentManager,
  host: String,
  cmd: Cmd,
) -> Result<Sse<EventStream>, StatusCode> {
  let (id, rx) = agents
    .request(&host, cmd)
    .await
    .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
  let guard = CancelGuard { agents, host, id };
  let stream = UnboundedReceiverStream::new(rx)
    .filter_map(move |resp| {
      let _ = &guard; // keep the cancel guard alive for the stream's lifetime
      async move {
        match resp {
          Resp::Log(l) => Event::default().json_data(l).ok().map(Ok::<_, Infallible>),
          Resp::Stat(s) => Event::default().json_data(s).ok().map(Ok::<_, Infallible>),
          Resp::HostStat(s) => Event::default().json_data(s).ok().map(Ok::<_, Infallible>),
          Resp::Error(e) => Some(Ok(Event::default().event("stream-error").data(e))),
          _ => None,
        }
      }
    })
    .boxed();
  Ok(Sse::new(stream))
}

async fn host_stats(
  Path(host): Path<String>,
  State(state): State<AppState>,
) -> Result<Sse<EventStream>, StatusCode> {
  if host != LOCAL_HOST {
    return remote_stream(state.agents.clone(), host, Cmd::HostStats).await;
  }
  let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<peekr_common::HostStat>();
  tokio::spawn(async move {
    let mut sampler = peekr_common::HostSampler::new();
    loop {
      tokio::time::sleep(std::time::Duration::from_secs(2)).await;
      if tx.send(sampler.sample()).is_err() {
        break;
      }
    }
  });
  let stream = UnboundedReceiverStream::new(rx)
    .filter_map(|st| async move { Event::default().json_data(st).ok().map(Ok::<_, Infallible>) })
    .boxed();
  Ok(Sse::new(stream))
}

async fn list_containers(
  Path(host): Path<String>,
  State(state): State<AppState>,
) -> Result<Json<Vec<ContainerInfo>>, (StatusCode, String)> {
  if host != LOCAL_HOST {
    return remote_list(&state, &host).await;
  }
  let opts = ListContainersOptionsBuilder::default().all(true).build();
  let containers = state
    .docker
    .list_containers(Some(opts))
    .await
    .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;

  let infos = containers
    .into_iter()
    .map(|c| ContainerInfo {
      id: c.id.unwrap_or_default(),
      name: c
        .names
        .and_then(|n| n.into_iter().next())
        .unwrap_or_default()
        .trim_start_matches('/')
        .to_string(),
      image: c.image.unwrap_or_default(),
      state: c.state.map(|s| format!("{s:?}")).unwrap_or_default(),
      status: c.status.unwrap_or_default(),
      project: c
        .labels
        .and_then(|l| l.get("com.docker.compose.project").cloned())
        .unwrap_or_default(),
    })
    .collect();

  Ok(Json(infos))
}

async fn remote_list(
  state: &AppState,
  host: &str,
) -> Result<Json<Vec<ContainerInfo>>, (StatusCode, String)> {
  let (_, mut rx) = state
    .agents
    .request(host, peekr_common::Cmd::List)
    .await
    .ok_or((StatusCode::SERVICE_UNAVAILABLE, "host offline".into()))?;
  match rx.recv().await {
    Some(peekr_common::Resp::Containers(list)) => Ok(Json(list)),
    Some(peekr_common::Resp::Error(e)) => Err((StatusCode::BAD_GATEWAY, e)),
    _ => Err((StatusCode::BAD_GATEWAY, "no response from agent".into())),
  }
}

async fn stream_logs(
  Path((host, id)): Path<(String, String)>,
  Query(q): Query<LogQuery>,
  State(state): State<AppState>,
) -> Result<Sse<EventStream>, StatusCode> {
  if host != LOCAL_HOST {
    return remote_stream(
      state.agents.clone(),
      host,
      Cmd::Logs {
        container: id,
        since: q.since,
      },
    )
    .await;
  }
  let mut builder = LogsOptionsBuilder::default()
    .stdout(true)
    .stderr(true)
    .follow(true)
    .timestamps(true);
  builder = match q.since {
    Some(since) => builder.since(since),
    None => builder.tail("200"),
  };
  let opts = builder.build();

  let stream = state.docker.logs(&id, Some(opts)).flat_map(|res| {
    let events: Vec<Result<Event, Infallible>> = match res {
      Ok(output) => {
        let (stream_name, bytes) = match &output {
          LogOutput::StdErr { message } => ("stderr", message),
          LogOutput::StdOut { message } => ("stdout", message),
          LogOutput::StdIn { message } => ("stdout", message),
          LogOutput::Console { message } => ("stdout", message),
        };
        let text = String::from_utf8_lossy(bytes);
        text
          .split('\n')
          .filter(|l| !l.is_empty())
          .map(|line| {
            let (ts, msg) = split_ts(line);
            let payload = LogLine {
              ts,
              stream: stream_name.to_string(),
              msg,
            };
            Ok(
              Event::default()
                .json_data(payload)
                .unwrap_or_else(|_| Event::default().data(line)),
            )
          })
          .collect()
      }
      Err(e) => vec![Ok(
        Event::default().event("stream-error").data(e.to_string()),
      )],
    };
    tokio_stream::iter(events)
  });

  Ok(Sse::new(stream.boxed()))
}

/// Docker prefixes each line with an RFC3339 timestamp when `timestamps(true)`.
fn split_ts(line: &str) -> (Option<String>, String) {
  match line.split_once(' ') {
    Some((ts, rest)) if ts.len() >= 20 && ts.contains('T') => {
      (Some(ts.to_string()), rest.to_string())
    }
    _ => (None, line.to_string()),
  }
}

async fn stream_stats(
  Path((host, id)): Path<(String, String)>,
  State(state): State<AppState>,
) -> Result<Sse<EventStream>, StatusCode> {
  if host != LOCAL_HOST {
    return remote_stream(state.agents.clone(), host, Cmd::Stats { container: id }).await;
  }
  let opts = StatsOptionsBuilder::default().stream(true).build();

  let stream = state
    .docker
    .stats(&id, Some(opts))
    .filter_map(|res| async move {
      let s = res.ok()?;
      let cpu = s.cpu_stats?;
      let precpu = s.precpu_stats?;
      let cpu_pct = cpu_percent(
        cpu
          .cpu_usage
          .as_ref()
          .and_then(|u| u.total_usage)
          .unwrap_or(0),
        precpu
          .cpu_usage
          .as_ref()
          .and_then(|u| u.total_usage)
          .unwrap_or(0),
        cpu.system_cpu_usage.unwrap_or(0),
        precpu.system_cpu_usage.unwrap_or(0),
        cpu.online_cpus.unwrap_or(1),
      );

      let mem = s.memory_stats?;
      let usage = mem.usage.unwrap_or(0);
      // docker subtracts reclaimable page cache from usage to match `docker stats`
      let cache = mem
        .stats
        .as_ref()
        .and_then(|m| m.get("inactive_file").or_else(|| m.get("cache")).copied())
        .unwrap_or(0);
      let mem_used = usage.saturating_sub(cache);
      let mem_limit = mem.limit.unwrap_or(0);
      let mem_pct = if mem_limit > 0 {
        mem_used as f64 / mem_limit as f64 * 100.0
      } else {
        0.0
      };

      let sample = StatsSample {
        ts: now_ms(),
        cpu_pct,
        mem_used,
        mem_limit,
        mem_pct,
      };
      Some(Ok(Event::default().json_data(sample).ok()?))
    });

  Ok(Sse::new(stream.boxed()))
}

/// Docker's CPU-usage formula: share of total host CPU time scaled by core count.
fn cpu_percent(cpu_total: u64, precpu_total: u64, system: u64, presystem: u64, online: u32) -> f64 {
  let cpu_delta = cpu_total.saturating_sub(precpu_total) as f64;
  let sys_delta = system.saturating_sub(presystem) as f64;
  if sys_delta > 0.0 && cpu_delta > 0.0 {
    (cpu_delta / sys_delta) * online.max(1) as f64 * 100.0
  } else {
    0.0
  }
}

fn now_ms() -> u64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map(|d| d.as_millis() as u64)
    .unwrap_or(0)
}

#[cfg(test)]
mod tests {
  use super::cpu_percent;

  #[test]
  fn cpu_percent_scales_by_cores() {
    // 10% of one core's delta, across 4 cores -> 40%
    assert_eq!(cpu_percent(200, 100, 2000, 1000, 4), 40.0);
  }

  #[test]
  fn cpu_percent_zero_when_no_delta() {
    assert_eq!(cpu_percent(100, 100, 2000, 1000, 4), 0.0);
    assert_eq!(cpu_percent(200, 100, 1000, 1000, 4), 0.0);
  }

  #[test]
  fn cpu_percent_treats_zero_cores_as_one() {
    assert_eq!(cpu_percent(200, 100, 2000, 1000, 0), 10.0);
  }
}

#[cfg(feature = "embed-ui")]
mod ui {
  use axum::body::Body;
  use axum::http::{StatusCode, Uri, header};
  use axum::response::{IntoResponse, Response};
  use rust_embed::Embed;

  #[derive(Embed)]
  #[folder = "../../web/dist"]
  struct Assets;

  /// Serve an embedded asset, falling back to index.html so SPA routes resolve.
  pub async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    if let Some(file) = Assets::get(path) {
      let mime = mime_guess::from_path(path).first_or_octet_stream();
      return Response::builder()
        .header(header::CONTENT_TYPE, mime.as_ref())
        .body(Body::from(file.data.into_owned()))
        .unwrap();
    }

    match Assets::get("index.html") {
      Some(file) => Response::builder()
        .header(header::CONTENT_TYPE, "text/html")
        .body(Body::from(file.data.into_owned()))
        .unwrap(),
      None => StatusCode::NOT_FOUND.into_response(),
    }
  }
}
