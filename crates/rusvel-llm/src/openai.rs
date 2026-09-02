//! `OpenAI` HTTP adapter implementing [`LlmPort`].

use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::debug;

use rusvel_core::domain::*;
use rusvel_core::error::{Result, RusvelError};
use rusvel_core::ports::LlmPort;

// ════════════════════════════════════════════════════════════════════
//  OpenAiProvider
// ════════════════════════════════════════════════════════════════════

/// `OpenAI` API adapter.
///
/// Talks to `https://api.openai.com/v1` (or a custom base URL for
/// Azure `OpenAI` / compatible proxies).
pub struct OpenAiProvider {
    base_url: String,
    api_key: String,
    client: Client,
}

impl OpenAiProvider {
    /// Create a provider with the given API key.
    pub fn new(api_key: impl Into<String>) -> Self {
        Self::with_base_url(api_key, "https://api.openai.com/v1")
    }

    /// Create a provider with a custom base URL.
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
impl LlmPort for OpenAiProvider {
    async fn generate(&self, request: LlmRequest) -> rusvel_core::error::Result<LlmResponse> {
        let oai_req = to_openai_request(&request);
        let url = format!("{}/chat/completions", self.base_url);

        debug!(model = %request.model.model, "openai generate");

        let http_resp = self
            .client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&oai_req)
            .send()
            .await
            .map_err(map_reqwest_error)?;

        let status = http_resp.status();
        if !status.is_success() {
            let body = http_resp.text().await.unwrap_or_default();
            return Err(map_openai_http_error(status.as_u16(), &body));
        }

        let oai_resp: OpenAiChatResponse = http_resp.json().await.map_err(map_reqwest_error)?;

        Ok(from_openai_response(oai_resp))
    }

    async fn stream(
        &self,
        request: LlmRequest,
    ) -> Result<tokio::sync::mpsc::Receiver<LlmStreamEvent>> {
        let oai_req = to_openai_request(&request);
        let mut body = serde_json::to_value(&oai_req)
            .map_err(|e| RusvelError::Llm(format!("openai stream body: {e}")))?;
        if let Some(obj) = body.as_object_mut() {
            obj.insert("stream".into(), serde_json::json!(true));
            obj.insert(
                "stream_options".into(),
                serde_json::json!({ "include_usage": true }),
            );
        }
        let url = format!("{}/chat/completions", self.base_url);
        let api_key = self.api_key.clone();
        let client = self.client.clone();

        debug!(model = %request.model.model, "openai stream");

        let (tx, rx) = tokio::sync::mpsc::channel(128);

        tokio::spawn(async move {
            let http_resp = match client
                .post(&url)
                .bearer_auth(&api_key)
                .json(&body)
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    let _ = tx
                        .send(LlmStreamEvent::Error(map_reqwest_error(e).to_string()))
                        .await;
                    return;
                }
            };

            let status = http_resp.status();
            if !status.is_success() {
                let body_txt = http_resp.text().await.unwrap_or_default();
                let _ = tx
                    .send(LlmStreamEvent::Error(
                        map_openai_http_error(status.as_u16(), &body_txt).to_string(),
                    ))
                    .await;
                return;
            }

            let mut full_text = String::new();
            let mut input_tokens: u32 = 0;
            let mut output_tokens: u32 = 0;
            let mut finish = FinishReason::Stop;
            let mut buf = String::new();
            let mut bytes = http_resp.bytes_stream();

            while let Some(chunk) = bytes.next().await {
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
                    let Some(data) = line.strip_prefix("data:") else {
                        continue;
                    };
                    let data = data.trim();
                    if data.is_empty() || data == "[DONE]" {
                        continue;
                    }
                    let Ok(ev) = serde_json::from_str::<OpenAiStreamChunk>(data) else {
                        continue;
                    };
                    if let Some(usage) = ev.usage {
                        input_tokens = usage.prompt_tokens;
                        output_tokens = usage.completion_tokens;
                    }
                    for choice in ev.choices {
                        if let Some(reason) = choice.finish_reason.as_deref() {
                            finish = match reason {
                                "stop" => FinishReason::Stop,
                                "length" => FinishReason::Length,
                                "tool_calls" | "function_call" => FinishReason::ToolUse,
                                "content_filter" => FinishReason::ContentFilter,
                                other => FinishReason::Other(other.into()),
                            };
                        }
                        if let Some(text) = choice.delta.content
                            && !text.is_empty()
                        {
                            full_text.push_str(&text);
                            let _ = tx.send(LlmStreamEvent::Delta(text)).await;
                        }
                    }
                }
            }

