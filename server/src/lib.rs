use std::convert::Infallible;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::{
  Json, Router,
  extract::{Path, Query, State},
  http::StatusCode,
  middleware::from_fn_with_state,
  response::sse::{Event, KeepAlive, Sse},
  routing::{get, post},
};
use bollard::Docker;
use bollard::container::LogOutput;
use bollard::query_parameters::{
  ListContainersOptionsBuilder, LogsOptionsBuilder, StatsOptionsBuilder,
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tokio_stream::Stream;
use tower_http::cors::CorsLayer;

pub mod auth;
pub mod db;

#[derive(Clone)]
pub struct AppState {
  pub docker: Arc<Docker>,
  pub db: SqlitePool,
  pub secret: Arc<Vec<u8>>,
}

#[derive(Serialize, Deserialize)]
pub struct ContainerInfo {
  pub id: String,
  pub name: String,
  pub image: String,
  pub state: String,
  pub status: String,
  /// docker compose project, empty for standalone containers
  pub project: String,
}

#[derive(Deserialize)]
struct LogQuery {
  /// unix seconds; set on reconnect to resume instead of re-dumping the tail
  since: Option<i32>,
}

#[derive(Serialize)]
struct LogLine {
  ts: Option<String>,
  stream: &'static str,
  msg: String,
}

pub async fn connect_docker() -> anyhow::Result<Docker> {
  let docker = Docker::connect_with_local_defaults()?;
  docker.ping().await?;
  Ok(docker)
}

pub fn build_app(state: AppState) -> Router {
  // docker routes require a valid session cookie; scoped per host (local + future agents)
  let protected = Router::new()
    .route("/api/hosts", get(list_hosts))
    .route("/api/hosts/{host}/containers", get(list_containers))
    .route("/api/hosts/{host}/containers/{id}/logs", get(stream_logs))
    .route("/api/hosts/{host}/containers/{id}/stats", get(stream_stats))
    .layer(from_fn_with_state(state.clone(), auth::require_auth));

  let public = Router::new()
    .route("/api/healthz", get(healthz))
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

async fn list_hosts() -> Json<Vec<HostInfo>> {
  // only the hub's own docker for now; agents join here in v2
  Json(vec![HostInfo {
    id: LOCAL_HOST.to_string(),
    name: std::env::var("PEEKR_HOST_NAME").unwrap_or_else(|_| LOCAL_HOST.to_string()),
    status: "online",
  }])
}

fn check_host(host: &str) -> Result<(), StatusCode> {
  if host == LOCAL_HOST {
    Ok(())
  } else {
    Err(StatusCode::NOT_FOUND)
  }
}

async fn list_containers(
  Path(host): Path<String>,
  State(state): State<AppState>,
) -> Result<Json<Vec<ContainerInfo>>, (StatusCode, String)> {
  check_host(&host).map_err(|s| (s, "unknown host".into()))?;
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

async fn stream_logs(
  Path((host, id)): Path<(String, String)>,
  Query(q): Query<LogQuery>,
  State(state): State<AppState>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, StatusCode> {
  check_host(&host)?;
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
              stream: stream_name,
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

  Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
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

#[derive(Serialize)]
struct StatsSample {
  ts: u64,
  cpu_pct: f64,
  mem_used: u64,
  mem_limit: u64,
  mem_pct: f64,
}

async fn stream_stats(
  Path((host, id)): Path<(String, String)>,
  State(state): State<AppState>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, StatusCode> {
  check_host(&host)?;
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

  Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
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
  #[folder = "../web/dist"]
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
