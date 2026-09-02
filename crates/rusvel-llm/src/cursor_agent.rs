//! Cursor CLI adapter implementing [`LlmPort`] via `cursor agent --print`.
//!
//! Spawns the Cursor CLI in headless mode. RUSVEL [`LlmRequest::tools`] are not
//! forwarded; Cursor runs its own agent/tools within [`CursorAgentProvider`] workspace.
//! Output JSON shape may drift with Cursor updates — see [`parse_cursor_stdout`].

use std::path::PathBuf;

use async_trait::async_trait;
use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use tracing::debug;
use tracing::warn;

use rusvel_core::domain::*;
use rusvel_core::error::{Result, RusvelError};
use rusvel_core::ports::LlmPort;

use crate::flat_prompt::flat_prompt;

/// Adapter that calls `cursor agent --print` with `--workspace` for a repo-bound agent.
pub struct CursorAgentProvider {
    command: String,
    workspace: PathBuf,
    /// Default model id reported by [`LlmPort::list_models`].
    default_model: String,
    timeout_secs: u64,
}

impl CursorAgentProvider {
    /// Build from environment: `RUSVEL_CURSOR_BIN` (default `cursor`),
    /// `RUSVEL_CURSOR_WORKSPACE` (default [`std::env::current_dir`] or `.`).
    pub fn from_env() -> Self {
        let command = std::env::var("RUSVEL_CURSOR_BIN").unwrap_or_else(|_| "cursor".into());
        let workspace = std::env::var("RUSVEL_CURSOR_WORKSPACE")
            .map(PathBuf::from)
            .or_else(|_| std::env::current_dir())
            .unwrap_or_else(|_| PathBuf::from("."));
        Self {
            command,
            workspace,
            default_model: "sonnet-4".into(),
            timeout_secs: 300,
        }
    }

    pub fn command(mut self, cmd: impl Into<String>) -> Self {
        self.command = cmd.into();
        self
    }

    pub fn workspace(mut self, path: impl Into<PathBuf>) -> Self {
        self.workspace = path.into();
        self
    }

    pub fn default_model(mut self, model: impl Into<String>) -> Self {
        self.default_model = model.into();
        self
    }

    pub fn timeout_secs(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    fn warn_if_tools(&self, request: &LlmRequest) {
        if !request.tools.is_empty() {
            warn!(
                "cursor agent provider: RUSVEL tool definitions are not forwarded; \
                 Cursor uses its own workspace tools within --workspace"
            );
        }
    }

    fn spawn(&self, request: &LlmRequest, output_format: &str) -> Result<tokio::process::Child> {
        let prompt = flat_prompt(request);
        let mut cmd = Command::new(&self.command);
        cmd.arg("agent")
            .arg("--print")
            .arg("--workspace")
            .arg(&self.workspace)
            .arg("--output-format")
            .arg(output_format);

        if !request.model.model.is_empty() {
            cmd.arg("--model").arg(&request.model.model);
        }

        cmd.arg("--").arg(prompt);

        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::inherit());

        cmd.spawn()
            .map_err(|e| RusvelError::Llm(format!("failed to spawn cursor agent: {e}")))
    }
}

#[derive(Deserialize)]
struct CursorCliResult {
    #[serde(default)]
    result: String,
    #[serde(default)]
    is_error: bool,
    #[serde(default, alias = "cost_usd")]
    total_cost_usd: f64,
    #[serde(default)]
    num_turns: u32,
    #[serde(default)]
    duration_ms: u64,
    #[serde(default)]
    duration_api_ms: u64,
    #[serde(default)]
    session_id: String,
}

/// Parse `cursor agent --print` stdout: JSON array, single object, or NDJSON lines.
/// Format may match Claude CLI stream-json result entries or drift — extend as needed.
fn parse_cursor_stdout(stdout: &str) -> std::result::Result<CursorCliResult, String> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Err("empty cursor agent stdout".into());
    }

    if let Ok(entries) = serde_json::from_str::<Vec<serde_json::Value>>(trimmed) {
        for entry in entries.iter().rev() {
            if entry.get("type").and_then(|t| t.as_str()) == Some("result") {
                return serde_json::from_value(entry.clone())
                    .map_err(|e| format!("failed to parse result entry: {e}"));
            }
        }
        return Err("no result entry found in cursor output array".into());
    }

    if let Ok(entry) = serde_json::from_str::<serde_json::Value>(trimmed) {
        if entry.get("type").and_then(|t| t.as_str()) == Some("result") {
            return serde_json::from_value(entry)
                .map_err(|e| format!("failed to parse result object: {e}"));
        }
    }

    for line in trimmed.lines().rev() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) {
            if entry.get("type").and_then(|t| t.as_str()) == Some("result") {
                return serde_json::from_value(entry)
                    .map_err(|e| format!("failed to parse NDJSON result: {e}"));
            }
        }
    }

    if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
        if let Some(text) = v.get("text").and_then(|t| t.as_str()) {
            return Ok(CursorCliResult {
                result: text.to_string(),
                is_error: false,
                total_cost_usd: 0.0,
                num_turns: 0,
                duration_ms: 0,
                duration_api_ms: 0,
                session_id: String::new(),
            });
        }
    }

    Err(format!(
        "could not parse cursor agent output: {}",
        trimmed.chars().take(200).collect::<String>()
    ))
}

