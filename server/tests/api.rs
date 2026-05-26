//! End-to-end API tests. They drive the real Docker daemon, so they need the
//! fixtures running and are #[ignore]d by default:
//!
//!   docker compose -f fixtures/compose.yaml up -d
//!   cargo test -p peekr-server -- --ignored
//!
//! If Docker is unreachable the test skips (prints a notice) rather than fails.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures_util::StreamExt;
use peekr_server::{AppState, ContainerInfo, build_app, connect_docker};
use tokio::time::timeout;

const FIXTURES: [&str; 5] = [
  "peekr-fx-json",
  "peekr-fx-plain",
  "peekr-fx-stderr",
  "peekr-fx-multiline",
  "peekr-fx-burst",
];

/// Spawn the app on an ephemeral port; `None` if Docker is unreachable (skip).
async fn spawn() -> Option<String> {
  let docker = connect_docker().await.ok()?;
  let state = AppState {
    docker: Arc::new(docker),
  };
  let app = build_app(state);
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let addr = listener.local_addr().unwrap();
  tokio::spawn(async move {
    axum::serve(listener, app).await.unwrap();
  });
  Some(format!("http://{addr}"))
}

async fn fetch_containers(base: &str) -> Vec<ContainerInfo> {
  reqwest::get(format!("{base}/api/containers"))
    .await
    .unwrap()
    .json()
    .await
    .unwrap()
}

/// Read the SSE body until one full event (`\n\n`) arrives or the timeout fires.
async fn read_first_event(resp: reqwest::Response, dur: Duration) -> String {
  let mut stream = resp.bytes_stream();
  let mut buf = Vec::new();
  let _ = timeout(dur, async {
    while let Some(Ok(chunk)) = stream.next().await {
      buf.extend_from_slice(&chunk);
      if String::from_utf8_lossy(&buf).contains("\n\n") {
        break;
      }
    }
  })
  .await;
  String::from_utf8_lossy(&buf).into_owned()
}

#[tokio::test]
#[ignore = "needs: docker compose -f fixtures/compose.yaml up -d"]
async fn lists_fixture_containers() {
  let Some(base) = spawn().await else {
    eprintln!("docker unavailable, skipping");
    return;
  };
  let containers = fetch_containers(&base).await;
  let names: Vec<&str> = containers.iter().map(|c| c.name.as_str()).collect();
  for fx in FIXTURES {
    assert!(names.contains(&fx), "missing fixture {fx}; have {names:?}");
  }
}

#[tokio::test]
#[ignore = "needs: docker compose -f fixtures/compose.yaml up -d"]
async fn streams_structured_log_lines() {
  let Some(base) = spawn().await else {
    eprintln!("docker unavailable, skipping");
    return;
  };
  let containers = fetch_containers(&base).await;
  let fx = containers
    .iter()
    .find(|c| c.name == "peekr-fx-json")
    .expect("peekr-fx-json fixture must be running");

  let resp = reqwest::get(format!("{base}/api/containers/{}/logs", fx.id))
    .await
    .unwrap();
  assert!(resp.status().is_success());

  let body = read_first_event(resp, Duration::from_secs(5)).await;
  let data_line = body
    .lines()
    .find(|l| l.starts_with("data: "))
    .expect("expected an SSE data line");

  let payload: serde_json::Value =
    serde_json::from_str(data_line.trim_start_matches("data: ")).expect("payload is JSON");
  assert_eq!(payload["stream"], "stdout");
  let msg = payload["msg"].as_str().expect("msg is a string");

  // the json fixture emits pino-style lines; msg itself should be JSON with a level
  let inner: serde_json::Value = serde_json::from_str(msg).expect("msg should be JSON");
  assert!(
    inner.get("level").is_some(),
    "expected a level field: {msg}"
  );
}

#[tokio::test]
#[ignore = "needs: docker compose -f fixtures/compose.yaml up -d"]
async fn streams_stats_samples() {
  let Some(base) = spawn().await else {
    eprintln!("docker unavailable, skipping");
    return;
  };
  let containers = fetch_containers(&base).await;
  let fx = containers
    .iter()
    .find(|c| c.name == "peekr-fx-burst")
    .expect("peekr-fx-burst fixture must be running");

  let resp = reqwest::get(format!("{base}/api/containers/{}/stats", fx.id))
    .await
    .unwrap();
  assert!(resp.status().is_success());

  // stats stream emits ~1/s; allow a couple of seconds for the first sample
  let body = read_first_event(resp, Duration::from_secs(8)).await;
  let data_line = body
    .lines()
    .find(|l| l.starts_with("data: "))
    .expect("expected an SSE data line");

  let payload: serde_json::Value =
    serde_json::from_str(data_line.trim_start_matches("data: ")).expect("payload is JSON");
  for key in ["ts", "cpu_pct", "mem_used", "mem_limit", "mem_pct"] {
    assert!(payload.get(key).is_some(), "missing {key}: {payload}");
  }
}

#[tokio::test]
#[ignore = "needs: docker compose -f fixtures/compose.yaml up -d"]
async fn accepts_since_query() {
  let Some(base) = spawn().await else {
    eprintln!("docker unavailable, skipping");
    return;
  };
  let containers = fetch_containers(&base).await;
  let fx = containers
    .iter()
    .find(|c| c.name == "peekr-fx-json")
    .expect("peekr-fx-json fixture must be running");

  let since = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap()
    .as_secs();
  let resp = reqwest::get(format!(
    "{base}/api/containers/{}/logs?since={since}",
    fx.id
  ))
  .await
  .unwrap();
  assert!(resp.status().is_success());
}
