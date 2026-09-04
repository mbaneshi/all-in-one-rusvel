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

/// Request to generate a video clip from a text prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoGenRequest {
    pub prompt: String,
    pub model: Option<String>,
    /// Clip length in seconds. Model-dependent valid values (e.g. Sora wants
    /// a multiple of 4) — validated by the provider, not here.
    pub seconds: Option<String>,
}

impl VideoGenRequest {
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            model: None,
            seconds: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedVideo {
    pub payload: ImagePayload,
    pub model: String,
}

/// Generate images and video clips from a text prompt.
///
/// Two different calling shapes on purpose: image generation is a single
/// synchronous request; video generation is create → poll → download —
/// AvalAI itself warns generation "starts billing the moment the request is
/// accepted" and that a dropped connection must never be blindly retried
/// (`~/aval-ai/MEDIA_APIS.md`). `generate_video` hides that poll loop behind
/// one `async fn` so callers keep a uniform port interface, but it is a
/// genuinely slower, costlier, more failure-prone call than
/// [`Self::generate_image`] — callers should treat it accordingly (an
/// explicit user go-ahead before spending on it, not a default step in a
/// bundle).
#[async_trait]
pub trait MediaGenPort: Send + Sync {
    /// Human-readable name of this adapter.
    fn name(&self) -> &str;

    /// Generate one or more images for the given request.
    async fn generate_image(&self, request: ImageGenRequest) -> Result<ImageGenResult>;

    /// Generate a video clip for the given request. Not every adapter
    /// supports this (e.g. it makes no sense for a non-media provider) —
    /// default errors clearly rather than silently doing nothing.
    async fn generate_video(&self, _request: VideoGenRequest) -> Result<GeneratedVideo> {
        Err(rusvel_core::error::RusvelError::Llm(format!(
            "{} does not support video generation",
            self.name()
        )))
    }
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
