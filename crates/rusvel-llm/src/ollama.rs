//! Ollama HTTP adapter implementing [`LlmPort`].

use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::debug;

use rusvel_core::domain::*;
use rusvel_core::error::{Result, RusvelError};
use rusvel_core::ports::LlmPort;

// ════════════════════════════════════════════════════════════════════
//  OllamaProvider
// ════════════════════════════════════════════════════════════════════

/// Ollama-backed LLM adapter.
///
/// Connects to a local (or remote) Ollama instance at `base_url`.
/// Default: `http://localhost:11434`.
pub struct OllamaProvider {
    base_url: String,
    client: Client,
}

impl OllamaProvider {
    /// Create a provider pointing at the default Ollama address.
    pub fn new() -> Self {
        Self::with_base_url("http://localhost:11434")
    }

    /// Create a provider pointing at a custom Ollama address.
    pub fn with_base_url(url: impl Into<String>) -> Self {
        Self {
            base_url: url.into().trim_end_matches('/').to_string(),
            client: Client::new(),
        }
    }
}

impl Default for OllamaProvider {
    fn default() -> Self {
        Self::new()
    }
}

// ════════════════════════════════════════════════════════════════════
//  LlmPort implementation
// ════════════════════════════════════════════════════════════════════

#[async_trait]
impl LlmPort for OllamaProvider {
    async fn generate(&self, request: LlmRequest) -> rusvel_core::error::Result<LlmResponse> {
        let ollama_req = to_ollama_chat(&request);
        let url = format!("{}/api/chat", self.base_url);

        debug!(model = %request.model.model, "ollama generate");

        let http_resp = self
            .client
            .post(&url)
            .json(&ollama_req)
            .send()
            .await
            .map_err(map_reqwest_error)?;

        let status = http_resp.status();
        if !status.is_success() {
            let body = http_resp.text().await.unwrap_or_default();
            return Err(map_ollama_http_error(status.as_u16(), &body));
        }

        let ollama_resp: OllamaChatResponse = http_resp.json().await.map_err(map_reqwest_error)?;

        Ok(from_ollama_chat(ollama_resp))
    }

