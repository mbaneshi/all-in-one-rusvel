//! Tests for the content engine.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Utc;
use rusvel_core::error::Result;
use rusvel_core::id::JobId;
use rusvel_core::ports::*;

use crate::*;

// ════════════════════════════════════════════════════════════════════
//  Null / in-memory test doubles
// ════════════════════════════════════════════════════════════════════

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
        // Apply session_id filter if present.
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

/// Event bus that records emitted events for assertions.
struct RecordingEventBus {
    events: Mutex<Vec<Event>>,
}
impl RecordingEventBus {
    fn new() -> Self {
        Self {
            events: Mutex::new(Vec::new()),
        }
    }
    fn emitted_kinds(&self) -> Vec<String> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.kind.clone())
            .collect()
    }

    fn last_payload_of_kind(&self, kind: &str) -> Option<serde_json::Value> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|e| e.kind == kind)
            .map(|e| e.payload.clone())
    }
}
#[async_trait]
impl EventPort for RecordingEventBus {
    async fn emit(&self, event: Event) -> Result<EventId> {
        let id = event.id;
        self.events.lock().unwrap().push(event);
        Ok(id)
    }
    async fn get(&self, _: &EventId) -> Result<Option<Event>> {
        Ok(None)
    }
    async fn query(&self, _: EventFilter) -> Result<Vec<Event>> {
        Ok(vec![])
    }
}

