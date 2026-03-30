//! Long-lived CDP WebSocket: Network domain + passive JSON capture per tab.

use std::collections::VecDeque;
use std::sync::Arc;

use futures::{SinkExt, StreamExt};
use serde_json::json;
use tokio::sync::{Mutex, broadcast};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

use rusvel_core::domain::BrowserEvent;
use rusvel_core::{Result, RusvelError};

use crate::CdpState;
use crate::platforms;

type WsStream = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

pub async fn run_network_tab_observer(
    ws_url: String,
    tab_id: String,
    tab_tx: broadcast::Sender<BrowserEvent>,
    state: Arc<Mutex<CdpState>>,
) {
    if let Err(e) = run_network_inner(ws_url, tab_id, tab_tx, state).await {
        tracing::warn!("browser network observer ended: {e}");
    }
}

async fn run_network_inner(
    ws_url: String,
    tab_id: String,
    tab_tx: broadcast::Sender<BrowserEvent>,
    state: Arc<Mutex<CdpState>>,
) -> Result<()> {
    let (mut ws, _) = connect_async(&ws_url)
        .await
        .map_err(|e| RusvelError::Internal(format!("cdp connect: {e}")))?;
    let mut next_id: u64 = 1;
    let mut pending_urls: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let mut queued: VecDeque<serde_json::Value> = VecDeque::new();

    let id_run = send_cmd(&mut ws, &mut next_id, "Runtime.enable", json!({})).await?;
    wait_cmd_response(&mut ws, &mut queued, id_run).await?;
    let id_net = send_cmd(&mut ws, &mut next_id, "Network.enable", json!({})).await?;
    wait_cmd_response(&mut ws, &mut queued, id_net).await?;

    loop {
        let msg = next_msg(&mut ws, &mut queued).await?;
        if let Some(id) = msg.get("id").and_then(|x| x.as_u64()) {
            let _ = id;
            continue;
        }
        let Some(method) = msg.get("method").and_then(|m| m.as_str()) else {
            continue;
        };
        match method {
            "Network.responseReceived" => {
                let params = msg.get("params").cloned().unwrap_or(json!({}));
                let request_id = params
                    .get("requestId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let response = params.get("response").cloned().unwrap_or(json!({}));
                let url = response.get("url").and_then(|v| v.as_str()).unwrap_or("");
                let mime = response
                    .get("mimeType")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if request_id.is_empty() {
                    continue;
                }
                let json_like = mime.contains("json")
                    || mime.contains("graphql")
                    || url.contains("/api/")
                    || url.contains("graphql");
                if !json_like {
                    continue;
                }
                if url.contains("upwork.com")
                    || platforms::upwork::matches_capture_url(url)
                    || url.contains("freelancer.com")
                    || platforms::freelancer::matches_capture_url(url)
                {
                    pending_urls.insert(request_id.to_string(), url.to_string());
                }
            }
            "Network.loadingFinished" => {
                let params = msg.get("params").cloned().unwrap_or(json!({}));
                let request_id = params
                    .get("requestId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if request_id.is_empty() {
                    continue;
                }
                let Some(url) = pending_urls.remove(request_id) else {
                    continue;
                };
                let id_body = send_cmd(
                    &mut ws,
                    &mut next_id,
                    "Network.getResponseBody",
                    json!({ "requestId": request_id }),
                )
                .await?;
                let result = wait_cmd_response(&mut ws, &mut queued, id_body).await?;
                let body_result = result.get("result").cloned().unwrap_or(json!({}));
                let body_str = body_result
                    .get("body")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let b64 = body_result.get("base64Encoded").and_then(|v| v.as_bool()) == Some(true);
                let text_body = if b64 {
                    let raw = base64::Engine::decode(
                        &base64::engine::general_purpose::STANDARD,
                        body_str,
                    )
                    .map_err(|e| RusvelError::Internal(format!("base64: {e}")))?;
                    String::from_utf8_lossy(&raw).into_owned()
                } else {
                    body_str.to_string()
                };
                let parsed: serde_json::Value = match serde_json::from_str(&text_body) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                let events = platforms::route_json_response(&url, &parsed, &tab_id);
                for ev in events {
                    fanout_event(&tab_tx, &state, ev).await;
                }
            }
            _ => {}
        }
    }
}

async fn fanout_event(
    tab_tx: &broadcast::Sender<BrowserEvent>,
    state: &Arc<Mutex<CdpState>>,
    event: BrowserEvent,
) {
    let _ = tab_tx.send(event.clone());
    let mut g = state.lock().await;
    if g.captures.len() >= 500 {
        g.captures.remove(0);
    }
    if let Ok(v) = serde_json::to_value(&event) {
        g.captures.push(v);
    }
    let _ = g.capture_tx.send(event);
}

async fn send_cmd(
    ws: &mut WsStream,
    next_id: &mut u64,
    method: &str,
    params: serde_json::Value,
) -> Result<u64> {
    let id = *next_id;
    *next_id += 1;
    let msg = json!({ "id": id, "method": method, "params": params });
    let text =
        serde_json::to_string(&msg).map_err(|e| RusvelError::Serialization(e.to_string()))?;
    ws.send(Message::Text(text.into()))
        .await
        .map_err(|e| RusvelError::Internal(e.to_string()))?;
    Ok(id)
}

async fn read_ws_json(ws: &mut WsStream) -> Result<serde_json::Value> {
    loop {
        let frame = match ws.next().await {
            Some(Ok(m)) => m,
            Some(Err(e)) => return Err(RusvelError::Internal(e.to_string())),
            None => return Err(RusvelError::Internal("cdp websocket closed".into())),
        };
        let Message::Text(text) = frame else {
            continue;
        };
        let v: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| RusvelError::Serialization(e.to_string()))?;
        return Ok(v);
    }
}

async fn next_msg(
    ws: &mut WsStream,
    queue: &mut VecDeque<serde_json::Value>,
) -> Result<serde_json::Value> {
    if let Some(v) = queue.pop_front() {
        return Ok(v);
    }
    read_ws_json(ws).await
}

/// Read from WebSocket until `id == want`, buffering other messages into `queue`.
async fn wait_cmd_response(
    ws: &mut WsStream,
    queue: &mut VecDeque<serde_json::Value>,
    want: u64,
) -> Result<serde_json::Value> {
    loop {
        let v = read_ws_json(ws).await?;
        if v.get("id").and_then(serde_json::Value::as_u64) == Some(want) {
            if let Some(err) = v.get("error") {
                return Err(RusvelError::Internal(err.to_string()));
            }
            return Ok(v);
        }
        queue.push_back(v);
    }
}
