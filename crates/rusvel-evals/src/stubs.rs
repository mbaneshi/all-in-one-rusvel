//! Stub port implementations used by [`crate::EvalCtx`].
//!
//! Only `AgentPort` is properly stubbed (recorded fixture map); the other
//! traits the engines need but `rusvel_db::Database` doesn't supply
//! (EventPort, SessionPort, MemoryPort, ConfigPort) are minimal in-memory
//! adapters — enough for evals to run deterministically.

use std::sync::Mutex;

use async_trait::async_trait;
use rusvel_core::domain::*;
use rusvel_core::error::Result;
use rusvel_core::id::*;
use rusvel_core::ports::*;

// ── AgentPort: ScriptedAgent ─────────────────────────────────────────

/// A recorded-fixture [`AgentPort`]. Each call to [`AgentPort::run`]
/// matches the input text against registered patterns and returns the
/// canned [`AgentOutput`] — the equivalent of a VCR cassette for LLMs.
///
/// Matching: longest-substring wins. If nothing matches, returns the
/// fallback response (plain text "OK").
pub struct ScriptedAgent {
    /// (pattern, canned-response-text) — first inserted wins on tie.
    fixtures: Mutex<Vec<(String, String)>>,
    fallback: String,
}

impl ScriptedAgent {
    pub fn new() -> Self {
        Self {
            fixtures: Mutex::new(Vec::new()),
            fallback: "OK".to_string(),
        }
    }

    /// Register a fixture: when the agent input contains `pattern`,
    /// return `response` as plain text.
    pub fn with_fixture(self, pattern: impl Into<String>, response: impl Into<String>) -> Self {
        self.fixtures
            .lock()
            .unwrap()
            .push((pattern.into(), response.into()));
        self
    }

    pub fn with_fallback(mut self, fallback: impl Into<String>) -> Self {
        self.fallback = fallback.into();
        self
    }

    fn lookup(&self, input: &str) -> String {
        let fx = self.fixtures.lock().unwrap();
        let mut best: Option<(usize, &str)> = None;
        for (pat, resp) in fx.iter() {
            if input.contains(pat.as_str()) {
                match best {
                    Some((len, _)) if pat.len() <= len => {}
                    _ => best = Some((pat.len(), resp.as_str())),
                }
            }
        }
        best.map(|(_, r)| r.to_string())
            .unwrap_or_else(|| self.fallback.clone())
    }
}

impl Default for ScriptedAgent {
    fn default() -> Self {
        Self::new()
    }
}

fn input_text(c: &Content) -> String {
    c.parts
        .iter()
        .filter_map(|p| match p {
            Part::Text(t) => Some(t.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[async_trait]
impl AgentPort for ScriptedAgent {
    async fn create(&self, _: AgentConfig) -> Result<RunId> {
        Ok(RunId::new())
    }

    async fn run(&self, _run_id: &RunId, input: Content) -> Result<AgentOutput> {
        let text = input_text(&input);
        let response = self.lookup(&text);
        Ok(AgentOutput {
            run_id: RunId::new(),
            content: Content::text(response),
            tool_calls: 0,
            usage: LlmUsage::default(),
            cost_estimate: 0.0,
            metadata: serde_json::json!({"source": "rusvel-evals scripted"}),
        })
    }

    async fn stop(&self, _: &RunId) -> Result<()> {
        Ok(())
    }

    async fn status(&self, _: &RunId) -> Result<AgentStatus> {
        Ok(AgentStatus::Idle)
    }
}

// ── EventPort: InMemoryEvents ────────────────────────────────────────

pub struct InMemoryEvents {
    pub emitted: Mutex<Vec<Event>>,
}

impl InMemoryEvents {
    pub fn new() -> Self {
        Self {
            emitted: Mutex::new(Vec::new()),
        }
    }

    pub fn kinds(&self) -> Vec<String> {
        self.emitted
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.kind.clone())
            .collect()
    }
}

impl Default for InMemoryEvents {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EventPort for InMemoryEvents {
    async fn emit(&self, event: Event) -> Result<EventId> {
        let id = event.id;
        self.emitted.lock().unwrap().push(event);
        Ok(id)
    }

    async fn get(&self, id: &EventId) -> Result<Option<Event>> {
        Ok(self
            .emitted
            .lock()
            .unwrap()
            .iter()
            .find(|e| e.id == *id)
            .cloned())
    }

    async fn query(&self, filter: EventFilter) -> Result<Vec<Event>> {
        let g = self.emitted.lock().unwrap();
        Ok(g.iter()
            .filter(|e| {
                if let Some(sid) = filter.session_id {
                    if e.session_id != Some(sid) {
                        return false;
                    }
                }
                if let Some(since) = filter.since {
                    if e.created_at < since {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect())
    }
}

// ── SessionPort: NoopSessions ────────────────────────────────────────

pub struct NoopSessions;

#[async_trait]
impl SessionPort for NoopSessions {
    async fn create(&self, _: Session) -> Result<SessionId> {
        Ok(SessionId::new())
    }
    async fn load(&self, _: &SessionId) -> Result<Session> {
        Err(rusvel_core::error::RusvelError::NotFound {
            kind: "session".into(),
            id: "noop".into(),
        })
    }
    async fn save(&self, _: &Session) -> Result<()> {
        Ok(())
    }
    async fn list(&self) -> Result<Vec<SessionSummary>> {
        Ok(vec![])
    }
}

// ── MemoryPort: NoopMemory ───────────────────────────────────────────

pub struct NoopMemory;

#[async_trait]
impl MemoryPort for NoopMemory {
    async fn store(&self, _: MemoryEntry) -> Result<uuid::Uuid> {
        Ok(uuid::Uuid::now_v7())
    }
    async fn recall(&self, _: &uuid::Uuid) -> Result<Option<MemoryEntry>> {
        Ok(None)
    }
    async fn search(&self, _: &SessionId, _: &str, _: usize) -> Result<Vec<MemoryEntry>> {
        Ok(vec![])
    }
    async fn forget(&self, _: &uuid::Uuid) -> Result<()> {
        Ok(())
    }
}

// ── ConfigPort: NoopConfig ───────────────────────────────────────────

pub struct NoopConfig;

impl ConfigPort for NoopConfig {
    fn get_value(&self, _: &str) -> Result<Option<serde_json::Value>> {
        Ok(None)
    }
    fn set_value(&self, _: &str, _: serde_json::Value) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn scripted_agent_matches_longest_pattern() {
        let agent = ScriptedAgent::new()
            .with_fixture("daily plan", r#"{"tasks":[]}"#)
            .with_fixture(
                "daily plan with goals",
                r#"{"tasks":[{"title":"A","priority":"High"}]}"#,
            );
        let out = agent
            .run(
                &RunId::new(),
                Content::text("Generate a daily plan with goals"),
            )
            .await
            .unwrap();
        let text = input_text(&out.content);
        assert!(
            text.contains("\"A\""),
            "expected longest-match response, got: {text}"
        );
    }

    #[tokio::test]
    async fn scripted_agent_falls_back() {
        let agent = ScriptedAgent::new().with_fallback("DEFAULT");
        let out = agent
            .run(&RunId::new(), Content::text("nothing matches"))
            .await
            .unwrap();
        assert_eq!(input_text(&out.content), "DEFAULT");
    }
}
