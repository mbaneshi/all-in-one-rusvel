//! In-process terminal multiplexer using `portable-pty`.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use rusvel_core::domain::Event;
use rusvel_core::error::{Result, RusvelError};
use rusvel_core::id::{EventId, FlowExecutionId, PaneId, RunId, SessionId, WindowId};
use rusvel_core::ports::{EventPort, StoragePort, TerminalPort};
use rusvel_core::terminal::{Layout, Pane, PaneSize, PaneSource, PaneStatus, Window, WindowSource};
use tokio::sync::{RwLock, broadcast};

const OUT_CHANNEL_CAP: usize = 256;
const OBJ_KIND_SESSION: &str = "terminal.session";

fn pty_size(size: PaneSize) -> PtySize {
    PtySize {
        rows: size.rows,
        cols: size.cols,
        pixel_width: 0,
        pixel_height: 0,
    }
}

fn map_pty_err(e: impl std::fmt::Display) -> RusvelError {
    RusvelError::Internal(format!("pty: {e}"))
}

/// PTY-backed terminal manager (platform service).
pub struct TerminalManager {
    inner: Arc<ManagerInner>,
}

struct ManagerInner {
    events: Arc<dyn EventPort>,
    storage: Arc<dyn StoragePort>,
    windows: RwLock<HashMap<WindowId, Window>>,
    session_windows: RwLock<HashMap<SessionId, Vec<WindowId>>>,
    panes: RwLock<HashMap<PaneId, Pane>>,
    active: RwLock<HashMap<PaneId, PaneRuntime>>,
}

struct PaneRuntime {
    #[allow(dead_code)]
    master: Arc<tokio::sync::Mutex<Box<dyn MasterPty + Send>>>,
    writer: Arc<tokio::sync::Mutex<Box<dyn Write + Send>>>,
    out: broadcast::Sender<Vec<u8>>,
    child: Arc<tokio::sync::Mutex<Box<dyn Child + Send + Sync>>>,
}

impl TerminalManager {
    /// Create a manager with event and storage ports for lifecycle + session snapshots.
    pub fn new(events: Arc<dyn EventPort>, storage: Arc<dyn StoragePort>) -> Self {
        Self {
            inner: Arc::new(ManagerInner {
                events,
                storage,
                windows: RwLock::new(HashMap::new()),
                session_windows: RwLock::new(HashMap::new()),
                panes: RwLock::new(HashMap::new()),
                active: RwLock::new(HashMap::new()),
            }),
        }
    }
}

impl ManagerInner {
    async fn persist_session(&self, session_id: &SessionId) -> Result<()> {
        let windows = self.list_windows_impl(session_id).await?;
        let payload = serde_json::json!({ "windows": windows });
        self.storage
            .objects()
            .put(OBJ_KIND_SESSION, &session_id.to_string(), payload)
            .await
    }

    async fn list_windows_impl(&self, session_id: &SessionId) -> Result<Vec<Window>> {
        let ids = self
            .session_windows
            .read()
            .await
            .get(session_id)
            .cloned()
            .unwrap_or_default();
        let windows = self.windows.read().await;
        let mut out: Vec<Window> = ids
            .iter()
            .filter_map(|wid| windows.get(wid).cloned())
            .collect();
        out.sort_by_key(|w| w.created_at);
        Ok(out)
    }

    async fn emit_terminal(
        &self,
        session_id: Option<SessionId>,
        kind: &str,
        payload: serde_json::Value,
    ) -> Result<()> {
        let event = Event {
            id: EventId::new(),
            session_id,
            run_id: None,
            source: "terminal".into(),
            kind: kind.into(),
            payload,
            created_at: Utc::now(),
            metadata: serde_json::json!({}),
        };
        let _ = self.events.emit(event).await?;
        Ok(())
    }

    fn extract_source_fields(
        source: &PaneSource,
    ) -> (Option<RunId>, Option<String>, Option<FlowExecutionId>) {
        match source {
            PaneSource::AgentTool { run_id } => (Some(*run_id), None, None),
            PaneSource::Delegation {
                delegated_run_id, ..
            } => (Some(*delegated_run_id), None, None),
            PaneSource::FlowNode {
                execution_id,
                node_id,
                ..
            } => {
                let exec = uuid::Uuid::parse_str(execution_id)
                    .ok()
                    .map(FlowExecutionId::from_uuid);
                (None, Some(node_id.clone()), exec)
            }
            PaneSource::PlaybookStep { run_id, .. } => {
                let rid = uuid::Uuid::parse_str(run_id).ok().map(RunId::from_uuid);
                (rid, None, None)
            }
            _ => (None, None, None),
        }
    }

