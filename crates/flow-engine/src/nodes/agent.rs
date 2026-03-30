//! Agent node — calls AgentPort to run an AI agent with a prompt.

use std::sync::Arc;

use async_trait::async_trait;
use rusvel_core::domain::{AgentConfig, Content, ModelProvider, ModelRef, Part};
use rusvel_core::error::{Result, RusvelError};
use rusvel_core::id::SessionId;
use rusvel_core::ports::AgentPort;

use super::{NodeContext, NodeHandler, NodeOutput};

pub struct AgentNode {
    agent_port: Arc<dyn AgentPort>,
}

impl AgentNode {
    pub fn new(agent_port: Arc<dyn AgentPort>) -> Self {
        Self { agent_port }
    }
}

#[async_trait]
impl NodeHandler for AgentNode {
    fn node_type(&self) -> &str {
        "agent"
    }

    async fn execute(&self, ctx: &NodeContext) -> Result<NodeOutput> {
        let prompt = ctx
            .node
            .parameters
            .get("prompt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                RusvelError::Validation("Agent node requires 'prompt' parameter".into())
            })?;

        // Resolve template expressions in prompt: replace {{key}} with input values
        let mut resolved_prompt = prompt.to_string();
        for (key, value) in &ctx.inputs {
            let placeholder = format!("{{{{{key}}}}}");
            let replacement = match value {
                serde_json::Value::String(s) => s.clone(),
                other => serde_json::to_string(other).unwrap_or_default(),
            };
            resolved_prompt = resolved_prompt.replace(&placeholder, &replacement);
        }
        for (key, value) in &ctx.variables {
            let placeholder = format!("{{{{{key}}}}}");
            resolved_prompt = resolved_prompt.replace(&placeholder, value);
        }

        let model = ctx
            .node
            .parameters
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("sonnet")
            .to_string();

        let config = AgentConfig {
            profile_id: None,
            session_id: SessionId::new(),
            model: Some(ModelRef {
                provider: ModelProvider::Claude,
                model,
            }),
            tools: vec![],
            instructions: Some(resolved_prompt.clone()),
            budget_limit: None,
            max_iterations: None,
            permission_mode: Default::default(),
            metadata: serde_json::json!({}),
        };

        let run_id = self.agent_port.create(config).await?;
        let output = self
            .agent_port
            .run(&run_id, Content::text(resolved_prompt))
            .await?;

        // Extract text from Content parts
        let text: String = output
            .content
            .parts
            .iter()
            .filter_map(|p| match p {
                Part::Text(t) => Some(t.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");

        Ok(NodeOutput {
            data: serde_json::json!({
                "text": text,
                "cost_usd": output.cost_estimate,
            }),
            output_name: "main".into(),
        })
    }
}
