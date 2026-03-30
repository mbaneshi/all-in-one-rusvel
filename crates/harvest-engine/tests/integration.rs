use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use harvest_engine::source::MockSource;
use harvest_engine::{HarvestConfig, HarvestEngine, HarvestScanParams, scan_from_params};
use rusvel_core::domain::*;
use rusvel_core::engine::Engine;
use rusvel_core::error::Result;
use rusvel_core::id::*;
use rusvel_core::ports::*;

// ── In-memory test storage ────────────────────────────────────────

#[derive(Default)]
struct MemObjectStore {
    data: Mutex<HashMap<String, HashMap<String, serde_json::Value>>>,
}
#[async_trait]
impl ObjectStore for MemObjectStore {
    async fn put(&self, kind: &str, id: &str, object: serde_json::Value) -> Result<()> {
        self.data
            .lock()
            .unwrap()
            .entry(kind.into())
            .or_default()
            .insert(id.into(), object);
        Ok(())
    }
    async fn get(&self, kind: &str, id: &str) -> Result<Option<serde_json::Value>> {
        Ok(self
            .data
            .lock()
            .unwrap()
            .get(kind)
            .and_then(|m| m.get(id).cloned()))
    }
    async fn delete(&self, kind: &str, id: &str) -> Result<()> {
        if let Some(m) = self.data.lock().unwrap().get_mut(kind) {
            m.remove(id);
        }
        Ok(())
    }
    async fn list(&self, kind: &str, filter: ObjectFilter) -> Result<Vec<serde_json::Value>> {
        let data = self.data.lock().unwrap();
        let Some(m) = data.get(kind) else {
            return Ok(vec![]);
        };
        let mut out: Vec<_> = m
            .values()
            .filter(|v| {
                filter.session_id.map_or(true, |sid| {
                    v.get("session_id")
                        .and_then(|x| x.as_str())
                        .is_some_and(|s| s == sid.to_string())
                })
            })
            .cloned()
            .collect();
        if let Some(lim) = filter.limit {
            out.truncate(lim as usize);
        }
        Ok(out)
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

struct TestStorage {
    objects: MemObjectStore,
}
impl TestStorage {
    fn new() -> Self {
        Self {
            objects: MemObjectStore::default(),
        }
    }
}
impl StoragePort for TestStorage {
    fn events(&self) -> &dyn EventStore {
        &StubEventStore
    }
    fn objects(&self) -> &dyn ObjectStore {
        &self.objects
    }
    fn sessions(&self) -> &dyn SessionStore {
        &StubSessionStore
    }
    fn jobs(&self) -> &dyn JobStore {
        &StubJobStore
    }
    fn metrics(&self) -> &dyn MetricStore {
        &StubMetricStore
    }
}

struct RecordingEvents(Mutex<Vec<Event>>);
#[async_trait]
impl EventPort for RecordingEvents {
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

fn make_engine(storage: Arc<TestStorage>, events: Arc<RecordingEvents>) -> HarvestEngine {
    HarvestEngine::new(storage as Arc<dyn StoragePort>)
        .with_events(events as Arc<dyn EventPort>)
        .with_config(HarvestConfig {
            skills: vec!["rust".into(), "axum".into()],
            min_budget: Some(500.0),
        })
}

// ── Tests ─────────────────────────────────────────────────────────

#[tokio::test]
async fn scan_from_params_mock_matches_direct_scan() {
    let sid = SessionId::new();
    let params = HarvestScanParams {
        sources: vec!["mock".into()],
        query: String::new(),
        cdp_extract_js: None,
        cdp_endpoint: None,
    };
    // Separate engines so dedup/upsert from the first scan does not empty the second scan's results.
    let storage_a = Arc::new(TestStorage::new());
    let events_a = Arc::new(RecordingEvents(Mutex::new(Vec::new())));
    let engine_a = make_engine(storage_a, events_a);
    let a = scan_from_params(&engine_a, &sid, &params, None)
        .await
        .unwrap();

    let storage_b = Arc::new(TestStorage::new());
    let events_b = Arc::new(RecordingEvents(Mutex::new(Vec::new())));
    let engine_b = make_engine(storage_b, events_b);
    let b = engine_b.scan(&sid, &MockSource).await.unwrap();
    assert_eq!(a.len(), b.len());
}

#[tokio::test]
async fn scan_from_params_unknown_source_errors() {
    let storage = Arc::new(TestStorage::new());
    let events = Arc::new(RecordingEvents(Mutex::new(Vec::new())));
    let engine = make_engine(storage, events);
    let sid = SessionId::new();
    let params = HarvestScanParams {
        sources: vec!["not_a_source".into()],
        query: String::new(),
        cdp_extract_js: None,
        cdp_endpoint: None,
    };
    assert!(
        scan_from_params(&engine, &sid, &params, None)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn scan_from_params_cdp_requires_query() {
    let storage = Arc::new(TestStorage::new());
    let events = Arc::new(RecordingEvents(Mutex::new(Vec::new())));
    let engine = make_engine(storage, events);
    let sid = SessionId::new();
    let params = HarvestScanParams {
        sources: vec!["cdp".into()],
        query: String::new(),
        cdp_extract_js: None,
        cdp_endpoint: None,
    };
    assert!(
        scan_from_params(&engine, &sid, &params, None)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn scan_discovers_and_stores_opportunities() {
    let storage = Arc::new(TestStorage::new());
    let events = Arc::new(RecordingEvents(Mutex::new(Vec::new())));
    let engine = make_engine(storage.clone(), events.clone());

    let sid = SessionId::new();
    let results = engine.scan(&sid, &MockSource).await.unwrap();

    assert_eq!(results.len(), 3);
    assert_eq!(results[0].stage, OpportunityStage::Cold);
    assert!(results[0].score > 0.0);

    let kinds: Vec<String> = events
        .0
        .lock()
        .unwrap()
        .iter()
        .map(|e| e.kind.clone())
        .collect();
    assert!(kinds.iter().any(|k| k == "harvest.scan.started"));
    assert!(kinds.iter().any(|k| k == "harvest.scan.completed"));
    assert!(kinds.iter().any(|k| k == "harvest.opportunity.discovered"));
}

#[tokio::test]
async fn pipeline_stats_reflect_scanned_opportunities() {
    let storage = Arc::new(TestStorage::new());
    let events = Arc::new(RecordingEvents(Mutex::new(Vec::new())));
    let engine = make_engine(storage, events);

    let sid = SessionId::new();
    engine.scan(&sid, &MockSource).await.unwrap();

    let stats = engine.pipeline(&sid).await.unwrap();
    assert_eq!(stats.total, 3);
    assert_eq!(*stats.by_stage.get("Cold").unwrap_or(&0), 3);
}

#[tokio::test]
async fn advance_opportunity_changes_stage() {
    let storage = Arc::new(TestStorage::new());
    let events = Arc::new(RecordingEvents(Mutex::new(Vec::new())));
    let engine = make_engine(storage, events);

    let sid = SessionId::new();
    let results = engine.scan(&sid, &MockSource).await.unwrap();
    let opp_id = results[0].id.to_string();

    engine
        .advance_opportunity(&opp_id, OpportunityStage::Contacted)
        .await
        .unwrap();

    let contacted = engine
        .list_opportunities(&sid, Some(&OpportunityStage::Contacted))
        .await
        .unwrap();
    assert_eq!(contacted.len(), 1);
    assert_eq!(contacted[0].id.to_string(), opp_id);
}

#[tokio::test]
async fn record_outcome_persists_and_lists() {
    let storage = Arc::new(TestStorage::new());
    let events = Arc::new(RecordingEvents(Mutex::new(Vec::new())));
    let engine = make_engine(storage, events.clone());

    let sid = SessionId::new();
    let results = engine.scan(&sid, &MockSource).await.unwrap();
    let opp_id = results[0].id.to_string();

    let record = engine
        .record_opportunity_outcome(
            &sid,
            &opp_id,
            harvest_engine::HarvestDealOutcome::Won,
            "Great client".into(),
        )
        .await
        .unwrap();
    assert_eq!(record.notes, "Great client");

    let outcomes = engine.list_harvest_outcomes(&sid, 10).await.unwrap();
    assert_eq!(outcomes.len(), 1);

    let kinds: Vec<String> = events
        .0
        .lock()
        .unwrap()
        .iter()
        .map(|e| e.kind.clone())
        .collect();
    assert!(kinds.iter().any(|k| k == "harvest.outcome.recorded"));
}

#[tokio::test]
async fn engine_trait_health_and_capabilities() {
    let storage = Arc::new(TestStorage::new());
    let engine = HarvestEngine::new(storage as Arc<dyn StoragePort>);

    assert_eq!(engine.kind(), "harvest");
    assert_eq!(engine.name(), "Harvest Engine");
    assert_eq!(
        engine.capabilities(),
        vec![Capability::OpportunityDiscovery]
    );

    let health = engine.health().await.unwrap();
    assert!(health.healthy);
}
