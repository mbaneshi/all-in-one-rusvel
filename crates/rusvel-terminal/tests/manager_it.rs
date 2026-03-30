//! Integration tests for [`rusvel_terminal::TerminalManager`].

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use rusvel_core::id::{RunId, SessionId};
use rusvel_core::ports::{EventPort, StoragePort, TerminalPort};
use rusvel_core::terminal::{PaneSize, PaneSource, PaneStatus, WindowSource};
use rusvel_db::Database;
use rusvel_event::EventBus;
use rusvel_terminal::TerminalManager;

const OBJ_KIND_SESSION: &str = "terminal.session";

fn test_db() -> Arc<Database> {
    Arc::new(Database::in_memory().expect("db"))
}

#[tokio::test]
async fn window_roundtrip_and_persist_object() {
    let db = test_db();
    let events: Arc<dyn EventPort> = Arc::new(EventBus::new(db.clone()));
    let storage: Arc<dyn StoragePort> = db.clone();
    let tm = TerminalManager::new(events, storage);

    let sid = SessionId::new();
    let wid = tm
        .create_window(&sid, "Test", WindowSource::Manual)
        .await
        .expect("window");
    let list = tm.list_windows(&sid).await.expect("list");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, wid);

    let raw = db
        .objects()
        .get(OBJ_KIND_SESSION, &sid.to_string())
        .await
        .expect("get")
        .expect("snapshot");
    assert!(raw.get("windows").is_some());
}

#[tokio::test]
async fn pane_exits_with_status_queryable_via_port() {
    let db = test_db();
    let events: Arc<dyn EventPort> = Arc::new(EventBus::new(db.clone()));
    let tm = TerminalManager::new(events, db.clone());

    let sid = SessionId::new();
    let wid = tm
        .create_window(&sid, "W", WindowSource::Manual)
        .await
        .unwrap();
    let run = RunId::new();
    let _pid = tm
        .create_pane(
            &wid,
            "exit 0",
            Path::new("/"),
            PaneSize { rows: 24, cols: 80 },
            PaneSource::AgentTool { run_id: run },
            None,
        )
        .await
        .expect("pane");

    tokio::time::sleep(Duration::from_millis(900)).await;

    let panes = tm.panes_for_run(&run).await.expect("q");
    let p = panes
        .iter()
        .find(|p| p.run_id == Some(run))
        .expect("pane with run_id");
    assert!(matches!(p.status, PaneStatus::Exited(0)));
}

#[tokio::test]
async fn inject_and_pane_scrollback_roundtrip() {
    let db = test_db();
    let events: Arc<dyn EventPort> = Arc::new(EventBus::new(db.clone()));
    let tm = TerminalManager::new(events, db.clone());

    let sid = SessionId::new();
    let wid = tm
        .create_window(&sid, "W", WindowSource::Manual)
        .await
        .unwrap();
    let pid = tm
        .create_pane(
            &wid,
            "sleep 3",
            Path::new("/"),
            PaneSize { rows: 24, cols: 80 },
            PaneSource::Shell,
            None,
        )
        .await
        .expect("pane");

    let line = b"hello-scrollback\r\n";
    tm.inject_pane_output(&pid, line).await.expect("inject");
    let back = tm.pane_scrollback(&pid).await.expect("scrollback");
    assert!(
        back.windows(line.len()).any(|w| w == line),
        "scrollback should contain injected line"
    );
}
