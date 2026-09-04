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

use crate::media_gen::{
    GeneratedImage, GeneratedVideo, ImageGenRequest, ImageGenResult, ImagePayload, MediaGenPort,
    VideoGenRequest,
};

/// Non-reasoning, cheapest GPT-Image tier — a reasonable default absent an
/// explicit model choice. Override per-request via [`ImageGenRequest::model`].
const DEFAULT_MODEL: &str = "gpt-image-1-mini";
const DEFAULT_BASE_URL: &str = "https://api.avalai.ir/v1";

/// A widely-available, comparatively fast Veo tier — not necessarily
/// cheapest (AvalAI's catalog had no pricing populated for video models at
/// design time, per `~/aval-ai/MEDIA_APIS.md` — check the live catalog
/// before relying on this default for anything that spends real money).
const DEFAULT_VIDEO_MODEL: &str = "veo-3.1-fast-generate-001";

/// How many times to poll a video job before giving up. At ~5s between
/// polls this is ~10 minutes — video generation is genuinely slow; callers
/// that need a tighter bound should treat a timeout as "still running, check
/// back later" rather than "failed".
const MAX_POLL_ATTEMPTS: u32 = 120;
const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);

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

#[derive(Serialize)]
struct VideoCreateRequest<'a> {
    model: &'a str,
    prompt: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    seconds: Option<&'a str>,
}

/// Shared shape for both the create response and every poll response — same
/// fields, same meaning, just re-fetched.
#[derive(Deserialize)]
struct VideoJob {
    id: String,
    status: String,
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

    async fn generate_video(&self, request: VideoGenRequest) -> Result<GeneratedVideo> {
        let model = request
            .model
            .clone()
            .unwrap_or_else(|| DEFAULT_VIDEO_MODEL.to_string());
        let body = VideoCreateRequest {
            model: &model,
            prompt: &request.prompt,
            seconds: request.seconds.as_deref(),
        };

        let create_url = format!("{}/videos", self.base_url);
        let http_resp = self
            .client
            .post(&create_url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| RusvelError::Llm(format!("avalai video create request failed: {e}")))?;

        let status = http_resp.status();
        let text = http_resp
            .text()
            .await
            .map_err(|e| RusvelError::Llm(format!("avalai video create response read failed: {e}")))?;

        if !status.is_success() {
            return Err(RusvelError::Llm(format!(
                "avalai video create error {status}: {text}"
            )));
        }

        let created: VideoJob = serde_json::from_str(&text).map_err(|e| {
            RusvelError::Llm(format!(
                "avalai video create response parse failed: {e} — body: {text}"
            ))
        })?;

        // Poll — per AvalAI's own operational warning (MEDIA_APIS.md), billing
        // starts the moment the create call is accepted, so from here on we
        // only ever poll the job we were just given, never re-create it.
        let mut job = created;
        let mut attempts = 0u32;
        while job.status != "completed" {
            if job.status == "failed" {
                return Err(RusvelError::Llm(format!(
                    "avalai video job {} failed",
                    job.id
                )));
            }
            attempts += 1;
            if attempts >= MAX_POLL_ATTEMPTS {
                return Err(RusvelError::Llm(format!(
                    "avalai video job {} still {} after {} polls — still running, not failed; check back rather than retrying the create call",
                    job.id, job.status, attempts
                )));
            }
            tokio::time::sleep(POLL_INTERVAL).await;

            let poll_url = format!("{}/videos/{}", self.base_url, job.id);
            let poll_resp = self
                .client
                .get(&poll_url)
                .bearer_auth(&self.api_key)
                .send()
                .await
                .map_err(|e| RusvelError::Llm(format!("avalai video poll request failed: {e}")))?;
            let poll_text = poll_resp
                .text()
                .await
                .map_err(|e| RusvelError::Llm(format!("avalai video poll response read failed: {e}")))?;
            job = serde_json::from_str(&poll_text).map_err(|e| {
                RusvelError::Llm(format!(
                    "avalai video poll response parse failed: {e} — body: {poll_text}"
                ))
            })?;
        }

        // Download the completed clip. AvalAI returns raw video bytes here,
        // not JSON — base64-encode into the same GeneratedVideo shape images
        // use, rather than adding a third payload representation.
        let download_url = format!("{}/videos/{}/content", self.base_url, job.id);
        let bytes_resp = self
            .client
            .get(&download_url)
            .bearer_auth(&self.api_key)
            .send()
            .await
            .map_err(|e| RusvelError::Llm(format!("avalai video download request failed: {e}")))?;
        if !bytes_resp.status().is_success() {
            return Err(RusvelError::Llm(format!(
                "avalai video download error {}",
                bytes_resp.status()
            )));
        }
        let bytes = bytes_resp
            .bytes()
            .await
            .map_err(|e| RusvelError::Llm(format!("avalai video download read failed: {e}")))?;

        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);

        Ok(GeneratedVideo {
            payload: ImagePayload::Base64(b64),
            model,
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

    // ── video ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn generates_video_when_job_completes_immediately() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/videos"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"id": "vid_1", "status": "completed"})),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/videos/vid_1/content"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"fake-mp4-bytes".to_vec()))
            .mount(&server)
            .await;

        let adapter = AvalAiMediaGen::with_base_url("test-key", server.uri());
        let result = adapter
            .generate_video(VideoGenRequest::new("a lighthouse at dawn"))
            .await
            .unwrap();

        use base64::Engine;
        match result.payload {
            ImagePayload::Base64(b64) => {
                let decoded = base64::engine::general_purpose::STANDARD.decode(&b64).unwrap();
                assert_eq!(decoded, b"fake-mp4-bytes");
            }
            ImagePayload::Url(_) => panic!("expected base64 payload for video"),
        }
    }

    #[tokio::test]
    async fn polls_until_the_job_completes() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/videos"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"id": "vid_2", "status": "queued"})),
            )
            .mount(&server)
            .await;
        // First poll: still processing.
        Mock::given(method("GET"))
            .and(path("/videos/vid_2"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"id": "vid_2", "status": "processing"})),
            )
            .up_to_n_times(1)
            .mount(&server)
            .await;
        // Second poll onward: completed.
        Mock::given(method("GET"))
            .and(path("/videos/vid_2"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"id": "vid_2", "status": "completed"})),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/videos/vid_2/content"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"clip".to_vec()))
            .mount(&server)
            .await;

        let adapter = AvalAiMediaGen::with_base_url("test-key", server.uri());
        let result = adapter
            .generate_video(VideoGenRequest::new("a slow pan across a workshop"))
            .await
            .unwrap();
        assert_eq!(result.model, DEFAULT_VIDEO_MODEL);
    }

    #[tokio::test]
    async fn a_failed_job_errors_without_downloading() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/videos"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"id": "vid_3", "status": "failed"})),
            )
            .mount(&server)
            .await;

        let adapter = AvalAiMediaGen::with_base_url("test-key", server.uri());
        let err = adapter
            .generate_video(VideoGenRequest::new("x"))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("failed"));
    }
}
