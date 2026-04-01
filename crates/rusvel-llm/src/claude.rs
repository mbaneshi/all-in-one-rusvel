//! Claude (Anthropic) HTTP adapter implementing [`LlmPort`].

use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::debug;

use rusvel_core::domain::*;
use rusvel_core::error::{Result, RusvelError};
use rusvel_core::ports::LlmPort;

/// Beta header for prompt caching.
const ANTHROPIC_BETA_PROMPT_CACHING: &str = "prompt-caching-2024-07-31";

/// Beta header for Claude computer use (legacy tool type `computer_20250124`).
const ANTHROPIC_BETA_COMPUTER_USE_LEGACY: &str = "computer-use-2025-01-24";
/// Beta header for newer computer-use tool schemas (Opus/Sonnet 4.6 family).
const ANTHROPIC_BETA_COMPUTER_USE_V2: &str = "computer-use-2025-11-24";

fn computer_use_beta_header(tools: &[serde_json::Value]) -> Option<&'static str> {
    let has_v2 = tools.iter().any(|t| {
        matches!(
            t.get("type").and_then(|v| v.as_str()),
            Some("computer_20251124" | "computer_20251015")
        )
    });
    if has_v2 {
        return Some(ANTHROPIC_BETA_COMPUTER_USE_V2);
    }
    let has_legacy = tools
        .iter()
        .any(|t| t.get("type").and_then(|v| v.as_str()) == Some("computer_20250124"));
    if has_legacy {
        return Some(ANTHROPIC_BETA_COMPUTER_USE_LEGACY);
    }
    None
}

/// Map UI shorthand (`sonnet`, `opus`, `haiku`) to Messages API model ids.
fn normalize_claude_messages_api_model(model: &str) -> String {
    match model.trim() {
        "" => "claude-sonnet-4-20250514".into(),
        m if m.starts_with("claude-") && m.len() > 15 => m.to_string(),
        "sonnet" => "claude-sonnet-4-20250514".into(),
        "opus" => "claude-opus-4-20250514".into(),
        "haiku" => "claude-haiku-4-20250414".into(),
        other => other.to_string(),
    }
}

// ════════════════════════════════════════════════════════════════════
//  ClaudeProvider
// ════════════════════════════════════════════════════════════════════

/// Anthropic Claude API adapter.
///
/// Talks to `https://api.anthropic.com/v1` (or a custom base URL).
pub struct ClaudeProvider {
    base_url: String,
    api_key: String,
    client: Client,
}

impl ClaudeProvider {
    /// Create a provider with the given API key.
    pub fn new(api_key: impl Into<String>) -> Self {
        Self::with_base_url(api_key, "https://api.anthropic.com/v1")
    }

    /// Create a provider with a custom base URL (e.g. for proxies).
    pub fn with_base_url(api_key: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            base_url: url.into().trim_end_matches('/').to_string(),
            api_key: api_key.into(),
            client: Client::new(),
        }
    }
}

// ════════════════════════════════════════════════════════════════════
//  LlmPort implementation
// ════════════════════════════════════════════════════════════════════

#[async_trait]
impl LlmPort for ClaudeProvider {
    async fn generate(&self, request: LlmRequest) -> rusvel_core::error::Result<LlmResponse> {
        let claude_req = to_claude_request(&request);
        let url = format!("{}/messages", self.base_url);

        debug!(model = %request.model.model, "claude generate");

        let mut req = self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json");
        // Prompt caching beta — always enabled when system blocks carry cache_control.
        if claude_req.system.is_some() {
            req = req.header("anthropic-beta", ANTHROPIC_BETA_PROMPT_CACHING);
        }
        if let Some(beta) = computer_use_beta_header(&request.tools) {
            req = req.header("anthropic-beta", beta);
        }
        let http_resp = req
            .json(&claude_req)
            .send()
            .await
            .map_err(map_reqwest_error)?;

        let status = http_resp.status();
        if !status.is_success() {
            let body = http_resp.text().await.unwrap_or_default();
            return Err(map_claude_http_error(status.as_u16(), &body));
        }

        let claude_resp: ClaudeResponse = http_resp.json().await.map_err(map_reqwest_error)?;

        Ok(from_claude_response(claude_resp))
    }

