//! Resolve [`ModelTier`] hints to concrete [`ModelRef::model`] ids per provider.

use rusvel_core::domain::*;

/// If [`RUSVEL_META_MODEL_TIER`] is set in metadata, replace `model` with the tier default for that provider.
pub fn apply_model_tier(mut req: LlmRequest) -> LlmRequest {
    let Some(tier) = ModelTier::from_request_metadata(&req.metadata) else {
        return req;
    };
    let new_model = model_for_tier(&req.model, tier);
    if let serde_json::Value::Object(ref mut o) = req.metadata {
        o.insert(
            "rusvel.effective_model".into(),
            serde_json::Value::String(new_model.model.clone()),
        );
    }
    req.model = new_model;
    req
}

fn model_for_tier(base: &ModelRef, tier: ModelTier) -> ModelRef {
    match base.provider {
        ModelProvider::Claude => ModelRef {
            provider: ModelProvider::Claude,
            model: claude_model_id(tier).into(),
        },
        ModelProvider::OpenAI => ModelRef {
            provider: ModelProvider::OpenAI,
            model: openai_model_id(tier).into(),
        },
        ModelProvider::Ollama | ModelProvider::Gemini | ModelProvider::Other(_) => base.clone(),
    }
}

fn claude_model_id(tier: ModelTier) -> &'static str {
    match tier {
        ModelTier::Fast => "claude-haiku-4-5",
        ModelTier::Balanced => "claude-sonnet-5",
        ModelTier::Premium => "claude-opus-5",
    }
}

fn openai_model_id(tier: ModelTier) -> &'static str {
    match tier {
        ModelTier::Fast => "gpt-4o-mini",
        ModelTier::Balanced => "gpt-4o",
        ModelTier::Premium => "gpt-4o",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusvel_core::domain::RUSVEL_META_MODEL_TIER;

    #[test]
    fn claude_fast_routes_to_haiku() {
        let req = LlmRequest {
            model: ModelRef {
                provider: ModelProvider::Claude,
                model: "claude-sonnet-5".into(),
            },
            messages: vec![],
            tools: vec![],
            temperature: None,
            max_tokens: None,
            metadata: serde_json::json!({
                RUSVEL_META_MODEL_TIER: "fast",
            }),
        };
        let out = apply_model_tier(req);
        assert!(out.model.model.contains("haiku"));
    }

    #[test]
    fn no_tier_leaves_model() {
        let req = LlmRequest {
            model: ModelRef {
                provider: ModelProvider::Claude,
                model: "claude-sonnet-5".into(),
            },
            messages: vec![],
            tools: vec![],
            temperature: None,
            max_tokens: None,
            metadata: serde_json::json!({}),
        };
        let out = apply_model_tier(req);
        assert_eq!(out.model.model, "claude-sonnet-5");
    }
}
