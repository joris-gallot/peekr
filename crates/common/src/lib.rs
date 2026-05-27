//! Types shared between the hub (`server`) and the `agent`: container/log/stat
//! DTOs and the WebSocket protocol the hub uses to drive a remote agent.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ContainerInfo {
  pub id: String,
  pub name: String,
  pub image: String,
  pub state: String,
  pub status: String,
  /// docker compose project, empty for standalone containers
  pub project: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LogLine {
  pub ts: Option<String>,
  pub stream: String,
  pub msg: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StatsSample {
  pub ts: u64,
  pub cpu_pct: f64,
  pub mem_used: u64,
  pub mem_limit: u64,
  pub mem_pct: f64,
}

/// Hub -> agent. `id` correlates responses; reused as the stream handle to cancel.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ClientMsg {
  pub id: u64,
  pub cmd: Cmd,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Cmd {
  List,
  Logs { container: String, since: Option<i32> },
  Stats { container: String },
  /// stop the stream started under this `id`
  Cancel,
}

/// Agent -> hub, tagged with the originating request `id`.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AgentMsg {
  pub id: u64,
  pub resp: Resp,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
// adjacently tagged: supports tuple variants (Vec / String), unlike internally tagged
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum Resp {
  Containers(Vec<ContainerInfo>),
  Log(LogLine),
  Stat(StatsSample),
  /// stream finished (no more data for this id)
  End,
  Error(String),
}