    async fn stream(
        &self,
        request: LlmRequest,
    ) -> Result<tokio::sync::mpsc::Receiver<LlmStreamEvent>> {
        let claude_req = to_claude_request(&request);
        let url = format!("{}/messages", self.base_url);
        let mut body = serde_json::to_value(&claude_req)
            .map_err(|e| RusvelError::Llm(format!("claude stream body: {e}")))?;
        if let Some(obj) = body.as_object_mut() {
            obj.insert("stream".into(), serde_json::json!(true));
        }
        let api_key = self.api_key.clone();
        let client = self.client.clone();
        let beta = computer_use_beta_header(&request.tools).map(str::to_string);

        let (tx, rx) = tokio::sync::mpsc::channel(128);

        tokio::spawn(async move {
            let mut req = client
                .post(&url)
                .header("x-api-key", &api_key)
                .header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json")
                .header("anthropic-beta", ANTHROPIC_BETA_PROMPT_CACHING);
            if let Some(ref b) = beta {
                req = req.header("anthropic-beta", b);
            }
            let http_resp = match req.json(&body).send().await {
                Ok(r) => r,
                Err(e) => {
                    let _ = tx.send(LlmStreamEvent::Error(e.to_string())).await.is_ok();
                    return;
                }
            };
            let status = http_resp.status();
            if !status.is_success() {
                let body_txt = http_resp.text().await.unwrap_or_default();
                let _ = tx
                    .send(LlmStreamEvent::Error(format!(
                        "HTTP {}: {body_txt}",
                        status.as_u16()
                    )))
                    .await
                    .is_ok();
                return;
            }

            let mut full_text = String::new();
            let mut input_tokens: u32 = 0;
            let mut output_tokens: u32 = 0;
            let mut buf = String::new();
            let mut stream = http_resp.bytes_stream();

            while let Some(chunk) = stream.next().await {
                let chunk = match chunk {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = tx.send(LlmStreamEvent::Error(e.to_string())).await;
                        return;
                    }
                };
                buf.push_str(&String::from_utf8_lossy(&chunk));
                while let Some(pos) = buf.find('\n') {
                    let line = buf[..pos].trim_end_matches('\r').to_string();
                    buf.drain(..=pos);
                    let line = line.trim();
                    if let Some(data) = line.strip_prefix("data:") {
                        let data = data.trim();
                        if data.is_empty() {
                            continue;
                        }
                        let Ok(ev) = serde_json::from_str::<serde_json::Value>(data) else {
                            continue;
                        };
                        let typ = ev.get("type").and_then(|t| t.as_str());
                        if typ == Some("content_block_delta") {
                            if let Some(delta) = ev.get("delta") {
                                if delta.get("type").and_then(|t| t.as_str()) == Some("text_delta")
                                {
                                    if let Some(text) = delta.get("text").and_then(|t| t.as_str()) {
                                        full_text.push_str(text);
                                        let _ =
                                            tx.send(LlmStreamEvent::Delta(text.to_string())).await;
                                    }
                                }
                            }
                        } else if typ == Some("message_delta") {
                            if let Some(u) = ev.get("usage") {
                                if let Some(ot) = u.get("output_tokens").and_then(|v| v.as_u64()) {
                                    output_tokens = ot as u32;
                                }
                            }
                        } else if typ == Some("message_start") {
                            if let Some(u) = ev.get("message").and_then(|m| m.get("usage")) {
                                if let Some(it) = u.get("input_tokens").and_then(|v| v.as_u64()) {
                                    input_tokens = it as u32;
                                }
                            }
                        }
                    }
                }
            }

            let done = LlmResponse {
                content: Content::text(&full_text),
                finish_reason: FinishReason::Stop,
                usage: LlmUsage {
                    input_tokens,
                    output_tokens,
                },
                metadata: serde_json::json!({}),
            };
            let _ = tx.send(LlmStreamEvent::Done(done)).await;
        });

        Ok(rx)
    }

    async fn embed(&self, _model: &ModelRef, _text: &str) -> rusvel_core::error::Result<Vec<f32>> {
        Err(RusvelError::Llm(
            "Claude does not support embeddings — use an embedding-capable provider".into(),
        ))
    }

    async fn list_models(&self) -> rusvel_core::error::Result<Vec<ModelRef>> {
        Ok(vec![
            model_ref("claude-sonnet-4-20250514"),
            model_ref("claude-haiku-4-20250414"),
            model_ref("claude-opus-4-20250514"),
            model_ref("claude-3-5-sonnet-20241022"),
            model_ref("claude-3-5-haiku-20241022"),
        ])
    }

    async fn submit_batch(&self, batch: LlmBatchRequest) -> Result<LlmBatchSubmitResult> {
        submit_message_batch(self, batch).await
    }

    async fn poll_batch(&self, handle: &BatchHandle) -> Result<LlmBatchPollResult> {
        poll_message_batch(self, handle).await
    }
}