    async fn session_id_for_pane(&self, pane_id: &PaneId) -> Option<SessionId> {
        let panes = self.panes.read().await;
        let pane = panes.get(pane_id)?;
        let windows = self.windows.read().await;
        let w = windows.get(&pane.window_id)?;
        Some(w.session_id)
    }

    async fn on_reader_eof(self: Arc<Self>, pane_id: PaneId) -> Result<()> {
        let mut code: i32 = -1;
        {
            let mut active = self.active.write().await;
            if let Some(rt) = active.remove(&pane_id) {
                let mut child = rt.child.lock().await;
                if let Ok(Some(status)) = child.try_wait() {
                    code = if status.success() {
                        0
                    } else {
                        i32::try_from(status.exit_code()).unwrap_or(-1)
                    };
                } else if let Ok(status) = child.wait() {
                    code = if status.success() {
                        0
                    } else {
                        i32::try_from(status.exit_code()).unwrap_or(-1)
                    };
                }
            }
        }

        let session_id = {
            let mut panes = self.panes.write().await;
            if let Some(pane) = panes.get_mut(&pane_id) {
                pane.status = PaneStatus::Exited(code);
            }
            drop(panes);
            self.session_id_for_pane(&pane_id).await
        };

        if let Some(sid) = session_id {
            self.persist_session(&sid).await.ok();
            self.emit_terminal(
                Some(sid),
                "terminal.pane.exited",
                serde_json::json!({ "pane_id": pane_id, "exit_code": code }),
            )
            .await?;
        }

        Ok(())
    }
}

#[async_trait]
impl TerminalPort for TerminalManager {
    async fn create_window(
        &self,
        session_id: &SessionId,
        name: &str,
        source: WindowSource,
    ) -> Result<WindowId> {
        let id = WindowId::new();
        let window = Window {
            id,
            session_id: *session_id,
            name: name.to_string(),
            dept_id: None,
            source,
            panes: vec![],
            layout: Layout::Single,
            created_at: Utc::now(),
            metadata: serde_json::json!({}),
        };
        {
            let mut windows = self.inner.windows.write().await;
            windows.insert(id, window);
        }
        {
            let mut idx = self.inner.session_windows.write().await;
            idx.entry(*session_id).or_default().push(id);
        }
        self.inner.persist_session(session_id).await?;
        self.inner
            .emit_terminal(
                Some(*session_id),
                "terminal.window.created",
                serde_json::json!({ "window_id": id, "name": name }),
            )
            .await?;
        Ok(id)
    }

    async fn list_windows(&self, session_id: &SessionId) -> Result<Vec<Window>> {
        self.inner.list_windows_impl(session_id).await
    }

    async fn list_panes_for_session(&self, session_id: &SessionId) -> Result<Vec<Pane>> {
        let window_ids = {
            let idx = self.inner.session_windows.read().await;
            idx.get(session_id).cloned().unwrap_or_default()
        };
        let windows = self.inner.windows.read().await;
        let panes = self.inner.panes.read().await;
        let mut out = Vec::new();
        for wid in window_ids {
            if let Some(w) = windows.get(&wid) {
                for pid in &w.panes {
                    if let Some(p) = panes.get(pid) {
                        out.push(p.clone());
                    }
                }
            }
        }
        out.sort_by_key(|p| p.created_at);
        Ok(out)
    }

    async fn close_window(&self, window_id: &WindowId) -> Result<()> {
        let session_id = {
            let windows = self.inner.windows.read().await;
            windows.get(window_id).map(|w| w.session_id)
        };
        let pane_ids: Vec<PaneId> = {
            let windows = self.inner.windows.read().await;
            windows
                .get(window_id)
                .map(|w| w.panes.clone())
                .unwrap_or_default()
        };
        for pid in pane_ids {
            let _ = self.close_pane(&pid).await;
        }
        {
            let mut windows = self.inner.windows.write().await;
            windows.remove(window_id);
        }
        if let Some(sid) = session_id {
            let mut idx = self.inner.session_windows.write().await;
            if let Some(list) = idx.get_mut(&sid) {
                list.retain(|w| w != window_id);
            }
            self.inner.persist_session(&sid).await?;
            self.inner
                .emit_terminal(
                    Some(sid),
                    "terminal.window.closed",
                    serde_json::json!({ "window_id": window_id }),
                )
                .await?;
        }
        Ok(())
    }

