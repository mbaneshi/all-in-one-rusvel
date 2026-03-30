//! CDP WebSocket adapter for [`BrowserPort`](rusvel_core::ports::BrowserPort).
//!
//! Passive network capture (Upwork JSON) and broadcast channels for [`observe`](BrowserPort::observe).

mod network;
mod observe;
mod platforms;
pub mod pool;
mod transport;

use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use rusvel_core::domain::{BrowserEvent, TabInfo};
use rusvel_core::ports::BrowserPort;
use rusvel_core::{Result, RusvelError};
use tokio::sync::{Mutex, broadcast};

pub use network::NetworkCapture;
pub use platforms::upwork;
pub use pool::{CdpPool, ChromeProfileConfig};

/// Chrome DevTools Protocol client (passive foundation + network capture).
pub struct CdpClient {
    state: Arc<Mutex<CdpState>>,
}

pub(crate) struct CdpState {
    connected: bool,
    http_base: String,
    targets: HashMap<String, TargetMeta>,
    observers: HashMap<String, broadcast::Sender<BrowserEvent>>,
    capture_tx: broadcast::Sender<BrowserEvent>,
    captures: Vec<serde_json::Value>,
    network_started: HashSet<String>,
}

struct TargetMeta {
    ws_url: String,
}

impl CdpState {
    fn new() -> Self {
        let (capture_tx, _) = broadcast::channel(256);
        Self {
            connected: false,
            http_base: String::new(),
            targets: HashMap::new(),
            observers: HashMap::new(),
            capture_tx,
            captures: Vec::new(),
            network_started: HashSet::new(),
        }
    }
}

impl CdpClient {
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(CdpState::new())),
        }
    }

    /// Subscribe to all capture events (for SSE / dashboard).
    pub async fn subscribe_captures(&self) -> broadcast::Receiver<BrowserEvent> {
        let g = self.state.lock().await;
        g.capture_tx.subscribe()
    }

    /// Recent capture records (ring buffer, newest last).
    pub async fn captures_snapshot(&self) -> Vec<serde_json::Value> {
        let g = self.state.lock().await;
        g.captures.clone()
    }

    pub async fn is_connected(&self) -> bool {
        self.state.lock().await.connected
    }

    fn platform_hint(url: &str) -> Option<String> {
        let u = url.to_lowercase();
        if u.contains("upwork.com") {
            Some("upwork".into())
        } else if u.contains("linkedin.com") {
            Some("linkedin".into())
        } else if u.contains("freelancer.com") {
            Some("freelancer".into())
        } else {
            None
        }
    }
}

impl Default for CdpClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl BrowserPort for CdpClient {
    async fn connect(&self, endpoint: &str) -> Result<()> {
        let base = transport::http_base(endpoint);
        let base = base.as_ref().to_string();
        transport::fetch_targets(&base).await?;
        let mut g = self.state.lock().await;
        g.connected = true;
        g.http_base = base;
        g.targets.clear();
        g.observers.clear();
        g.network_started.clear();
        g.captures.clear();
        Ok(())
    }

    async fn disconnect(&self) -> Result<()> {
        let mut g = self.state.lock().await;
        g.connected = false;
        g.http_base.clear();
        g.targets.clear();
        g.observers.clear();
        g.network_started.clear();
        Ok(())
    }

    async fn tabs(&self) -> Result<Vec<TabInfo>> {
        let http_base = {
            let g = self.state.lock().await;
            if !g.connected {
                return Err(RusvelError::Internal(
                    "browser not connected; call connect() first".into(),
                ));
            }
            g.http_base.clone()
        };
        let list = transport::fetch_targets(&http_base).await?;
        let mut g = self.state.lock().await;
        g.targets.clear();
        let mut out = Vec::new();
        for t in list {
            if t.target_type != "page" {
                continue;
            }
            let Some(ws) = t.web_socket_debugger_url.clone() else {
                continue;
            };
            g.targets.insert(t.id.clone(), TargetMeta { ws_url: ws });
            out.push(TabInfo {
                id: t.id,
                url: t.url.clone(),
                title: t.title,
                platform: Self::platform_hint(&t.url),
                metadata: Default::default(),
            });
        }
        Ok(out)
    }

