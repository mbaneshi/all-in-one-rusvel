//! Growth engine — funnels, cohorts, and KPI tracking.

use std::sync::Arc;

use async_trait::async_trait;
use rusvel_core::domain::{Capability, Event, HealthStatus};
use rusvel_core::error::Result;
use rusvel_core::id::EventId;
use rusvel_core::ports::{AgentPort, EventPort, JobPort, StoragePort};

pub mod cohort;
pub mod funnel;
pub mod kpi;

pub use cohort::{Cohort, CohortId, CohortManager};
pub use funnel::{FunnelManager, FunnelStage, FunnelStageId};
pub use kpi::{KpiEntry, KpiId, KpiManager};

pub mod events {
    pub const FUNNEL_UPDATED: &str = "growth.funnel.updated";
    pub const COHORT_ANALYZED: &str = "growth.cohort.analyzed";
    pub const KPI_RECORDED: &str = "growth.kpi.recorded";
    pub const CHURN_DETECTED: &str = "growth.churn.detected";
}

pub struct GrowthEngine {
    #[allow(dead_code)]
    storage: Arc<dyn StoragePort>,
    events: Arc<dyn EventPort>,
    #[allow(dead_code)]
    agent: Arc<dyn AgentPort>,
    #[allow(dead_code)]
    jobs: Arc<dyn JobPort>,
    funnel: FunnelManager,
    cohort: CohortManager,
    kpi: KpiManager,
}

impl GrowthEngine {
    pub fn new(
        storage: Arc<dyn StoragePort>,
        events: Arc<dyn EventPort>,
        agent: Arc<dyn AgentPort>,
        jobs: Arc<dyn JobPort>,
    ) -> Self {
        let funnel = FunnelManager::new(Arc::clone(&storage));
        let cohort = CohortManager::new(Arc::clone(&storage));
        let kpi = KpiManager::new(Arc::clone(&storage));
        Self {
            storage,
            events,
            agent,
            jobs,
            funnel,
            cohort,
            kpi,
        }
    }

    pub fn funnel(&self) -> &FunnelManager {
        &self.funnel
    }
    pub fn cohort(&self) -> &CohortManager {
        &self.cohort
    }
    pub fn kpi(&self) -> &KpiManager {
        &self.kpi
    }

    pub async fn emit_event(&self, kind: &str, payload: serde_json::Value) -> Result<EventId> {
        let event = Event {
            id: EventId::new(),
            session_id: None,
            run_id: None,
            source: "growth".into(),
            kind: kind.into(),
            payload,
            created_at: chrono::Utc::now(),
            metadata: serde_json::json!({}),
        };
        self.events.emit(event).await
    }
}

