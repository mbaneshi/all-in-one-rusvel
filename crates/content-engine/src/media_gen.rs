//! Media generation port — image (and future audio/video) generation.
//!
//! Engine-internal, not one of `rusvel_core::ports`'s 12 cross-cutting traits
//! (ADR-006's precedent: only one engine needs this today, same reasoning as
//! `harvest_engine::HarvestSource`). Lives beside its concrete adapters in
//! this crate rather than in `rusvel-core` + a separate adapter crate.

use async_trait::async_trait;
use rusvel_core::error::Result;
use serde::{Deserialize, Serialize};

/// Request to generate one or more images from a text prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageGenRequest {
    pub prompt: String,
    pub model: Option<String>,
    pub n: Option<u32>,
    pub size: Option<String>,
}

impl ImageGenRequest {
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            model: None,
            n: None,
            size: None,
        }
    }
}

/// One generated image — either an inline base64 payload or a hosted URL,
/// depending on what the provider returned for the chosen model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImagePayload {
    Base64(String),
    Url(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedImage {
    pub payload: ImagePayload,
    pub revised_prompt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageGenResult {
    pub images: Vec<GeneratedImage>,
    pub model: String,
    /// Cost in Iranian toman, when the adapter can compute it. `None` when
    /// pricing isn't known for the model (e.g. per-token image billing that
    /// needs a live catalog lookup — see `~/aval-ai/MEDIA_APIS.md`).
    pub cost_irt: Option<f64>,
}

/// Generate images from a text prompt.
#[async_trait]
pub trait MediaGenPort: Send + Sync {
    /// Human-readable name of this adapter.
    fn name(&self) -> &str;

    /// Generate one or more images for the given request.
    async fn generate_image(&self, request: ImageGenRequest) -> Result<ImageGenResult>;
}

// ════════════════════════════════════════════════════════════════════
//  MockMediaGen
// ════════════════════════════════════════════════════════════════════

/// Returns a single placeholder image. Development/testing only.
pub struct MockMediaGen;

#[async_trait]
impl MediaGenPort for MockMediaGen {
    fn name(&self) -> &str {
        "mock"
    }

    async fn generate_image(&self, request: ImageGenRequest) -> Result<ImageGenResult> {
        let slug: String = request
            .prompt
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .take(24)
            .collect();
        Ok(ImageGenResult {
            images: vec![GeneratedImage {
                payload: ImagePayload::Url(format!("https://mock.local/image/{slug}")),
                revised_prompt: None,
            }],
            model: request.model.unwrap_or_else(|| "mock".into()),
            cost_irt: Some(0.0),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_returns_one_image() {
        let mock = MockMediaGen;
        let result = mock
            .generate_image(ImageGenRequest::new("a red bicycle"))
            .await
            .unwrap();
        assert_eq!(result.images.len(), 1);
        assert_eq!(result.model, "mock");
    }
}
