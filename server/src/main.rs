use std::sync::Arc;

use peekr_server::{AppState, build_app, connect_docker};
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
  tracing_subscriber::fmt::init();

  let docker = connect_docker().await?;
  info!("connected to docker daemon");

  let state = AppState {
    docker: Arc::new(docker),
  };
  let app = build_app(state);

  let addr = std::env::var("PEEKR_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into());
  let listener = tokio::net::TcpListener::bind(&addr).await?;
  info!("peekr listening on http://{addr}");
  axum::serve(listener, app).await?;
  Ok(())
}