#[async_trait]
impl rusvel_core::engine::Engine for GrowthEngine {
    fn kind(&self) -> &str {
        "growth"
    }
    fn name(&self) -> &'static str {
        "Growth Engine"
    }
    fn capabilities(&self) -> Vec<Capability> {
        vec![
            Capability::Custom("funnel".into()),
            Capability::Custom("cohort".into()),
            Capability::Custom("kpi".into()),
        ]
    }
    async fn initialize(&self) -> Result<()> {
        Ok(())
    }
    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
    async fn health(&self) -> Result<HealthStatus> {
        Ok(HealthStatus {
            healthy: true,
            message: None,
            metadata: serde_json::json!({}),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusvel_core::domain::*;
    use rusvel_core::id::*;
    use rusvel_core::ports::*;
    use std::sync::Mutex;

    struct StubStore {
        objects: StubObjects,
    }
    impl StubStore {
        fn new() -> Self {
            Self {
                objects: StubObjects::new(),
            }
        }
    }
    struct StubEvents;
    struct StubSessions;
    struct StubJobStore;
    struct StubMetrics;
    struct StubObjects {
        data: Mutex<Vec<(String, String, serde_json::Value)>>,
    }
    impl StubObjects {
        fn new() -> Self {
            Self {
                data: Mutex::new(Vec::new()),
            }
        }
    }

    impl StoragePort for StubStore {
        fn events(&self) -> &dyn EventStore {
            &StubEvents
        }
        fn objects(&self) -> &dyn ObjectStore {
            &self.objects
        }
        fn sessions(&self) -> &dyn SessionStore {
            &StubSessions
        }
        fn jobs(&self) -> &dyn JobStore {
            &StubJobStore
        }
        fn metrics(&self) -> &dyn MetricStore {
            &StubMetrics
        }
    }
    #[async_trait]
    impl EventStore for StubEvents {
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
    #[async_trait]
    impl ObjectStore for StubObjects {
        async fn put(&self, kind: &str, id: &str, object: serde_json::Value) -> Result<()> {
            let mut data = self.data.lock().unwrap();
            data.retain(|(k, i, _)| !(k == kind && i == id));
            data.push((kind.into(), id.into(), object));
            Ok(())
        }
        async fn get(&self, kind: &str, id: &str) -> Result<Option<serde_json::Value>> {
            let data = self.data.lock().unwrap();
            Ok(data
                .iter()
                .find(|(k, i, _)| k == kind && i == id)
                .map(|(_, _, v)| v.clone()))
        }
        async fn delete(&self, kind: &str, id: &str) -> Result<()> {
            let mut data = self.data.lock().unwrap();
            data.retain(|(k, i, _)| !(k == kind && i == id));
            Ok(())
        }
        async fn list(&self, kind: &str, _filter: ObjectFilter) -> Result<Vec<serde_json::Value>> {
            let data = self.data.lock().unwrap();
            Ok(data
                .iter()
                .filter(|(k, _, _)| k == kind)
                .map(|(_, _, v)| v.clone())
                .collect())
        }
    }
    #[async_trait]
    impl SessionStore for StubSessions {
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
    #[async_trait]
    impl MetricStore for StubMetrics {
        async fn record(&self, _: &MetricPoint) -> Result<()> {
            Ok(())
        }
        async fn query(&self, _: MetricFilter) -> Result<Vec<MetricPoint>> {
            Ok(vec![])
        }
    }
    struct StubEventPort;
    #[async_trait]
    impl EventPort for StubEventPort {
        async fn emit(&self, event: Event) -> Result<EventId> {
            Ok(event.id)
        }
        async fn get(&self, _: &EventId) -> Result<Option<Event>> {
            Ok(None)
        }
        async fn query(&self, _: EventFilter) -> Result<Vec<Event>> {
            Ok(vec![])
        }
    }
    struct StubAgentPort;
    #[async_trait]
    impl AgentPort for StubAgentPort {
        async fn create(&self, _: AgentConfig) -> Result<RunId> {
            Ok(RunId::new())
        }
        async fn run(&self, _: &RunId, _: Content) -> Result<AgentOutput> {
            Ok(AgentOutput {
                run_id: RunId::new(),
                content: Content::text("ok"),
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
    struct StubJobPort;
    #[async_trait]
    impl JobPort for StubJobPort {
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

    fn make_engine() -> GrowthEngine {
        GrowthEngine::new(
            Arc::new(StubStore::new()),
            Arc::new(StubEventPort),
            Arc::new(StubAgentPort),
            Arc::new(StubJobPort),
        )
    }

    #[tokio::test]
    async fn funnel_add_stage() {
        let engine = make_engine();
        let sid = SessionId::new();
        engine
            .funnel()
            .add_stage(sid, "Awareness".into(), 1)
            .await
            .unwrap();
        let stages = engine.funnel().list_stages(sid).await.unwrap();
        assert_eq!(stages.len(), 1);
        assert_eq!(stages[0].name, "Awareness");
    }

    #[tokio::test]
    async fn kpi_record() {
        let engine = make_engine();
        let sid = SessionId::new();
        engine
            .kpi()
            .record_kpi(sid, "MRR".into(), 5000.0, "USD".into())
            .await
            .unwrap();
        let kpis = engine.kpi().list_kpis(sid).await.unwrap();
        assert_eq!(kpis.len(), 1);
        assert!((kpis[0].value - 5000.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn health_returns_healthy() {
        use rusvel_core::engine::Engine;
        let engine = make_engine();
        assert!(engine.health().await.unwrap().healthy);
    }
}
