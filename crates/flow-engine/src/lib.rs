//! Flow Engine — DAG-based workflow automation.
//!
//! Executes directed acyclic graphs of nodes (agent, code, condition)
//! with parallel branch execution and error routing.

use std::collections::HashMap;
use std::sync::Arc;

use rusvel_core::domain::*;
use rusvel_core::error::{Result, RusvelError};
use rusvel_core::id::{FlowExecutionId, FlowId, FlowNodeId};
use rusvel_core::ports::{AgentPort, BrowserPort, EventPort, StoragePort, TerminalPort};

pub mod executor;
pub mod expression;
pub mod expressions;
pub mod nodes;
pub mod templates;

use nodes::NodeRegistry;

pub use templates::cross_engine_handoff_template;

/// The flow engine: creates, stores, and executes DAG workflows.
pub struct FlowEngine {
    storage: Arc<dyn StoragePort>,
    events: Arc<dyn EventPort>,
    registry: NodeRegistry,
    terminal: Option<Arc<dyn TerminalPort>>,
    #[allow(dead_code)]
    browser: Option<Arc<dyn BrowserPort>>,
}

const FLOW_STORE: &str = "flows";
const EXECUTION_STORE: &str = "flow_executions";
pub(crate) const CHECKPOINT_STORE: &str = "flow_checkpoints";

impl FlowEngine {
    pub fn new(
        storage: Arc<dyn StoragePort>,
        events: Arc<dyn EventPort>,
        agent: Arc<dyn AgentPort>,
        terminal: Option<Arc<dyn TerminalPort>>,
        browser: Option<Arc<dyn BrowserPort>>,
    ) -> Self {
        let mut registry = NodeRegistry::new();
        registry.register(Arc::new(nodes::code::CodeNode));
        registry.register(Arc::new(nodes::condition::ConditionNode));
        registry.register(Arc::new(nodes::agent::AgentNode::new(agent.clone())));
        registry.register(Arc::new(nodes::browser::BrowserTriggerNode));
        registry.register(Arc::new(nodes::browser::BrowserActionNode::new(
            browser.clone(),
        )));
        registry.register(Arc::new(nodes::parallel::ParallelEvaluateNode::new(
            agent.clone(),
        )));

        Self {
            storage,
            events,
            registry,
            terminal,
            browser,
        }
    }

    /// List available node types.
    pub fn node_types(&self) -> Vec<String> {
        self.registry.node_types()
    }

    /// Save a flow definition.
    pub async fn save_flow(&self, flow: &FlowDef) -> Result<rusvel_core::id::FlowId> {
        let value = serde_json::to_value(flow)?;
        self.storage
            .objects()
            .put(FLOW_STORE, &flow.id.to_string(), value)
            .await?;
        Ok(flow.id)
    }

    /// Get a flow definition by ID.
    pub async fn get_flow(&self, id: &rusvel_core::id::FlowId) -> Result<Option<FlowDef>> {
        let value = self
            .storage
            .objects()
            .get(FLOW_STORE, &id.to_string())
            .await?;
        match value {
            Some(v) => Ok(Some(serde_json::from_value(v)?)),
            None => Ok(None),
        }
    }

    /// List all flow definitions.
    pub async fn list_flows(&self) -> Result<Vec<FlowDef>> {
        let values = self
            .storage
            .objects()
            .list(FLOW_STORE, ObjectFilter::default())
            .await?;
        let flows: Vec<FlowDef> = values
            .into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect();
        Ok(flows)
    }

    /// Delete a flow definition.
    pub async fn delete_flow(&self, id: &rusvel_core::id::FlowId) -> Result<()> {
        self.storage
            .objects()
            .delete(FLOW_STORE, &id.to_string())
            .await
    }

    /// Execute a flow and return the execution record.
    pub async fn run_flow(
        &self,
        id: &rusvel_core::id::FlowId,
        trigger_data: serde_json::Value,
    ) -> Result<FlowExecution> {
        let flow =
            self.get_flow(id)
                .await?
                .ok_or_else(|| rusvel_core::error::RusvelError::NotFound {
                    kind: "flow".into(),
                    id: id.to_string(),
                })?;

        let execution_id = FlowExecutionId::new();
        let started_at = chrono::Utc::now();
        let checkpoint = executor::CheckpointCtx {
            storage: &self.storage,
            flow: &flow,
            execution_id,
            trigger_data: trigger_data.clone(),
            started_at,
        };
        let execution = executor::execute_flow_with_config(
            &flow,
            trigger_data,
            &self.registry,
            executor::ExecuteFlowConfig {
                execution_id,
                started_at,
                resume: None,
                checkpoint: Some(checkpoint),
                terminal: self.terminal.clone(),
            },
        )
        .await?;

        if execution.status == FlowExecutionStatus::Succeeded {
            let _ = self
                .storage
                .objects()
                .delete(CHECKPOINT_STORE, &execution.id.to_string())
                .await;
        }

        let exec_value = serde_json::to_value(&execution)?;
        self.storage
            .objects()
            .put(EXECUTION_STORE, &execution.id.to_string(), exec_value)
            .await?;

        let _ = self
            .events
            .emit(Event {
                id: rusvel_core::id::EventId::new(),
                session_id: None,
                run_id: None,
                source: "flow".into(),
                kind: "flow.execution.completed".into(),
                payload: serde_json::json!({
                    "flow_id": id.to_string(),
                    "execution_id": execution.id.to_string(),
                    "status": format!("{:?}", execution.status),
                }),
                created_at: chrono::Utc::now(),
                metadata: serde_json::json!({}),
            })
            .await;

        Ok(execution)
    }

