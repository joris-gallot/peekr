use std::convert::Infallible;
use std::sync::Arc;

use axum::{
  Json, Router,
  extract::{Path, Query, State},
  http::StatusCode,
  response::sse::{Event, KeepAlive, Sse},
  routing::get,
};
use bollard::Docker;
use bollard::container::LogOutput;
use bollard::query_parameters::{ListContainersOptionsBuilder, LogsOptionsBuilder};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio_stream::Stream;
use tower_http::cors::CorsLayer;

#[derive(Clone)]
pub struct AppState {
  pub docker: Arc<Docker>,
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
  Router::new()
    .route("/api/healthz", get(healthz))
    .route("/api/containers", get(list_containers))
    .route("/api/containers/{id}/logs", get(stream_logs))
    .layer(CorsLayer::permissive())
    .with_state(state)
}

async fn healthz() -> &'static str {
  "ok"
}

async fn list_containers(
  State(state): State<AppState>,
) -> Result<Json<Vec<ContainerInfo>>, (StatusCode, String)> {
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
  Path(id): Path<String>,
  Query(q): Query<LogQuery>,
  State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
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

  Sse::new(stream).keep_alive(KeepAlive::default())
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
