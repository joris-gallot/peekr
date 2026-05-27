use std::sync::Arc;

use peekr_server::{AppState, build_app, connect_docker, db};
use rand::RngCore;
use tracing::{info, warn};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
  tracing_subscriber::fmt::init();

  let docker = connect_docker().await?;
  info!("connected to docker daemon");

  let db_url = std::env::var("PEEKR_DB").unwrap_or_else(|_| "sqlite:peekr.db?mode=rwc".into());
  let pool = db::connect(&db_url).await?;
  info!("database ready at {db_url}");

  let state = AppState {
    docker: Arc::new(docker),
    db: pool,
    secret: Arc::new(load_secret()),
  };
  let app = build_app(state);

  let addr = std::env::var("PEEKR_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into());
  let listener = tokio::net::TcpListener::bind(&addr).await?;
  info!("peekr listening on http://{addr}");
  axum::serve(listener, app).await?;
  Ok(())
}

fn load_secret() -> Vec<u8> {
  if let Ok(s) = std::env::var("PEEKR_SECRET") {
    return s.into_bytes();
  }
  warn!("PEEKR_SECRET not set; using a random ephemeral secret (sessions reset on restart)");
  let mut bytes = [0u8; 32];
  rand::rng().fill_bytes(&mut bytes);
  bytes.to_vec()
}
