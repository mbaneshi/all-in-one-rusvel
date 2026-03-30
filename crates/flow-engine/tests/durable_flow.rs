//! Integration tests for durable execution (checkpoints, resume, retry).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use flow_engine::FlowEngine;
use rusvel_core::domain::*;
use rusvel_core::error::Result;
use rusvel_core::id::*;
use rusvel_core::ports::*;

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
    async fn list(&self, kind: &str, _filter: ObjectFilter) -> Result<Vec<serde_json::Value>> {
        let map = self.0.lock().unwrap();
        let prefix = format!("{kind}:");
        Ok(map
            .iter()
            .filter(|(k, _)| k.starts_with(&prefix))
            .map(|(_, v)| v.clone())
            .collect())
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

struct StubAgent;
#[async_trait]
impl AgentPort for StubAgent {
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

fn engine() -> FlowEngine {
    FlowEngine::new(
        Arc::new(TestStorage::new()),
        Arc::new(StubEventPort),
        Arc::new(StubAgent),
        None,
        None,
    )
}

fn linear_two_code_flow() -> FlowDef {
    let n1 = FlowNodeId::new();
    let n2 = FlowNodeId::new();
    FlowDef {
        id: FlowId::new(),
        name: "linear".into(),
        description: "a→b".into(),
        nodes: vec![
            FlowNodeDef {
                id: n1,
                node_type: "code".into(),
                name: "a".into(),
                parameters: serde_json::json!({"value": 1}),
                position: (0.0, 0.0),
                on_error: FlowErrorBehavior::StopFlow,
                metadata: serde_json::json!({}),
            },
            FlowNodeDef {
                id: n2,
                node_type: "code".into(),
                name: "b".into(),
                parameters: serde_json::json!({"value": 2}),
                position: (1.0, 0.0),
                on_error: FlowErrorBehavior::StopFlow,
                metadata: serde_json::json!({}),
            },
        ],
        connections: vec![FlowConnectionDef {
            source_node: n1,
            source_output: "main".into(),
            target_node: n2,
            target_input: "main".into(),
            metadata: Default::default(),
        }],
        variables: Default::default(),
        metadata: serde_json::json!({}),
    }
}

/// Second node uses browser_action without BrowserPort → fails after first code node succeeds.
fn flow_code_then_browser_fail() -> (FlowDef, FlowNodeId, FlowNodeId) {
    let n1 = FlowNodeId::new();
    let n2 = FlowNodeId::new();
    let flow = FlowDef {
        id: FlowId::new(),
        name: "fail-at-browser".into(),
        description: String::new(),
        nodes: vec![
            FlowNodeDef {
                id: n1,
                node_type: "code".into(),
                name: "ok".into(),
                parameters: serde_json::json!({"value": "done"}),
                position: (0.0, 0.0),
                on_error: FlowErrorBehavior::StopFlow,
                metadata: serde_json::json!({}),
            },
            FlowNodeDef {
                id: n2,
                node_type: "browser_action".into(),
                name: "needs-browser".into(),
                parameters: serde_json::json!({}),
                position: (1.0, 0.0),
                on_error: FlowErrorBehavior::StopFlow,
                metadata: serde_json::json!({}),
            },
        ],
        connections: vec![FlowConnectionDef {
            source_node: n1,
            source_output: "main".into(),
            target_node: n2,
            target_input: "main".into(),
            metadata: Default::default(),
        }],
        variables: Default::default(),
        metadata: serde_json::json!({}),
    };
    (flow, n1, n2)
}

#[tokio::test]
async fn successful_run_removes_checkpoint() {
    let engine = engine();
    let flow = linear_two_code_flow();
    engine.save_flow(&flow).await.unwrap();

    let exec = engine
        .run_flow(&flow.id, serde_json::json!({}))
        .await
        .unwrap();
    assert_eq!(exec.status, FlowExecutionStatus::Succeeded);

    let ck = engine.get_checkpoint(&exec.id.to_string()).await.unwrap();
    assert!(ck.is_none());
}

#[tokio::test]
async fn failed_run_persists_checkpoint_with_completed_and_failed_nodes() {
    let engine = engine();
    let (flow, n1, n2) = flow_code_then_browser_fail();
    engine.save_flow(&flow).await.unwrap();

    let exec = engine
        .run_flow(&flow.id, serde_json::json!({}))
        .await
        .unwrap();
    assert_eq!(exec.status, FlowExecutionStatus::Failed);

    let ck = engine
        .get_checkpoint(&exec.id.to_string())
        .await
        .unwrap()
        .expect("checkpoint should exist after partial failure");

    assert_eq!(ck.failed_node, Some(n2.to_string()));
    assert!(ck.error.is_some());
    assert!(ck.completed_nodes.contains(&n1.to_string()));
    assert!(ck.node_outputs.contains_key(&n1.to_string()));
}

#[tokio::test]
async fn resume_re_runs_failed_node_and_still_fails_without_browser() {
    let engine = engine();
    let (flow, _n1, n2) = flow_code_then_browser_fail();
    engine.save_flow(&flow).await.unwrap();

    let exec = engine
        .run_flow(&flow.id, serde_json::json!({}))
        .await
        .unwrap();
    assert_eq!(exec.status, FlowExecutionStatus::Failed);
    let eid = exec.id.to_string();

    let exec2 = engine.resume_flow(&eid).await.unwrap();
    assert_eq!(exec2.status, FlowExecutionStatus::Failed);
    let failed = exec2.node_results.get(&n2.to_string()).unwrap();
    assert_eq!(failed.status, FlowNodeStatus::Failed);
}

#[tokio::test]
async fn retry_node_updates_checkpoint() {
    let engine = engine();
    let (flow, _n1, n2) = flow_code_then_browser_fail();
    engine.save_flow(&flow).await.unwrap();

    let exec = engine
        .run_flow(&flow.id, serde_json::json!({}))
        .await
        .unwrap();
    assert_eq!(exec.status, FlowExecutionStatus::Failed);
    let eid = exec.id.to_string();

    let r = engine.retry_node(&eid, &n2.to_string()).await.unwrap();
    assert_eq!(r.status, FlowNodeStatus::Failed);

    let ck = engine.get_checkpoint(&eid).await.unwrap().unwrap();
    assert_eq!(ck.failed_node, Some(n2.to_string()));
}
