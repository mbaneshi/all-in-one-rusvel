//! Terminal multiplexer domain types (platform service, not a department).
//!
//! Phase 1: PTY-backed panes grouped under windows per session. See
//! `docs/plans/native-terminal-multiplexer.md`.

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::id::{FlowExecutionId, PaneId, RunId, SessionId, WindowId};

/// Rows/columns for a PTY or emulator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaneSize {
    pub rows: u16,
    pub cols: u16,
}

/// How a terminal window was created.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum WindowSource {
    Manual,
    Department(String),
    Playbook(String),
    DelegationChain(RunId),
}

/// How a pane was created.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum PaneSource {
    Shell,
    /// Session-scoped transcript for job/flow inject lines (`sleep` PTY; prefer read-only WS).
    SessionLog,
    /// Pane tied to a department (e.g. dept panel terminal).
    Department(String),
    AgentTool {
        run_id: RunId,
    },
    Delegation {
        /// Orchestrator run when known (optional in tool args; may equal `delegated_run_id`).
        parent_run_id: RunId,
        /// Sub-agent run — indexed by [`Pane::run_id`] for `panes_for_run`.
        delegated_run_id: RunId,
        persona: String,
    },
    FlowNode {
        flow_id: String,
        node_id: String,
        execution_id: String,
    },
    PlaybookStep {
        playbook_id: String,
        step_index: usize,
        run_id: String,
    },
    Browser {
        tab_id: String,
        #[serde(default)]
        platform: String,
    },
    Builder {
        agent_id: String,
    },
}

/// Layout for a window (split panes).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum Layout {
    Single,
    HSplit(Vec<f32>),
    VSplit(Vec<f32>),
    Grid { rows: u16, cols: u16 },
    Custom(serde_json::Value),
}

/// Runtime status of a pane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum PaneStatus {
    Running,
    Exited(i32),
    Suspended,
}

/// A terminal window (tab/workspace) within a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Window {
    pub id: WindowId,
    pub session_id: SessionId,
    pub name: String,
    pub dept_id: Option<String>,
    pub source: WindowSource,
    pub panes: Vec<PaneId>,
    pub layout: Layout,
    pub created_at: DateTime<Utc>,
    pub metadata: serde_json::Value,
}

/// One PTY-backed pane.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pane {
    pub id: PaneId,
    pub window_id: WindowId,
    pub title: String,
    pub command: String,
    pub cwd: PathBuf,
    pub env: HashMap<String, String>,
    pub size: PaneSize,
    pub status: PaneStatus,
    pub source: PaneSource,
    pub run_id: Option<RunId>,
    pub node_id: Option<String>,
    pub flow_execution_id: Option<FlowExecutionId>,
    pub created_at: DateTime<Utc>,
    pub metadata: serde_json::Value,
}
