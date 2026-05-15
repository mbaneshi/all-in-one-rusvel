//! flow eval — execute a linear 3-node code DAG, assert every node ran.

use async_trait::async_trait;
use flow_engine::FlowEngine;
use rusvel_core::domain::{
    FlowConnectionDef, FlowDef, FlowErrorBehavior, FlowExecutionStatus, FlowNodeDef, FlowNodeStatus,
};
use rusvel_core::id::{FlowId, FlowNodeId};

use crate::{Eval, EvalCtx, EvalResult};

pub struct FlowLinearDagEval;

#[async_trait]
impl Eval for FlowLinearDagEval {
    fn name(&self) -> &str {
        "flow.linear_dag_all_ran"
    }

    fn suite(&self) -> &str {
        "flow"
    }

    async fn run(&self, ctx: &EvalCtx) -> EvalResult {
        // Flow engine wants an agent for "agent" nodes; ours has none, but
        // we pass the stub anyway so construction succeeds.
        let engine = FlowEngine::new(ctx.storage(), ctx.events_port(), ctx.agent(), None, None);

        let a = FlowNodeId::new();
        let b = FlowNodeId::new();
        let c = FlowNodeId::new();

        let flow = FlowDef {
            id: FlowId::new(),
            name: "evals.linear_three_step".into(),
            description: "A → B → C (all code nodes)".into(),
            nodes: vec![
                code_node(a, "alpha", serde_json::json!({"value": "a"})),
                code_node(b, "beta", serde_json::json!({"value": "b"})),
                code_node(c, "gamma", serde_json::json!({"value": "c"})),
            ],
            connections: vec![edge(a, b), edge(b, c)],
            variables: Default::default(),
            metadata: serde_json::json!({}),
        };

        if let Err(e) = engine.save_flow(&flow).await {
            return EvalResult::fail(format!("save_flow failed: {e}"));
        }

        let exec = match engine.run_flow(&flow.id, serde_json::json!({})).await {
            Ok(e) => e,
            Err(e) => return EvalResult::fail(format!("run_flow failed: {e}")),
        };

        if exec.status != FlowExecutionStatus::Succeeded {
            return EvalResult::fail(format!(
                "expected status=Succeeded, got {:?} (error: {:?})",
                exec.status, exec.error
            ));
        }

        if exec.node_results.len() != 3 {
            return EvalResult::fail(format!(
                "expected 3 node results, got {} (keys: {:?})",
                exec.node_results.len(),
                exec.node_results.keys().collect::<Vec<_>>()
            ));
        }

        let unhealthy: Vec<_> = exec
            .node_results
            .iter()
            .filter(|(_, r)| r.status != FlowNodeStatus::Succeeded)
            .map(|(k, r)| format!("{k}={:?}", r.status))
            .collect();
        if !unhealthy.is_empty() {
            return EvalResult::fail(format!(
                "some nodes did not succeed: {}",
                unhealthy.join(", ")
            ));
        }

        EvalResult::pass(format!(
            "linear DAG executed — {} nodes all Succeeded",
            exec.node_results.len()
        ))
        .with_metrics(serde_json::json!({
            "nodes_ran": exec.node_results.len(),
        }))
    }
}

fn code_node(id: FlowNodeId, name: &str, params: serde_json::Value) -> FlowNodeDef {
    FlowNodeDef {
        id,
        node_type: "code".into(),
        name: name.into(),
        parameters: params,
        position: (0.0, 0.0),
        on_error: FlowErrorBehavior::StopFlow,
        metadata: serde_json::json!({}),
    }
}

fn edge(source: FlowNodeId, target: FlowNodeId) -> FlowConnectionDef {
    FlowConnectionDef {
        source_node: source,
        source_output: "main".into(),
        target_node: target,
        target_input: "main".into(),
        metadata: serde_json::json!({}),
    }
}