            let done = LlmResponse {
                content: Content::text(&full_text),
                finish_reason: finish,
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

    async fn embed(&self, model: &ModelRef, text: &str) -> rusvel_core::error::Result<Vec<f32>> {
        let url = format!("{}/embeddings", self.base_url);
        let body = serde_json::json!({
            "model": model.model,
            "input": text,
        });

        debug!(model = %model.model, "openai embed");

        let http_resp = self
            .client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(map_reqwest_error)?;

        let status = http_resp.status();
        if !status.is_success() {
            let body = http_resp.text().await.unwrap_or_default();
            return Err(map_openai_http_error(status.as_u16(), &body));
        }

        let resp: OpenAiEmbedResponse = http_resp.json().await.map_err(map_reqwest_error)?;

        resp.data
            .into_iter()
            .next()
            .map(|d| d.embedding)
            .ok_or_else(|| RusvelError::Llm("OpenAI returned empty embeddings".into()))
    }

    async fn list_models(&self) -> rusvel_core::error::Result<Vec<ModelRef>> {
        let url = format!("{}/models", self.base_url);

        debug!("openai list_models");

        let http_resp = self
            .client
            .get(&url)
            .bearer_auth(&self.api_key)
            .send()
            .await
            .map_err(map_reqwest_error)?;

        let status = http_resp.status();
        if !status.is_success() {
            let body = http_resp.text().await.unwrap_or_default();
            return Err(map_openai_http_error(status.as_u16(), &body));
        }

        let resp: OpenAiModelsResponse = http_resp.json().await.map_err(map_reqwest_error)?;

        Ok(resp
            .data
            .into_iter()
            .map(|m| ModelRef {
                provider: ModelProvider::OpenAI,
                model: m.id,
            })
            .collect())
    }
}

// ════════════════════════════════════════════════════════════════════
//  OpenAI wire types
// ════════════════════════════════════════════════════════════════════

#[derive(Serialize)]
struct OpenAiChatRequest {
    model: String,
    messages: Vec<OpenAiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<serde_json::Value>,
}

#[derive(Default, Serialize, Deserialize)]
struct OpenAiMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct OpenAiChatResponse {
    #[serde(default)]
    choices: Vec<OpenAiChoice>,
    #[serde(default)]
    usage: Option<OpenAiUsage>,
}

#[derive(Deserialize)]
struct OpenAiChoice {
    #[serde(default)]
    message: OpenAiMessage,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Default, Deserialize)]
struct OpenAiUsage {
    #[serde(default)]
    prompt_tokens: u32,
    #[serde(default)]
    completion_tokens: u32,
}

/// One SSE frame from `/v1/chat/completions?stream=true`.
#[derive(Deserialize)]
struct OpenAiStreamChunk {
    #[serde(default)]
    choices: Vec<OpenAiStreamChoice>,
    #[serde(default)]
    usage: Option<OpenAiUsage>,
}

#[derive(Deserialize)]
struct OpenAiStreamChoice {
    #[serde(default)]
    delta: OpenAiStreamDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Default, Deserialize)]
struct OpenAiStreamDelta {
    #[serde(default)]
    content: Option<String>,
}

#[derive(Deserialize)]
struct OpenAiEmbedResponse {
    #[serde(default)]
    data: Vec<OpenAiEmbedding>,
}

#[derive(Deserialize)]
struct OpenAiEmbedding {
    #[serde(default)]
    embedding: Vec<f32>,
}

#[derive(Deserialize)]
struct OpenAiModelsResponse {
    #[serde(default)]
    data: Vec<OpenAiModelInfo>,
}

#[derive(Deserialize)]
struct OpenAiModelInfo {
    id: String,
}

// ════════════════════════════════════════════════════════════════════
//  Mapping helpers
// ════════════════════════════════════════════════════════════════════

fn to_openai_request(req: &LlmRequest) -> OpenAiChatRequest {
    let messages = req
        .messages
        .iter()
        .map(|m| OpenAiMessage {
            role: match m.role {
                LlmRole::System => "system".into(),
                LlmRole::User => "user".into(),
                LlmRole::Assistant => "assistant".into(),
                LlmRole::Tool => "tool".into(),
            },
            content: extract_text(&m.content),
        })
        .collect();

    // Map tool definitions to OpenAI function-calling format.
    let tools: Vec<serde_json::Value> = req
        .tools
        .iter()
        .map(|t| {
            serde_json::json!({
                "type": "function",
                "function": t,
            })
        })
        .collect();

    OpenAiChatRequest {
        model: req.model.model.clone(),
        messages,
        temperature: req.temperature,
        max_tokens: req.max_tokens,
        tools,
    }
}