    /// Resume a flow from a persisted checkpoint (same `execution_id`).
    pub async fn resume_flow(&self, execution_id: &str) -> Result<FlowExecution> {
        let exec_uuid = uuid::Uuid::parse_str(execution_id)
            .map_err(|_| RusvelError::Validation("invalid execution id".into()))?;
        let exec_id = FlowExecutionId::from_uuid(exec_uuid);

        let raw = self
            .storage
            .objects()
            .get(CHECKPOINT_STORE, execution_id)
            .await?
            .ok_or_else(|| RusvelError::NotFound {
                kind: "flow_checkpoint".into(),
                id: execution_id.into(),
            })?;
        let ck: FlowCheckpoint = serde_json::from_value(raw)?;

        let flow_id = uuid::Uuid::parse_str(&ck.flow_id)
            .map(FlowId::from_uuid)
            .map_err(|_| RusvelError::Validation("invalid flow_id in checkpoint".into()))?;

        let flow = self
            .get_flow(&flow_id)
            .await?
            .ok_or_else(|| RusvelError::NotFound {
                kind: "flow".into(),
                id: ck.flow_id.clone(),
            })?;

        let resume = executor::ResumeState::from_checkpoint(&ck)?;
        let started_at = ck.started_at.unwrap_or(ck.created_at);
        let trigger_data = ck.trigger_data.clone();

        let checkpoint = executor::CheckpointCtx {
            storage: &self.storage,
            flow: &flow,
            execution_id: exec_id,
            trigger_data: trigger_data.clone(),
            started_at,
        };

        let execution = executor::execute_flow_with_config(
            &flow,
            trigger_data,
            &self.registry,
            executor::ExecuteFlowConfig {
                execution_id: exec_id,
                started_at,
                resume: Some(resume),
                checkpoint: Some(checkpoint),
                terminal: self.terminal.clone(),
            },
        )
        .await?;

        if execution.status == FlowExecutionStatus::Succeeded {
            let _ = self
                .storage
                .objects()
                .delete(CHECKPOINT_STORE, execution_id)
                .await;
        }

        let exec_value = serde_json::to_value(&execution)?;
        self.storage
            .objects()
            .put(EXECUTION_STORE, &execution.id.to_string(), exec_value)
            .await?;

        let _ = self
            .events
            .emit(Event {
                id: rusvel_core::id::EventId::new(),
                session_id: None,
                run_id: None,
                source: "flow".into(),
                kind: "flow.execution.completed".into(),
                payload: serde_json::json!({
                    "flow_id": flow_id.to_string(),
                    "execution_id": execution.id.to_string(),
                    "status": format!("{:?}", execution.status),
                }),
                created_at: chrono::Utc::now(),
                metadata: serde_json::json!({}),
            })
            .await;

        Ok(execution)
    }

