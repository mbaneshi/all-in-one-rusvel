//! Proposal generation for opportunities using LLM.

use std::sync::Arc;

use rusvel_core::ports::AgentPort;
use rusvel_core::{AgentConfig, Content, Opportunity, Result};
use serde::{Deserialize, Serialize};

/// A generated proposal for an opportunity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    pub body: String,
    pub estimated_value: Option<f64>,
    pub tone: String,
    pub metadata: serde_json::Value,
}

/// Generates tailored proposals using the `AgentPort`.
pub struct ProposalGenerator {
    agent: Arc<dyn AgentPort>,
}

impl ProposalGenerator {
    pub fn new(agent: Arc<dyn AgentPort>) -> Self {
        Self { agent }
    }

    /// Generate a proposal for the given opportunity and freelancer profile.
    pub async fn generate(&self, opportunity: &Opportunity, profile: &str) -> Result<Proposal> {
        let prompt = format!(
            "Write a tailored freelance proposal for this opportunity.\n\n\
             ## Opportunity\n\
             Title: {}\n\
             Description: {}\n\
             URL: {}\n\
             Estimated value: {}\n\n\
             ## My Profile\n\
             {}\n\n\
             Respond with ONLY a JSON object:\n\
             {{\n\
               \"body\": \"the full proposal text\",\n\
               \"estimated_value\": 5000.0,\n\
               \"tone\": \"professional\"\n\
             }}",
            opportunity.title,
            opportunity.description,
            opportunity.url.as_deref().unwrap_or("N/A"),
            opportunity
                .value_estimate
                .map_or_else(|| "not specified".into(), |v| format!("${v}")),
            profile,
        );

        let config = AgentConfig {
            profile_id: None,
            session_id: opportunity.session_id,
            model: None,
            tools: vec![],
            instructions: Some(
                "You are an expert freelance proposal writer. \
                 Write compelling, concise proposals."
                    .into(),
            ),
            budget_limit: Some(0.05),
            max_iterations: None,
            permission_mode: Default::default(),
            metadata: serde_json::json!({}),
        };

        let run_id = self.agent.create(config).await?;
        let output = self.agent.run(&run_id, Content::text(prompt)).await?;

        let text = output
            .content
            .parts
            .iter()
            .filter_map(|p| match p {
                rusvel_core::Part::Text(t) => Some(t.as_str()),
                _ => None,
            })
            .collect::<String>();

        // Try to parse structured JSON response
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) {
            let body = parsed["body"].as_str().unwrap_or(&text).to_string();
            let estimated_value = parsed["estimated_value"].as_f64();
            let tone = parsed["tone"]
                .as_str()
                .unwrap_or("professional")
                .to_string();

            return Ok(Proposal {
                body,
                estimated_value,
                tone,
                metadata: serde_json::json!({"opportunity_id": opportunity.id.to_string()}),
            });
        }

        // Fallback: use raw text as proposal body
        Ok(Proposal {
            body: text,
            estimated_value: opportunity.value_estimate,
            tone: "professional".into(),
            metadata: serde_json::json!({"opportunity_id": opportunity.id.to_string()}),
        })
    }
}
