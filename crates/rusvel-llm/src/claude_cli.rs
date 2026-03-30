//! Claude CLI adapter implementing [`LlmPort`] via `claude -p`.
//!
//! Spawns the `claude` CLI binary as a subprocess with env vars that
//! route through a Claude Max subscription ($0 API credits).
//!
//! **Caveat:** The env vars (`CLAUDE_CODE_ENTRYPOINT`, `CLAUDE_USE_SUBSCRIPTION`,
//! `CLAUDE_BYPASS_BALANCE_CHECK`) are undocumented and could break in any CLI update.

use async_trait::async_trait;
use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use tracing::debug;

use rusvel_core::domain::*;
use rusvel_core::error::{Result, RusvelError};
use rusvel_core::ports::LlmPort;

use crate::flat_prompt::flat_prompt;

/// Adapter that calls the `claude` CLI in non-interactive mode (`-p`).
///
/// Uses Max subscription env vars by default. Falls back to API key
/// billing if `use_subscription` is set to `false` and an API key is provided.
pub struct ClaudeCliProvider {
    /// Path to the `claude` binary (default: `"claude"`).
    command: String,
    /// Use Max subscription env vars instead of API key.
    use_subscription: bool,
    /// Optional API key for fallback billing.
    api_key: Option<String>,
    /// Default model to report (the CLI picks its own model internally).
    model: String,
    /// Timeout in seconds for the subprocess.
    timeout_secs: u64,
}

impl ClaudeCliProvider {
    /// Create a provider that uses Claude Max subscription (no API key needed).
    pub fn max_subscription() -> Self {
        Self {
            command: "claude".into(),
            use_subscription: true,
            api_key: None,
            model: "claude-sonnet-4-20250514".into(),
            timeout_secs: 300,
        }
    }

    /// Create a provider that uses an API key for billing.
    pub fn with_api_key(api_key: impl Into<String>) -> Self {
        Self {
            command: "claude".into(),
            use_subscription: false,
            api_key: Some(api_key.into()),
            model: "claude-sonnet-4-20250514".into(),
            timeout_secs: 300,
        }
    }

    /// Override the path to the `claude` binary.
    pub fn command(mut self, cmd: impl Into<String>) -> Self {
        self.command = cmd.into();
        self
    }

    /// Override the default model name reported by `list_models`.
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// Override the subprocess timeout (default: 300s).
    pub fn timeout_secs(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// Build the prompt string from an [`LlmRequest`].
    ///
    /// Concatenates system instructions and user messages into a single
    /// prompt since `claude -p` takes a flat string, not a message array.
    fn build_prompt(request: &LlmRequest) -> String {
        flat_prompt(request)
    }
}

// ════════════════════════════════════════════════════════════════════
//  Claude CLI JSON output shape
// ════════════════════════════════════════════════════════════════════

#[derive(Deserialize)]
struct CliResult {
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

/// Parse the CLI output, which may be a JSON array (verbose mode) or a
/// single JSON object. Extract the result entry.
fn parse_cli_output(stdout: &str) -> std::result::Result<CliResult, String> {
    // Try as JSON array first (verbose mode returns [...])
    if let Ok(entries) = serde_json::from_str::<Vec<serde_json::Value>>(stdout) {
        for entry in entries.iter().rev() {
            if entry.get("type").and_then(|t| t.as_str()) == Some("result") {
                return serde_json::from_value(entry.clone())
                    .map_err(|e| format!("failed to parse result entry: {e}"));
            }
        }
        return Err("no result entry found in CLI output".into());
    }
    // Fall back to single JSON object
    serde_json::from_str(stdout).map_err(|e| format!("failed to parse CLI output: {e}"))
}

// ════════════════════════════════════════════════════════════════════
//  LlmPort implementation
// ════════════════════════════════════════════════════════════════════

#[async_trait]
impl LlmPort for ClaudeCliProvider {
    async fn stream(&self, request: LlmRequest) -> Result<mpsc::Receiver<LlmStreamEvent>> {
        let prompt = Self::build_prompt(&request);
        let (tx, rx) = mpsc::channel(64);

        let mut cmd = Command::new(&self.command);
        cmd.arg("-p")
            .arg(&prompt)
            .arg("--output-format")
            .arg("stream-json")
            .arg("--verbose")
            .arg("--no-session-persistence");

        if !request.model.model.is_empty() {
            cmd.arg("--model").arg(&request.model.model);
        }
        if let Some(max) = request.max_tokens {
            cmd.arg("--max-turns").arg(max.to_string());
        }

        if self.use_subscription {
            cmd.env_remove("ANTHROPIC_API_KEY");
            cmd.env_remove("CLAUDECODE");
            cmd.env("CLAUDE_CODE_ENTRYPOINT", "sdk-max");
            cmd.env("CLAUDE_USE_SUBSCRIPTION", "true");
            cmd.env("CLAUDE_BYPASS_BALANCE_CHECK", "true");
        } else if let Some(ref key) = self.api_key {
            cmd.env("ANTHROPIC_API_KEY", key);
        }

        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::null());