    /// Re-run a single node using upstream outputs from the checkpoint.
    pub async fn retry_node(&self, execution_id: &str, node_id: &str) -> Result<FlowNodeResult> {
        let raw = self
            .storage
            .objects()
            .get(CHECKPOINT_STORE, execution_id)
            .await?
            .ok_or_else(|| RusvelError::NotFound {
                kind: "flow_checkpoint".into(),
                id: execution_id.into(),
            })?;
        let mut ck: FlowCheckpoint = serde_json::from_value(raw)?;

        let flow_id = uuid::Uuid::parse_str(&ck.flow_id)
            .map(FlowId::from_uuid)
            .map_err(|_| RusvelError::Validation("invalid flow_id in checkpoint".into()))?;

        let flow = self
            .get_flow(&flow_id)
            .await?
            .ok_or_else(|| RusvelError::NotFound {
                kind: "flow".into(),
                id: ck.flow_id.clone(),
            })?;

        let node_uuid = uuid::Uuid::parse_str(node_id)
            .map_err(|_| RusvelError::Validation("invalid node id".into()))?;
        let node_fid = FlowNodeId::from_uuid(node_uuid);
        if !flow.nodes.iter().any(|n| n.id == node_fid) {
            return Err(RusvelError::Validation("node not in flow".into()));
        }

        let mut outputs: HashMap<FlowNodeId, serde_json::Value> = HashMap::new();
        for (k, v) in &ck.node_outputs {
            let uuid = uuid::Uuid::parse_str(k).map_err(|_| {
                RusvelError::Validation(format!("invalid node id in checkpoint: {k}"))
            })?;
            outputs.insert(FlowNodeId::from_uuid(uuid), v.clone());
        }

        let result = executor::execute_single_node(
            &flow,
            node_fid,
            &outputs,
            &ck.trigger_data,
            &self.registry,
        )
        .await?;

        ck.node_results.insert(node_id.to_string(), result.clone());
        if result.status == FlowNodeStatus::Succeeded {
            if let Some(out) = result.output.clone() {
                ck.node_outputs.insert(node_id.to_string(), out);
            }
            if !ck.completed_nodes.contains(&node_id.to_string()) {
                ck.completed_nodes.push(node_id.to_string());
            }
            ck.failed_node = None;
            ck.error = None;
        } else {
            ck.failed_node = Some(node_id.to_string());
            ck.error = result.error.clone();
        }
        ck.created_at = chrono::Utc::now();

        let val = serde_json::to_value(&ck)?;
        self.storage
            .objects()
            .put(CHECKPOINT_STORE, execution_id, val)
            .await?;

        Ok(result)
    }

    /// Load the current checkpoint for an execution (if any).
    pub async fn get_checkpoint(&self, execution_id: &str) -> Result<Option<FlowCheckpoint>> {
        let raw = self
            .storage
            .objects()
            .get(CHECKPOINT_STORE, execution_id)
            .await?;
        match raw {
            Some(v) => Ok(Some(serde_json::from_value(v)?)),
            None => Ok(None),
        }
    }

    /// Get an execution by ID.
    pub async fn get_execution(
        &self,
        id: &rusvel_core::id::FlowExecutionId,
    ) -> Result<Option<FlowExecution>> {
        let value = self
            .storage
            .objects()
            .get(EXECUTION_STORE, &id.to_string())
            .await?;
        match value {
            Some(v) => Ok(Some(serde_json::from_value(v)?)),
            None => Ok(None),
        }
    }

    /// List executions for a flow.
    pub async fn list_executions(
        &self,
        flow_id: &rusvel_core::id::FlowId,
    ) -> Result<Vec<FlowExecution>> {
        let values = self
            .storage
            .objects()
            .list(EXECUTION_STORE, ObjectFilter::default())
            .await?;
        let executions: Vec<FlowExecution> = values
            .into_iter()
            .filter_map(|v| serde_json::from_value::<FlowExecution>(v).ok())
            .filter(|e| e.flow_id == *flow_id)
            .collect();
        Ok(executions)
    }

