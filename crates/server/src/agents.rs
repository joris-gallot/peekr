//! Manages live WebSocket connections from remote agents and the request/response
//! multiplexing over them (one connection, many concurrent streams keyed by id).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use futures_util::{SinkExt, StreamExt};
use peekr_common::{AgentMsg, ClientMsg, Cmd, Resp};
use serde::Deserialize;
use tokio::sync::{Mutex, mpsc};
use tracing::info;

use crate::AppState;

type Pending = Arc<Mutex<HashMap<u64, mpsc::UnboundedSender<Resp>>>>;

struct AgentConn {
  to_agent: mpsc::UnboundedSender<ClientMsg>,
  pending: Pending,
  counter: AtomicU64,
}

#[derive(Clone, Default)]
pub struct AgentManager {
  conns: Arc<Mutex<HashMap<String, Arc<AgentConn>>>>,
}

impl AgentManager {
  pub async fn online_ids(&self) -> HashSet<String> {
    self.conns.lock().await.keys().cloned().collect()
  }

  /// Start a request; returns its id and a receiver of response frames.
  pub async fn request(&self, host: &str, cmd: Cmd) -> Option<(u64, mpsc::UnboundedReceiver<Resp>)> {
    let conn = self.conns.lock().await.get(host)?.clone();
    let id = conn.counter.fetch_add(1, Ordering::Relaxed);
    let (tx, rx) = mpsc::unbounded_channel();
    conn.pending.lock().await.insert(id, tx);
    conn.to_agent.send(ClientMsg { id, cmd }).ok()?;
    Some((id, rx))
  }

  /// Tell the agent to stop the stream under `id` and drop its routing entry.
  pub async fn cancel(&self, host: &str, id: u64) {
    if let Some(conn) = self.conns.lock().await.get(host) {
      let _ = conn.to_agent.send(ClientMsg { id, cmd: Cmd::Cancel });
      conn.pending.lock().await.remove(&id);
    }
  }
}

#[derive(Deserialize)]
pub struct ConnectQuery {
  token: String,
}

/// Agent connects here (WebSocket), authenticating with its host token.
pub async fn agent_connect(
  ws: WebSocketUpgrade,
  Query(q): Query<ConnectQuery>,
  State(state): State<AppState>,
) -> Response {
  let host_id: Option<String> = sqlx::query_scalar("SELECT id FROM hosts WHERE token = ?")
    .bind(&q.token)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);

  let Some(host_id) = host_id else {
    return StatusCode::UNAUTHORIZED.into_response();
  };
  ws.on_upgrade(move |socket| handle_agent(socket, host_id, state.agents.clone()))
}

async fn handle_agent(socket: WebSocket, host_id: String, mgr: AgentManager) {
  let (mut write, mut read) = socket.split();
  let (to_agent, mut outbox) = mpsc::unbounded_channel::<ClientMsg>();
  let pending: Pending = Arc::new(Mutex::new(HashMap::new()));

  let conn = Arc::new(AgentConn {
    to_agent,
    pending: pending.clone(),
    counter: AtomicU64::new(0),
  });
  mgr.conns.lock().await.insert(host_id.clone(), conn);
  info!("agent connected: {host_id}");

  // forward queued ClientMsgs to the socket
  let writer = tokio::spawn(async move {
    while let Some(msg) = outbox.recv().await {
      let Ok(txt) = serde_json::to_string(&msg) else { continue };
      if write.send(Message::Text(txt.into())).await.is_err() {
        break;
      }
    }
  });

  // route agent responses to the matching pending request
  while let Some(Ok(msg)) = read.next().await {
    if let Message::Text(t) = msg {
      let Ok(am) = serde_json::from_str::<AgentMsg>(t.as_str()) else { continue };
      let sender = pending.lock().await.get(&am.id).cloned();
      if let Some(s) = sender {
        let done = matches!(am.resp, Resp::End | Resp::Error(_));
        let _ = s.send(am.resp);
        if done {
          pending.lock().await.remove(&am.id);
        }
      }
    }
  }

  writer.abort();
  mgr.conns.lock().await.remove(&host_id);
  info!("agent disconnected: {host_id}");
}