        let mut child = cmd
            .spawn()
            .map_err(|e| RusvelError::Llm(format!("failed to spawn claude: {e}")))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| RusvelError::Llm("failed to capture stdout".into()))?;

        let timeout_secs = self.timeout_secs;

        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            let mut full_text = String::new();

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
                                                    full_text.push_str(text);
                                                    let _ = tx
                                                        .send(LlmStreamEvent::Delta(
                                                            text.to_string(),
                                                        ))
                                                        .await;
                                                }
                                            }
                                        } else if block_type == Some("tool_use") {
                                            let id = block
                                                .get("id")
                                                .and_then(|v| v.as_str())
                                                .unwrap_or("")
                                                .to_string();
                                            let name = block
                                                .get("name")
                                                .and_then(|v| v.as_str())
                                                .unwrap_or("")
                                                .to_string();
                                            let args = block
                                                .get("input")
                                                .cloned()
                                                .unwrap_or(serde_json::json!({}));
                                            let _ = tx
                                                .send(LlmStreamEvent::ToolUse { id, name, args })
                                                .await;
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

                                    let finish_reason = FinishReason::Stop;

                                    let _ = tx
                                        .send(LlmStreamEvent::Done(LlmResponse {
                                            content: Content::text(result_text),
                                            finish_reason,
                                            usage: LlmUsage {
                                                input_tokens: 0,
                                                output_tokens: 0,
                                            },
                                            metadata: serde_json::json!({
                                                "source": "claude-cli-stream",
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
                    .send(LlmStreamEvent::Error("claude CLI stream timed out".into()))
                    .await;
            }
        });

        Ok(rx)
    }

    async fn generate(&self, request: LlmRequest) -> Result<LlmResponse> {
        let prompt = Self::build_prompt(&request);

        debug!(
            model = %request.model.model,
            prompt_len = prompt.len(),
            "claude-cli generate"
        );

        let mut cmd = Command::new(&self.command);
        cmd.arg("-p")
            .arg(&prompt)
            .arg("--output-format")
            .arg("json")
            .arg("--verbose");

        // Model override: pass --model if the request specifies one.
        if !request.model.model.is_empty() {
            cmd.arg("--model").arg(&request.model.model);
        }

        // Max tokens.
        if let Some(max) = request.max_tokens {
            cmd.arg("--max-turns").arg(max.to_string());
        }

        // Env var setup for Max subscription.
        if self.use_subscription {
            cmd.env_remove("ANTHROPIC_API_KEY");
            cmd.env_remove("CLAUDECODE"); // avoid recursion detection
            cmd.env("CLAUDE_CODE_ENTRYPOINT", "sdk-max");
            cmd.env("CLAUDE_USE_SUBSCRIPTION", "true");
            cmd.env("CLAUDE_BYPASS_BALANCE_CHECK", "true");
        } else if let Some(ref key) = self.api_key {
            cmd.env("ANTHROPIC_API_KEY", key);
        }

        // Capture stdout, inherit stderr for debug logs.
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::inherit());

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(self.timeout_secs),
            cmd.output(),
        )
        .await
        .map_err(|_| RusvelError::Llm("claude CLI timed out".into()))?
        .map_err(|e| RusvelError::Llm(format!("failed to spawn claude CLI: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(RusvelError::Llm(format!(
                "claude CLI exited with {}: {}",
                output.status,
                stderr.chars().take(500).collect::<String>()
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let cli_result: CliResult = parse_cli_output(&stdout).map_err(|e| {
            RusvelError::Llm(format!(
                "{e}\nraw: {}",
                stdout.chars().take(300).collect::<String>()
            ))
        })?;

        if cli_result.is_error {
            return Err(RusvelError::Llm(format!(
                "claude CLI returned error: {}",
                cli_result.result.chars().take(500).collect::<String>()
            )));
        }

        debug!(
            cost_usd = cli_result.total_cost_usd,
            turns = cli_result.num_turns,
            duration_ms = cli_result.duration_ms,
            "claude-cli response"
        );

        Ok(LlmResponse {
            content: Content::text(cli_result.result),
            finish_reason: FinishReason::Stop,
            usage: LlmUsage {
                input_tokens: 0, // CLI doesn't report token counts
                output_tokens: 0,
            },
            metadata: serde_json::json!({
                "source": "claude-cli",
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
            "claude CLI does not support embeddings".into(),
        ))
    }

    async fn list_models(&self) -> Result<Vec<ModelRef>> {
        Ok(vec![ModelRef {
            provider: ModelProvider::Claude,
            model: self.model.clone(),
        }])
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
    fn build_prompt_includes_system_and_user() {
        let req = sample_request();
        let prompt = ClaudeCliProvider::build_prompt(&req);
        assert!(prompt.contains("<system>"));
        assert!(prompt.contains("You are helpful."));
        assert!(prompt.contains("Hello!"));
    }

    #[test]
    fn build_prompt_skips_empty_messages() {
        let req = LlmRequest {
            model: ModelRef {
                provider: ModelProvider::Claude,
                model: "test".into(),
            },
            messages: vec![
                LlmMessage {
                    role: LlmRole::User,
                    content: Content { parts: vec![] },
                },
                LlmMessage {
                    role: LlmRole::User,
                    content: Content::text("real message"),
                },
            ],
            tools: vec![],
            temperature: None,
            max_tokens: None,
            metadata: serde_json::json!({}),
        };
        let prompt = ClaudeCliProvider::build_prompt(&req);
        assert_eq!(prompt, "real message");
    }

    #[test]
    fn parse_cli_result_single_object() {
        let json = r#"{
            "type": "result",
            "subtype": "success",
            "result": "Hello there!",
            "session_id": "abc-123",
            "total_cost_usd": 0.0,
            "duration_ms": 1500,
            "duration_api_ms": 1200,
            "num_turns": 1,
            "is_error": false
        }"#;
        let parsed = parse_cli_output(json).unwrap();
        assert_eq!(parsed.result, "Hello there!");
        assert!(!parsed.is_error);
        assert!((parsed.total_cost_usd - 0.0).abs() < f64::EPSILON);
        assert_eq!(parsed.session_id, "abc-123");
    }

    #[test]
    fn parse_cli_result_from_array() {
        let json = r#"[
            {"type": "system", "subtype": "init", "session_id": "abc"},
            {"type": "assistant", "message": {}},
            {"type": "result", "subtype": "success", "result": "Hello!", "is_error": false,
             "total_cost_usd": 0.07, "num_turns": 1, "duration_ms": 2000,
             "duration_api_ms": 1800, "session_id": "abc-123"}
        ]"#;
        let parsed = parse_cli_output(json).unwrap();
        assert_eq!(parsed.result, "Hello!");
        assert!(!parsed.is_error);
        assert!((parsed.total_cost_usd - 0.07).abs() < 1e-9);
    }

    #[test]
    fn parse_cli_error_result() {
        let json = r#"{
            "type": "result",
            "subtype": "error_during_execution",
            "result": "Something went wrong",
            "is_error": true,
            "total_cost_usd": 0.0,
            "duration_ms": 100,
            "duration_api_ms": 0,
            "num_turns": 0,
            "session_id": ""
        }"#;
        let parsed = parse_cli_output(json).unwrap();
        assert!(parsed.is_error);
    }

    #[test]
    fn parse_cli_legacy_cost_field() {
        let json = r#"{
            "type": "result",
            "result": "Hi",
            "is_error": false,
            "cost_usd": 0.05,
            "num_turns": 1,
            "duration_ms": 100,
            "duration_api_ms": 80,
            "session_id": "x"
        }"#;
        let parsed = parse_cli_output(json).unwrap();
        assert!((parsed.total_cost_usd - 0.05).abs() < 1e-9);
    }

    #[test]
    fn max_subscription_defaults() {
        let provider = ClaudeCliProvider::max_subscription();
        assert!(provider.use_subscription);
        assert!(provider.api_key.is_none());
        assert_eq!(provider.command, "claude");
    }

    #[test]
    fn api_key_fallback() {
        let provider = ClaudeCliProvider::with_api_key("sk-test");
        assert!(!provider.use_subscription);
        assert_eq!(provider.api_key.as_deref(), Some("sk-test"));
    }

    #[test]
    fn builder_methods() {
        let provider = ClaudeCliProvider::max_subscription()
            .command("/usr/local/bin/claude")
            .model("claude-opus-4-20250514")
            .timeout_secs(600);
        assert_eq!(provider.command, "/usr/local/bin/claude");
        assert_eq!(provider.model, "claude-opus-4-20250514");
        assert_eq!(provider.timeout_secs, 600);
    }
}