fn from_openai_response(resp: OpenAiChatResponse) -> LlmResponse {
    let choice = resp.choices.into_iter().next();

    let (text, finish_reason) = match choice {
        Some(c) => {
            let reason = match c.finish_reason.as_deref() {
                Some("stop") => FinishReason::Stop,
                Some("length") => FinishReason::Length,
                Some("tool_calls" | "function_call") => FinishReason::ToolUse,
                Some("content_filter") => FinishReason::ContentFilter,
                Some(other) => FinishReason::Other(other.into()),
                None => FinishReason::Other("unknown".into()),
            };
            (c.message.content, reason)
        }
        None => (String::new(), FinishReason::Other("no_choices".into())),
    };

    let usage = resp.usage.unwrap_or_default();

    LlmResponse {
        content: Content::text(text),
        finish_reason,
        usage: LlmUsage {
            input_tokens: usage.prompt_tokens,
            output_tokens: usage.completion_tokens,
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
//  Error mapping
// ════════════════════════════════════════════════════════════════════

fn map_reqwest_error(e: reqwest::Error) -> RusvelError {
    if e.is_connect() {
        RusvelError::Llm(format!("cannot connect to OpenAI API: {e}"))
    } else if e.is_timeout() {
        RusvelError::Llm(format!("OpenAI request timed out: {e}"))
    } else {
        RusvelError::Llm(format!("OpenAI HTTP error: {e}"))
    }
}

fn map_openai_http_error(status: u16, body: &str) -> RusvelError {
    match status {
        401 => RusvelError::Unauthorized("invalid or missing OpenAI API key".into()),
        404 => RusvelError::NotFound {
            kind: "model".into(),
            id: body.to_string(),
        },
        429 => RusvelError::Llm("OpenAI rate limit exceeded — retry later".into()),
        _ => RusvelError::Llm(format!("OpenAI returned HTTP {status}: {body}")),
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
                provider: ModelProvider::OpenAI,
                model: "gpt-4o".into(),
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
            max_tokens: Some(512),
            metadata: serde_json::json!({}),
        }
    }

    #[test]
    fn to_openai_request_maps_roles() {
        let req = sample_request();
        let wire = to_openai_request(&req);
        assert_eq!(wire.model, "gpt-4o");
        assert_eq!(wire.messages.len(), 2);
        assert_eq!(wire.messages[0].role, "system");
        assert_eq!(wire.messages[1].role, "user");
    }

    #[test]
    fn to_openai_request_wraps_tools() {
        let mut req = sample_request();
        req.tools = vec![serde_json::json!({
            "name": "get_weather",
            "parameters": {"type": "object"}
        })];
        let wire = to_openai_request(&req);
        assert_eq!(wire.tools.len(), 1);
        assert_eq!(wire.tools[0]["type"], "function");
    }

    #[test]
    fn from_openai_response_maps_stop() {
        let resp = OpenAiChatResponse {
            choices: vec![OpenAiChoice {
                message: OpenAiMessage {
                    role: "assistant".into(),
                    content: "Hi!".into(),
                },
                finish_reason: Some("stop".into()),
            }],
            usage: Some(OpenAiUsage {
                prompt_tokens: 10,
                completion_tokens: 3,
            }),
        };
        let llm_resp = from_openai_response(resp);
        assert_eq!(llm_resp.finish_reason, FinishReason::Stop);
        assert_eq!(llm_resp.usage.input_tokens, 10);
        assert_eq!(llm_resp.usage.output_tokens, 3);
    }

    #[test]
    fn from_openai_response_empty_choices() {
        let resp = OpenAiChatResponse {
            choices: vec![],
            usage: None,
        };
        let llm_resp = from_openai_response(resp);
        assert_eq!(
            llm_resp.finish_reason,
            FinishReason::Other("no_choices".into())
        );
    }

    #[test]
    fn map_openai_http_error_401() {
        let err = map_openai_http_error(401, "{}");
        assert!(matches!(err, RusvelError::Unauthorized(_)));
    }

    #[test]
    fn map_openai_http_error_429() {
        let err = map_openai_http_error(429, "{}");
        match err {
            RusvelError::Llm(msg) => assert!(msg.contains("rate limit")),
            other => panic!("expected Llm, got: {other}"),
        }
    }

    #[test]
    fn embed_response_deserialize() {
        let json =
            r#"{"data":[{"embedding":[0.1,0.2,0.3],"index":0}],"model":"text-embedding-3-small"}"#;
        let resp: OpenAiEmbedResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.data.len(), 1);
        assert_eq!(resp.data[0].embedding, vec![0.1, 0.2, 0.3]);
    }

    #[test]
    fn models_response_deserialize() {
        let json = r#"{"data":[{"id":"gpt-4o"},{"id":"gpt-4o-mini"}]}"#;
        let resp: OpenAiModelsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.data.len(), 2);
        assert_eq!(resp.data[0].id, "gpt-4o");
    }

    #[test]
    fn stream_chunk_deserialize_delta() {
        let s = r#"{"choices":[{"delta":{"content":"Hi"},"index":0,"finish_reason":null}]}"#;
        let chunk: OpenAiStreamChunk = serde_json::from_str(s).unwrap();
        assert_eq!(chunk.choices.len(), 1);
        assert_eq!(chunk.choices[0].delta.content.as_deref(), Some("Hi"));
        assert!(chunk.choices[0].finish_reason.is_none());
    }

    #[test]
    fn stream_chunk_deserialize_finish_with_usage() {
        let s = r#"{"choices":[{"delta":{},"index":0,"finish_reason":"stop"}],"usage":{"prompt_tokens":12,"completion_tokens":7,"total_tokens":19}}"#;
        let chunk: OpenAiStreamChunk = serde_json::from_str(s).unwrap();
        assert_eq!(chunk.choices[0].finish_reason.as_deref(), Some("stop"));
        let u = chunk.usage.unwrap();
        assert_eq!(u.prompt_tokens, 12);
        assert_eq!(u.completion_tokens, 7);
    }

    /// Bind a TCP listener on a random port and serve one SSE response built
    /// from `events` (each entry written as `data: <json>\n\n`, then `data: [DONE]`).
    async fn spawn_sse_server(events: &'static [&'static str]) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 4096];
            let mut total = Vec::new();
            loop {
                let n = sock.read(&mut buf).await.unwrap_or(0);
                if n == 0 {
                    break;
                }
                total.extend_from_slice(&buf[..n]);
                if total.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            let mut header = String::new();
            header.push_str("HTTP/1.1 200 OK\r\n");
            header.push_str("Content-Type: text/event-stream\r\n");
            header.push_str("Transfer-Encoding: chunked\r\n");
            header.push_str("\r\n");
            sock.write_all(header.as_bytes()).await.unwrap();

            for ev in events {
                let payload = format!("data: {ev}\n\n");
                let bytes = payload.into_bytes();
                let chunk_header = format!("{:x}\r\n", bytes.len());
                sock.write_all(chunk_header.as_bytes()).await.unwrap();
                sock.write_all(&bytes).await.unwrap();
                sock.write_all(b"\r\n").await.unwrap();
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
            // Closing sentinel.
            let done = b"data: [DONE]\n\n";
            let chunk_header = format!("{:x}\r\n", done.len());
            sock.write_all(chunk_header.as_bytes()).await.unwrap();
            sock.write_all(done).await.unwrap();
            sock.write_all(b"\r\n").await.unwrap();
            sock.write_all(b"0\r\n\r\n").await.unwrap();
            let _ = sock.shutdown().await;
        });
        format!("http://127.0.0.1:{port}")
    }

    #[tokio::test]
    async fn stream_emits_deltas_and_done() {
        let events: &[&str] = &[
            r#"{"choices":[{"delta":{"role":"assistant","content":""},"index":0,"finish_reason":null}]}"#,
            r#"{"choices":[{"delta":{"content":"Hello"},"index":0,"finish_reason":null}]}"#,
            r#"{"choices":[{"delta":{"content":", "},"index":0,"finish_reason":null}]}"#,
            r#"{"choices":[{"delta":{"content":"world!"},"index":0,"finish_reason":null}]}"#,
            r#"{"choices":[{"delta":{},"index":0,"finish_reason":"stop"}]}"#,
            r#"{"choices":[],"usage":{"prompt_tokens":11,"completion_tokens":3,"total_tokens":14}}"#,
        ];
        let url = spawn_sse_server(events).await;
        let provider = OpenAiProvider::with_base_url("test-key", url);
        let mut rx = provider.stream(sample_request()).await.unwrap();
        let mut deltas = Vec::new();
        let mut done_resp: Option<LlmResponse> = None;
        while let Some(ev) = rx.recv().await {
            match ev {
                LlmStreamEvent::Delta(t) => deltas.push(t),
                LlmStreamEvent::Done(r) => {
                    done_resp = Some(r);
                    break;
                }
                LlmStreamEvent::Error(e) => panic!("stream error: {e}"),
                LlmStreamEvent::ToolUse { .. } => {}
            }
        }
        assert_eq!(deltas, vec!["Hello", ", ", "world!"]);
        let resp = done_resp.expect("missing Done event");
        match &resp.content.parts[0] {
            Part::Text(t) => assert_eq!(t, "Hello, world!"),
            _ => panic!("expected text"),
        }
        assert_eq!(resp.usage.input_tokens, 11);
        assert_eq!(resp.usage.output_tokens, 3);
        assert_eq!(resp.finish_reason, FinishReason::Stop);
    }
}
