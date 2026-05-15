//! `EvalCtx` — the harness handed to every [`crate::Eval`].
//!
//! Wraps a real in-memory SQLite ([`rusvel_db::Database`]) for `StoragePort` +
//! `JobPort`, plus the minimal stub ports described in [`crate::stubs`].
//!
//! The agent is held as a `Mutex<Arc<...>>` so an eval can install its own
//! fixture map before running the engine. Default agent returns "OK" for
//! every prompt.

use std::sync::{Arc, Mutex};

use rusvel_core::ports::{
    AgentPort, ConfigPort, EventPort, JobPort, MemoryPort, SessionPort, StoragePort,
};
use rusvel_db::Database;

use crate::stubs::{InMemoryEvents, NoopConfig, NoopMemory, NoopSessions, ScriptedAgent};

pub struct EvalCtx {
    pub db: Arc<Database>,
    pub events: Arc<InMemoryEvents>,
    pub sessions: Arc<NoopSessions>,
    pub memory: Arc<NoopMemory>,
    pub config: Arc<NoopConfig>,
    agent: Mutex<Arc<dyn AgentPort>>,
}

impl EvalCtx {
    /// Build an `EvalCtx` with an in-memory SQLite database, all migrations
    /// applied, and a fresh stub-agent that replies `OK` to every prompt.
    pub fn new() -> rusvel_core::error::Result<Self> {
        let db = Arc::new(Database::in_memory()?);
        Ok(Self {
            db,
            events: Arc::new(InMemoryEvents::new()),
            sessions: Arc::new(NoopSessions),
            memory: Arc::new(NoopMemory),
            config: Arc::new(NoopConfig),
            agent: Mutex::new(Arc::new(ScriptedAgent::new())),
        })
    }

    /// Install a custom (typically pre-loaded with fixtures) agent.
    pub fn set_agent(&self, agent: Arc<dyn AgentPort>) {
        *self.agent.lock().unwrap() = agent;
    }

    /// Current agent port (cloneable Arc, safe to hand to engines).
    pub fn agent(&self) -> Arc<dyn AgentPort> {
        self.agent.lock().unwrap().clone()
    }

    pub fn storage(&self) -> Arc<dyn StoragePort> {
        self.db.clone()
    }

    pub fn jobs(&self) -> Arc<dyn JobPort> {
        self.db.clone()
    }

    pub fn events_port(&self) -> Arc<dyn EventPort> {
        self.events.clone()
    }

    pub fn session_port(&self) -> Arc<dyn SessionPort> {
        self.sessions.clone()
    }

    pub fn memory_port(&self) -> Arc<dyn MemoryPort> {
        self.memory.clone()
    }

    pub fn config_port(&self) -> Arc<dyn ConfigPort> {
        self.config.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn ctx_builds_with_real_sqlite() {
        let ctx = EvalCtx::new().expect("EvalCtx::new");
        // Trivial smoke: enqueue a job through the real JobPort.
        use rusvel_core::domain::{JobKind, NewJob};
        use rusvel_core::id::SessionId;
        let id = ctx
            .jobs()
            .enqueue(NewJob {
                kind: JobKind::Custom("evals.smoke".into()),
                payload: serde_json::json!({}),
                max_retries: 0,
                session_id: SessionId::new(),
                metadata: serde_json::json!({}),
                scheduled_at: None,
            })
            .await
            .unwrap();
        assert!(!id.to_string().is_empty());
    }
}
