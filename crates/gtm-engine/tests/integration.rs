use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Utc;
use gtm_engine::*;
use rusvel_core::domain::*;
use rusvel_core::engine::Engine;
use rusvel_core::error::Result;
use rusvel_core::id::*;
use rusvel_core::ports::*;

// ── Stub stores ───────────────────────────────────────────────────

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
#[async_trait]
impl ObjectStore for StubObjects {
    async fn put(&self, kind: &str, id: &str, object: serde_json::Value) -> Result<()> {
        let mut data = self.data.lock().unwrap();
        data.retain(|(k, i, _)| !(k == kind && i == id));
        data.push((kind.into(), id.into(), object));
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
    async fn list(&self, kind: &str, _: ObjectFilter) -> Result<Vec<serde_json::Value>> {
        Ok(self
            .data
            .lock()
            .unwrap()
            .iter()
            .filter(|(k, _, _)| k == kind)
            .map(|(_, _, v)| v.clone())
            .collect())
    }
}

struct NullEventStore;
#[async_trait]
impl EventStore for NullEventStore {
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
struct NullSessionStore;
#[async_trait]
impl SessionStore for NullSessionStore {
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
struct NullJobStore;
#[async_trait]
impl JobStore for NullJobStore {
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
struct NullMetricStore;
#[async_trait]
impl MetricStore for NullMetricStore {
    async fn record(&self, _: &MetricPoint) -> Result<()> {
        Ok(())
    }
    async fn query(&self, _: MetricFilter) -> Result<Vec<MetricPoint>> {
        Ok(vec![])
    }
}

struct StubStorage {
    objects: StubObjects,
}
impl StubStorage {
    fn new() -> Self {
        Self {
            objects: StubObjects::new(),
        }
    }
}
impl StoragePort for StubStorage {
    fn events(&self) -> &dyn EventStore {
        &NullEventStore
    }
    fn objects(&self) -> &dyn ObjectStore {
        &self.objects
    }
    fn sessions(&self) -> &dyn SessionStore {
        &NullSessionStore
    }
    fn jobs(&self) -> &dyn JobStore {
        &NullJobStore
    }
    fn metrics(&self) -> &dyn MetricStore {
        &NullMetricStore
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
            content: Content::text("Hello!"),
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

fn make_engine() -> GtmEngine {
    GtmEngine::new(
        Arc::new(StubStorage::new()),
        Arc::new(StubEventPort),
        Arc::new(StubAgentPort),
        Arc::new(StubJobPort),
    )
}

fn make_contact(sid: SessionId) -> Contact {
    Contact {
        id: ContactId::new(),
        session_id: sid,
        name: "Bob Jones".into(),
        emails: vec!["bob@example.com".into()],
        links: vec![],
        company: Some("Widgets Inc".into()),
        role: Some("CEO".into()),
        tags: vec!["lead".into()],
        last_contacted_at: None,
        metadata: serde_json::json!({}),
    }
}

// ── Tests ─────────────────────────────────────────────────────────

#[tokio::test]
async fn crm_contact_add_get_list_roundtrip() {
    let engine = make_engine();
    let sid = SessionId::new();
    let contact = make_contact(sid);

    let id = engine
        .crm()
        .add_contact(sid, contact.clone())
        .await
        .unwrap();
    let fetched = engine.crm().get_contact(&id).await.unwrap();
    assert_eq!(fetched.name, "Bob Jones");
    assert_eq!(fetched.company, Some("Widgets Inc".into()));

    let all = engine.crm().list_contacts(sid).await.unwrap();
    assert_eq!(all.len(), 1);
}

#[tokio::test]
async fn deal_lifecycle_lead_to_closed() {
    let engine = make_engine();
    let sid = SessionId::new();

    let deal = Deal {
        id: DealId::new(),
        session_id: sid,
        contact_id: ContactId::new(),
        title: "Platform license".into(),
        value: 25_000.0,
        stage: DealStage::Lead,
        notes: String::new(),
        created_at: Utc::now(),
        metadata: serde_json::json!({}),
    };
    let deal_id = engine.crm().add_deal(sid, deal).await.unwrap();

    engine
        .crm()
        .advance_deal(&deal_id, DealStage::Qualified)
        .await
        .unwrap();
    engine
        .crm()
        .advance_deal(&deal_id, DealStage::Won)
        .await
        .unwrap();

    let won = engine
        .crm()
        .list_deals(sid, Some(DealStage::Won))
        .await
        .unwrap();
    assert_eq!(won.len(), 1);
    assert_eq!(won[0].stage, DealStage::Won);
}

#[tokio::test]
async fn invoice_create_pay_revenue() {
    let engine = make_engine();
    let sid = SessionId::new();

    let items = vec![LineItem {
        description: "Dev work".into(),
        quantity: 5.0,
        unit_price: 200.0,
    }];
    let inv_id = engine
        .invoices()
        .create_invoice(sid, ContactId::new(), items, Utc::now())
        .await
        .unwrap();

    let rev_before = engine.invoices().total_revenue(sid).await.unwrap();
    assert!((rev_before - 0.0).abs() < f64::EPSILON);

    engine.invoices().mark_paid(&inv_id).await.unwrap();
    let rev_after = engine.invoices().total_revenue(sid).await.unwrap();
    assert!((rev_after - 1000.0).abs() < f64::EPSILON);
}

#[tokio::test]
async fn outreach_sequence_create_activate_execute() {
    let engine = make_engine();
    let sid = SessionId::new();
    let contact = make_contact(sid);
    let cid = engine.crm().add_contact(sid, contact).await.unwrap();

    let steps = vec![
        SequenceStep {
            delay_days: 0,
            channel: "email".into(),
            template: "intro".into(),
        },
        SequenceStep {
            delay_days: 3,
            channel: "email".into(),
            template: "follow".into(),
        },
    ];
    let seq_id = engine
        .outreach()
        .create_sequence(sid, "Drip".into(), steps)
        .await
        .unwrap();

    let seqs = engine.outreach().list_sequences(sid).await.unwrap();
    assert_eq!(seqs.len(), 1);
    assert_eq!(seqs[0].name, "Drip");

    let err = engine.outreach().execute_sequence(sid, seq_id, cid).await;
    assert!(err.is_err());

    engine.outreach().activate_sequence(&seq_id).await.unwrap();
    let job_id = engine
        .outreach()
        .execute_sequence(sid, seq_id, cid)
        .await
        .unwrap();
    assert!(!job_id.to_string().is_empty());
}

#[tokio::test]
async fn engine_trait_health_and_capabilities() {
    let engine = make_engine();
    assert_eq!(engine.kind(), "gtm");
    assert_eq!(engine.name(), "GoToMarket Engine");

    let caps = engine.capabilities();
    assert!(caps.contains(&Capability::Outreach));

    let health = engine.health().await.unwrap();
    assert!(health.healthy);
}
