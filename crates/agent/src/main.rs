//! peekr agent: runs on a remote host, connects out to the hub over WebSocket,
//! and answers the hub's commands (list/logs/stats) about the local Docker.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bollard::Docker;
use bollard::container::LogOutput;
use bollard::query_parameters::{
  ListContainersOptionsBuilder, LogsOptionsBuilder, StatsOptionsBuilder,
};
use futures_util::{SinkExt, StreamExt};
use peekr_common::{AgentMsg, ClientMsg, Cmd, ContainerInfo, LogLine, Resp, StatsSample};
use tokio::sync::{Mutex, mpsc};
use tokio::task::AbortHandle;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tracing::{info, warn};

type Out = mpsc::UnboundedSender<AgentMsg>;
type Tasks = Arc<Mutex<HashMap<u64, AbortHandle>>>;

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
  let (out, mut outbox) = mpsc::unbounded_channel::<AgentMsg>();
  let tasks: Tasks = Arc::new(Mutex::new(HashMap::new()));

  let writer = tokio::spawn(async move {
    while let Some(msg) = outbox.recv().await {
      let Ok(txt) = serde_json::to_string(&msg) else {
        continue;
      };
      if write.send(Message::Text(txt)).await.is_err() {
        break;
      }
    }
  });

  while let Some(Ok(msg)) = read.next().await {
    if let Message::Text(t) = msg {
      let Ok(cm) = serde_json::from_str::<ClientMsg>(&t) else {
        continue;
      };
      if matches!(cm.cmd, Cmd::Cancel) {
        if let Some(h) = tasks.lock().await.remove(&cm.id) {
          h.abort();
        }
        continue;
      }
      let (docker, out, tasks2) = (docker.clone(), out.clone(), tasks.clone());
      let handle = tokio::spawn(async move { handle(cm.id, cm.cmd, docker, out, tasks2).await });
      tasks.lock().await.insert(cm.id, handle.abort_handle());
    }
  }

  writer.abort();
  Ok(())
}

async fn handle(id: u64, cmd: Cmd, docker: Docker, out: Out, tasks: Tasks) {
  match cmd {
    Cmd::List => {
      let resp = match list_containers(&docker).await {
        Ok(list) => Resp::Containers(list),
        Err(e) => Resp::Error(e.to_string()),
      };
      let _ = out.send(AgentMsg { id, resp });
    }
    Cmd::Logs { container, since } => stream_logs(id, &docker, &container, since, &out).await,
    Cmd::Stats { container } => stream_stats(id, &docker, &container, &out).await,
    Cmd::HostStats => stream_host_stats(id, &out).await,
    Cmd::Cancel => {}
  }
  let _ = out.send(AgentMsg {
    id,
    resp: Resp::End,
  });
  tasks.lock().await.remove(&id);
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

async fn stream_logs(id: u64, docker: &Docker, container: &str, since: Option<i32>, out: &Out) {
  let mut builder = LogsOptionsBuilder::default()
    .stdout(true)
    .stderr(true)
    .follow(true)
    .timestamps(true);
  builder = match since {
    Some(s) => builder.since(s),
    None => builder.tail("200"),
  };
  let mut stream = docker.logs(container, Some(builder.build()));
  while let Some(res) = stream.next().await {
    let Ok(output) = res else { break };
    let (stream_name, bytes) = match &output {
      LogOutput::StdErr { message } => ("stderr", message),
      LogOutput::StdOut { message } => ("stdout", message),
      LogOutput::StdIn { message } => ("stdout", message),
      LogOutput::Console { message } => ("stdout", message),
    };
    let text = String::from_utf8_lossy(bytes);
    for line in text.split('\n').filter(|l| !l.is_empty()) {
      let (ts, msg) = split_ts(line);
      let line = LogLine {
        ts,
        stream: stream_name.to_string(),
        msg,
      };
      if out
        .send(AgentMsg {
          id,
          resp: Resp::Log(line),
        })
        .is_err()
      {
        return;
      }
    }
  }
}

async fn stream_stats(id: u64, docker: &Docker, container: &str, out: &Out) {
  let opts = StatsOptionsBuilder::default().stream(true).build();
  let mut stream = docker.stats(container, Some(opts));
  while let Some(Ok(s)) = stream.next().await {
    let Some(cpu) = s.cpu_stats else { continue };
    let Some(precpu) = s.precpu_stats else {
      continue;
    };
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
    let Some(mem) = s.memory_stats else { continue };
    let usage = mem.usage.unwrap_or(0);
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
    if out
      .send(AgentMsg {
        id,
        resp: Resp::Stat(sample),
      })
      .is_err()
    {
      return;
    }
  }
}

async fn stream_host_stats(id: u64, out: &Out) {
  let mut sampler = peekr_common::HostSampler::new();
  loop {
    tokio::time::sleep(Duration::from_secs(2)).await;
    if out
      .send(AgentMsg {
        id,
        resp: Resp::HostStat(sampler.sample()),
      })
      .is_err()
    {
      return;
    }
  }
}

fn split_ts(line: &str) -> (Option<String>, String) {
  match line.split_once(' ') {
    Some((ts, rest)) if ts.len() >= 20 && ts.contains('T') => {
      (Some(ts.to_string()), rest.to_string())
    }
    _ => (None, line.to_string()),
  }
}

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