#[async_trait]
impl LlmPort for CursorAgentProvider {
    async fn stream(&self, request: LlmRequest) -> Result<mpsc::Receiver<LlmStreamEvent>> {
        self.warn_if_tools(&request);
        let (tx, rx) = mpsc::channel(64);

        let mut child = self.spawn(&request, "stream-json")?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| RusvelError::Llm("failed to capture cursor agent stdout".into()))?;

        let timeout_secs = self.timeout_secs;

        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();

            let stream_result =
                tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), async {
                    while let Ok(Some(line)) = lines.next_line().await {
                        if line.trim().is_empty() {
                            continue;
                        }
                        let parsed: serde_json::Value = match serde_json::from_str(&line) {
                            Ok(v) => v,
                            Err(_) => continue,
                        };
                        let event_type = parsed.get("type").and_then(|t| t.as_str()).unwrap_or("");

                        match event_type {
                            "assistant" => {
                                if let Some(content) = parsed
                                    .pointer("/message/content")
                                    .and_then(|c| c.as_array())
                                {
                                    for block in content {
                                        let block_type = block.get("type").and_then(|t| t.as_str());
                                        if block_type == Some("text") {
                                            if let Some(text) =
                                                block.get("text").and_then(|t| t.as_str())
                                            {
                                                if !text.is_empty() {
                                                    let _ = tx
                                                        .send(LlmStreamEvent::Delta(
                                                            text.to_string(),
                                                        ))
                                                        .await;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            "result" => {
                                let is_error = parsed
                                    .get("is_error")
                                    .and_then(serde_json::Value::as_bool)
                                    .unwrap_or(false);

                                if is_error {
                                    let msg = parsed
                                        .get("result")
                                        .and_then(|r| r.as_str())
                                        .unwrap_or("unknown error")
                                        .to_string();
                                    let _ = tx.send(LlmStreamEvent::Error(msg)).await;
                                } else {
                                    let result_text = parsed
                                        .get("result")
                                        .and_then(|r| r.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                    let cost_usd = parsed
                                        .get("total_cost_usd")
                                        .and_then(serde_json::Value::as_f64)
                                        .unwrap_or(0.0);
                                    let num_turns = parsed
                                        .get("num_turns")
                                        .and_then(serde_json::Value::as_u64)
                                        .unwrap_or(0)
                                        as u32;
                                    let duration_ms = parsed
                                        .get("duration_ms")
                                        .and_then(serde_json::Value::as_u64)
                                        .unwrap_or(0);
                                    let duration_api_ms = parsed
                                        .get("duration_api_ms")
                                        .and_then(serde_json::Value::as_u64)
                                        .unwrap_or(0);
                                    let session_id = parsed
                                        .get("session_id")
                                        .and_then(|s| s.as_str())
                                        .unwrap_or("")
                                        .to_string();

                                    let _ = tx
                                        .send(LlmStreamEvent::Done(LlmResponse {
                                            content: Content::text(result_text),
                                            finish_reason: FinishReason::Stop,
                                            usage: LlmUsage {
                                                input_tokens: 0,
                                                output_tokens: 0,
                                            },
                                            metadata: serde_json::json!({
                                                "source": "cursor-agent-stream",
                                                "cost_usd": cost_usd,
                                                "num_turns": num_turns,
                                                "duration_ms": duration_ms,
                                                "duration_api_ms": duration_api_ms,
                                                "session_id": session_id,
                                            }),
                                        }))
                                        .await;
                                }
                            }
                            _ => {}
                        }
                    }
                })
                .await;

            let _ = child.kill().await;

            if stream_result.is_err() {
                let _ = tx
                    .send(LlmStreamEvent::Error(
                        "cursor agent stream timed out".into(),
                    ))
                    .await;
            }
        });

        debug!(
            workspace = %self.workspace.display(),
            "cursor agent stream"
        );

        Ok(rx)
    }

    async fn generate(&self, request: LlmRequest) -> Result<LlmResponse> {
        self.warn_if_tools(&request);
        let prompt = flat_prompt(&request);

        debug!(
            model = %request.model.model,
            prompt_len = prompt.len(),
            workspace = %self.workspace.display(),
            "cursor agent generate"
        );

        let mut cmd = Command::new(&self.command);
        cmd.arg("agent")
            .arg("--print")
            .arg("--workspace")
            .arg(&self.workspace)
            .arg("--output-format")
            .arg("json");

        if !request.model.model.is_empty() {
            cmd.arg("--model").arg(&request.model.model);
        }

        cmd.arg("--").arg(&prompt);

        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::inherit());

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(self.timeout_secs),
            cmd.output(),
        )
        .await
        .map_err(|_| RusvelError::Llm("cursor agent timed out".into()))?
        .map_err(|e| RusvelError::Llm(format!("failed to spawn cursor agent: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(RusvelError::Llm(format!(
                "cursor agent exited with {}: {}",
                output.status,
                stderr.chars().take(500).collect::<String>()
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let cli_result = parse_cursor_stdout(&stdout).map_err(|e| {
            RusvelError::Llm(format!(
                "{e}\nraw: {}",
                stdout.chars().take(300).collect::<String>()
            ))
        })?;

        if cli_result.is_error {
            return Err(RusvelError::Llm(format!(
                "cursor agent returned error: {}",
                cli_result.result.chars().take(500).collect::<String>()
            )));
        }

        Ok(LlmResponse {
            content: Content::text(cli_result.result),
            finish_reason: FinishReason::Stop,
            usage: LlmUsage {
                input_tokens: 0,
                output_tokens: 0,
            },
            metadata: serde_json::json!({
                "source": "cursor-agent",
                "cost_usd": cli_result.total_cost_usd,
                "num_turns": cli_result.num_turns,
                "duration_ms": cli_result.duration_ms,
                "duration_api_ms": cli_result.duration_api_ms,
                "session_id": cli_result.session_id,
            }),
        })
    }

    async fn embed(&self, _model: &ModelRef, _text: &str) -> Result<Vec<f32>> {
        Err(RusvelError::Llm(
            "cursor agent provider does not support embeddings".into(),
        ))
    }

    async fn list_models(&self) -> Result<Vec<ModelRef>> {
        Ok(vec![ModelRef {
            provider: ModelProvider::Other("cursor".into()),
            model: self.default_model.clone(),
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cursor_result_object() {
        let json = r#"{
            "type": "result",
            "subtype": "success",
            "result": "Hello from Cursor",
            "session_id": "s1",
            "total_cost_usd": 0.0,
            "duration_ms": 100,
            "duration_api_ms": 90,
            "num_turns": 1,
            "is_error": false
        }"#;
        let p = parse_cursor_stdout(json).unwrap();
        assert_eq!(p.result, "Hello from Cursor");
        assert!(!p.is_error);
    }

    #[test]
    fn parse_cursor_ndjson_last_line() {
        let s = r#"{"type":"log","msg":"x"}
{"type":"result","result":"done","is_error":false,"total_cost_usd":0,"num_turns":1,"duration_ms":1,"duration_api_ms":1,"session_id":""}"#;
        let p = parse_cursor_stdout(s).unwrap();
        assert_eq!(p.result, "done");
    }

    #[test]
    fn parse_cursor_fallback_text_field() {
        let json = r#"{"text": "plain"}"#;
        let p = parse_cursor_stdout(json).unwrap();
        assert_eq!(p.result, "plain");
    }

    #[test]
    fn multi_provider_routes_cursor() {
        use std::sync::Arc;

        struct FakeCursor {
            tag: &'static str,
        }

        #[async_trait]
        impl LlmPort for FakeCursor {
            async fn generate(&self, _request: LlmRequest) -> Result<LlmResponse> {
                Ok(LlmResponse {
                    content: Content::text(format!("from {}", self.tag)),
                    finish_reason: FinishReason::Stop,
                    usage: LlmUsage::default(),
                    metadata: serde_json::json!({}),
                })
            }

            async fn embed(&self, _model: &ModelRef, _text: &str) -> Result<Vec<f32>> {
                Err(RusvelError::Llm("no".into()))
            }

            async fn list_models(&self) -> Result<Vec<ModelRef>> {
                Ok(vec![])
            }
        }

        let multi = crate::MultiProvider::new();
        multi.register(
            ModelProvider::Other("cursor".into()),
            Arc::new(FakeCursor { tag: "cursor" }),
        );

        let req = LlmRequest {
            model: ModelRef {
                provider: ModelProvider::Other("cursor".into()),
                model: "x".into(),
            },
            messages: vec![LlmMessage {
                role: LlmRole::User,
                content: Content::text("hi"),
            }],
            tools: vec![],
            temperature: None,
            max_tokens: None,
            metadata: serde_json::json!({}),
        };

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let resp = rt.block_on(multi.generate(req)).unwrap();
        match &resp.content.parts[0] {
            Part::Text(t) => assert_eq!(t, "from cursor"),
            _ => panic!("expected text"),
        }
    }

    /// Run manually: `cargo test -p rusvel-llm cursor_agent_integration_smoke -- --ignored`
    /// Requires `cursor` on PATH and valid Cursor auth (`cursor agent login`).
    #[tokio::test]
    #[ignore = "requires local Cursor CLI and account auth"]
    async fn cursor_agent_integration_smoke() {
        let llm = CursorAgentProvider::from_env().timeout_secs(120);
        let req = LlmRequest {
            model: ModelRef {
                provider: ModelProvider::Other("cursor".into()),
                model: String::new(),
            },
            messages: vec![LlmMessage {
                role: LlmRole::User,
                content: Content::text("Reply with exactly: ok"),
            }],
            tools: vec![],
            temperature: None,
            max_tokens: None,
            metadata: serde_json::json!({}),
        };
        let r = llm.generate(req).await.expect("cursor agent generate");
        assert!(!r.content.parts.is_empty());
    }
}