fn model_ref(name: &str) -> ModelRef {
    ModelRef {
        provider: ModelProvider::Claude,
        model: name.into(),
    }
}

// ════════════════════════════════════════════════════════════════════
//  Claude wire types
// ════════════════════════════════════════════════════════════════════

#[derive(Serialize)]
struct ClaudeRequest {
    model: String,
    messages: Vec<ClaudeMessage>,
    max_tokens: u32,
    /// System prompt as an array of text blocks, each optionally carrying
    /// `cache_control` for Anthropic's prompt caching.
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<serde_json::Value>,
}

#[derive(Serialize, Deserialize)]
struct ClaudeMessage {
    role: String,
    content: serde_json::Value,
}

#[derive(Deserialize)]
struct ClaudeResponse {
    #[serde(default)]
    content: Vec<serde_json::Value>,
    #[serde(default)]
    stop_reason: Option<String>,
    #[serde(default)]
    usage: ClaudeUsage,
}

#[derive(Default, Deserialize)]
struct ClaudeUsage {
    #[serde(default)]
    input_tokens: u32,
    #[serde(default)]
    output_tokens: u32,
}

// ════════════════════════════════════════════════════════════════════
//  Mapping helpers
// ════════════════════════════════════════════════════════════════════

fn parse_claude_content_block(block: &serde_json::Value) -> Option<Part> {
    let ty = block.get("type").and_then(|t| t.as_str())?;
    match ty {
        "text" => {
            let text = block.get("text").and_then(|v| v.as_str())?.to_string();
            Some(Part::Text(text))
        }
        "tool_use" | "server_tool_use" => {
            let id = block.get("id").and_then(|v| v.as_str())?.to_string();
            let name = block.get("name").and_then(|v| v.as_str())?.to_string();
            let input = block
                .get("input")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            Some(Part::ToolCall {
                id,
                name,
                args: input,
            })
        }
        "image" => {
            let source = block.get("source")?;
            if source.get("type").and_then(|v| v.as_str()) != Some("base64") {
                return None;
            }
            let data = source.get("data").and_then(|v| v.as_str())?.to_string();
            let media_type = source
                .get("media_type")
                .and_then(|v| v.as_str())
                .unwrap_or("image/png")
                .to_string();
            Some(Part::ImageBase64 {
                base64: data,
                media_type,
            })
        }
        _ => None,
    }
}

fn user_content_to_claude_value(content: &Content) -> serde_json::Value {
    let blocks: Vec<serde_json::Value> = content
        .parts
        .iter()
        .filter_map(part_to_user_claude_block)
        .collect();
    if blocks.is_empty() {
        return serde_json::Value::String(extract_text(content));
    }
    if blocks.len() == 1 && content.parts.len() == 1 {
        if let Part::Text(t) = &content.parts[0] {
            return serde_json::Value::String(t.clone());
        }
    }
    serde_json::Value::Array(blocks)
}

fn part_to_user_claude_block(p: &Part) -> Option<serde_json::Value> {
    match p {
        Part::Text(t) => Some(serde_json::json!({
            "type": "text",
            "text": t
        })),
        Part::ImageBase64 { base64, media_type } => Some(serde_json::json!({
            "type": "image",
            "source": {
                "type": "base64",
                "media_type": media_type,
                "data": base64
            }
        })),
        _ => None,
    }
}

fn tool_message_to_claude_blocks(content: &Content) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    let parts = &content.parts;
    let mut i = 0;
    while i < parts.len() {
        match &parts[i] {
            Part::ToolResult {
                tool_call_id,
                content: text,
                is_error,
            } => {
                let mut j = i + 1;
                let mut imgs: Vec<(String, String)> = Vec::new();
                while j < parts.len() {
                    match &parts[j] {
                        Part::ImageBase64 { base64, media_type } => {
                            imgs.push((base64.clone(), media_type.clone()));
                            j += 1;
                        }
                        Part::ToolResult { .. } => break,
                        _ => break,
                    }
                }
                let content_val = if imgs.is_empty() {
                    serde_json::Value::String(text.clone())
                } else {
                    let mut arr = vec![serde_json::json!({
                        "type": "text",
                        "text": text.clone()
                    })];
                    for (b64, mt) in imgs {
                        arr.push(serde_json::json!({
                            "type": "image",
                            "source": {
                                "type": "base64",
                                "media_type": mt,
                                "data": b64
                            }
                        }));
                    }
                    serde_json::Value::Array(arr)
                };
                out.push(serde_json::json!({
                    "type": "tool_result",
                    "tool_use_id": tool_call_id,
                    "content": content_val,
                    "is_error": is_error,
                }));
                i = j;
            }
            _ => {
                i += 1;
            }
        }
    }
    out
}

