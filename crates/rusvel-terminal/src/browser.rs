//! Bridge CDP [`BrowserEvent`](rusvel_core::domain::BrowserEvent) into read-only browser panes (log via [`TerminalPort::inject_pane_output`](rusvel_core::ports::TerminalPort::inject_pane_output)).

use std::path::PathBuf;

use rusvel_core::domain::BrowserEvent;
use rusvel_core::error::Result;
use rusvel_core::id::{PaneId, SessionId};
use rusvel_core::ports::{BrowserPort, TerminalPort};
use rusvel_core::terminal::{PaneSize, PaneSource, WindowSource};
use tokio::sync::broadcast::error::RecvError;

/// Ensure a long-lived pane exists for this browser tab; used for CDP log streaming.
pub async fn ensure_browser_log_pane(
    terminal: &dyn TerminalPort,
    session_id: &SessionId,
    tab_id: &str,
    platform: &str,
) -> Result<PaneId> {
    for p in terminal.list_panes_for_session(session_id).await? {
        if let PaneSource::Browser {
            tab_id: ref t,
            platform: ref plat,
        } = p.source
        {
            if t == tab_id && plat == platform {
                return Ok(p.id);
            }
        }
    }

    let window_id = terminal
        .create_window(
            session_id,
            &format!("browser-{tab_id}"),
            WindowSource::Manual,
        )
        .await?;
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
    let pane_id = terminal
        .create_pane(
            &window_id,
            "sleep 86400",
            &cwd,
            PaneSize { rows: 24, cols: 80 },
            PaneSource::Browser {
                tab_id: tab_id.to_string(),
                platform: platform.to_string(),
            },
        )
        .await?;
    Ok(pane_id)
}

/// Append a formatted line for [`BrowserEvent::DataCaptured`](rusvel_core::domain::BrowserEvent::DataCaptured).
pub async fn inject_browser_event_log(
    terminal: &dyn TerminalPort,
    pane_id: &PaneId,
    event: &BrowserEvent,
) -> Result<()> {
    if let Some(line) = event.terminal_log_line() {
        let mut buf = line.into_bytes();
        buf.push(b'\n');
        terminal.inject_pane_output(pane_id, &buf).await?;
    }
    Ok(())
}

/// Subscribe to [`BrowserPort::observe`] and mirror capture lines into the pane.
pub async fn spawn_browser_log_bridge(
    browser: std::sync::Arc<dyn BrowserPort>,
    terminal: std::sync::Arc<dyn TerminalPort>,
    session_id: SessionId,
    tab_id: String,
    platform: String,
) -> Result<()> {
    let mut rx = browser.observe(&tab_id).await?;
    let pane_id =
        ensure_browser_log_pane(terminal.as_ref(), &session_id, &tab_id, &platform).await?;

    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    if let Err(e) =
                        inject_browser_event_log(terminal.as_ref(), &pane_id, &event).await
                    {
                        tracing::debug!(error = %e, "inject_browser_event_log failed");
                    }
                }
                Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => break,
            }
        }
    });
    Ok(())
}
