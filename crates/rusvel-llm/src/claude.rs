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
        "" => "claude-sonnet-5".into(),
        "sonnet" => "claude-sonnet-5".into(),
        "opus" => "claude-opus-5".into(),
        "haiku" => "claude-haiku-4-5".into(),
        m if m.starts_with("claude-") => m.to_string(),
        other => other.to_string(),
    }
}

/// Models that reject sampling params (`temperature`, `top_p`, `top_k`) with HTTP 400.
///
/// Opus 5 rejects them entirely; Sonnet 5 rejects non-default values. Older
/// models (Haiku 4.5 and earlier) still accept them.
fn model_rejects_sampling_params(model: &str) -> bool {
    model.starts_with("claude-opus-5")
        || model.starts_with("claude-sonnet-5")
        || model.starts_with("claude-fable")
}

/// Request metadata key: set to `true` (or `"adaptive"`) to send `thinking: {"type": "adaptive"}`.
///
/// Opus 5 / Sonnet 5 default to adaptive thinking anyway, so this is only
/// needed to be explicit. `budget_tokens` is not supported (HTTP 400 on
/// current models).
pub const CLAUDE_META_THINKING: &str = "claude.thinking";

/// Request metadata key: effort level for `output_config` (`low`|`medium`|`high`|`xhigh`|`max`).
pub const CLAUDE_META_EFFORT: &str = "claude.effort";

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

            let mut acc = ClaudeStreamAccumulator::default();
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
                        for out in acc.handle_event(&ev) {
                            if tx.send(out).await.is_err() {
                                return;
                            }
                        }
                    }
                }
            }

            let _ = tx.send(LlmStreamEvent::Done(acc.into_response())).await;
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
            model_ref("claude-opus-5"),
            model_ref("claude-sonnet-5"),
            model_ref("claude-haiku-4-5"),
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

/// Accumulates token usage across streaming SSE events.
#[derive(Debug, Default, Clone, Copy)]
#[allow(clippy::struct_field_names)] // field names mirror the wire format
struct StreamUsageAcc {
    input_tokens: u32,
    output_tokens: u32,
    cache_creation_input_tokens: u64,
    cache_read_input_tokens: u64,
}