/// Split a system prompt string at the [`SYSTEM_PROMPT_CACHE_BOUNDARY`] marker.
///
/// Returns an array of text blocks suitable for the Claude `system` field.
/// Content before the boundary gets `cache_control: { type: "ephemeral" }`;
/// content after has no cache_control (changes per session).
///
/// If the marker is absent the entire prompt is sent as a single cached block
/// (backwards-compatible — existing callers that don't insert the marker
/// still benefit from caching the whole prompt).
fn split_system_prompt(prompt: &str) -> Vec<serde_json::Value> {
    if let Some(idx) = prompt.find(SYSTEM_PROMPT_CACHE_BOUNDARY) {
        let static_part = prompt[..idx].trim();
        let dynamic_part = prompt[idx + SYSTEM_PROMPT_CACHE_BOUNDARY.len()..].trim();

        let mut blocks = Vec::with_capacity(2);
        if !static_part.is_empty() {
            blocks.push(serde_json::json!({
                "type": "text",
                "text": static_part,
                "cache_control": { "type": "ephemeral" }
            }));
        }
        if !dynamic_part.is_empty() {
            blocks.push(serde_json::json!({
                "type": "text",
                "text": dynamic_part,
            }));
        }
        blocks
    } else if !prompt.is_empty() {
        // No boundary — cache the whole prompt.
        vec![serde_json::json!({
            "type": "text",
            "text": prompt,
            "cache_control": { "type": "ephemeral" }
        })]
    } else {
        vec![]
    }
}

fn to_claude_request(req: &LlmRequest) -> ClaudeRequest {
    let mut system: Option<Vec<serde_json::Value>> = None;
    let mut messages = Vec::new();

    for m in &req.messages {
        match m.role {
            LlmRole::System => {
                let text = extract_text(&m.content);
                let blocks = split_system_prompt(&text);
                if !blocks.is_empty() {
                    system = Some(blocks);
                }
            }
            LlmRole::User => messages.push(ClaudeMessage {
                role: "user".into(),
                content: user_content_to_claude_value(&m.content),
            }),
            LlmRole::Assistant => {
                // Assistant messages may contain both text and tool_use blocks.
                let blocks: Vec<serde_json::Value> = m
                    .content
                    .parts
                    .iter()
                    .filter_map(|p| match p {
                        Part::Text(t) => Some(serde_json::json!({"type": "text", "text": t})),
                        Part::ToolCall { id, name, args } => Some(serde_json::json!({
                            "type": "tool_use",
                            "id": id,
                            "name": name,
                            "input": args,
                        })),
                        Part::ImageBase64 { base64, media_type } => Some(serde_json::json!({
                            "type": "image",
                            "source": {
                                "type": "base64",
                                "media_type": media_type,
                                "data": base64
                            }
                        })),
                        _ => None,
                    })
                    .collect();

                messages.push(ClaudeMessage {
                    role: "assistant".into(),
                    content: serde_json::Value::Array(blocks),
                });
            }
            LlmRole::Tool => {
                let blocks = tool_message_to_claude_blocks(&m.content);
                if blocks.is_empty() {
                    messages.push(ClaudeMessage {
                        role: "user".into(),
                        content: serde_json::json!([{
                            "type": "tool_result",
                            "tool_use_id": "unknown",
                            "content": extract_text(&m.content),
                        }]),
                    });
                } else {
                    messages.push(ClaudeMessage {
                        role: "user".into(),
                        content: serde_json::Value::Array(blocks),
                    });
                }
            }
        }
    }

    ClaudeRequest {
        model: normalize_claude_messages_api_model(&req.model.model),
        messages,
        max_tokens: req.max_tokens.unwrap_or(4096),
        system,
        temperature: req.temperature,
        tools: req.tools.clone(),
    }
}

