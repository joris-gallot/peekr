use std::convert::Infallible;
use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    response::sse::{Event, KeepAlive, Sse},
    routing::get,
};
use bollard::Docker;
use bollard::query_parameters::{ListContainersOptionsBuilder, LogsOptionsBuilder};
use futures_util::StreamExt;
use serde::Serialize;
use tokio_stream::Stream;
use tower_http::cors::CorsLayer;
use tracing::info;

#[derive(Clone)]
struct AppState {
    docker: Arc<Docker>,
}

#[derive(Serialize)]
struct ContainerInfo {
    id: String,
    name: String,
    image: String,
    state: String,
    status: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let docker = Docker::connect_with_local_defaults()?;
    docker.ping().await?;
    info!("connected to docker daemon");

    let state = AppState {
        docker: Arc::new(docker),
    };

    let app = Router::new()
        .route("/api/healthz", get(healthz))
        .route("/api/containers", get(list_containers))
        .route("/api/containers/{id}/logs", get(stream_logs))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    info!("peekr listening on http://0.0.0.0:8080");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn healthz() -> &'static str {
    "ok"
}

async fn list_containers(State(state): State<AppState>) -> Json<Vec<ContainerInfo>> {
    let opts = ListContainersOptionsBuilder::default().all(true).build();
    let containers = state
        .docker
        .list_containers(Some(opts))
        .await
        .unwrap_or_default();

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
        })
        .collect();

    Json(infos)
}

async fn stream_logs(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let opts = LogsOptionsBuilder::default()
        .stdout(true)
        .stderr(true)
        .follow(true)
        .tail("200")
        .timestamps(true)
        .build();

    let stream = state.docker.logs(&id, Some(opts)).filter_map(|res| async move {
        match res {
            Ok(output) => Some(Ok(Event::default().data(output.to_string().trim_end()))),
            Err(_) => None,
        }
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}