    async fn create_pane(
        &self,
        window_id: &WindowId,
        cmd: &str,
        cwd: &Path,
        size: PaneSize,
        source: PaneSource,
    ) -> Result<PaneId> {
        let (session_id, title) = {
            let windows = self.inner.windows.read().await;
            let w = windows
                .get(window_id)
                .ok_or_else(|| RusvelError::NotFound {
                    kind: "Window".into(),
                    id: window_id.to_string(),
                })?;
            let title = if cmd.len() > 48 {
                format!("{}…", &cmd[..47])
            } else {
                cmd.to_string()
            };
            (w.session_id, title)
        };

        let pane_id = PaneId::new();
        let (run_id, node_id, flow_execution_id) = ManagerInner::extract_source_fields(&source);

        let pty_system = native_pty_system();
        let pair = pty_system.openpty(pty_size(size)).map_err(map_pty_err)?;

        let mut cmd_builder = CommandBuilder::new("sh");
        cmd_builder.arg("-c");
        cmd_builder.arg(cmd);
        if !cwd.as_os_str().is_empty() {
            cmd_builder.cwd(cwd);
        }

        let child = pair.slave.spawn_command(cmd_builder).map_err(map_pty_err)?;

        let master = Arc::new(tokio::sync::Mutex::new(pair.master));
        let writer = {
            let m = master.lock().await;
            m.take_writer().map_err(map_pty_err)?
        };
        let writer = Arc::new(tokio::sync::Mutex::new(writer));
        let reader = {
            let m = master.lock().await;
            m.try_clone_reader().map_err(map_pty_err)?
        };

        let (out_tx, _out_rx) = broadcast::channel::<Vec<u8>>(OUT_CHANNEL_CAP);
        let out_tx_task = out_tx.clone();

        let pane = Pane {
            id: pane_id,
            window_id: *window_id,
            title,
            command: cmd.to_string(),
            cwd: cwd.to_path_buf(),
            env: HashMap::new(),
            size,
            status: PaneStatus::Running,
            source,
            run_id,
            node_id,
            flow_execution_id,
            created_at: Utc::now(),
            metadata: serde_json::json!({}),
        };

        {
            let mut panes = self.inner.panes.write().await;
            panes.insert(pane_id, pane.clone());
        }
        {
            let mut windows = self.inner.windows.write().await;
            if let Some(w) = windows.get_mut(window_id) {
                w.panes.push(pane_id);
            }
        }

        let child = Arc::new(tokio::sync::Mutex::new(child));
        let rt = PaneRuntime {
            master: master.clone(),
            writer: writer.clone(),
            out: out_tx,
            child: child.clone(),
        };
        {
            let mut active = self.inner.active.write().await;
            active.insert(pane_id, rt);
        }

        let inner = Arc::clone(&self.inner);
        let read_handle = tokio::task::spawn_blocking(move || {
            let mut buf = [0u8; 4096];
            let mut reader = reader;
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let _ = out_tx_task.send(buf[..n].to_vec());
                    }
                    Err(_) => break,
                }
            }
        });

        tokio::spawn(async move {
            let _ = read_handle.await;
            let _ = inner.on_reader_eof(pane_id).await;
        });

        self.inner.persist_session(&session_id).await?;
        self.inner
            .emit_terminal(
                Some(session_id),
                "terminal.pane.created",
                serde_json::json!({
                    "pane_id": pane_id,
                    "window_id": window_id,
                    "command": cmd,
                }),
            )
            .await?;

        Ok(pane_id)
    }

    async fn write_pane(&self, pane_id: &PaneId, data: &[u8]) -> Result<()> {
        let active = self.inner.active.read().await;
        let rt = active.get(pane_id).ok_or_else(|| RusvelError::NotFound {
            kind: "Pane".into(),
            id: pane_id.to_string(),
        })?;
        let mut w = rt.writer.lock().await;
        w.write_all(data)
            .map_err(|e| RusvelError::Internal(e.to_string()))?;
        w.flush().ok();
        Ok(())
    }

    async fn inject_pane_output(&self, pane_id: &PaneId, data: &[u8]) -> Result<()> {
        let active = self.inner.active.read().await;
        let rt = active.get(pane_id).ok_or_else(|| RusvelError::NotFound {
            kind: "Pane".into(),
            id: pane_id.to_string(),
        })?;
        let _ = rt.out.send(data.to_vec());
        Ok(())
    }

    async fn resize_pane(&self, pane_id: &PaneId, size: PaneSize) -> Result<()> {
        let master = {
            let active = self.inner.active.read().await;
            let rt = active.get(pane_id).ok_or_else(|| RusvelError::NotFound {
                kind: "Pane".into(),
                id: pane_id.to_string(),
            })?;
            rt.master.clone()
        };
        {
            let m = master.lock().await;
            m.resize(pty_size(size)).map_err(map_pty_err)?;
        }
        {
            let mut panes = self.inner.panes.write().await;
            if let Some(p) = panes.get_mut(pane_id) {
                p.size = size;
            }
        }
        let sid = self.inner.session_id_for_pane(pane_id).await;
        if let Some(s) = sid {
            self.inner
                .emit_terminal(
                    Some(s),
                    "terminal.pane.resized",
                    serde_json::json!({ "pane_id": pane_id, "rows": size.rows, "cols": size.cols }),
                )
                .await?;
        }
        Ok(())
    }

    async fn close_pane(&self, pane_id: &PaneId) -> Result<()> {
        let session_id = self.inner.session_id_for_pane(pane_id).await;
        if let Some(rt) = self.inner.active.write().await.remove(pane_id) {
            let mut c = rt.child.lock().await;
            let _ = c.kill();
        }
        {
            let mut panes = self.inner.panes.write().await;
            if let Some(p) = panes.get_mut(pane_id) {
                p.status = PaneStatus::Exited(137);
            }
        }
        {
            let mut windows = self.inner.windows.write().await;
            for w in windows.values_mut() {
                w.panes.retain(|p| p != pane_id);
            }
        }
        if let Some(sid) = session_id {
            self.inner.persist_session(&sid).await.ok();
            self.inner
                .emit_terminal(
                    Some(sid),
                    "terminal.pane.exited",
                    serde_json::json!({ "pane_id": pane_id, "exit_code": 137 }),
                )
                .await?;
        }
        Ok(())
    }

    async fn subscribe_pane(&self, pane_id: &PaneId) -> Result<broadcast::Receiver<Vec<u8>>> {
        let active = self.inner.active.read().await;
        let rt = active.get(pane_id).ok_or_else(|| RusvelError::NotFound {
            kind: "Pane".into(),
            id: pane_id.to_string(),
        })?;
        Ok(rt.out.subscribe())
    }

    async fn get_layout(&self, window_id: &WindowId) -> Result<Layout> {
        let windows = self.inner.windows.read().await;
        let w = windows
            .get(window_id)
            .ok_or_else(|| RusvelError::NotFound {
                kind: "Window".into(),
                id: window_id.to_string(),
            })?;
        Ok(w.layout.clone())
    }

    async fn set_layout(&self, window_id: &WindowId, layout: Layout) -> Result<()> {
        let session_id = {
            let mut windows = self.inner.windows.write().await;
            let w = windows
                .get_mut(window_id)
                .ok_or_else(|| RusvelError::NotFound {
                    kind: "Window".into(),
                    id: window_id.to_string(),
                })?;
            w.layout = layout.clone();
            w.session_id
        };
        self.inner.persist_session(&session_id).await?;
        Ok(())
    }

    async fn panes_for_run(&self, run_id: &RunId) -> Result<Vec<Pane>> {
        let panes = self.inner.panes.read().await;
        let mut out: Vec<Pane> = panes
            .values()
            .filter(|p| p.run_id == Some(*run_id))
            .cloned()
            .collect();
        out.sort_by_key(|p| p.created_at);
        Ok(out)
    }

    async fn panes_for_flow(&self, execution_id: &FlowExecutionId) -> Result<Vec<Pane>> {
        let panes = self.inner.panes.read().await;
        let mut out: Vec<Pane> = panes
            .values()
            .filter(|p| p.flow_execution_id == Some(*execution_id))
            .cloned()
            .collect();
        out.sort_by_key(|p| p.created_at);
        Ok(out)
    }
}
