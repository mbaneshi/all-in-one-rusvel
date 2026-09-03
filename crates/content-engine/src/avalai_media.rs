//! AvalAI adapter for [`MediaGenPort`] — image generation via `/v1/images/generations`.
//!
//! Text generation does **not** need an adapter here: AvalAI's chat surface is
//! already OpenAI-compatible, so it's served by `rusvel-llm`'s existing
//! `OpenAiProvider::with_base_url(key, "https://api.avalai.ir/v1")` — no new
//! code (see `rusvel-app`'s `llm_bootstrap.rs`, which already documents AvalAI
//! by name via `OPENAI_BASE_URL`). This adapter covers only the image-generation
//! surface `LlmPort` has no method for.
//!
//! Request/response shape verified live against `api.avalai.ir` 2026-09-03
//! (chat surface; image surface per `~/aval-ai/MEDIA_APIS.md`, not yet
//! independently live-tested — no image-capable key on hand at design time).

use async_trait::async_trait;
use reqwest::Client;
use rusvel_core::error::{Result, RusvelError};
use serde::{Deserialize, Serialize};

use crate::media_gen::{GeneratedImage, ImageGenRequest, ImageGenResult, ImagePayload, MediaGenPort};

/// Non-reasoning, cheapest GPT-Image tier — a reasonable default absent an
/// explicit model choice. Override per-request via [`ImageGenRequest::model`].
const DEFAULT_MODEL: &str = "gpt-image-1-mini";
const DEFAULT_BASE_URL: &str = "https://api.avalai.ir/v1";

pub struct AvalAiMediaGen {
    api_key: String,
    base_url: String,
    client: Client,
}

impl AvalAiMediaGen {
    /// Create an adapter against AvalAI's production endpoint.
    pub fn new(api_key: impl Into<String>) -> Self {
        Self::with_base_url(api_key, DEFAULT_BASE_URL)
    }

    /// Create an adapter against a custom base URL (tests use this to point
    /// at a `wiremock` server).
    pub fn with_base_url(api_key: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
            client: Client::new(),
        }
    }
}

#[derive(Serialize)]
struct ImageGenerationsRequest<'a> {
    model: &'a str,
    prompt: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    n: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<&'a str>,
}

#[derive(Deserialize)]
struct ImageGenerationsResponse {
    #[serde(default)]
    data: Vec<ImageDatum>,
}

#[derive(Deserialize)]
struct ImageDatum {
    b64_json: Option<String>,
    url: Option<String>,
    revised_prompt: Option<String>,
}

#[async_trait]
impl MediaGenPort for AvalAiMediaGen {
    fn name(&self) -> &str {
        "avalai"
    }

    async fn generate_image(&self, request: ImageGenRequest) -> Result<ImageGenResult> {
        let model = request
            .model
            .clone()
            .unwrap_or_else(|| DEFAULT_MODEL.to_string());
        let body = ImageGenerationsRequest {
            model: &model,
            prompt: &request.prompt,
            n: request.n,
            size: request.size.as_deref(),
        };

        let url = format!("{}/images/generations", self.base_url);
        let http_resp = self
            .client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| RusvelError::Llm(format!("avalai image request failed: {e}")))?;

        let status = http_resp.status();
        let text = http_resp
            .text()
            .await
            .map_err(|e| RusvelError::Llm(format!("avalai image response read failed: {e}")))?;

        if !status.is_success() {
            return Err(RusvelError::Llm(format!(
                "avalai image generation error {status}: {text}"
            )));
        }

        let parsed: ImageGenerationsResponse = serde_json::from_str(&text).map_err(|e| {
            RusvelError::Llm(format!(
                "avalai image response parse failed: {e} — body: {text}"
            ))
        })?;

        if parsed.data.is_empty() {
            return Err(RusvelError::Llm("avalai returned no images".into()));
        }

        let images = parsed
            .data
            .into_iter()
            .map(|d| {
                let payload = match (d.b64_json, d.url) {
                    (Some(b64), _) => ImagePayload::Base64(b64),
                    (None, Some(url)) => ImagePayload::Url(url),
                    // Neither field set — shouldn't happen per the documented
                    // contract; surface as an empty base64 rather than panic.
                    (None, None) => ImagePayload::Base64(String::new()),
                };
                GeneratedImage {
                    payload,
                    revised_prompt: d.revised_prompt,
                }
            })
            .collect();

        // Image billing is per-unit/per-token, not the flat per-request price
        // this adapter can compute without a live catalog lookup — leave cost
        // tracking to a follow-up once a real key + `~/aval-ai/models.json`
        // snapshot are wired in, rather than guess.
        Ok(ImageGenResult {
            images,
            model,
            cost_irt: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn generates_image_from_url_response() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/images/generations"))
            .and(header("authorization", "Bearer test-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "created": 1234567890,
                "data": [{"url": "https://cdn.avalai.ir/img/abc.png", "revised_prompt": "a cat"}]
            })))
            .mount(&server)
            .await;

        let adapter = AvalAiMediaGen::with_base_url("test-key", server.uri());
        let result = adapter
            .generate_image(ImageGenRequest::new("a cat"))
            .await
            .unwrap();

        assert_eq!(result.images.len(), 1);
        match &result.images[0].payload {
            ImagePayload::Url(u) => assert_eq!(u, "https://cdn.avalai.ir/img/abc.png"),
            ImagePayload::Base64(_) => panic!("expected URL payload"),
        }
        assert_eq!(result.images[0].revised_prompt.as_deref(), Some("a cat"));
    }

    #[tokio::test]
    async fn generates_image_from_base64_response() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/images/generations"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "created": 1234567890,
                "data": [{"b64_json": "aGVsbG8=", "revised_prompt": null}]
            })))
            .mount(&server)
            .await;

        let adapter = AvalAiMediaGen::with_base_url("test-key", server.uri());
        let result = adapter
            .generate_image(ImageGenRequest::new("x"))
            .await
            .unwrap();

        match &result.images[0].payload {
            ImagePayload::Base64(b) => assert_eq!(b, "aGVsbG8="),
            ImagePayload::Url(_) => panic!("expected base64 payload"),
        }
    }

    #[tokio::test]
    async fn propagates_error_status_and_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/images/generations"))
            .respond_with(ResponseTemplate::new(400).set_body_string("bad request"))
            .mount(&server)
            .await;

        let adapter = AvalAiMediaGen::with_base_url("test-key", server.uri());
        let err = adapter
            .generate_image(ImageGenRequest::new("x"))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("400"));
    }

    #[tokio::test]
    async fn errors_on_empty_data() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/images/generations"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"created": 1, "data": []})),
            )
            .mount(&server)
            .await;

        let adapter = AvalAiMediaGen::with_base_url("test-key", server.uri());
        let err = adapter
            .generate_image(ImageGenRequest::new("x"))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("no images"));
    }
}