    async fn stream(
        &self,
        request: LlmRequest,
    ) -> Result<tokio::sync::mpsc::Receiver<LlmStreamEvent>> {
        let mut ollama_req = to_ollama_chat(&request);
        ollama_req.stream = true;
        let url = format!("{}/api/chat", self.base_url);
        let client = self.client.clone();

        debug!(model = %request.model.model, "ollama stream");

        let (tx, rx) = tokio::sync::mpsc::channel(128);

        tokio::spawn(async move {
            let http_resp = match client.post(&url).json(&ollama_req).send().await {
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
                let body = http_resp.text().await.unwrap_or_default();
                let _ = tx
                    .send(LlmStreamEvent::Error(
                        map_ollama_http_error(status.as_u16(), &body).to_string(),
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
                    if line.is_empty() {
                        continue;
                    }
                    let Ok(ev) = serde_json::from_str::<OllamaStreamChunk>(line) else {
                        continue;
                    };
                    if !ev.message.content.is_empty() {
                        full_text.push_str(&ev.message.content);
                        let _ = tx
                            .send(LlmStreamEvent::Delta(ev.message.content))
                            .await;
                    }
                    if ev.done {
                        input_tokens = ev.prompt_eval_count;
                        output_tokens = ev.eval_count;
                        if let Some(reason) = ev.done_reason.as_deref() {
                            finish = match reason {
                                "stop" => FinishReason::Stop,
                                "length" => FinishReason::Length,
                                other => FinishReason::Other(other.into()),
                            };
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
        let url = format!("{}/api/embed", self.base_url);
        let body = serde_json::json!({
            "model": model.model,
            "input": text,
        });

        debug!(model = %model.model, "ollama embed");

        let http_resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(map_reqwest_error)?;

        let status = http_resp.status();
        if !status.is_success() {
            let body = http_resp.text().await.unwrap_or_default();
            return Err(map_ollama_http_error(status.as_u16(), &body));
        }

        let resp: OllamaEmbedResponse = http_resp.json().await.map_err(map_reqwest_error)?;

        // Ollama returns `embeddings: [[f32]]`; take the first vector.
        resp.embeddings
            .into_iter()
            .next()
            .ok_or_else(|| RusvelError::Llm("ollama returned empty embeddings".into()))
    }

    async fn list_models(&self) -> rusvel_core::error::Result<Vec<ModelRef>> {
        let url = format!("{}/api/tags", self.base_url);

        debug!("ollama list_models");

        let http_resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(map_reqwest_error)?;

        let status = http_resp.status();
        if !status.is_success() {
            let body = http_resp.text().await.unwrap_or_default();
            return Err(map_ollama_http_error(status.as_u16(), &body));
        }

        let resp: OllamaTagsResponse = http_resp.json().await.map_err(map_reqwest_error)?;

        Ok(resp
            .models
            .into_iter()
            .map(|m| ModelRef {
                provider: ModelProvider::Ollama,
                model: m.name,
            })
            .collect())
    }
}

// ════════════════════════════════════════════════════════════════════
//  Ollama wire types (serde)
// ════════════════════════════════════════════════════════════════════

#[derive(Serialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<OllamaMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaOptions>,
}

#[derive(Serialize)]
struct OllamaOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<u32>,
}

#[derive(Default, Serialize, Deserialize)]
struct OllamaMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct OllamaChatResponse {
    #[serde(default)]
    message: OllamaMessage,
    #[serde(default)]
    done: bool,
    #[serde(default)]
    eval_count: u32,
    #[serde(default)]
    prompt_eval_count: u32,
}

/// One NDJSON frame from `/api/chat?stream=true`.
#[derive(Deserialize)]
struct OllamaStreamChunk {
    #[serde(default)]
    message: OllamaMessage,
    #[serde(default)]
    done: bool,
    #[serde(default)]
    done_reason: Option<String>,
    #[serde(default)]
    eval_count: u32,
    #[serde(default)]
    prompt_eval_count: u32,
}

#[derive(Deserialize)]
struct OllamaEmbedResponse {
    #[serde(default)]
    embeddings: Vec<Vec<f32>>,
}

#[derive(Deserialize)]
struct OllamaTagsResponse {
    #[serde(default)]
    models: Vec<OllamaModelInfo>,
}

#[derive(Deserialize)]
struct OllamaModelInfo {
    name: String,
}

// ════════════════════════════════════════════════════════════════════
//  Mapping helpers
// ════════════════════════════════════════════════════════════════════

fn to_ollama_chat(req: &LlmRequest) -> OllamaChatRequest {
    let messages = req
        .messages
        .iter()
        .map(|m| OllamaMessage {
            role: match m.role {
                LlmRole::System => "system".into(),
                LlmRole::User => "user".into(),
                LlmRole::Assistant => "assistant".into(),
                LlmRole::Tool => "tool".into(),
            },
            content: extract_text(&m.content),
        })
        .collect();

    let has_options = req.temperature.is_some() || req.max_tokens.is_some();

    OllamaChatRequest {
        model: req.model.model.clone(),
        messages,
        stream: false,
        options: if has_options {
            Some(OllamaOptions {
                temperature: req.temperature,
                num_predict: req.max_tokens,
            })
        } else {
            None
        },
    }
}

fn from_ollama_chat(resp: OllamaChatResponse) -> LlmResponse {
    let finish_reason = if resp.done {
        FinishReason::Stop
    } else {
        FinishReason::Other("incomplete".into())
    };

    LlmResponse {
        content: Content::text(resp.message.content),
        finish_reason,
        usage: LlmUsage {
            input_tokens: resp.prompt_eval_count,
            output_tokens: resp.eval_count,
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
        RusvelError::Llm(format!("cannot connect to Ollama — is it running? ({e})"))
    } else if e.is_timeout() {
        RusvelError::Llm(format!("Ollama request timed out: {e}"))
    } else {
        RusvelError::Llm(format!("Ollama HTTP error: {e}"))
    }
}

fn map_ollama_http_error(status: u16, body: &str) -> RusvelError {
    match status {
        404 => RusvelError::NotFound {
            kind: "model".into(),
            id: body.to_string(),
        },
        _ => RusvelError::Llm(format!("Ollama returned HTTP {status}: {body}")),
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
                provider: ModelProvider::Ollama,
                model: "llama3".into(),
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
            max_tokens: Some(256),
            metadata: serde_json::json!({}),
        }
    }

    #[test]
    fn to_ollama_chat_maps_roles() {
        let req = sample_request();
        let wire = to_ollama_chat(&req);
        assert_eq!(wire.model, "llama3");
        assert_eq!(wire.messages.len(), 2);
        assert_eq!(wire.messages[0].role, "system");
        assert_eq!(wire.messages[0].content, "You are helpful.");
        assert_eq!(wire.messages[1].role, "user");
        assert!(!wire.stream);
    }

    #[test]
    fn to_ollama_chat_sets_options() {
        let req = sample_request();
        let wire = to_ollama_chat(&req);
        let opts = wire.options.expect("options should be set");
        assert_eq!(opts.temperature, Some(0.7));
        assert_eq!(opts.num_predict, Some(256));
    }

    #[test]
    fn to_ollama_chat_omits_options_when_none() {
        let mut req = sample_request();
        req.temperature = None;
        req.max_tokens = None;
        let wire = to_ollama_chat(&req);
        assert!(wire.options.is_none());
    }

    #[test]
    fn from_ollama_chat_maps_response() {
        let resp = OllamaChatResponse {
            message: OllamaMessage {
                role: "assistant".into(),
                content: "Hi there!".into(),
            },
            done: true,
            eval_count: 12,
            prompt_eval_count: 8,
        };
        let llm_resp = from_ollama_chat(resp);
        assert_eq!(llm_resp.finish_reason, FinishReason::Stop);
        assert_eq!(llm_resp.usage.input_tokens, 8);
        assert_eq!(llm_resp.usage.output_tokens, 12);
        match &llm_resp.content.parts[0] {
            Part::Text(t) => assert_eq!(t, "Hi there!"),
            _ => panic!("expected text"),
        }
    }

    #[test]
    fn from_ollama_chat_incomplete() {
        let resp = OllamaChatResponse {
            message: OllamaMessage {
                role: "assistant".into(),
                content: "partial".into(),
            },
            done: false,
            eval_count: 0,
            prompt_eval_count: 0,
        };
        let llm_resp = from_ollama_chat(resp);
        assert_eq!(
            llm_resp.finish_reason,
            FinishReason::Other("incomplete".into())
        );
    }

    #[test]
    fn extract_text_concatenates_parts() {
        let c = Content {
            parts: vec![Part::Text("a".into()), Part::Text("b".into())],
        };
        assert_eq!(extract_text(&c), "ab");
    }

    #[test]
    fn extract_text_skips_non_text() {
        let c = Content {
            parts: vec![
                Part::Text("hello".into()),
                Part::Image(vec![1, 2, 3]),
                Part::Text(" world".into()),
            ],
        };
        assert_eq!(extract_text(&c), "hello world");
    }

    #[test]
    fn map_ollama_http_error_404_is_not_found() {
        let err = map_ollama_http_error(404, "model 'foo' not found");
        match err {
            RusvelError::NotFound { kind, .. } => assert_eq!(kind, "model"),
            other => panic!("expected NotFound, got: {other}"),
        }
    }

    #[test]
    fn map_ollama_http_error_500_is_llm() {
        let err = map_ollama_http_error(500, "internal");
        match err {
            RusvelError::Llm(msg) => assert!(msg.contains("500")),
            other => panic!("expected Llm, got: {other}"),
        }
    }

    #[test]
    fn default_base_url() {
        let p = OllamaProvider::new();
        assert_eq!(p.base_url, "http://localhost:11434");
    }

    #[test]
    fn custom_base_url_trims_slash() {
        let p = OllamaProvider::with_base_url("http://myhost:1234/");
        assert_eq!(p.base_url, "http://myhost:1234");
    }

    #[test]
    fn ollama_chat_response_deserialize() {
        let json = r#"{
            "model": "llama3",
            "message": {"role": "assistant", "content": "Hello"},
            "done": true,
            "eval_count": 5,
            "prompt_eval_count": 3,
            "total_duration": 123456
        }"#;
        let resp: OllamaChatResponse = serde_json::from_str(json).unwrap();
        assert!(resp.done);
        assert_eq!(resp.eval_count, 5);
        assert_eq!(resp.message.content, "Hello");
    }

    #[test]
    fn embed_response_deserialize() {
        let json = r#"{"model":"nomic","embeddings":[[0.1,0.2,0.3]]}"#;
        let resp: OllamaEmbedResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.embeddings.len(), 1);
        assert_eq!(resp.embeddings[0], vec![0.1, 0.2, 0.3]);
    }

    #[test]
    fn tags_response_deserialize() {
        let json = r#"{"models":[{"name":"llama3:latest"},{"name":"mistral:7b"}]}"#;
        let resp: OllamaTagsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.models.len(), 2);
        assert_eq!(resp.models[0].name, "llama3:latest");
    }

    #[tokio::test]
    async fn generate_connection_refused() {
        // Point at a port nothing is listening on.
        let provider = OllamaProvider::with_base_url("http://127.0.0.1:19999");
        let req = sample_request();
        let result = provider.generate(req).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("connect") || msg.contains("Ollama"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn stream_chunk_deserialize_delta() {
        let line = r#"{"model":"llama3","message":{"role":"assistant","content":"He"},"done":false}"#;
        let chunk: OllamaStreamChunk = serde_json::from_str(line).unwrap();
        assert!(!chunk.done);
        assert_eq!(chunk.message.content, "He");
    }

    #[test]
    fn stream_chunk_deserialize_done() {
        let line = r#"{"model":"llama3","message":{"role":"assistant","content":""},"done":true,"done_reason":"stop","prompt_eval_count":7,"eval_count":4}"#;
        let chunk: OllamaStreamChunk = serde_json::from_str(line).unwrap();
        assert!(chunk.done);
        assert_eq!(chunk.done_reason.as_deref(), Some("stop"));
        assert_eq!(chunk.prompt_eval_count, 7);
        assert_eq!(chunk.eval_count, 4);
    }

    /// Bind a TCP listener on a random port, hand back the URL.
    /// The closure receives the TCP listener and serves one request, writing
    /// the supplied raw HTTP body bytes back as the response.
    async fn spawn_ndjson_server(body: &'static [u8]) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 4096];
            // Read until \r\n\r\n (end of headers + small body).
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
            // Write a chunked HTTP/1.1 response: each NDJSON line as its own chunk
            // so reqwest's bytes_stream sees multiple deltas, not one.
            let mut header = String::new();
            header.push_str("HTTP/1.1 200 OK\r\n");
            header.push_str("Content-Type: application/x-ndjson\r\n");
            header.push_str("Transfer-Encoding: chunked\r\n");
            header.push_str("\r\n");
            sock.write_all(header.as_bytes()).await.unwrap();

            for line in body.split(|&b| b == b'\n') {
                if line.is_empty() {
                    continue;
                }
                let mut chunk = Vec::new();
                chunk.extend_from_slice(line);
                chunk.push(b'\n');
                let chunk_header = format!("{:x}\r\n", chunk.len());
                sock.write_all(chunk_header.as_bytes()).await.unwrap();
                sock.write_all(&chunk).await.unwrap();
                sock.write_all(b"\r\n").await.unwrap();
                // Slight delay so chunks land separately.
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
            sock.write_all(b"0\r\n\r\n").await.unwrap();
            let _ = sock.shutdown().await;
        });
        format!("http://127.0.0.1:{port}")
    }

    #[tokio::test]
    async fn stream_emits_deltas_and_done() {
        let ndjson = b"{\"message\":{\"role\":\"assistant\",\"content\":\"Hello\"},\"done\":false}\n\
                       {\"message\":{\"role\":\"assistant\",\"content\":\", \"},\"done\":false}\n\
                       {\"message\":{\"role\":\"assistant\",\"content\":\"world!\"},\"done\":false}\n\
                       {\"message\":{\"role\":\"assistant\",\"content\":\"\"},\"done\":true,\"done_reason\":\"stop\",\"prompt_eval_count\":3,\"eval_count\":5}\n";
        let url = spawn_ndjson_server(ndjson).await;
        let provider = OllamaProvider::with_base_url(url);
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
        assert_eq!(resp.usage.input_tokens, 3);
        assert_eq!(resp.usage.output_tokens, 5);
        assert_eq!(resp.finish_reason, FinishReason::Stop);
    }
}
