use std::sync::Arc;

use async_trait::async_trait;
use rusvel_core::department::{DepartmentApp, RegistrationContext};
use rusvel_core::domain::*;
use rusvel_core::error::Result;
use rusvel_core::id::*;
use rusvel_core::ports::*;

// ════════════════════════════════════════════════════════════════════
//  Stub ports — minimal Ok/empty implementations
// ════════════════════════════════════════════════════════════════════

struct StubAgent;

#[async_trait]
impl AgentPort for StubAgent {
    async fn create(&self, _: AgentConfig) -> Result<RunId> {
        Ok(RunId::new())
    }
    async fn run(&self, _: &RunId, _: Content) -> Result<AgentOutput> {
        Ok(AgentOutput {
            run_id: RunId::new(),
            content: Content::text("stub"),
            tool_calls: 0,
            usage: LlmUsage::default(),
            cost_estimate: 0.0,
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

struct StubEvents;

#[async_trait]
impl EventPort for StubEvents {
    async fn emit(&self, _: Event) -> Result<EventId> {
        Ok(EventId::new())
    }
    async fn get(&self, _: &EventId) -> Result<Option<Event>> {
        Ok(None)
    }
    async fn query(&self, _: EventFilter) -> Result<Vec<Event>> {
        Ok(vec![])
    }
}

struct StubObjectStore;

#[async_trait]
impl ObjectStore for StubObjectStore {
    async fn put(&self, _: &str, _: &str, _: serde_json::Value) -> Result<()> {
        Ok(())
    }
    async fn get(&self, _: &str, _: &str) -> Result<Option<serde_json::Value>> {
        Ok(None)
    }
    async fn delete(&self, _: &str, _: &str) -> Result<()> {
        Ok(())
    }
    async fn list(&self, _: &str, _: ObjectFilter) -> Result<Vec<serde_json::Value>> {
        Ok(vec![])
    }
}

struct StubEventStore;

#[async_trait]
impl EventStore for StubEventStore {
    async fn append(&self, _: &Event) -> Result<()> {
        Ok(())
    }
    async fn get(&self, _: &EventId) -> Result<Option<Event>> {
        Ok(None)
    }
    async fn query(&self, _: EventFilter) -> Result<Vec<Event>> {
        Ok(vec![])
    }
}

struct StubSessionStore;

#[async_trait]
impl SessionStore for StubSessionStore {
    async fn put_session(&self, _: &Session) -> Result<()> {
        Ok(())
    }
    async fn get_session(&self, _: &SessionId) -> Result<Option<Session>> {
        Ok(None)
    }
    async fn list_sessions(&self) -> Result<Vec<SessionSummary>> {
        Ok(vec![])
    }
    async fn put_run(&self, _: &Run) -> Result<()> {
        Ok(())
    }
    async fn get_run(&self, _: &RunId) -> Result<Option<Run>> {
        Ok(None)
    }
    async fn list_runs(&self, _: &SessionId) -> Result<Vec<Run>> {
        Ok(vec![])
    }
    async fn put_thread(&self, _: &Thread) -> Result<()> {
        Ok(())
    }
    async fn get_thread(&self, _: &ThreadId) -> Result<Option<Thread>> {
        Ok(None)
    }
    async fn list_threads(&self, _: &RunId) -> Result<Vec<Thread>> {
        Ok(vec![])
    }
}

struct StubJobStore;

#[async_trait]
impl JobStore for StubJobStore {
    async fn enqueue(&self, _: &Job) -> Result<()> {
        Ok(())
    }
    async fn dequeue(&self, _: &[JobKind]) -> Result<Option<Job>> {
        Ok(None)
    }
    async fn update(&self, _: &Job) -> Result<()> {
        Ok(())
    }
    async fn get(&self, _: &JobId) -> Result<Option<Job>> {
        Ok(None)
    }
    async fn list(&self, _: JobFilter) -> Result<Vec<Job>> {
        Ok(vec![])
    }
}

struct StubMetricStore;

#[async_trait]
impl MetricStore for StubMetricStore {
    async fn record(&self, _: &MetricPoint) -> Result<()> {
        Ok(())
    }
    async fn query(&self, _: MetricFilter) -> Result<Vec<MetricPoint>> {
        Ok(vec![])
    }
}

struct StubStorage {
    objects: StubObjectStore,
    events: StubEventStore,
    sessions: StubSessionStore,
    jobs: StubJobStore,
    metrics: StubMetricStore,
}

impl StubStorage {
    fn new() -> Self {
        Self {
            objects: StubObjectStore,
            events: StubEventStore,
            sessions: StubSessionStore,
            jobs: StubJobStore,
            metrics: StubMetricStore,
        }
    }
}

impl StoragePort for StubStorage {
    fn events(&self) -> &dyn EventStore {
        &self.events
    }
    fn objects(&self) -> &dyn ObjectStore {
        &self.objects
    }
    fn sessions(&self) -> &dyn SessionStore {
        &self.sessions
    }
    fn jobs(&self) -> &dyn JobStore {
        &self.jobs
    }
    fn metrics(&self) -> &dyn MetricStore {
        &self.metrics
    }
}

struct StubJobs;

#[async_trait]
impl JobPort for StubJobs {
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

struct StubMemory;

#[async_trait]
impl MemoryPort for StubMemory {
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

struct StubSession;

#[async_trait]
impl SessionPort for StubSession {
    async fn create(&self, s: Session) -> Result<SessionId> {
        Ok(s.id)
    }
    async fn load(&self, id: &SessionId) -> Result<Session> {
        Err(rusvel_core::error::RusvelError::NotFound {
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

struct StubConfig;

impl ConfigPort for StubConfig {
    fn get_value(&self, _: &str) -> Result<Option<serde_json::Value>> {
        Ok(None)
    }
    fn set_value(&self, _: &str, _: serde_json::Value) -> Result<()> {
        Ok(())
    }
}

struct StubAuth;

#[async_trait]
impl AuthPort for StubAuth {
    async fn store_credential(&self, _: &str, _: Credential) -> Result<()> {
        Ok(())
    }
    async fn get_credential(&self, _: &str) -> Result<Option<Credential>> {
        Ok(None)
    }
    async fn refresh(&self, _: &str) -> Result<Credential> {
        Err(rusvel_core::error::RusvelError::Unauthorized(
            "no credential".into(),
        ))
    }
    async fn delete_credential(&self, _: &str) -> Result<()> {
        Ok(())
    }
}

// ════════════════════════════════════════════════════════════════════
//  Test context builder
// ════════════════════════════════════════════════════════════════════

fn build_test_context() -> RegistrationContext {
    RegistrationContext::new(
        Arc::new(StubAgent),
        Arc::new(StubEvents),
        Arc::new(StubStorage::new()),
        Arc::new(StubJobs),
        Arc::new(StubMemory),
        Arc::new(StubSession),
        Arc::new(StubConfig),
        Arc::new(StubAuth),
        None,
        None,
    )
}

// ════════════════════════════════════════════════════════════════════
//  Contract test macro
// ════════════════════════════════════════════════════════════════════

macro_rules! test_department_contract {
    ($mod_name:ident, $dept_type:ty, $expected_id:expr, min_tools = $min_tools:expr) => {
        mod $mod_name {
            use super::*;

            #[test]
            fn manifest_purity() {
                let dept = <$dept_type>::default();
                let m1 = dept.manifest();
                let m2 = dept.manifest();
                assert_eq!(m1.id, $expected_id);
                assert_eq!(m2.id, $expected_id);
                assert_eq!(m1.id, m2.id);
                assert_eq!(m1.name, m2.name);
                assert_eq!(m1.capabilities, m2.capabilities);
                assert_eq!(m1.tools.len(), m2.tools.len());
                assert_eq!(m1.routes.len(), m2.routes.len());
                assert_eq!(m1.quick_actions.len(), m2.quick_actions.len());
            }

            #[test]
            fn manifest_has_nonempty_id_and_name() {
                let dept = <$dept_type>::default();
                let m = dept.manifest();
                assert!(!m.id.is_empty());
                assert!(!m.name.is_empty());
            }

            #[tokio::test]
            async fn register_populates_context() {
                let dept = <$dept_type>::default();
                let mut ctx = build_test_context();
                dept.register(&mut ctx)
                    .await
                    .expect("register should succeed");
                {
                    const MIN_TOOLS: usize = $min_tools;
                    if MIN_TOOLS > 0 {
                        assert!(
                            ctx.tools.len() >= MIN_TOOLS,
                            "{}: expected >= {} tools, got {}",
                            $expected_id,
                            MIN_TOOLS,
                            ctx.tools.len()
                        );
                    }
                }
            }

            #[tokio::test]
            async fn shutdown_ok() {
                let dept = <$dept_type>::default();
                dept.shutdown()
                    .await
                    .expect("shutdown before register should be ok");

                let mut ctx = build_test_context();
                dept.register(&mut ctx).await.expect("register");
                dept.shutdown()
                    .await
                    .expect("shutdown after register should be ok");
            }

            #[tokio::test]
            async fn tools_belong_to_department() {
                let dept = <$dept_type>::default();
                let mut ctx = build_test_context();
                dept.register(&mut ctx).await.expect("register");
                let tools = ctx.tools.into_tools();
                for tool in &tools {
                    assert_eq!(
                        tool.department_id, $expected_id,
                        "tool '{}' should belong to dept '{}'",
                        tool.name, $expected_id
                    );
                    assert!(
                        tool.name.starts_with($expected_id),
                        "tool '{}' should be namespaced under '{}'",
                        tool.name,
                        $expected_id
                    );
                }
            }
        }
    };
}

// ════════════════════════════════════════════════════════════════════
//  Contract tests for all 14 DepartmentApp implementations
// ════════════════════════════════════════════════════════════════════

test_department_contract!(forge, dept_forge::ForgeDepartment, "forge", min_tools = 5);
test_department_contract!(code, dept_code::CodeDepartment, "code", min_tools = 2);
test_department_contract!(
    content,
    dept_content::ContentDepartment,
    "content",
    min_tools = 2
);
test_department_contract!(
    harvest,
    dept_harvest::HarvestDepartment,
    "harvest",
    min_tools = 0
);
test_department_contract!(flow, dept_flow::FlowDepartment, "flow", min_tools = 7);
test_department_contract!(gtm, dept_gtm::GtmDepartment, "gtm", min_tools = 5);
test_department_contract!(
    finance,
    dept_finance::FinanceDepartment,
    "finance",
    min_tools = 4
);
test_department_contract!(
    product,
    dept_product::ProductDepartment,
    "product",
    min_tools = 4
);
test_department_contract!(
    growth,
    dept_growth::GrowthDepartment,
    "growth",
    min_tools = 4
);
test_department_contract!(
    distro,
    dept_distro::DistroDepartment,
    "distro",
    min_tools = 4
);
test_department_contract!(legal, dept_legal::LegalDepartment, "legal", min_tools = 4);
test_department_contract!(
    support,
    dept_support::SupportDepartment,
    "support",
    min_tools = 4
);
test_department_contract!(infra, dept_infra::InfraDepartment, "infra", min_tools = 4);
test_department_contract!(
    messaging,
    dept_messaging::MessagingDepartment,
    "messaging",
    min_tools = 0
);