fn from_claude_response(resp: ClaudeResponse) -> LlmResponse {
    let mut parts = Vec::new();

    for block in &resp.content {
        if let Some(p) = parse_claude_content_block(block) {
            parts.push(p);
        }
    }

    let finish_reason = match resp.stop_reason.as_deref() {
        Some("end_turn" | "stop") => FinishReason::Stop,
        Some("max_tokens") => FinishReason::Length,
        Some("tool_use") => FinishReason::ToolUse,
        Some(other) => FinishReason::Other(other.into()),
        None => FinishReason::Other("unknown".into()),
    };

    LlmResponse {
        content: Content { parts },
        finish_reason,
        usage: LlmUsage {
            input_tokens: resp.usage.input_tokens,
            output_tokens: resp.usage.output_tokens,
        },
        metadata: serde_json::json!({}),
    }
}

/// Extract concatenated text from all `Part::Text` parts.
fn extract_text(content: &Content) -> String {
    content
        .parts
        .iter()
        .filter_map(|p| match p {
            Part::Text(t) => Some(t.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

// ════════════════════════════════════════════════════════════════════
//  Message Batches API (async, discounted)
// ════════════════════════════════════════════════════════════════════

const CLAUDE_BATCH_MAX_ITEMS: usize = 500;
const ANTHROPIC_BATCH_BETA: &str = "message-batches-2024-09-24";

#[derive(Serialize)]
struct BatchCreateBody {
    requests: Vec<BatchRequestRow>,
}

#[derive(Serialize)]
struct BatchRequestRow {
    custom_id: String,
    params: serde_json::Value,
}

#[derive(Deserialize)]
struct MessageBatchRetrieve {
    id: String,
    processing_status: String,
    #[serde(default)]
    results_url: Option<String>,
}

fn apply_anthropic_headers(
    provider: &ClaudeProvider,
    req: reqwest::RequestBuilder,
) -> reqwest::RequestBuilder {
    req.header("x-api-key", &provider.api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
}

fn apply_batch_headers(
    provider: &ClaudeProvider,
    req: reqwest::RequestBuilder,
) -> reqwest::RequestBuilder {
    apply_anthropic_headers(provider, req).header("anthropic-beta", ANTHROPIC_BATCH_BETA)
}

async fn submit_message_batch(
    provider: &ClaudeProvider,
    batch: LlmBatchRequest,
) -> Result<LlmBatchSubmitResult> {
    if batch.items.is_empty() {
        return Err(RusvelError::Validation("batch has no items".into()));
    }
    if batch.items.len() > CLAUDE_BATCH_MAX_ITEMS {
        return Err(RusvelError::Validation(format!(
            "batch exceeds max of {CLAUDE_BATCH_MAX_ITEMS} items"
        )));
    }
    for item in &batch.items {
        if item.request.model.provider != ModelProvider::Claude {
            return Err(RusvelError::Validation(
                "Claude batch requires ModelProvider::Claude for every item".into(),
            ));
        }
    }

    let mut requests = Vec::with_capacity(batch.items.len());
    for item in &batch.items {
        let claude_req = to_claude_request(&item.request);
        let params = serde_json::to_value(&claude_req)
            .map_err(|e| RusvelError::Serialization(format!("batch params: {e}")))?;
        requests.push(BatchRequestRow {
            custom_id: item.id.clone(),
            params,
        });
    }

    let url = format!("{}/messages/batches", provider.base_url);
    debug!(url = %url, n = requests.len(), "claude submit batch");

    let req = provider.client.post(&url);
    let http_resp = apply_batch_headers(provider, req)
        .json(&BatchCreateBody { requests })
        .send()
        .await
        .map_err(map_reqwest_error)?;

    let status = http_resp.status();
    if !status.is_success() {
        let body = http_resp.text().await.unwrap_or_default();
        return Err(map_claude_http_error(status.as_u16(), &body));
    }

    let created: MessageBatchRetrieve = http_resp.json().await.map_err(map_reqwest_error)?;
    Ok(LlmBatchSubmitResult {
        handle: BatchHandle {
            provider: ModelProvider::Claude,
            id: created.id,
        },
        metadata: serde_json::json!({}),
    })
}

async fn poll_message_batch(
    provider: &ClaudeProvider,
    handle: &BatchHandle,
) -> Result<LlmBatchPollResult> {
    if handle.provider != ModelProvider::Claude {
        return Err(RusvelError::Llm(
            "batch handle is not for Claude provider".into(),
        ));
    }

    let url = format!("{}/messages/batches/{}", provider.base_url, handle.id);
    let req = provider.client.get(&url);
    let http_resp = apply_batch_headers(provider, req)
        .send()
        .await
        .map_err(map_reqwest_error)?;
    let status = http_resp.status();
    if !status.is_success() {
        let body = http_resp.text().await.unwrap_or_default();
        return Err(map_claude_http_error(status.as_u16(), &body));
    }

    let batch: MessageBatchRetrieve = http_resp.json().await.map_err(map_reqwest_error)?;

    match batch.processing_status.as_str() {
        "in_progress" => Ok(LlmBatchPollResult {
            status: BatchJobStatus::InProgress,
            items: vec![],
            metadata: serde_json::json!({ "batch_id": batch.id }),
        }),
        "canceling" => Ok(LlmBatchPollResult {
            status: BatchJobStatus::Canceling,
            items: vec![],
            metadata: serde_json::json!({ "batch_id": batch.id }),
        }),
        "ended" => {
            let Some(results_url) = batch.results_url else {
                return Ok(LlmBatchPollResult {
                    status: BatchJobStatus::Ended,
                    items: vec![],
                    metadata: serde_json::json!({
                        "batch_id": batch.id,
                        "note": "no results_url yet",
                    }),
                });
            };
            fetch_batch_results_jsonl(provider, &results_url).await
        }
        other => Err(RusvelError::Llm(format!(
            "unknown batch processing_status: {other}"
        ))),
    }
}

async fn fetch_batch_results_jsonl(
    provider: &ClaudeProvider,
    results_url: &str,
) -> Result<LlmBatchPollResult> {
    // Presigned `results_url` must be fetched without Anthropic auth headers.
    let http_resp = provider
        .client
        .get(results_url)
        .send()
        .await
        .map_err(map_reqwest_error)?;
    let status = http_resp.status();
    if !status.is_success() {
        let body = http_resp.text().await.unwrap_or_default();
        return Err(RusvelError::Llm(format!(
            "batch results fetch HTTP {}: {}",
            status.as_u16(),
            body
        )));
    }

    let text = http_resp.text().await.map_err(map_reqwest_error)?;
    let mut items = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: serde_json::Value = serde_json::from_str(line)
            .map_err(|e| RusvelError::Llm(format!("batch jsonl: {e}")))?;
        let custom_id = v
            .get("custom_id")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let result = v.get("result");
        let Some(result) = result else {
            continue;
        };
        let ty = result.get("type").and_then(|x| x.as_str()).unwrap_or("");
        match ty {
            "succeeded" => {
                let msg = result
                    .get("message")
                    .cloned()
                    .ok_or_else(|| RusvelError::Llm("batch line missing message".into()))?;
                let mut llm = message_value_to_llm_response(&msg)?;
                let model = msg
                    .get("model")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string();
                let mut meta = serde_json::Map::new();
                meta.insert(RUSVEL_META_BATCH.to_string(), serde_json::json!(true));
                meta.insert(
                    RUSVEL_META_BATCH_DISCOUNT.to_string(),
                    serde_json::json!(LLM_BATCH_COST_MULTIPLIER),
                );
                meta.insert(
                    RUSVEL_META_COST_MODEL.to_string(),
                    serde_json::json!(&model),
                );
                meta.insert(
                    RUSVEL_META_COST_PROVIDER.to_string(),
                    serde_json::json!("Claude"),
                );
                if let serde_json::Value::Object(m) = &mut llm.metadata {
                    m.extend(meta);
                } else {
                    llm.metadata = serde_json::Value::Object(meta);
                }
                let model_ref = ModelRef {
                    provider: ModelProvider::Claude,
                    model: model.clone(),
                };
                items.push(LlmBatchItemOutcome::ok_with_model(
                    custom_id, model_ref, llm,
                ));
            }
            "errored" => {
                let err = result
                    .get("error")
                    .map(|e| e.to_string())
                    .unwrap_or_else(|| "unknown batch error".into());
                items.push(LlmBatchItemOutcome::err(custom_id, err));
            }
            _ => {
                items.push(LlmBatchItemOutcome::err(
                    custom_id,
                    format!("unknown batch result type: {ty}"),
                ));
            }
        }
    }

    Ok(LlmBatchPollResult {
        status: BatchJobStatus::Ended,
        items,
        metadata: serde_json::json!({}),
    })
}

fn message_value_to_llm_response(msg: &serde_json::Value) -> Result<LlmResponse> {
    let claude_resp: ClaudeResponse = serde_json::from_value(msg.clone())
        .map_err(|e| RusvelError::Llm(format!("batch message parse: {e}")))?;
    Ok(from_claude_response(claude_resp))
}

// ════════════════════════════════════════════════════════════════════
//  Error mapping
// ════════════════════════════════════════════════════════════════════

fn map_reqwest_error(e: reqwest::Error) -> RusvelError {
    if e.is_connect() {
        RusvelError::Llm(format!("cannot connect to Claude API: {e}"))
    } else if e.is_timeout() {
        RusvelError::Llm(format!("Claude request timed out: {e}"))
    } else {
        RusvelError::Llm(format!("Claude HTTP error: {e}"))
    }
}

fn map_claude_http_error(status: u16, body: &str) -> RusvelError {
    match status {
        401 => RusvelError::Unauthorized("invalid or missing Claude API key".into()),
        404 => RusvelError::NotFound {
            kind: "model".into(),
            id: body.to_string(),
        },
        429 => RusvelError::Llm("Claude rate limit exceeded — retry later".into()),
        529 => RusvelError::Llm("Claude API overloaded — retry later".into()),
        _ => RusvelError::Llm(format!("Claude returned HTTP {status}: {body}")),
    }
}

// ════════════════════════════════════════════════════════════════════
//  Tests
// ════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_request() -> LlmRequest {
        LlmRequest {
            model: ModelRef {
                provider: ModelProvider::Claude,
                model: "claude-sonnet-4-20250514".into(),
            },
            messages: vec![
                LlmMessage {
                    role: LlmRole::System,
                    content: Content::text("You are helpful."),
                },
                LlmMessage {
                    role: LlmRole::User,
                    content: Content::text("Hello!"),
                },
            ],
            tools: vec![],
            temperature: Some(0.7),
            max_tokens: Some(1024),
            metadata: serde_json::json!({}),
        }
    }

    #[test]
    fn to_claude_request_extracts_system() {
        let req = sample_request();
        let wire = to_claude_request(&req);
        // System is now an array of text blocks with cache_control.
        let blocks = wire.system.expect("system should be set");
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0]["text"], "You are helpful.");
        assert_eq!(blocks[0]["cache_control"]["type"], "ephemeral");
        // System message should NOT appear in the messages array
        assert_eq!(wire.messages.len(), 1);
        assert_eq!(wire.messages[0].role, "user");
    }

    #[test]
    fn split_system_prompt_with_boundary() {
        let prompt = format!(
            "You are a helpful assistant.\n{}\nSession context here.",
            SYSTEM_PROMPT_CACHE_BOUNDARY
        );
        let blocks = split_system_prompt(&prompt);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0]["text"], "You are a helpful assistant.");
        assert!(blocks[0].get("cache_control").is_some());
        assert_eq!(blocks[1]["text"], "Session context here.");
        assert!(blocks[1].get("cache_control").is_none());
    }

    #[test]
    fn split_system_prompt_without_boundary() {
        let blocks = split_system_prompt("Just a plain prompt.");
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0]["text"], "Just a plain prompt.");
        assert!(blocks[0].get("cache_control").is_some());
    }

    #[test]
    fn to_claude_request_sets_max_tokens() {
        let req = sample_request();
        let wire = to_claude_request(&req);
        assert_eq!(wire.max_tokens, 1024);
    }

    #[test]
    fn to_claude_request_default_max_tokens() {
        let mut req = sample_request();
        req.max_tokens = None;
        let wire = to_claude_request(&req);
        assert_eq!(wire.max_tokens, 4096);
    }

    #[test]
    fn from_claude_response_text() {
        let resp = ClaudeResponse {
            content: vec![serde_json::json!({
                "type": "text",
                "text": "Hi there!"
            })],
            stop_reason: Some("end_turn".into()),
            usage: ClaudeUsage {
                input_tokens: 10,
                output_tokens: 5,
            },
        };
        let llm_resp = from_claude_response(resp);
        assert_eq!(llm_resp.finish_reason, FinishReason::Stop);
        assert_eq!(llm_resp.usage.input_tokens, 10);
        match &llm_resp.content.parts[0] {
            Part::Text(t) => assert_eq!(t, "Hi there!"),
            _ => panic!("expected text"),
        }
    }

    #[test]
    fn from_claude_response_tool_use() {
        let resp = ClaudeResponse {
            content: vec![serde_json::json!({
                "type": "tool_use",
                "id": "call_1",
                "name": "get_weather",
                "input": {"city": "London"}
            })],
            stop_reason: Some("tool_use".into()),
            usage: ClaudeUsage::default(),
        };
        let llm_resp = from_claude_response(resp);
        assert_eq!(llm_resp.finish_reason, FinishReason::ToolUse);
        // Verify Part::ToolCall is emitted (not Part::Text).
        match &llm_resp.content.parts[0] {
            Part::ToolCall { id, name, args } => {
                assert_eq!(id, "call_1");
                assert_eq!(name, "get_weather");
                assert_eq!(args, &serde_json::json!({"city": "London"}));
            }
            other => panic!("expected ToolCall, got: {other:?}"),
        }
    }

    #[test]
    fn from_claude_response_image_base64() {
        let resp = ClaudeResponse {
            content: vec![serde_json::json!({
                "type": "image",
                "source": {
                    "type": "base64",
                    "media_type": "image/png",
                    "data": "Zm9v"
                }
            })],
            stop_reason: Some("end_turn".into()),
            usage: ClaudeUsage::default(),
        };
        let llm_resp = from_claude_response(resp);
        match &llm_resp.content.parts[0] {
            Part::ImageBase64 { base64, media_type } => {
                assert_eq!(base64, "Zm9v");
                assert_eq!(media_type, "image/png");
            }
            other => panic!("expected ImageBase64, got: {other:?}"),
        }
    }

    #[test]
    fn to_claude_request_computer_tool_triggers_beta_scan() {
        assert!(computer_use_beta_header(&[]).is_none());
        assert!(
            computer_use_beta_header(&[serde_json::json!({
                "type": "computer_20250124",
                "name": "computer",
                "display_width_px": 1024,
                "display_height_px": 768
            })])
            .is_some()
        );
    }

    #[test]
    fn to_claude_request_tool_result_merges_image_parts() {
        let req = LlmRequest {
            model: ModelRef {
                provider: ModelProvider::Claude,
                model: "claude-sonnet-4-20250514".into(),
            },
            messages: vec![LlmMessage {
                role: LlmRole::Tool,
                content: Content {
                    parts: vec![
                        Part::ToolResult {
                            tool_call_id: "tu_1".into(),
                            content: "ok".into(),
                            is_error: false,
                        },
                        Part::ImageBase64 {
                            base64: "YmFy".into(),
                            media_type: "image/png".into(),
                        },
                    ],
                },
            }],
            tools: vec![],
            temperature: None,
            max_tokens: Some(1024),
            metadata: serde_json::json!({}),
        };
        let wire = to_claude_request(&req);
        let msg = &wire.messages[0];
        let arr = msg.content.as_array().expect("array");
        assert_eq!(arr[0]["type"], "tool_result");
        assert!(arr[0]["content"].is_array());
        let blocks = arr[0]["content"].as_array().unwrap();
        assert_eq!(blocks[0]["type"], "text");
        assert_eq!(blocks[1]["type"], "image");
        assert_eq!(blocks[1]["source"]["data"], "YmFy");
    }

    #[test]
    fn map_claude_http_error_401() {
        let err = map_claude_http_error(401, "{}");
        assert!(matches!(err, RusvelError::Unauthorized(_)));
    }

    #[test]
    fn map_claude_http_error_429() {
        let err = map_claude_http_error(429, "{}");
        match err {
            RusvelError::Llm(msg) => assert!(msg.contains("rate limit")),
            other => panic!("expected Llm, got: {other}"),
        }
    }

    #[test]
    fn list_models_returns_known_models() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let provider = ClaudeProvider::new("test-key");
        let models = rt.block_on(provider.list_models()).unwrap();
        assert!(models.len() >= 3);
        assert!(models.iter().all(|m| m.provider == ModelProvider::Claude));
    }

    #[test]
    fn batch_fixture_message_maps_to_response() {
        let json = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/batch_succeeded.json"
        ));
        let v: serde_json::Value = serde_json::from_str(json).unwrap();
        let msg = v["result"]["message"].clone();
        let llm = message_value_to_llm_response(&msg).unwrap();
        assert_eq!(llm.usage.input_tokens, 100);
        match &llm.content.parts[0] {
            Part::Text(t) => assert!(t.contains("batch")),
            _ => panic!("expected text"),
        }
    }
}
