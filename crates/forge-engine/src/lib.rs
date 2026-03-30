//! # Forge Engine
//!
//! The meta-engine of RUSVEL. Orchestrates AI agents and includes the
//! Mission sub-module (goals, daily planning, reviews).
//!
//! Depends **only** on [`rusvel_core`] port traits (ADR-010).

pub mod artifacts;
pub mod events;
pub mod mission;
pub mod personas;
pub mod pipeline;
pub mod safety;

use async_trait::async_trait;
use rusvel_core::domain::*;
use rusvel_core::engine::Engine;
use rusvel_core::error::Result;
use rusvel_core::id::*;
use rusvel_core::ports::*;
use std::sync::Arc;

pub use artifacts::{ARTIFACT_KIND, ArtifactRecord, list_artifacts, save_artifact};
pub use mission::{
    DailyPlan, Review, forge_route_contributions_for_manifest,
    mission_tool_contributions_for_manifest,
};
pub use personas::{PersonaManager, persona_contributions_for_manifest};
pub use pipeline::{
    FLOW_EXECUTIONS_OBJECT_KIND, PipelineOrchestrationDef, PipelineStepKind, PipelineStepRunner,
};
pub use safety::SafetyGuard;

/// The Forge Engine — agent orchestration + Mission planning.
///
/// All ports are injected via the constructor; the engine never
/// instantiates concrete adapter types.
#[allow(dead_code)]
pub struct ForgeEngine {
    pub(crate) agent: Arc<dyn AgentPort>,
    pub(crate) events: Arc<dyn EventPort>,
    pub(crate) memory: Arc<dyn MemoryPort>,
    pub(crate) storage: Arc<dyn StoragePort>,
    pub(crate) jobs: Arc<dyn JobPort>,
    pub(crate) session: Arc<dyn SessionPort>,
    pub(crate) config: Arc<dyn ConfigPort>,
    pub(crate) personas: PersonaManager,
    pub(crate) safety: SafetyGuard,
}

impl ForgeEngine {
    pub fn new(
        agent: Arc<dyn AgentPort>,
        events: Arc<dyn EventPort>,
        memory: Arc<dyn MemoryPort>,
        storage: Arc<dyn StoragePort>,
        jobs: Arc<dyn JobPort>,
        session: Arc<dyn SessionPort>,
        config: Arc<dyn ConfigPort>,
    ) -> Self {
        Self {
            agent,
            events,
            memory,
            storage,
            jobs,
            session,
            config,
            personas: PersonaManager::new(),
            safety: SafetyGuard::default(),
        }
    }

    /// List all available personas.
    pub fn list_personas(&self) -> &[AgentProfile] {
        self.personas.list()
    }

    /// Look up a persona by name.
    pub fn get_persona(&self, name: &str) -> Option<&AgentProfile> {
        self.personas.get(name)
    }

    /// Create an [`AgentConfig`] from a named persona for a given session.
    pub fn hire_persona(&self, name: &str, session_id: &SessionId) -> Result<AgentConfig> {
        let profile =
            self.personas
                .get(name)
                .ok_or_else(|| rusvel_core::error::RusvelError::NotFound {
                    kind: "persona".into(),
                    id: name.into(),
                })?;
        Ok(AgentConfig {
            profile_id: Some(profile.id),
            session_id: *session_id,
            model: Some(profile.default_model.clone()),
            tools: profile.allowed_tools.clone(),
            instructions: Some(profile.instructions.clone()),
            budget_limit: profile.budget_limit,
            max_iterations: None,
            permission_mode: Default::default(),
            metadata: serde_json::json!({}),
        })
    }
}

