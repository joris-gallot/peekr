//! peekr agent: runs on a remote host, connects out to the hub over WebSocket,
//! and answers the hub's commands (list/logs/stats) about the local Docker.

use std::time::Duration;

use bollard::Docker;
use bollard::query_parameters::ListContainersOptionsBuilder;
use futures_util::{SinkExt, StreamExt};
use peekr_common::{AgentMsg, ClientMsg, Cmd, ContainerInfo, Resp};
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tracing::{info, warn};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
  tracing_subscriber::fmt::init();

  let hub = std::env::var("PEEKR_HUB").unwrap_or_else(|_| "ws://localhost:8080".into());
  let token = std::env::var("PEEKR_TOKEN").map_err(|_| anyhow::anyhow!("PEEKR_TOKEN required"))?;
  let url = format!("{hub}/api/agents/connect?token={token}");

  let docker = Docker::connect_with_local_defaults()?;
  docker.ping().await?;
  info!("agent: connected to docker; hub = {hub}");

  loop {
    if let Err(e) = run(&url, &docker).await {
      warn!("connection lost: {e}");
    }
    tokio::time::sleep(Duration::from_secs(3)).await;
  }
}

async fn run(url: &str, docker: &Docker) -> anyhow::Result<()> {
  let (ws, _) = connect_async(url).await?;
  info!("agent: linked to hub");
  let (mut write, mut read) = ws.split();
  let (out_tx, mut out_rx) = mpsc::unbounded_channel::<AgentMsg>();

  let writer = tokio::spawn(async move {
    while let Some(msg) = out_rx.recv().await {
      let Ok(txt) = serde_json::to_string(&msg) else { continue };
      if write.send(Message::Text(txt.into())).await.is_err() {
        break;
      }
    }
  });

  while let Some(Ok(msg)) = read.next().await {
    if let Message::Text(t) = msg {
      if let Ok(cm) = serde_json::from_str::<ClientMsg>(&t) {
        let docker = docker.clone();
        let out = out_tx.clone();
        tokio::spawn(async move { handle(cm, docker, out).await });
      }
    }
  }

  writer.abort();
  Ok(())
}

async fn handle(cm: ClientMsg, docker: Docker, out: mpsc::UnboundedSender<AgentMsg>) {
  let id = cm.id;
  match cm.cmd {
    Cmd::List => {
      let resp = match list_containers(&docker).await {
        Ok(list) => Resp::Containers(list),
        Err(e) => Resp::Error(e.to_string()),
      };
      let _ = out.send(AgentMsg { id, resp });
      let _ = out.send(AgentMsg { id, resp: Resp::End });
    }
    // logs/stats streaming lands in the next step
    Cmd::Logs { .. } | Cmd::Stats { .. } => {
      let _ = out.send(AgentMsg { id, resp: Resp::Error("not implemented yet".into()) });
      let _ = out.send(AgentMsg { id, resp: Resp::End });
    }
    Cmd::Cancel => {}
  }
}

async fn list_containers(docker: &Docker) -> anyhow::Result<Vec<ContainerInfo>> {
  let opts = ListContainersOptionsBuilder::default().all(true).build();
  let containers = docker.list_containers(Some(opts)).await?;
  Ok(
    containers
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
      .collect(),
  )
}
