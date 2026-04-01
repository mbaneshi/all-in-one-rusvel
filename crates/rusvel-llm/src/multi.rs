//! Multi-provider router that dispatches by [`ModelProvider`].

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;

use rusvel_core::domain::*;
use rusvel_core::error::RusvelError;
use rusvel_core::ports::LlmPort;

// ════════════════════════════════════════════════════════════════════
//  MultiProvider
// ════════════════════════════════════════════════════════════════════

/// Routes LLM requests to the correct provider based on
/// [`ModelRef::provider`].
///
/// Providers can be swapped at runtime via [`Self::swap_provider`]
/// (e.g. switching Claude between CLI and API without restarting).
///
/// ```ignore
/// let multi = MultiProvider::new();
/// multi.register(ModelProvider::Ollama, Arc::new(OllamaProvider::new()));
/// multi.register(ModelProvider::Claude, Arc::new(ClaudeProvider::new(key)));
/// multi.register(ModelProvider::OpenAI, Arc::new(OpenAiProvider::new(key)));
/// ```
pub struct MultiProvider {
    providers: RwLock<HashMap<ModelProvider, Arc<dyn LlmPort>>>,
}

impl MultiProvider {
    /// Create an empty router with no providers registered.
    pub fn new() -> Self {
        Self {
            providers: RwLock::new(HashMap::new()),
        }
    }

    /// Register a provider for a given [`ModelProvider`] variant.
    pub fn register(&self, provider: ModelProvider, llm: Arc<dyn LlmPort>) {
        self.providers.write().unwrap().insert(provider, llm);
    }

    /// Hot-swap a provider at runtime (e.g. CLI ↔ API for Claude).
    pub fn swap_provider(&self, provider: ModelProvider, llm: Arc<dyn LlmPort>) {
        self.providers.write().unwrap().insert(provider, llm);
    }

    /// Look up the adapter for a provider, returning a user-friendly error
    /// if it has not been registered.
    fn get(&self, provider: &ModelProvider) -> rusvel_core::error::Result<Arc<dyn LlmPort>> {
        self.providers
            .read()
            .unwrap()
            .get(provider)
            .cloned()
            .ok_or_else(|| {
                RusvelError::Llm(format!("no adapter registered for provider {provider:?}"))
            })
    }
}

impl Default for MultiProvider {
    fn default() -> Self {
        Self::new()
    }
}

// ════════════════════════════════════════════════════════════════════
//  LlmPort implementation — delegate to inner provider
// ════════════════════════════════════════════════════════════════════

#[async_trait]
impl LlmPort for MultiProvider {
    async fn generate(&self, request: LlmRequest) -> rusvel_core::error::Result<LlmResponse> {
        let provider = self.get(&request.model.provider)?;
        provider.generate(request).await
    }

    async fn stream(
        &self,
        request: LlmRequest,
    ) -> rusvel_core::error::Result<tokio::sync::mpsc::Receiver<LlmStreamEvent>> {
        let provider = self.get(&request.model.provider)?;
        provider.stream(request).await
    }

    async fn embed(&self, model: &ModelRef, text: &str) -> rusvel_core::error::Result<Vec<f32>> {
        let provider = self.get(&model.provider)?;
        provider.embed(model, text).await
    }

    async fn list_models(&self) -> rusvel_core::error::Result<Vec<ModelRef>> {
        let providers: Vec<Arc<dyn LlmPort>> =
            self.providers.read().unwrap().values().cloned().collect();
        let mut all = Vec::new();
        for provider in &providers {
            match provider.list_models().await {
                Ok(models) => all.extend(models),
                Err(e) => {
                    tracing::warn!("failed to list models from a provider: {e}");
                }
            }
        }
        Ok(all)
    }

    async fn submit_batch(
        &self,
        batch: LlmBatchRequest,
    ) -> rusvel_core::error::Result<LlmBatchSubmitResult> {
        let first = batch
            .items
            .first()
            .ok_or_else(|| RusvelError::Validation("batch has no items".into()))?;
        let p = &first.request.model.provider;
        for item in &batch.items[1..] {
            if &item.request.model.provider != p {
                return Err(RusvelError::Validation(
                    "batch items must use the same model provider".into(),
                ));
            }
        }
        let provider = self.get(p)?;
        provider.submit_batch(batch).await
    }

    async fn poll_batch(
        &self,
        handle: &BatchHandle,
    ) -> rusvel_core::error::Result<LlmBatchPollResult> {
        let provider = self.get(&handle.provider)?;
        provider.poll_batch(handle).await
    }
}

// ════════════════════════════════════════════════════════════════════
//  Tests
// ════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny fake provider for testing the router.
    struct FakeProvider {
        tag: &'static str,
    }

    #[async_trait]
    impl LlmPort for FakeProvider {
        async fn generate(&self, _request: LlmRequest) -> rusvel_core::error::Result<LlmResponse> {
            Ok(LlmResponse {
                content: Content::text(format!("from {}", self.tag)),
                finish_reason: FinishReason::Stop,
                usage: LlmUsage::default(),
                metadata: serde_json::json!({}),
            })
        }

        async fn embed(
            &self,
            _model: &ModelRef,
            _text: &str,
        ) -> rusvel_core::error::Result<Vec<f32>> {
            Ok(vec![0.0])
        }

        async fn list_models(&self) -> rusvel_core::error::Result<Vec<ModelRef>> {
            Ok(vec![ModelRef {
                provider: ModelProvider::Other(self.tag.into()),
                model: "fake-model".into(),
            }])
        }
    }

    fn make_request(provider: ModelProvider) -> LlmRequest {
        LlmRequest {
            model: ModelRef {
                provider,
                model: "test".into(),
            },
            messages: vec![LlmMessage {
                role: LlmRole::User,
                content: Content::text("hi"),
            }],
            tools: vec![],
            temperature: None,
            max_tokens: None,
            metadata: serde_json::json!({}),
        }
    }

    #[tokio::test]
    async fn routes_to_correct_provider() {
        let multi = MultiProvider::new();
        multi.register(
            ModelProvider::Ollama,
            Arc::new(FakeProvider { tag: "ollama" }),
        );
        multi.register(
            ModelProvider::Claude,
            Arc::new(FakeProvider { tag: "claude" }),
        );

        let resp = multi
            .generate(make_request(ModelProvider::Claude))
            .await
            .unwrap();
        match &resp.content.parts[0] {
            Part::Text(t) => assert_eq!(t, "from claude"),
            _ => panic!("expected text"),
        }

        let resp = multi
            .generate(make_request(ModelProvider::Ollama))
            .await
            .unwrap();
        match &resp.content.parts[0] {
            Part::Text(t) => assert_eq!(t, "from ollama"),
            _ => panic!("expected text"),
        }
    }

    #[tokio::test]
    async fn unregistered_provider_returns_error() {
        let multi = MultiProvider::new();
        let result = multi.generate(make_request(ModelProvider::OpenAI)).await;
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("no adapter registered"), "got: {msg}");
    }

    #[tokio::test]
    async fn list_models_aggregates() {
        let multi = MultiProvider::new();
        multi.register(
            ModelProvider::Ollama,
            Arc::new(FakeProvider { tag: "ollama" }),
        );
        multi.register(
            ModelProvider::Claude,
            Arc::new(FakeProvider { tag: "claude" }),
        );
        let models = multi.list_models().await.unwrap();
        assert_eq!(models.len(), 2);
    }
}
