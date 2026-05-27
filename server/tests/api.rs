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
use peekr_server::{AppState, ContainerInfo, build_app, connect_docker, db};
use serde_json::json;
use sqlx::sqlite::SqlitePoolOptions;
use tokio::time::timeout;

const FIXTURES: [&str; 5] = [
  "peekr-fx-json",
  "peekr-fx-plain",
  "peekr-fx-stderr",
  "peekr-fx-multiline",
  "peekr-fx-burst",
];

/// Spawn the app (in-memory DB) on an ephemeral port; `None` if Docker is unreachable.
async fn spawn() -> Option<String> {
  let docker = connect_docker().await.ok()?;
  let pool = SqlitePoolOptions::new()
    .max_connections(1)
    .connect("sqlite::memory:")
    .await
    .unwrap();
  db::init_schema(&pool).await.unwrap();
  let state = AppState {
    docker: Arc::new(docker),
    db: pool,
    secret: Arc::new(b"test-secret".to_vec()),
  };
  let app = build_app(state);
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let addr = listener.local_addr().unwrap();
  tokio::spawn(async move {
    axum::serve(listener, app).await.unwrap();
  });
  Some(format!("http://{addr}"))
}

/// A cookie-jar client that has signed up as the first (admin) user.
async fn authed(base: &str) -> reqwest::Client {
  let client = reqwest::Client::builder()
    .cookie_store(true)
    .build()
    .unwrap();
  let res = client
    .post(format!("{base}/api/auth/signup"))
    .json(&json!({ "email": "test@peekr.local", "password": "password123" }))
    .send()
    .await
    .unwrap();
  assert!(res.status().is_success(), "signup failed: {}", res.status());
  client
}

async fn fetch_containers(base: &str, client: &reqwest::Client) -> Vec<ContainerInfo> {
  client
    .get(format!("{base}/api/containers"))
    .send()
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
async fn auth_flow() {
  let Some(base) = spawn().await else {
    eprintln!("docker unavailable, skipping");
    return;
  };
  let client = reqwest::Client::builder()
    .cookie_store(true)
    .build()
    .unwrap();

  let fr: serde_json::Value = client
    .get(format!("{base}/api/auth/first-run"))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(fr["firstRun"], true);

  // protected route is 401 before auth
  let r = client
    .get(format!("{base}/api/containers"))
    .send()
    .await
    .unwrap();
  assert_eq!(r.status(), 401);

  // signup the first user -> sets the session cookie
  let s = client
    .post(format!("{base}/api/auth/signup"))
    .json(&json!({ "email": "a@b.c", "password": "password123" }))
    .send()
    .await
    .unwrap();
  assert!(s.status().is_success());

  // me + protected now work
  assert!(
    client
      .get(format!("{base}/api/auth/me"))
      .send()
      .await
      .unwrap()
      .status()
      .is_success()
  );
  assert!(
    client
      .get(format!("{base}/api/containers"))
      .send()
      .await
      .unwrap()
      .status()
      .is_success()
  );

  // a second signup is forbidden (registration closed)
  let s2 = reqwest::Client::new()
    .post(format!("{base}/api/auth/signup"))
    .json(&json!({ "email": "x@y.z", "password": "password123" }))
    .send()
    .await
    .unwrap();
  assert_eq!(s2.status(), 403);
}

#[tokio::test]
#[ignore = "needs: docker compose -f fixtures/compose.yaml up -d"]
async fn lists_fixture_containers() {
  let Some(base) = spawn().await else {
    eprintln!("docker unavailable, skipping");
    return;
  };
  let client = authed(&base).await;
  let containers = fetch_containers(&base, &client).await;
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
  let client = authed(&base).await;
  let containers = fetch_containers(&base, &client).await;
  let fx = containers
    .iter()
    .find(|c| c.name == "peekr-fx-json")
    .expect("peekr-fx-json fixture must be running");

  let resp = client
    .get(format!("{base}/api/containers/{}/logs", fx.id))
    .send()
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
  let client = authed(&base).await;
  let containers = fetch_containers(&base, &client).await;
  let fx = containers
    .iter()
    .find(|c| c.name == "peekr-fx-burst")
    .expect("peekr-fx-burst fixture must be running");

  let resp = client
    .get(format!("{base}/api/containers/{}/stats", fx.id))
    .send()
    .await
    .unwrap();
  assert!(resp.status().is_success());

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
  let client = authed(&base).await;
  let containers = fetch_containers(&base, &client).await;
  let fx = containers
    .iter()
    .find(|c| c.name == "peekr-fx-json")
    .expect("peekr-fx-json fixture must be running");

  let since = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap()
    .as_secs();
  let resp = client
    .get(format!(
      "{base}/api/containers/{}/logs?since={since}",
      fx.id
    ))
    .send()
    .await
    .unwrap();
  assert!(resp.status().is_success());
}
