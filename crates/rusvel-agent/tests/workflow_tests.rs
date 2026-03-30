use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use async_trait::async_trait;
use tokio::sync::RwLock;

use rusvel_agent::{Workflow, WorkflowRunner, WorkflowStep};
use rusvel_core::domain::*;
use rusvel_core::error::Result;
use rusvel_core::id::*;
use rusvel_core::ports::AgentPort;

// ── Mock AgentPort ──────────────────────────────────────────────

struct MockAgent {
    call_count: AtomicU32,
    responses: RwLock<Vec<Content>>,
}

impl MockAgent {
    fn new(responses: Vec<Content>) -> Self {
        Self {
            call_count: AtomicU32::new(0),
            responses: RwLock::new(responses),
        }
    }
}

#[async_trait]
impl AgentPort for MockAgent {
    async fn create(&self, _config: AgentConfig) -> Result<RunId> {
        Ok(RunId::new())
    }

    async fn run(&self, run_id: &RunId, _input: Content) -> Result<AgentOutput> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        let mut resps = self.responses.write().await;
        let content = if resps.is_empty() {
            Content::text("default")
        } else {
            resps.remove(0)
        };
        Ok(AgentOutput {
            run_id: *run_id,
            content,
            tool_calls: 0,
            usage: LlmUsage::default(),
            cost_estimate: 0.0,
            metadata: serde_json::json!({}),
        })
    }

    async fn stop(&self, _run_id: &RunId) -> Result<()> {
        Ok(())
    }
    async fn status(&self, _run_id: &RunId) -> Result<AgentStatus> {
        Ok(AgentStatus::Completed)
    }
}

fn cfg() -> AgentConfig {
    AgentConfig {
        profile_id: None,
        session_id: SessionId::new(),
        model: None,
        tools: vec![],
        instructions: Some("test".into()),
        budget_limit: None,
        max_iterations: None,
        permission_mode: Default::default(),
        metadata: serde_json::json!({}),
    }
}

// ── Tests ───────────────────────────────────────────────────────

#[tokio::test]
async fn sequential_chains_output() {
    let mock = Arc::new(MockAgent::new(vec![
        Content::text("step1-out"),
        Content::text("step2-out"),
    ]));
    let runner = WorkflowRunner::new(mock.clone());

    let wf = Workflow {
        name: "seq-test".into(),
        steps: vec![WorkflowStep::Sequential {
            steps: vec![
                WorkflowStep::Agent {
                    config: cfg(),
                    input_mapping: None,
                },
                WorkflowStep::Agent {
                    config: cfg(),
                    input_mapping: None,
                },
            ],
        }],
    };

    let results = runner
        .run_workflow(&wf, Content::text("start"))
        .await
        .unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(mock.call_count.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn parallel_runs_concurrently() {
    let mock = Arc::new(MockAgent::new(vec![Content::text("a"), Content::text("b")]));
    let runner = WorkflowRunner::new(mock.clone());

    let wf = Workflow {
        name: "par-test".into(),
        steps: vec![WorkflowStep::Parallel {
            steps: vec![
                WorkflowStep::Agent {
                    config: cfg(),
                    input_mapping: None,
                },
                WorkflowStep::Agent {
                    config: cfg(),
                    input_mapping: None,
                },
            ],
        }],
    };

    let results = runner.run_workflow(&wf, Content::text("go")).await.unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(mock.call_count.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn loop_stops_at_max_iterations() {
    let mock = Arc::new(MockAgent::new(vec![
        Content::text("not yet"),
        Content::text("still going"),
        Content::text("nope"),
    ]));
    let runner = WorkflowRunner::new(mock.clone());

    let wf = Workflow {
        name: "loop-test".into(),
        steps: vec![WorkflowStep::Loop {
            step: Box::new(WorkflowStep::Agent {
                config: cfg(),
                input_mapping: None,
            }),
            max_iterations: 3,
            until: None,
        }],
    };

    let results = runner.run_workflow(&wf, Content::text("go")).await.unwrap();
    assert_eq!(results.len(), 3);
    assert_eq!(mock.call_count.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn loop_stops_on_sentinel() {
    let mock = Arc::new(MockAgent::new(vec![
        Content::text("working..."),
        Content::text("result DONE"),
    ]));
    let runner = WorkflowRunner::new(mock.clone());

    let wf = Workflow {
        name: "loop-done".into(),
        steps: vec![WorkflowStep::Loop {
            step: Box::new(WorkflowStep::Agent {
                config: cfg(),
                input_mapping: None,
            }),
            max_iterations: 10,
            until: None,
        }],
    };

    let results = runner.run_workflow(&wf, Content::text("go")).await.unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(mock.call_count.load(Ordering::SeqCst), 2);
}
