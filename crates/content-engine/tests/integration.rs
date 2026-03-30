use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use content_engine::*;
use rusvel_core::domain::*;
use rusvel_core::engine::Engine;
use rusvel_core::error::Result;
use rusvel_core::id::*;
use rusvel_core::ports::*;

// ── In-memory test doubles ────────────────────────────────────────

struct MemObjects(Mutex<HashMap<String, serde_json::Value>>);
impl MemObjects {
    fn new() -> Self {
        Self(Mutex::new(HashMap::new()))
    }
}
#[async_trait]
impl ObjectStore for MemObjects {
    async fn put(&self, kind: &str, id: &str, obj: serde_json::Value) -> Result<()> {
        self.0.lock().unwrap().insert(format!("{kind}:{id}"), obj);
        Ok(())
    }
    async fn get(&self, kind: &str, id: &str) -> Result<Option<serde_json::Value>> {
        Ok(self.0.lock().unwrap().get(&format!("{kind}:{id}")).cloned())
    }
    async fn delete(&self, kind: &str, id: &str) -> Result<()> {
        self.0.lock().unwrap().remove(&format!("{kind}:{id}"));
        Ok(())
    }
    async fn list(&self, kind: &str, filter: ObjectFilter) -> Result<Vec<serde_json::Value>> {
        let map = self.0.lock().unwrap();
        let prefix = format!("{kind}:");
        let mut items: Vec<_> = map
            .iter()
            .filter(|(k, _)| k.starts_with(&prefix))
            .map(|(_, v)| v.clone())
            .collect();
        if let Some(sid) = &filter.session_id {
            let sid_str = sid.to_string();
            items.retain(|v| {
                v.get("session_id")
                    .and_then(|s| s.as_str())
                    .is_some_and(|s| s == sid_str)
            });
        }
        Ok(items)
    }
}

