//! Parallel evaluate — runs multiple agent prompts concurrently (S-048).
//!
//! Parameters: `{ "branches": [ { "prompt": "..." }, ... ], "model": "sonnet" }` (optional model applies to all branches).

use std::sync::Arc;

use async_trait::async_trait;
use rusvel_core::domain::FlowNodeDef;
use rusvel_core::error::{Result, RusvelError};
use rusvel_core::ports::AgentPort;

use super::agent::AgentNode;
use super::{NodeContext, NodeHandler, NodeOutput};

pub struct ParallelEvaluateNode {
    agent: Arc<dyn AgentPort>,
}

impl ParallelEvaluateNode {
    pub fn new(agent: Arc<dyn AgentPort>) -> Self {
        Self { agent }
    }

    fn branch_ctx(
        base: &NodeContext,
        idx: usize,
        branch: &serde_json::Value,
    ) -> Result<NodeContext> {
        let prompt = branch
            .get("prompt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                RusvelError::Validation("parallel_evaluate branch needs prompt".into())
            })?;
        let mut params = serde_json::json!({ "prompt": prompt });
        if let Some(m) = base.node.parameters.get("model") {
            params
                .as_object_mut()
                .expect("object")
                .insert("model".into(), m.clone());
        }
        if let Some(m) = branch.get("model") {
            params
                .as_object_mut()
                .expect("object")
                .insert("model".into(), m.clone());
        }
        let node = FlowNodeDef {
            id: base.node.id,
            node_type: "agent".into(),
            name: format!("{}_parallel_{idx}", base.node.name),
            parameters: params,
            position: base.node.position,
            on_error: base.node.on_error.clone(),
            metadata: base.node.metadata.clone(),
        };
        Ok(NodeContext {
            node,
            inputs: base.inputs.clone(),
            variables: base.variables.clone(),
        })
    }
}

#[async_trait]
impl NodeHandler for ParallelEvaluateNode {
    fn node_type(&self) -> &str {
        "parallel_evaluate"
    }

    async fn execute(&self, ctx: &NodeContext) -> Result<NodeOutput> {
        let branches = ctx
            .node
            .parameters
            .get("branches")
            .and_then(|v| v.as_array())
            .ok_or_else(|| {
                RusvelError::Validation(
                    "parallel_evaluate requires parameters.branches (array of {prompt})".into(),
                )
            })?;
        if branches.is_empty() {
            return Err(RusvelError::Validation(
                "parallel_evaluate branches must not be empty".into(),
            ));
        }

        let mut futs = Vec::new();
        for (i, b) in branches.iter().enumerate() {
            let sub = Self::branch_ctx(ctx, i, b)?;
            let agent = self.agent.clone();
            futs.push(async move {
                let inner = AgentNode::new(agent);
                let out = inner.execute(&sub).await?;
                Ok::<_, RusvelError>(out.data)
            });
        }
        let results = futures::future::try_join_all(futs).await?;

        Ok(NodeOutput {
            data: serde_json::json!({ "results": results }),
            output_name: "main".into(),
        })
    }
}