#[async_trait]
impl Engine for ForgeEngine {
    fn kind(&self) -> &str {
        "forge"
    }
    fn name(&self) -> &'static str {
        "Forge Engine"
    }

    fn capabilities(&self) -> Vec<Capability> {
        vec![
            Capability::Planning,
            Capability::ToolUse,
            Capability::CodeAnalysis,
            Capability::ContentCreation,
            Capability::OpportunityDiscovery,
            Capability::Outreach,
        ]
    }

    async fn initialize(&self) -> Result<()> {
        tracing::info!("Forge Engine initializing");
        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        tracing::info!("Forge Engine shutting down");
        Ok(())
    }

    async fn health(&self) -> Result<HealthStatus> {
        Ok(HealthStatus {
            healthy: true,
            message: Some("Forge Engine is operational".into()),
            metadata: serde_json::json!({}),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusvel_core::error::{Result, RusvelError};
    use std::sync::Mutex;

    // ── Mocks ──────────────────────────────────────────────────────

    struct MockAgent;
    #[async_trait]
    impl AgentPort for MockAgent {
        async fn create(&self, _: AgentConfig) -> Result<RunId> {
            Ok(RunId::new())
        }
        async fn run(&self, _: &RunId, _: Content) -> Result<AgentOutput> {
            Ok(AgentOutput {
                run_id: RunId::new(),
                content: Content::text(
                    serde_json::json!({
                        "tasks": [
                            {"title": "Review PRs", "priority": "High"},
                            {"title": "Write docs", "priority": "Medium"},
                            {"title": "Fix CI", "priority": "Low"}
                        ],
                        "focus_areas": ["code quality", "documentation"],
                        "notes": "Focus on shipping."
                    })
                    .to_string(),
                ),
                tool_calls: 0,
                usage: LlmUsage::default(),
                cost_estimate: 0.01,
                metadata: serde_json::json!({}),
            })
        }
        async fn stop(&self, _: &RunId) -> Result<()> {
            Ok(())
        }
        async fn status(&self, _: &RunId) -> Result<AgentStatus> {
            Ok(AgentStatus::Idle)
        }
    }

    struct MockEvents {
        emitted: Mutex<Vec<Event>>,
    }
    impl MockEvents {
        fn new() -> Self {
            Self {
                emitted: Mutex::new(vec![]),
            }
        }
    }
    #[async_trait]
    impl EventPort for MockEvents {
        async fn emit(&self, event: Event) -> Result<EventId> {
            let id = event.id;
            self.emitted.lock().unwrap().push(event);
            Ok(id)
        }
        async fn get(&self, _: &EventId) -> Result<Option<Event>> {
            Ok(None)
        }
        async fn query(&self, _: EventFilter) -> Result<Vec<Event>> {
            Ok(vec![])
        }
    }

    struct MockMemory;
    #[async_trait]
    impl MemoryPort for MockMemory {
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

    struct MockObjectStore {
        data: Mutex<Vec<(String, String, serde_json::Value)>>,
    }
    impl MockObjectStore {
        fn new() -> Self {
            Self {
                data: Mutex::new(vec![]),
            }
        }
    }
    #[async_trait]
    impl ObjectStore for MockObjectStore {
        async fn put(&self, kind: &str, id: &str, obj: serde_json::Value) -> Result<()> {
            let mut d = self.data.lock().unwrap();
            d.retain(|(k, i, _)| !(k == kind && i == id));
            d.push((kind.into(), id.into(), obj));
            Ok(())
        }
        async fn get(&self, kind: &str, id: &str) -> Result<Option<serde_json::Value>> {
            Ok(self
                .data
                .lock()
                .unwrap()
                .iter()
                .find(|(k, i, _)| k == kind && i == id)
                .map(|(_, _, v)| v.clone()))
        }
        async fn delete(&self, kind: &str, id: &str) -> Result<()> {
            self.data
                .lock()
                .unwrap()
                .retain(|(k, i, _)| !(k == kind && i == id));
            Ok(())
        }
        async fn list(&self, kind: &str, filter: ObjectFilter) -> Result<Vec<serde_json::Value>> {
            Ok(self
                .data
                .lock()
                .unwrap()
                .iter()
                .filter(|(k, _, v)| {
                    if k != kind {
                        return false;
                    }
                    if let Some(ref sid) = filter.session_id {
                        return v.get("session_id") == serde_json::to_value(sid).ok().as_ref();
                    }
                    true
                })
                .map(|(_, _, v)| v.clone())
                .collect())
        }
    }

    struct MockStorage {
        objects: MockObjectStore,
    }
    impl MockStorage {
        fn new() -> Self {
            Self {
                objects: MockObjectStore::new(),
            }
        }
    }
    impl StoragePort for MockStorage {
        fn events(&self) -> &dyn EventStore {
            panic!("not used in tests")
        }
        fn objects(&self) -> &dyn ObjectStore {
            &self.objects
        }
        fn sessions(&self) -> &dyn SessionStore {
            panic!("not used in tests")
        }
        fn jobs(&self) -> &dyn JobStore {
            panic!("not used in tests")
        }
        fn metrics(&self) -> &dyn MetricStore {
            panic!("not used in tests")
        }
    }

    struct MockJobs;
    #[async_trait]
    impl JobPort for MockJobs {
        async fn enqueue(&self, _: NewJob) -> Result<JobId> {
            Ok(JobId::new())
        }
        async fn dequeue(&self, _: &[JobKind]) -> Result<Option<Job>> {
            Ok(None)
        }
        async fn complete(&self, _: &JobId, _: JobResult) -> Result<()> {
            Ok(())
        }
        async fn hold_for_approval(&self, _: &JobId, _: JobResult) -> Result<()> {
            Ok(())
        }
        async fn fail(&self, _: &JobId, _: String) -> Result<()> {
            Ok(())
        }
        async fn schedule(&self, _: NewJob, _: &str) -> Result<JobId> {
            Ok(JobId::new())
        }
        async fn cancel(&self, _: &JobId) -> Result<()> {
            Ok(())
        }
        async fn approve(&self, _: &JobId) -> Result<()> {
            Ok(())
        }
        async fn list(&self, _: JobFilter) -> Result<Vec<Job>> {
            Ok(vec![])
        }
    }

    struct MockSession;
    #[async_trait]
    impl SessionPort for MockSession {
        async fn create(&self, _: Session) -> Result<SessionId> {
            Ok(SessionId::new())
        }
        async fn load(&self, id: &SessionId) -> Result<Session> {
            Err(RusvelError::NotFound {
                kind: "session".into(),
                id: id.to_string(),
            })
        }
        async fn save(&self, _: &Session) -> Result<()> {
            Ok(())
        }
        async fn list(&self) -> Result<Vec<SessionSummary>> {
            Ok(vec![])
        }
    }

    struct MockConfig;
    impl ConfigPort for MockConfig {
        fn get_value(&self, _: &str) -> Result<Option<serde_json::Value>> {
            Ok(None)
        }
        fn set_value(&self, _: &str, _: serde_json::Value) -> Result<()> {
            Ok(())
        }
    }

    fn build_engine() -> (ForgeEngine, Arc<MockEvents>, Arc<MockStorage>) {
        let events = Arc::new(MockEvents::new());
        let storage = Arc::new(MockStorage::new());
        let engine = ForgeEngine::new(
            Arc::new(MockAgent),
            events.clone(),
            Arc::new(MockMemory),
            storage.clone(),
            Arc::new(MockJobs),
            Arc::new(MockSession),
            Arc::new(MockConfig),
        );
        (engine, events, storage)
    }

    #[test]
    fn engine_metadata() {
        let (engine, _, _) = build_engine();
        assert_eq!(engine.kind(), "forge");
        assert_eq!(engine.name(), "Forge Engine");
        assert_eq!(engine.capabilities().len(), 6);
        assert!(engine.capabilities().contains(&Capability::Planning));
    }

    #[tokio::test]
    async fn engine_lifecycle() {
        let (engine, _, _) = build_engine();
        engine.initialize().await.unwrap();
        assert!(engine.health().await.unwrap().healthy);
        engine.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn set_and_list_goals() {
        let (engine, events, _) = build_engine();
        let sid = SessionId::new();
        let g1 = engine
            .set_goal(&sid, "Ship MVP".into(), "v0.1".into(), Timeframe::Month)
            .await
            .unwrap();
        let g2 = engine
            .set_goal(&sid, "Write blog".into(), "Post".into(), Timeframe::Week)
            .await
            .unwrap();
        assert_eq!(g1.status, GoalStatus::Active);
        assert_eq!(g2.timeframe, Timeframe::Week);
        assert_eq!(engine.list_goals(&sid).await.unwrap().len(), 2);
        let emitted = events.emitted.lock().unwrap();
        assert_eq!(emitted.len(), 2);
        assert!(
            emitted
                .iter()
                .all(|e| e.kind == events::MISSION_GOAL_CREATED)
        );
    }

    #[tokio::test]
    async fn mission_today_generates_plan() {
        let (engine, events, _) = build_engine();
        let sid = SessionId::new();
        engine
            .set_goal(&sid, "Ship MVP".into(), "v0.1".into(), Timeframe::Month)
            .await
            .unwrap();
        let plan = engine.mission_today(&sid).await.unwrap();
        assert_eq!(plan.tasks.len(), 3);
        assert_eq!(plan.tasks[0].title, "Review PRs");
        assert_eq!(plan.focus_areas.len(), 2);
        assert!(
            events
                .emitted
                .lock()
                .unwrap()
                .iter()
                .any(|e| e.kind == events::MISSION_PLAN_GENERATED)
        );
    }

    #[test]
    fn engine_has_default_personas() {
        let (engine, _, _) = build_engine();
        assert_eq!(engine.list_personas().len(), 10);
        assert!(engine.get_persona("CodeWriter").is_some());
        assert!(engine.get_persona("SecurityAuditor").is_some());
    }

    #[test]
    fn hire_persona_creates_agent_config() {
        let (engine, _, _) = build_engine();
        let sid = SessionId::new();
        let cfg = engine.hire_persona("Tester", &sid).unwrap();
        assert_eq!(cfg.session_id, sid);
        assert!(cfg.instructions.unwrap().contains("QA"));
        assert!(cfg.tools.contains(&"shell".to_string()));
    }

    #[test]
    fn hire_unknown_persona_fails() {
        let (engine, _, _) = build_engine();
        let sid = SessionId::new();
        assert!(engine.hire_persona("Unknown", &sid).is_err());
    }

    #[test]
    fn safety_guard_is_initialized() {
        let (engine, _, _) = build_engine();
        assert!(engine.safety.check_budget(5.0).is_ok());
        assert!(engine.safety.check_circuit().is_ok());
    }
}