    async fn observe(&self, tab_id: &str) -> Result<broadcast::Receiver<BrowserEvent>> {
        let ws_url = {
            let g = self.state.lock().await;
            if !g.connected {
                return Err(RusvelError::Internal(
                    "browser not connected; call connect() first".into(),
                ));
            }
            g.targets
                .get(tab_id)
                .map(|m| m.ws_url.clone())
                .ok_or_else(|| RusvelError::NotFound {
                    kind: "browser_tab".into(),
                    id: tab_id.to_string(),
                })?
        };

        let tid = tab_id.to_string();
        let (rx, net_tx) = {
            let mut g = self.state.lock().await;
            if let Some(tx) = g.observers.get(&tid) {
                (tx.subscribe(), None)
            } else {
                let need_net = !g.network_started.contains(&tid);
                if need_net {
                    g.network_started.insert(tid.clone());
                }
                let (tx, _) = broadcast::channel(64);
                let rx = tx.subscribe();
                let tx_spawn = need_net.then(|| tx.clone());
                g.observers.insert(tid.clone(), tx);
                (rx, tx_spawn)
            }
        };

        if let Some(tx) = net_tx {
            let tab = tab_id.to_string();
            let state = self.state.clone();
            tokio::spawn(async move {
                observe::run_network_tab_observer(ws_url, tab, tx, state).await;
            });
        }

        Ok(rx)
    }

    async fn evaluate_js(&self, tab_id: &str, script: &str) -> Result<serde_json::Value> {
        let ws_url = {
            let g = self.state.lock().await;
            if !g.connected {
                return Err(RusvelError::Internal(
                    "browser not connected; call connect() first".into(),
                ));
            }
            g.targets
                .get(tab_id)
                .map(|m| m.ws_url.clone())
                .ok_or_else(|| RusvelError::NotFound {
                    kind: "browser_tab".into(),
                    id: tab_id.to_string(),
                })?
        };
        transport::cdp_evaluate(&ws_url, script).await
    }

    async fn navigate(&self, tab_id: &str, url: &str) -> Result<()> {
        let ws_url = {
            let g = self.state.lock().await;
            if !g.connected {
                return Err(RusvelError::Internal(
                    "browser not connected; call connect() first".into(),
                ));
            }
            g.targets
                .get(tab_id)
                .map(|m| m.ws_url.clone())
                .ok_or_else(|| RusvelError::NotFound {
                    kind: "browser_tab".into(),
                    id: tab_id.to_string(),
                })?
        };
        transport::cdp_navigate(&ws_url, url).await
    }
}

impl CdpClient {
    /// Deliver a [`BrowserEvent`] to [`BrowserPort::observe`] subscribers (tests, manual injection, CDP capture).
    pub async fn publish_browser_event(&self, tab_id: &str, event: BrowserEvent) -> Result<()> {
        let mut g = self.state.lock().await;
        if !g.connected {
            return Err(RusvelError::Internal(
                "browser not connected; call connect() first".into(),
            ));
        }
        if !g.targets.contains_key(tab_id) {
            return Err(RusvelError::NotFound {
                kind: "browser_tab".into(),
                id: tab_id.to_string(),
            });
        }
        let tx = match g.observers.entry(tab_id.to_string()) {
            Entry::Occupied(e) => e.get().clone(),
            Entry::Vacant(e) => {
                let (tx, _) = broadcast::channel(64);
                e.insert(tx.clone());
                tx
            }
        };
        let _ = tx.send(event.clone());
        if g.captures.len() >= 500 {
            g.captures.remove(0);
        }
        if let Ok(v) = serde_json::to_value(&event) {
            g.captures.push(v);
        }
        let _ = g.capture_tx.send(event);
        Ok(())
    }
}