    /// All persisted executions, newest [`FlowExecution::started_at`] first (single store scan).
    pub async fn list_recent_executions(&self, limit: usize) -> Result<Vec<FlowExecution>> {
        const MAX: usize = 200;
        let cap = limit.clamp(1, MAX);
        let values = self
            .storage
            .objects()
            .list(EXECUTION_STORE, ObjectFilter::default())
            .await?;
        let mut executions: Vec<FlowExecution> = values
            .into_iter()
            .filter_map(|v| serde_json::from_value::<FlowExecution>(v).ok())
            .collect();
        executions.sort_by_key(|a| std::cmp::Reverse(a.started_at));
        executions.truncate(cap);
        Ok(executions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusvel_core::id::{FlowId, FlowNodeId};

    // Helper: create a simple linear flow (code node A → code node B)
    fn make_linear_flow() -> FlowDef {
        let n1 = FlowNodeId::new();
        let n2 = FlowNodeId::new();
        FlowDef {
            id: FlowId::new(),
            name: "linear-test".into(),
            description: "A → B".into(),
            nodes: vec![
                FlowNodeDef {
                    id: n1,
                    node_type: "code".into(),
                    name: "step1".into(),
                    parameters: serde_json::json!({"value": "hello"}),
                    position: (0.0, 0.0),
                    on_error: FlowErrorBehavior::StopFlow,
                    metadata: serde_json::json!({}),
                },
                FlowNodeDef {
                    id: n2,
                    node_type: "code".into(),
                    name: "step2".into(),
                    parameters: serde_json::json!({"value": "world"}),
                    position: (200.0, 0.0),
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

    fn make_registry() -> NodeRegistry {
        let mut reg = NodeRegistry::new();
        reg.register(Arc::new(nodes::code::CodeNode));
        reg.register(Arc::new(nodes::condition::ConditionNode));
        reg
    }

    #[tokio::test]
    async fn linear_flow_executes() {
        let flow = make_linear_flow();
        let reg = make_registry();
        let exec = executor::execute_flow(&flow, serde_json::json!({}), &reg, None)
            .await
            .unwrap();
        assert_eq!(exec.status, FlowExecutionStatus::Succeeded);
        assert_eq!(exec.node_results.len(), 2);
    }

    #[tokio::test]
    async fn condition_branches() {
        let n1 = FlowNodeId::new();
        let n_true = FlowNodeId::new();
        let n_false = FlowNodeId::new();

        let flow = FlowDef {
            id: FlowId::new(),
            name: "branch-test".into(),
            description: "cond → true/false".into(),
            nodes: vec![
                FlowNodeDef {
                    id: n1,
                    node_type: "condition".into(),
                    name: "check".into(),
                    parameters: serde_json::json!({"result": true}),
                    position: (0.0, 0.0),
                    on_error: FlowErrorBehavior::StopFlow,
                    metadata: serde_json::json!({}),
                },
                FlowNodeDef {
                    id: n_true,
                    node_type: "code".into(),
                    name: "yes".into(),
                    parameters: serde_json::json!({"value": "yes"}),
                    position: (200.0, -100.0),
                    on_error: FlowErrorBehavior::StopFlow,
                    metadata: serde_json::json!({}),
                },
                FlowNodeDef {
                    id: n_false,
                    node_type: "code".into(),
                    name: "no".into(),
                    parameters: serde_json::json!({"value": "no"}),
                    position: (200.0, 100.0),
                    on_error: FlowErrorBehavior::StopFlow,
                    metadata: serde_json::json!({}),
                },
            ],
            connections: vec![
                FlowConnectionDef {
                    source_node: n1,
                    source_output: "true".into(),
                    target_node: n_true,
                    target_input: "main".into(),
                    metadata: Default::default(),
                },
                FlowConnectionDef {
                    source_node: n1,
                    source_output: "false".into(),
                    target_node: n_false,
                    target_input: "main".into(),
                    metadata: Default::default(),
                },
            ],
            variables: Default::default(),
            metadata: serde_json::json!({}),
        };

        let reg = make_registry();
        let exec = executor::execute_flow(&flow, serde_json::json!({}), &reg, None)
            .await
            .unwrap();

        // The condition is `true`, so `n_false` should be skipped
        let false_result = exec.node_results.get(&n_false.to_string()).unwrap();
        assert_eq!(false_result.status, FlowNodeStatus::Skipped);
    }

    #[tokio::test]
    async fn code_node_returns_value() {
        let n1 = FlowNodeId::new();
        let flow = FlowDef {
            id: FlowId::new(),
            name: "value-test".into(),
            description: "literal value".into(),
            nodes: vec![FlowNodeDef {
                id: n1,
                node_type: "code".into(),
                name: "literal".into(),
                parameters: serde_json::json!({"value": {"score": 85, "name": "test"}}),
                position: (0.0, 0.0),
                on_error: FlowErrorBehavior::StopFlow,
                metadata: serde_json::json!({}),
            }],
            connections: vec![],
            variables: Default::default(),
            metadata: serde_json::json!({}),
        };

        let reg = make_registry();
        let exec = executor::execute_flow(&flow, serde_json::json!({}), &reg, None)
            .await
            .unwrap();

        assert_eq!(exec.status, FlowExecutionStatus::Succeeded);
        let result = exec.node_results.get(&n1.to_string()).unwrap();
        assert_eq!(
            result.output,
            Some(serde_json::json!({"score": 85, "name": "test"}))
        );
    }

    #[tokio::test]
    async fn unknown_node_type_fails() {
        let n1 = FlowNodeId::new();
        let flow = FlowDef {
            id: FlowId::new(),
            name: "error-test".into(),
            description: "bad type".into(),
            nodes: vec![FlowNodeDef {
                id: n1,
                node_type: "nonexistent".into(),
                name: "bad".into(),
                parameters: serde_json::json!({}),
                position: (0.0, 0.0),
                on_error: FlowErrorBehavior::StopFlow,
                metadata: serde_json::json!({}),
            }],
            connections: vec![],
            variables: Default::default(),
            metadata: serde_json::json!({}),
        };

        let reg = make_registry();
        let result = executor::execute_flow(&flow, serde_json::json!({}), &reg, None).await;
        assert!(result.is_err());
    }

    #[test]
    fn node_registry_lists_types() {
        let reg = make_registry();
        let types = reg.node_types();
        assert!(types.contains(&"code".to_string()));
        assert!(types.contains(&"condition".to_string()));
    }
}