impl StreamUsageAcc {
    /// Fold usage from one streaming event.
    ///
    /// Anthropic-native streams report input/cache tokens on `message_start`
    /// and cumulative output tokens on `message_delta`. Some gateways (e.g.
    /// AvalAI) report zeros on `message_start` and the real totals in the
    /// final `message_delta` usage instead — so any later nonzero value wins,
    /// while zeros never overwrite an earlier real count.
    fn apply_event(&mut self, ev: &serde_json::Value) {
        let usage = match ev.get("type").and_then(|t| t.as_str()) {
            Some("message_start") => ev.pointer("/message/usage"),
            Some("message_delta") => ev.get("usage"),
            _ => None,
        };
        let Some(u) = usage else { return };
        let get = |key: &str| {
            u.get(key)
                .and_then(serde_json::Value::as_u64)
                .filter(|&n| n > 0)
        };
        if let Some(it) = get("input_tokens") {
            self.input_tokens = it as u32;
        }
        if let Some(ot) = get("output_tokens") {
            self.output_tokens = ot as u32;
        }
        if let Some(cc) = get("cache_creation_input_tokens") {
            self.cache_creation_input_tokens = cc;
        }
        if let Some(cr) = get("cache_read_input_tokens") {
            self.cache_read_input_tokens = cr;
        }
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
    /// `{"type": "adaptive"}` when enabled — `budget_tokens` is rejected on current models.
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<serde_json::Value>,
    /// `{"effort": "low"|"medium"|"high"|"xhigh"|"max"}` when set.
    #[serde(skip_serializing_if = "Option::is_none")]
    output_config: Option<serde_json::Value>,
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
#[allow(clippy::struct_field_names)]
struct ClaudeUsage {
    #[serde(default)]
    input_tokens: u32,
    #[serde(default)]
    output_tokens: u32,
    /// Tokens written to the prompt cache (~1.25x input price).
    #[serde(default)]
    cache_creation_input_tokens: u64,
    /// Tokens served from the prompt cache (~0.1x input price).
    #[serde(default)]
    cache_read_input_tokens: u64,
}

// ════════════════════════════════════════════════════════════════════
//  Streaming accumulator
// ════════════════════════════════════════════════════════════════════

/// One in-flight content block from a Claude SSE stream, keyed by block index.
enum StreamBlock {
    Text(String),
    ToolUse {
        id: String,
        name: String,
        partial_json: String,
    },
    /// Blocks we do not surface (e.g. `thinking`).
    Ignored,
}

/// Accumulates Claude streaming SSE events into [`LlmStreamEvent`]s and a
/// final [`LlmResponse`].
///
/// Tracks content blocks by index (`text` and `tool_use`), the terminal
/// `stop_reason` from `message_delta`, and usage counters. Historically the
/// stream path dropped `tool_use` blocks and hardcoded
/// [`FinishReason::Stop`], which made agent runs that ended in a tool call
/// (or truncation) look like clean empty completions (#12).
#[derive(Default)]
struct ClaudeStreamAccumulator {
    blocks: std::collections::BTreeMap<u64, StreamBlock>,
    stop_reason: Option<String>,
    /// Usage folded from `message_start` + `message_delta` (nonzero-wins —
    /// gateways like AvalAI report zeros early and real totals late).
    usage: StreamUsageAcc,
}

impl ClaudeStreamAccumulator {
    /// Process one parsed SSE `data:` JSON event, returning events to forward.
    fn handle_event(&mut self, ev: &serde_json::Value) -> Vec<LlmStreamEvent> {
        let mut out = Vec::new();
        let index = ev.get("index").and_then(|v| v.as_u64()).unwrap_or(0);
        match ev.get("type").and_then(|t| t.as_str()) {
            Some("content_block_start") => {
                let block = ev.get("content_block");
                let ty = block
                    .and_then(|b| b.get("type"))
                    .and_then(|t| t.as_str())
                    .unwrap_or("");
                let entry = match ty {
                    "text" => StreamBlock::Text(
                        block
                            .and_then(|b| b.get("text"))
                            .and_then(|t| t.as_str())
                            .unwrap_or("")
                            .to_string(),
                    ),
                    "tool_use" | "server_tool_use" => StreamBlock::ToolUse {
                        id: block
                            .and_then(|b| b.get("id"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        name: block
                            .and_then(|b| b.get("name"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        partial_json: String::new(),
                    },
                    _ => StreamBlock::Ignored,
                };
                self.blocks.insert(index, entry);
            }
            Some("content_block_delta") => {
                let delta = ev.get("delta");
                match delta.and_then(|d| d.get("type")).and_then(|t| t.as_str()) {
                    Some("text_delta") => {
                        if let Some(text) =
                            delta.and_then(|d| d.get("text")).and_then(|t| t.as_str())
                        {
                            match self
                                .blocks
                                .entry(index)
                                .or_insert_with(|| StreamBlock::Text(String::new()))
                            {
                                StreamBlock::Text(t) => t.push_str(text),
                                _ => {}
                            }
                            out.push(LlmStreamEvent::Delta(text.to_string()));
                        }
                    }
                    Some("input_json_delta") => {
                        if let Some(pj) = delta
                            .and_then(|d| d.get("partial_json"))
                            .and_then(|t| t.as_str())
                        {
                            if let Some(StreamBlock::ToolUse { partial_json, .. }) =
                                self.blocks.get_mut(&index)
                            {
                                partial_json.push_str(pj);
                            }
                        }
                    }
                    _ => {}
                }
            }
            Some("content_block_stop") => {
                if let Some(StreamBlock::ToolUse {
                    id,
                    name,
                    partial_json,
                }) = self.blocks.get(&index)
                {
                    out.push(LlmStreamEvent::ToolUse {
                        id: id.clone(),
                        name: name.clone(),
                        args: parse_tool_input_json(partial_json),
                    });
                }
            }
            Some("message_delta") => {
                if let Some(sr) = ev
                    .get("delta")
                    .and_then(|d| d.get("stop_reason"))
                    .and_then(|v| v.as_str())
                {
                    self.stop_reason = Some(sr.to_string());
                }
                self.usage.apply_event(ev);
            }
            Some("message_start") => {
                self.usage.apply_event(ev);
            }
            _ => {}
        }
        out
    }

    /// Build the aggregated terminal [`LlmResponse`].
    fn into_response(self) -> LlmResponse {
        let mut parts = Vec::new();
        let mut has_tool_call = false;
        for (_, block) in self.blocks {
            match block {
                StreamBlock::Text(t) => {
                    if !t.is_empty() {
                        parts.push(Part::Text(t));
                    }
                }
                StreamBlock::ToolUse {
                    id,
                    name,
                    partial_json,
                } => {
                    has_tool_call = true;
                    parts.push(Part::ToolCall {
                        id,
                        name,
                        args: parse_tool_input_json(&partial_json),
                    });
                }
                StreamBlock::Ignored => {}
            }
        }
        // A missing stop_reason (e.g. truncated stream) falls back on the
        // shape of the content so tool calls are never mistaken for Stop.
        let finish_reason = match self.stop_reason.as_deref() {
            Some(sr) => finish_reason_from_stop(sr),
            None if has_tool_call => FinishReason::ToolUse,
            None => FinishReason::Stop,
        };
        LlmResponse {
            content: Content { parts },
            finish_reason,
            usage: LlmUsage {
                input_tokens: self.usage.input_tokens,
                output_tokens: self.usage.output_tokens,
            },
            metadata: serde_json::json!({
                "cache_creation_input_tokens": self.usage.cache_creation_input_tokens,
                "cache_read_input_tokens": self.usage.cache_read_input_tokens,
            }),
        }
    }
}

/// Parse accumulated `input_json_delta` fragments; empty input means `{}`.
fn parse_tool_input_json(partial_json: &str) -> serde_json::Value {
    if partial_json.trim().is_empty() {
        return serde_json::json!({});
    }
    serde_json::from_str(partial_json).unwrap_or_else(|_| serde_json::json!({}))
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

    let model = normalize_claude_messages_api_model(&req.model.model);
    let temperature = if model_rejects_sampling_params(&model) {
        None
    } else {
        req.temperature
    };
    let thinking = match req.metadata.get(CLAUDE_META_THINKING) {
        Some(serde_json::Value::Bool(true)) => Some(serde_json::json!({"type": "adaptive"})),
        Some(serde_json::Value::String(s)) if s == "adaptive" => {
            Some(serde_json::json!({"type": "adaptive"}))
        }
        _ => None,
    };
    let output_config = req
        .metadata
        .get(CLAUDE_META_EFFORT)
        .and_then(|v| v.as_str())
        .filter(|e| matches!(*e, "low" | "medium" | "high" | "xhigh" | "max"))
        .map(|e| serde_json::json!({"effort": e}));

    ClaudeRequest {
        model,
        messages,
        max_tokens: req.max_tokens.unwrap_or(8192),
        system,
        temperature,
        thinking,
        output_config,
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
        Some(sr) => finish_reason_from_stop(sr),
        None => FinishReason::Other("unknown".into()),
    };

    LlmResponse {
        content: Content { parts },
        finish_reason,
        usage: LlmUsage {
            input_tokens: resp.usage.input_tokens,
            output_tokens: resp.usage.output_tokens,
        },
        metadata: serde_json::json!({
            "cache_creation_input_tokens": resp.usage.cache_creation_input_tokens,
            "cache_read_input_tokens": resp.usage.cache_read_input_tokens,
        }),
    }
}

/// Map a Claude `stop_reason` string to a [`FinishReason`].
fn finish_reason_from_stop(stop_reason: &str) -> FinishReason {
    match stop_reason {
        "end_turn" | "stop" => FinishReason::Stop,
        "max_tokens" => FinishReason::Length,
        "tool_use" => FinishReason::ToolUse,
        other => FinishReason::Other(other.into()),
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
                model: "claude-sonnet-5".into(),
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
    fn normalize_model_maps_shorthands_and_passthrough() {
        assert_eq!(normalize_claude_messages_api_model(""), "claude-sonnet-5");
        assert_eq!(
            normalize_claude_messages_api_model("sonnet"),
            "claude-sonnet-5"
        );
        assert_eq!(normalize_claude_messages_api_model("opus"), "claude-opus-5");
        assert_eq!(
            normalize_claude_messages_api_model("haiku"),
            "claude-haiku-4-5"
        );
        // Short current ids must pass through unmangled.
        assert_eq!(
            normalize_claude_messages_api_model("claude-opus-5"),
            "claude-opus-5"
        );
        assert_eq!(
            normalize_claude_messages_api_model("claude-haiku-4-5"),
            "claude-haiku-4-5"
        );
    }

    #[test]
    fn to_claude_request_strips_sampling_params_for_current_models() {
        // Sonnet 5 / Opus 5 reject temperature with HTTP 400 — must be omitted.
        let req = sample_request();
        assert_eq!(req.temperature, Some(0.7));
        let wire = to_claude_request(&req);
        assert!(wire.temperature.is_none());
    }

    #[test]
    fn to_claude_request_keeps_temperature_for_older_models() {
        let mut req = sample_request();
        req.model.model = "claude-haiku-4-5".into();
        let wire = to_claude_request(&req);
        assert_eq!(wire.temperature, Some(0.7));
    }

    #[test]
    fn to_claude_request_thinking_and_effort_from_metadata() {
        let mut req = sample_request();
        req.metadata = serde_json::json!({
            CLAUDE_META_THINKING: true,
            CLAUDE_META_EFFORT: "high",
        });
        let wire = to_claude_request(&req);
        assert_eq!(wire.thinking, Some(serde_json::json!({"type": "adaptive"})));
        assert_eq!(
            wire.output_config,
            Some(serde_json::json!({"effort": "high"}))
        );
    }

    #[test]
    fn to_claude_request_omits_thinking_and_effort_by_default() {
        let wire = to_claude_request(&sample_request());
        assert!(wire.thinking.is_none());
        assert!(wire.output_config.is_none());
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
        // 8192 leaves headroom for thinking-enabled models.
        assert_eq!(wire.max_tokens, 8192);
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
                ..ClaudeUsage::default()
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
                model: "claude-sonnet-5".into(),
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

    // ── Streaming accumulator (#12: dept chat empty output) ──────

    fn feed(acc: &mut ClaudeStreamAccumulator, evs: &[serde_json::Value]) -> Vec<LlmStreamEvent> {
        let mut out = Vec::new();
        for ev in evs {
            out.extend(acc.handle_event(ev));
        }
        out
    }

    #[test]
    fn stream_accumulator_text_flow() {
        let mut acc = ClaudeStreamAccumulator::default();
        let emitted = feed(
            &mut acc,
            &[
                serde_json::json!({"type": "message_start", "message": {"usage": {"input_tokens": 12}}}),
                serde_json::json!({"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}}),
                serde_json::json!({"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": "Hello "}}),
                serde_json::json!({"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": "world"}}),
                serde_json::json!({"type": "content_block_stop", "index": 0}),
                serde_json::json!({"type": "message_delta", "delta": {"stop_reason": "end_turn"}, "usage": {"output_tokens": 7}}),
            ],
        );
        let deltas: Vec<&str> = emitted
            .iter()
            .filter_map(|e| match e {
                LlmStreamEvent::Delta(t) => Some(t.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(deltas, vec!["Hello ", "world"]);

        let resp = acc.into_response();
        assert_eq!(resp.finish_reason, FinishReason::Stop);
        assert_eq!(resp.usage.input_tokens, 12);
        assert_eq!(resp.usage.output_tokens, 7);
        match &resp.content.parts[0] {
            Part::Text(t) => assert_eq!(t, "Hello world"),
            other => panic!("expected text, got {other:?}"),
        }
    }

    #[test]
    fn stream_accumulator_tool_use_flow() {
        // A run that ends in a tool call must surface the ToolCall part and
        // FinishReason::ToolUse — previously it produced an empty Stop
        // response, which the agent loop reported as a clean completion.
        let mut acc = ClaudeStreamAccumulator::default();
        let emitted = feed(
            &mut acc,
            &[
                serde_json::json!({"type": "content_block_start", "index": 0, "content_block": {"type": "tool_use", "id": "toolu_1", "name": "finance_add_entry", "input": {}}}),
                serde_json::json!({"type": "content_block_delta", "index": 0, "delta": {"type": "input_json_delta", "partial_json": "{\"amount_usd\":"}}),
                serde_json::json!({"type": "content_block_delta", "index": 0, "delta": {"type": "input_json_delta", "partial_json": "50}"}}),
                serde_json::json!({"type": "content_block_stop", "index": 0}),
                serde_json::json!({"type": "message_delta", "delta": {"stop_reason": "tool_use"}, "usage": {"output_tokens": 40}}),
            ],
        );
        match emitted.last() {
            Some(LlmStreamEvent::ToolUse { id, name, args }) => {
                assert_eq!(id, "toolu_1");
                assert_eq!(name, "finance_add_entry");
                assert_eq!(args, &serde_json::json!({"amount_usd": 50}));
            }
            other => panic!("expected ToolUse event, got {other:?}"),
        }

        let resp = acc.into_response();
        assert_eq!(resp.finish_reason, FinishReason::ToolUse);
        match &resp.content.parts[0] {
            Part::ToolCall { id, name, args } => {
                assert_eq!(id, "toolu_1");
                assert_eq!(name, "finance_add_entry");
                assert_eq!(args, &serde_json::json!({"amount_usd": 50}));
            }
            other => panic!("expected ToolCall part, got {other:?}"),
        }
    }

    #[test]
    fn stream_accumulator_thinking_then_tool_use_preserves_order() {
        let mut acc = ClaudeStreamAccumulator::default();
        feed(
            &mut acc,
            &[
                serde_json::json!({"type": "content_block_start", "index": 0, "content_block": {"type": "thinking"}}),
                serde_json::json!({"type": "content_block_delta", "index": 0, "delta": {"type": "thinking_delta", "thinking": "hmm"}}),
                serde_json::json!({"type": "content_block_stop", "index": 0}),
                serde_json::json!({"type": "content_block_start", "index": 1, "content_block": {"type": "text", "text": ""}}),
                serde_json::json!({"type": "content_block_delta", "index": 1, "delta": {"type": "text_delta", "text": "Adding it now."}}),
                serde_json::json!({"type": "content_block_stop", "index": 1}),
                serde_json::json!({"type": "content_block_start", "index": 2, "content_block": {"type": "tool_use", "id": "toolu_2", "name": "t", "input": {}}}),
                serde_json::json!({"type": "content_block_stop", "index": 2}),
                serde_json::json!({"type": "message_delta", "delta": {"stop_reason": "tool_use"}}),
            ],
        );
        let resp = acc.into_response();
        assert_eq!(resp.finish_reason, FinishReason::ToolUse);
        assert_eq!(resp.content.parts.len(), 2); // thinking block dropped
        assert!(matches!(&resp.content.parts[0], Part::Text(t) if t == "Adding it now."));
        assert!(matches!(&resp.content.parts[1], Part::ToolCall { .. }));
    }

    #[test]
    fn stream_accumulator_max_tokens_maps_to_length() {
        let mut acc = ClaudeStreamAccumulator::default();
        feed(
            &mut acc,
            &[
                serde_json::json!({"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}}),
                serde_json::json!({"type": "message_delta", "delta": {"stop_reason": "max_tokens"}, "usage": {"output_tokens": 200}}),
            ],
        );
        let resp = acc.into_response();
        assert_eq!(resp.finish_reason, FinishReason::Length);
        assert!(resp.content.parts.is_empty());
    }

    #[test]
    fn stream_accumulator_missing_stop_reason_with_tool_call_is_tool_use() {
        let mut acc = ClaudeStreamAccumulator::default();
        feed(
            &mut acc,
            &[
                serde_json::json!({"type": "content_block_start", "index": 0, "content_block": {"type": "tool_use", "id": "x", "name": "y", "input": {}}}),
            ],
        );
        assert_eq!(acc.into_response().finish_reason, FinishReason::ToolUse);
    }

    #[test]
    fn stream_usage_anthropic_native_event_order() {
        // Native API: input + cache tokens on message_start, output on message_delta.
        let mut acc = StreamUsageAcc::default();
        acc.apply_event(&serde_json::json!({
            "type": "message_start",
            "message": {"usage": {"input_tokens": 25, "output_tokens": 1,
                "cache_creation_input_tokens": 7, "cache_read_input_tokens": 3}}
        }));
        acc.apply_event(&serde_json::json!({
            "type": "message_delta",
            "delta": {"stop_reason": "end_turn"},
            "usage": {"output_tokens": 13}
        }));
        assert_eq!(acc.input_tokens, 25);
        assert_eq!(acc.output_tokens, 13);
        assert_eq!(acc.cache_creation_input_tokens, 7);
        assert_eq!(acc.cache_read_input_tokens, 3);
    }

    #[test]
    fn stream_usage_gateway_reports_totals_in_message_delta() {
        // AvalAI-style gateway: zeros in message_start, real usage in the
        // final message_delta (observed live against api.avalai.ir).
        let mut acc = StreamUsageAcc::default();
        acc.apply_event(&serde_json::json!({
            "type": "message_start",
            "message": {"usage": {"input_tokens": 0, "output_tokens": 0,
                "cache_creation_input_tokens": 0, "cache_read_input_tokens": 0}}
        }));
        acc.apply_event(&serde_json::json!({
            "type": "message_delta",
            "delta": {"stop_reason": "end_turn"},
            "usage": {"input_tokens": 9, "output_tokens": 16}
        }));
        assert_eq!(acc.input_tokens, 9);
        assert_eq!(acc.output_tokens, 16);
    }

    #[test]
    fn stream_usage_zero_never_overwrites_real_count() {
        let mut acc = StreamUsageAcc::default();
        acc.apply_event(&serde_json::json!({
            "type": "message_start",
            "message": {"usage": {"input_tokens": 25}}
        }));
        acc.apply_event(&serde_json::json!({
            "type": "message_delta",
            "usage": {"input_tokens": 0, "output_tokens": 13}
        }));
        assert_eq!(acc.input_tokens, 25);
        assert_eq!(acc.output_tokens, 13);
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