struct NullEvents;
#[async_trait]
impl EventStore for NullEvents {
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
struct NullSessions;
#[async_trait]
impl SessionStore for NullSessions {
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
struct NullJobs;
#[async_trait]
impl JobStore for NullJobs {
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
struct NullMetrics;
#[async_trait]
impl MetricStore for NullMetrics {
    async fn record(&self, _: &MetricPoint) -> Result<()> {
        Ok(())
    }
    async fn query(&self, _: MetricFilter) -> Result<Vec<MetricPoint>> {
        Ok(vec![])
    }
}

struct TestStorage {
    objects: MemObjects,
}
impl TestStorage {
    fn new() -> Self {
        Self {
            objects: MemObjects::new(),
        }
    }
}
impl StoragePort for TestStorage {
    fn events(&self) -> &dyn EventStore {
        &NullEvents
    }
    fn objects(&self) -> &dyn ObjectStore {
        &self.objects
    }
    fn sessions(&self) -> &dyn SessionStore {
        &NullSessions
    }
    fn jobs(&self) -> &dyn JobStore {
        &NullJobs
    }
    fn metrics(&self) -> &dyn MetricStore {
        &NullMetrics
    }
}

struct RecordingEventBus(Mutex<Vec<Event>>);
impl RecordingEventBus {
    fn new() -> Self {
        Self(Mutex::new(Vec::new()))
    }
    fn kinds(&self) -> Vec<String> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.kind.clone())
            .collect()
    }
}
#[async_trait]
impl EventPort for RecordingEventBus {
    async fn emit(&self, event: Event) -> Result<EventId> {
        let id = event.id;
        self.0.lock().unwrap().push(event);
        Ok(id)
    }
    async fn get(&self, _: &EventId) -> Result<Option<Event>> {
        Ok(None)
    }
    async fn query(&self, _: EventFilter) -> Result<Vec<Event>> {
        Ok(vec![])
    }
}

struct FakeAgent;
#[async_trait]
impl AgentPort for FakeAgent {
    async fn create(&self, _: AgentConfig) -> Result<RunId> {
        Ok(RunId::new())
    }
    async fn run(&self, _: &RunId, input: Content) -> Result<AgentOutput> {
        let prompt = input
            .parts
            .iter()
            .find_map(|p| match p {
                Part::Text(t) => Some(t.clone()),
                _ => None,
            })
            .unwrap_or_default();
        Ok(AgentOutput {
            run_id: RunId::new(),
            content: Content::text(format!("# Title\n\nAbout: {prompt}")),
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
        Ok(AgentStatus::Completed)
    }
}

struct FakeJobPort;
#[async_trait]
impl JobPort for FakeJobPort {
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

fn make_engine() -> (ContentEngine, Arc<RecordingEventBus>, Arc<TestStorage>) {
    let storage = Arc::new(TestStorage::new());
    let events = Arc::new(RecordingEventBus::new());
    let engine = ContentEngine::new(
        Arc::clone(&storage) as Arc<dyn StoragePort>,
        Arc::clone(&events) as Arc<dyn EventPort>,
        Arc::new(FakeAgent) as Arc<dyn AgentPort>,
        Arc::new(FakeJobPort) as Arc<dyn JobPort>,
    );
    (engine, events, storage)
}

// ── Tests ─────────────────────────────────────────────────────────

#[tokio::test]
async fn draft_and_list_content_roundtrip() {
    let (engine, events, _) = make_engine();
    let sid = SessionId::new();

    let item = engine
        .draft(&sid, "Rust async", ContentKind::Blog)
        .await
        .unwrap();
    assert_eq!(item.status, ContentStatus::Draft);
    assert_eq!(item.kind, ContentKind::Blog);
    assert!(!item.body_markdown.is_empty());

    let all = engine.list_content(&sid, None).await.unwrap();
    assert_eq!(all.len(), 1);

    assert!(events.kinds().contains(&"content.drafted".to_string()));
}

#[tokio::test]
async fn adapt_creates_platform_variant() {
    let (engine, events, _) = make_engine();
    let sid = SessionId::new();

    let original = engine
        .draft(&sid, "Testing", ContentKind::LongForm)
        .await
        .unwrap();
    let adapted = engine
        .adapt(&sid, original.id, Platform::Twitter)
        .await
        .unwrap();

    assert_ne!(adapted.id, original.id);
    assert_eq!(adapted.status, ContentStatus::Adapted);
    assert_eq!(adapted.platform_targets, vec![Platform::Twitter]);
    assert!(events.kinds().contains(&"content.adapted".to_string()));
}

#[tokio::test]
async fn publish_requires_approval() {
    let (engine, _, _) = make_engine();
    let mock = Arc::new(MockPlatformAdapter::new(Platform::DevTo));
    engine.register_platform(mock);

    let sid = SessionId::new();
    let item = engine
        .draft(&sid, "Blocked", ContentKind::Blog)
        .await
        .unwrap();

    let err = engine.publish(&sid, item.id, Platform::DevTo).await;
    assert!(err.is_err());
    assert!(err.unwrap_err().to_string().contains("approved"));
}

#[tokio::test]
async fn approve_then_publish_succeeds() {
    let (engine, events, storage) = make_engine();
    let mock = Arc::new(MockPlatformAdapter::new(Platform::DevTo));
    engine.register_platform(mock.clone());

    let sid = SessionId::new();
    let mut item = engine
        .draft(&sid, "Approved", ContentKind::Blog)
        .await
        .unwrap();
    item.approval = ApprovalStatus::Approved;
    storage
        .objects()
        .put(
            "content",
            &item.id.to_string(),
            serde_json::to_value(&item).unwrap(),
        )
        .await
        .unwrap();

    let result = engine
        .publish(&sid, item.id, Platform::DevTo)
        .await
        .unwrap();
    assert!(!result.post_id.is_empty());
    assert!(events.kinds().contains(&"content.published".to_string()));
    assert_eq!(mock.published_items().len(), 1);
}

#[tokio::test]
async fn engine_trait_health_and_capabilities() {
    let (engine, _, _) = make_engine();
    assert_eq!(engine.kind(), "content");
    assert_eq!(engine.name(), "Content Engine");
    assert_eq!(engine.capabilities(), vec![Capability::ContentCreation]);

    let health = engine.health().await.unwrap();
    assert!(health.healthy);
}