/// Fake agent that returns canned content.
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
        let body = format!("# Generated Title\n\nContent about: {prompt}");
        Ok(AgentOutput {
            run_id: RunId::new(),
            content: Content::text(body),
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

/// Fake job port that just records enqueued jobs.
struct FakeJobPort {
    jobs: Mutex<Vec<NewJob>>,
}
impl FakeJobPort {
    fn new() -> Self {
        Self {
            jobs: Mutex::new(Vec::new()),
        }
    }
}
#[async_trait]
impl JobPort for FakeJobPort {
    async fn enqueue(&self, job: NewJob) -> Result<JobId> {
        self.jobs.lock().unwrap().push(job);
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

/// Build a fully-wired engine for testing.
fn test_engine() -> (
    ContentEngine,
    Arc<RecordingEventBus>,
    Arc<TestStorage>,
    Arc<FakeJobPort>,
) {
    let storage = Arc::new(TestStorage::new());
    let events = Arc::new(RecordingEventBus::new());
    let agent: Arc<dyn AgentPort> = Arc::new(FakeAgent);
    let jobs = Arc::new(FakeJobPort::new());
    let engine = ContentEngine::new(
        Arc::clone(&storage) as Arc<dyn StoragePort>,
        Arc::clone(&events) as Arc<dyn EventPort>,
        agent,
        Arc::clone(&jobs) as Arc<dyn JobPort>,
    );
    (engine, events, storage, jobs)
}

// ════════════════════════════════════════════════════════════════════
//  Test cases
// ════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn health_returns_healthy() {
    let (engine, _, _, _) = test_engine();
    let status = engine.health().await.unwrap();
    assert!(status.healthy);
}

#[tokio::test]
async fn draft_creates_content_in_draft_status() {
    let (engine, events, _, _) = test_engine();
    let sid = SessionId::new();
    let item = engine
        .draft(&sid, "Rust async patterns", ContentKind::Blog)
        .await
        .unwrap();
    assert_eq!(item.status, ContentStatus::Draft);
    assert_eq!(item.kind, ContentKind::Blog);
    assert!(!item.body_markdown.is_empty());
    assert!(
        events
            .emitted_kinds()
            .contains(&events::CONTENT_DRAFTED.to_string())
    );
}

#[tokio::test]
async fn adapt_generates_platform_variant() {
    let (engine, events, _, _) = test_engine();
    let sid = SessionId::new();
    let original = engine
        .draft(&sid, "Testing in Rust", ContentKind::LongForm)
        .await
        .unwrap();
    let adapted = engine
        .adapt(&sid, original.id, Platform::Twitter)
        .await
        .unwrap();
    assert_eq!(adapted.status, ContentStatus::Adapted);
    assert_eq!(adapted.platform_targets, vec![Platform::Twitter]);
    assert_ne!(adapted.id, original.id);
    assert!(
        events
            .emitted_kinds()
            .contains(&events::CONTENT_ADAPTED.to_string())
    );
}

#[tokio::test]
async fn publish_with_mock_adapter_stores_result() {
    let (engine, events, storage, _) = test_engine();
    let mock = Arc::new(MockPlatformAdapter::new(Platform::DevTo));
    engine.register_platform(mock.clone());

    let sid = SessionId::new();
    let mut item = engine
        .draft(&sid, "Publishing test", ContentKind::Blog)
        .await
        .unwrap();

    // Approve it first.
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
    assert!(result.url.starts_with("https://mock.test/posts/"));
    assert!(
        events
            .emitted_kinds()
            .contains(&events::CONTENT_PUBLISHED.to_string())
    );

    // Verify mock recorded the publish.
    assert_eq!(mock.published_items().len(), 1);
}

#[tokio::test]
async fn publish_without_approval_fails() {
    let (engine, _, _, _) = test_engine();
    let mock = Arc::new(MockPlatformAdapter::new(Platform::DevTo));
    engine.register_platform(mock);

    let sid = SessionId::new();
    let item = engine
        .draft(&sid, "Unapproved post", ContentKind::Blog)
        .await
        .unwrap();
    let err = engine.publish(&sid, item.id, Platform::DevTo).await;
    assert!(err.is_err());
}

#[tokio::test]
async fn calendar_schedules_and_lists_posts() {
    let (engine, _, storage, _) = test_engine();
    let sid = SessionId::new();
    let item = engine
        .draft(&sid, "Scheduled post", ContentKind::Blog)
        .await
        .unwrap();

    let publish_at = Utc::now() + chrono::Duration::hours(24);
    engine
        .schedule(&sid, item.id, Platform::LinkedIn, publish_at)
        .await
        .unwrap();

    // Reload and verify status changed.
    let json = storage
        .objects()
        .get("content", &item.id.to_string())
        .await
        .unwrap()
        .unwrap();
    let updated: ContentItem = serde_json::from_value(json).unwrap();
    assert_eq!(updated.status, ContentStatus::Scheduled);
}

#[tokio::test]
async fn schedule_emits_content_scheduled_with_platform_and_publish_at() {
    let (engine, events, _, _) = test_engine();
    let sid = SessionId::new();
    let item = engine
        .draft(&sid, "Event payload", ContentKind::Blog)
        .await
        .unwrap();
    let publish_at = Utc::now() + chrono::Duration::hours(12);
    engine
        .schedule(&sid, item.id, Platform::LinkedIn, publish_at)
        .await
        .unwrap();

    let payload = events
        .last_payload_of_kind(crate::events::CONTENT_SCHEDULED)
        .expect("content.scheduled event");
    let id_str = item.id.to_string();
    assert_eq!(
        payload["content_id"].as_str(),
        Some(id_str.as_str()),
        "{payload:?}"
    );
    assert!(payload.get("platform").is_some());
    assert!(payload.get("publish_at").is_some());
}

#[tokio::test]
async fn draft_blog_from_code_snapshot_uses_stored_analysis() {
    let (engine, events, storage, _) = test_engine();
    let sid = SessionId::new();
    let snapshot_id = uuid::Uuid::now_v7().to_string();
    let analysis_json = serde_json::json!({
        "snapshot": {
            "id": snapshot_id,
            "repo": { "local_path": "/proj", "remote_url": null },
            "analyzed_at": "2025-01-01T00:00:00Z"
        },
        "symbols": [],
        "metrics": { "total_files": 1, "total_symbols": 2, "largest_function": null },
        "graph": { "nodes": [], "edges": [] }
    });
    storage
        .objects()
        .put("code_analysis", &snapshot_id, analysis_json)
        .await
        .unwrap();

    let item = engine
        .draft_blog_from_code_snapshot(&sid, &snapshot_id)
        .await
        .unwrap();
    assert_eq!(item.kind, ContentKind::Blog);
    assert!(
        events
            .emitted_kinds()
            .contains(&events::CONTENT_DRAFTED.to_string())
    );
}

#[tokio::test]
async fn execute_content_publish_job_roundtrip() {
    let (engine, _, storage, _) = test_engine();
    let mock = Arc::new(MockPlatformAdapter::new(Platform::DevTo));
    engine.register_platform(mock.clone());

    let sid = SessionId::new();
    let mut item = engine
        .draft(&sid, "Job publish", ContentKind::Blog)
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

    let job = Job {
        id: JobId::new(),
        session_id: sid,
        kind: JobKind::ContentPublish,
        payload: serde_json::json!({
            "content_id": item.id.to_string(),
            "platform": "DevTo"
        }),
        status: JobStatus::Queued,
        scheduled_at: None,
        started_at: None,
        completed_at: None,
        retries: 0,
        max_retries: 0,
        error: None,
        metadata: serde_json::json!({}),
    };

    let out = engine.execute_content_publish_job(job).await.unwrap();
    assert!(out.get("url").is_some());
    assert_eq!(mock.published_items().len(), 1);
}

#[tokio::test]
async fn list_content_filters_by_status() {
    let (engine, _, _, _) = test_engine();
    let sid = SessionId::new();

    engine
        .draft(&sid, "Draft 1", ContentKind::Blog)
        .await
        .unwrap();
    engine
        .draft(&sid, "Draft 2", ContentKind::Tweet)
        .await
        .unwrap();

    let all = engine.list_content(&sid, None).await.unwrap();
    assert_eq!(all.len(), 2);

    let drafts = engine
        .list_content(&sid, Some(ContentStatus::Draft))
        .await
        .unwrap();
    assert_eq!(drafts.len(), 2);

    let published = engine
        .list_content(&sid, Some(ContentStatus::Published))
        .await
        .unwrap();
    assert_eq!(published.len(), 0);
}
