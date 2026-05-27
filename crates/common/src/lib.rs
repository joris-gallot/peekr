//! Types shared between the hub (`server`) and the `agent`: container/log/stat
//! DTOs and the WebSocket protocol the hub uses to drive a remote agent.

use std::time::{Instant, SystemTime, UNIX_EPOCH};

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

/// Host-level (VPS) system stats, distinct from per-container stats.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HostStat {
  pub ts: u64,
  pub cpu_pct: f64,
  pub mem_used: u64,
  pub mem_total: u64,
  pub mem_pct: f64,
  pub disk_used: u64,
  pub disk_total: u64,
  pub disk_pct: f64,
  /// bytes/sec
  pub net_rx: u64,
  pub net_tx: u64,
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
  Logs {
    container: String,
    since: Option<i32>,
  },
  Stats {
    container: String,
  },
  /// host-level (VPS) system stats
  HostStats,
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
  HostStat(HostStat),
  /// stream finished (no more data for this id)
  End,
  Error(String),
}

pub fn now_ms() -> u64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map(|d| d.as_millis() as u64)
    .unwrap_or(0)
}

/// Samples host CPU / memory / disk / network. CPU and net are diff-based, so call
/// `sample` on an interval (the first call after `new` gives the rate since `new`).
pub struct HostSampler {
  sys: sysinfo::System,
  prev_rx: u64,
  prev_tx: u64,
  prev_t: Instant,
}

impl Default for HostSampler {
  fn default() -> Self {
    Self::new()
  }
}

impl HostSampler {
  pub fn new() -> Self {
    let mut sys = sysinfo::System::new();
    sys.refresh_cpu_usage();
    sys.refresh_memory();
    let (rx, tx) = net_totals();
    Self {
      sys,
      prev_rx: rx,
      prev_tx: tx,
      prev_t: Instant::now(),
    }
  }

  pub fn sample(&mut self) -> HostStat {
    self.sys.refresh_cpu_usage();
    self.sys.refresh_memory();

    let cpu_pct = self.sys.global_cpu_usage() as f64;
    let mem_total = self.sys.total_memory();
    let mem_used = self.sys.used_memory();
    let mem_pct = pct(mem_used, mem_total);

    let (disk_used, disk_total) = root_disk();
    let disk_pct = pct(disk_used, disk_total);

    let (rx, tx) = net_totals();
    let now = Instant::now();
    let secs = now.duration_since(self.prev_t).as_secs_f64().max(0.001);
    let net_rx = ((rx.saturating_sub(self.prev_rx)) as f64 / secs) as u64;
    let net_tx = ((tx.saturating_sub(self.prev_tx)) as f64 / secs) as u64;
    self.prev_rx = rx;
    self.prev_tx = tx;
    self.prev_t = now;

    HostStat {
      ts: now_ms(),
      cpu_pct,
      mem_used,
      mem_total,
      mem_pct,
      disk_used,
      disk_total,
      disk_pct,
      net_rx,
      net_tx,
    }
  }
}

fn pct(used: u64, total: u64) -> f64 {
  if total > 0 {
    used as f64 / total as f64 * 100.0
  } else {
    0.0
  }
}

/// Cumulative rx/tx across real interfaces (loopback excluded).
fn net_totals() -> (u64, u64) {
  let nets = sysinfo::Networks::new_with_refreshed_list();
  let (mut rx, mut tx) = (0u64, 0u64);
  for (name, data) in &nets {
    if name == "lo" || name.starts_with("lo") {
      continue;
    }
    rx += data.total_received();
    tx += data.total_transmitted();
  }
  (rx, tx)
}

/// (used, total) for the root filesystem, falling back to the largest disk.
fn root_disk() -> (u64, u64) {
  let disks = sysinfo::Disks::new_with_refreshed_list();
  let mut best: Option<(u64, u64)> = None;
  for d in &disks {
    let total = d.total_space();
    let used = total.saturating_sub(d.available_space());
    if d.mount_point() == std::path::Path::new("/") {
      return (used, total);
    }
    if best.map(|(_, t)| total > t).unwrap_or(true) {
      best = Some((used, total));
    }
  }
  best.unwrap_or((0, 0))
}
