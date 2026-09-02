//! # rusvel-agent
//!
//! Implements [`AgentPort`] by orchestrating [`LlmPort`] + [`ToolPort`] +
//! [`MemoryPort`] into a request/response agent loop (ADR-009).
//!
//! The core type is [`AgentRuntime`] which holds `Arc` references to the
//! three backing ports and manages active runs in a concurrent map.
//!
//! ## Workflow patterns
//!
//! The [`workflow`] module adds Sequential, Parallel, and Loop execution
//! patterns for composing multiple agent runs. The [`persona`] module
//! provides a catalog of reusable agent personas.

pub mod context_pack;
pub mod persona;
pub mod verification;
pub mod workflow;

pub use context_pack::{ContextPack, to_prompt_section};
pub use persona::PersonaCatalog;
pub use verification::{
    LlmCritiqueStep, RulesComplianceStep, VerificationChain, VerificationContext,
    VerificationResult, VerificationStep,
};
pub use workflow::{Workflow, WorkflowRunner, WorkflowStep};

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use rusvel_core::domain::*;
use rusvel_core::error::{Result, RusvelError};
use rusvel_core::id::*;
use rusvel_core::ports::{AgentPort, LlmPort, MemoryPort, ToolPort};

/// A hook handler invoked before or after a tool call.
pub type HookHandler = Arc<dyn Fn(&str, &serde_json::Value) -> HookDecision + Send + Sync>;

/// Default maximum iterations in the agent loop.
const DEFAULT_MAX_ITERATIONS: u32 = 24;

/// Default per-request output token budget for agent runs.
///
/// Must stay generous (≥ 8192): on adaptive-thinking models (Claude Sonnet 5 /
/// Opus 5) thinking tokens count against `max_tokens`, so a small budget can be
/// consumed entirely by thinking, yielding an empty visible completion (#12).
const DEFAULT_MAX_OUTPUT_TOKENS: u32 = 8192;

fn agent_permission_blocks_tool(
    config: &AgentConfig,
    tool_name: &str,
    tool_defs: &[ToolDefinition],
) -> Option<String> {
    match config.permission_mode {
        ToolPermissionMode::Auto => None,
        ToolPermissionMode::Locked => Some(format!(
            "Tool requires approval in Locked mode: {tool_name}"
        )),
        ToolPermissionMode::Supervised => {
            let destructive = tool_defs
                .iter()
                .find(|t| t.name == tool_name)
                .and_then(|d| d.metadata.get("destructive"))
                .and_then(|v| v.as_bool())
                == Some(true);
            if destructive {
                Some(format!(
                    "Tool requires approval in Supervised mode: {tool_name}"
                ))
            } else {
                None
            }
        }
    }
}

/// USD cost of one LLM response, for [`AgentOutput::cost_estimate`] accumulation.
///
/// The cost-tracking LLM wrapper (`rusvel-llm`) stamps the per-call cost it
/// records into `metadata.cost_usd` — including catalog/actual pricing and
/// cache tokens — so that figure is authoritative when present. When the loop
/// runs against a bare provider (tests, direct wiring) fall back to the shared
/// list-price estimator for the requested model.
fn response_cost_usd(request_model: &ModelRef, response: &LlmResponse) -> f64 {
    response
        .metadata
        .get("cost_usd")
        .and_then(|v| v.as_f64())
        .unwrap_or_else(|| {
            estimate_llm_cost_usd(
                &request_model.provider,
                &request_model.model,
                &response.usage,
            )
        })
}

/// When conversation turns exceed this count, older turns are summarized.
const COMPACT_THRESHOLD: usize = 42;

/// After compaction, this many trailing [`LlmMessage`] entries are kept verbatim (plus optional system).
const COMPACT_KEEP_RECENT: usize = 10;

fn tool_allowed_by_dept_config(name: &str, allow: &[String]) -> bool {
    if allow.is_empty() {
        return true;
    }
    if name == "tool_search" {
        return true;
    }
    allow.iter().any(|pat| {
        if let Some(prefix) = pat.strip_suffix('*') {
            name.starts_with(prefix)
        } else {
            name == pat.as_str()
        }
    })
}

fn filter_tool_defs_for_config(defs: &mut Vec<ToolDefinition>, allow: &[String]) {
    if allow.is_empty() {
        return;
    }
    defs.retain(|t| tool_allowed_by_dept_config(&t.name, allow));
}

/// Events emitted during a streaming agent run.
#[derive(Debug, Clone)]
pub enum AgentEvent {
    /// Incremental text chunk from the LLM.
    TextDelta { text: String },
    /// A tool is being called (includes provider tool call id when available).
    ToolCall {
        tool_call_id: String,
        name: String,
        args: serde_json::Value,
    },
    /// Tool execution completed.
    ToolResult {
        tool_call_id: String,
        name: String,
        output: String,
        is_error: bool,
    },
    /// Partial state for AG-UI clients (JSON Patch / arbitrary delta); rarely emitted.
    StateDelta { delta: serde_json::Value },
    /// The agent run completed successfully.
    Done { output: AgentOutput },
    /// An error occurred.
    Error { message: String },
}

/// Wire-format events for AG-UI (Agent–User Interaction) compatible clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgUiEvent {
    RunStarted {
        run_id: String,
        timestamp: String,
    },
    TextDelta {
        text: String,
    },
    ToolCallStart {
        tool_call_id: String,
        tool_name: String,
        args: serde_json::Value,
    },
    ToolCallEnd {
        tool_call_id: String,
        tool_name: String,
        output: String,
        is_error: bool,
    },
    StateDelta {
        delta: serde_json::Value,
    },
    StepStarted {
        step_id: String,
        step_name: String,
    },
    StepCompleted {
        step_id: String,
    },
    RunCompleted {
        run_id: String,
        output: String,
    },
    RunFailed {
        run_id: String,
        error: String,
    },
}

impl AgUiEvent {
    /// SSE `event:` field (upper snake case).
    pub fn sse_name(&self) -> &'static str {
        match self {
            Self::RunStarted { .. } => "RUN_STARTED",
            Self::TextDelta { .. } => "TEXT_DELTA",
            Self::ToolCallStart { .. } => "TOOL_CALL_START",
            Self::ToolCallEnd { .. } => "TOOL_CALL_END",
            Self::StateDelta { .. } => "STATE_DELTA",
            Self::StepStarted { .. } => "STEP_STARTED",
            Self::StepCompleted { .. } => "STEP_COMPLETED",
            Self::RunCompleted { .. } => "RUN_COMPLETED",
            Self::RunFailed { .. } => "RUN_FAILED",
        }
    }
}

fn agent_output_to_plain(output: &AgentOutput) -> String {
    output
        .content
        .parts
        .iter()
        .filter_map(|p| match p {
            Part::Text(t) => Some(t.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

/// Maps internal [`AgentEvent`] to AG-UI wire format (excluding run prelude).
pub fn agent_event_to_ag_ui(run_id: &str, event: AgentEvent) -> AgUiEvent {
    match event {
        AgentEvent::TextDelta { text } => AgUiEvent::TextDelta { text },
        AgentEvent::ToolCall {
            tool_call_id,
            name,
            args,
        } => AgUiEvent::ToolCallStart {
            tool_call_id,
            tool_name: name,
            args,
        },
        AgentEvent::ToolResult {
            tool_call_id,
            name,
            output,
            is_error,
        } => AgUiEvent::ToolCallEnd {
            tool_call_id,
            tool_name: name,
            output,
            is_error,
        },
        AgentEvent::StateDelta { delta } => AgUiEvent::StateDelta { delta },
        AgentEvent::Done { output } => AgUiEvent::RunCompleted {
            run_id: run_id.to_string(),
            output: agent_output_to_plain(&output),
        },
        AgentEvent::Error { message } => AgUiEvent::RunFailed {
            run_id: run_id.to_string(),
            error: message,
        },
    }
}

/// JSON body for one SSE `data:` line, with `conversation_id` for routing.
pub fn ag_ui_json_with_conversation(ev: &AgUiEvent, conversation_id: &str) -> String {
    let mut v = match serde_json::to_value(ev) {
        Ok(v) => v,
        Err(_) => serde_json::json!({}),
    };
    if let Some(obj) = v.as_object_mut() {
        obj.insert(
            "conversation_id".into(),
            serde_json::Value::String(conversation_id.to_string()),
        );
    }
    serde_json::to_string(&v).unwrap_or_else(|_| "{}".to_string())
}

/// Internal state for a single agent run.
struct RunState {
    config: AgentConfig,
    status: AgentStatus,
}

fn merge_llm_request_metadata(config: &AgentConfig) -> serde_json::Value {
    let mut m = serde_json::Map::new();
    if let serde_json::Value::Object(ref o) = config.metadata {
        for (k, v) in o {
            m.insert(k.clone(), v.clone());
        }
    }
    m.insert(
        RUSVEL_META_SESSION_ID.into(),
        serde_json::Value::String(config.session_id.to_string()),
    );
    serde_json::Value::Object(m)
}

/// Agent orchestration runtime.
///
/// Wraps [`LlmPort`], [`ToolPort`], and [`MemoryPort`] into a coherent
/// agent loop. Each call to [`create`](AgentPort::create) registers a new
/// run; [`run`](AgentPort::run) executes the loop.
pub struct AgentRuntime {
    llm: Arc<dyn LlmPort>,
    tools: Arc<dyn ToolPort>,
    #[allow(dead_code)] // will be used for context retrieval in future iterations
    memory: Arc<dyn MemoryPort>,
    runs: RwLock<HashMap<RunId, RunState>>,
    hooks: RwLock<Vec<(ToolHookConfig, HookHandler)>>,
}

impl AgentRuntime {
    /// Build a new runtime from the three backing ports.
    pub fn new(
        llm: Arc<dyn LlmPort>,
        tools: Arc<dyn ToolPort>,
        memory: Arc<dyn MemoryPort>,
    ) -> Self {
        Self {
            llm,
            tools,
            memory,
            runs: RwLock::new(HashMap::new()),
            hooks: RwLock::new(Vec::new()),
        }
    }

    /// Register a hook to run before or after tool calls.
    pub fn register_hook(&self, config: ToolHookConfig, handler: HookHandler) {
        // Use blocking write since this is called during setup, not in async hot path.
        self.hooks.blocking_write().push((config, handler));
    }

    /// Run pre-tool-use hooks, returning the (possibly modified) args or a denial.
    async fn run_pre_hooks(
        &self,
        tool_name: &str,
        args: &serde_json::Value,
    ) -> std::result::Result<serde_json::Value, String> {
        let hooks = self.hooks.read().await;
        let mut current_args = args.clone();
        for (cfg, handler) in hooks.iter() {
            if cfg.hook_point != HookPoint::PreToolUse {
                continue;
            }
            if !match_tool_pattern(&cfg.tool_pattern, tool_name) {
                continue;
            }
            match handler(tool_name, &current_args) {
                HookDecision::Allow => {}
                HookDecision::Modify(new_args) => {
                    debug!(hook_id = %cfg.id, %tool_name, "pre-hook modified args");
                    current_args = new_args;
                }
                HookDecision::Deny(reason) => {
                    debug!(hook_id = %cfg.id, %tool_name, %reason, "pre-hook denied tool call");
                    return Err(reason);
                }
            }
        }
        Ok(current_args)
    }

    /// Run post-tool-use hooks (informational only, results are ignored).
    async fn run_post_hooks(&self, tool_name: &str, args: &serde_json::Value) {
        let hooks = self.hooks.read().await;
        for (cfg, handler) in hooks.iter() {
            if cfg.hook_point != HookPoint::PostToolUse {
                continue;
            }
            if !match_tool_pattern(&cfg.tool_pattern, tool_name) {
                continue;
            }
            let _ = handler(tool_name, args);
        }
    }

    /// Build an [`LlmRequest`] from config, messages, and available tool definitions.
    fn build_request(
        config: &AgentConfig,
        messages: &[LlmMessage],
        tool_defs: &[ToolDefinition],
    ) -> LlmRequest {
        let model = config.model.clone().unwrap_or_else(|| ModelRef {
            provider: ModelProvider::Claude,
            model: "claude-sonnet-4-20250514".into(),
        });

        // Convert ToolDefinitions to the JSON Schema format LLM providers expect.
        // Tools marked `searchable` were discovered via tool_search and carry
        // `defer_loading: true` — the Claude API won't expand their schema into
        // the context window until the model actually invokes them.
        let tools: Vec<serde_json::Value> = tool_defs
            .iter()
            .map(|t| {
                let mut schema = serde_json::json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.parameters,
                });
                if t.searchable {
                    schema["defer_loading"] = serde_json::json!(true);
                }
                schema
            })
            .collect();

        LlmRequest {
            model,
            messages: messages.to_vec(),
            tools,
            temperature: None,
            max_tokens: Some(DEFAULT_MAX_OUTPUT_TOKENS),
            metadata: merge_llm_request_metadata(config),
        }
    }

    /// Extract the first tool call from an LLM response.
    ///
    /// Scans `Part::ToolCall` variants in the response content.
    fn extract_tool_call(response: &LlmResponse) -> Option<(String, String, serde_json::Value)> {
        for part in &response.content.parts {
            if let Part::ToolCall { id, name, args } = part {
                return Some((id.clone(), name.clone(), args.clone()));
            }
        }
        None
    }

    /// Execute a streaming agent run.
    ///
    /// Unlike [`AgentPort::run()`] which returns a complete `AgentOutput`,
    /// this emits incremental [`AgentEvent`]s via an `mpsc::Receiver`:
    /// - `TextDelta` for each LLM text chunk
    /// - `ToolCall` / `ToolResult` for tool interactions
    /// - `Done` with the final `AgentOutput`
    /// - `Error` on failure
    ///
    /// The `run_id` must have been created via [`AgentPort::create()`] first.
    pub async fn run_streaming(
        &self,
        run_id: &RunId,
        input: Content,
    ) -> Result<tokio::sync::mpsc::Receiver<AgentEvent>> {
        // Transition to Running + snapshot config.
        let config = {
            let mut runs = self.runs.write().await;
            let state = runs.get_mut(run_id).ok_or_else(|| RusvelError::NotFound {
                kind: "AgentRun".into(),
                id: run_id.to_string(),
            })?;
            if state.status == AgentStatus::Stopped {
                return Err(RusvelError::Agent("run has been stopped".into()));
            }
            state.status = AgentStatus::Running;
            state.config.clone()
        };

        let (tx, rx) = tokio::sync::mpsc::channel(64);
        let llm = self.llm.clone();
        let tools = self.tools.clone();
        let hooks_snapshot = self.hooks.read().await.clone();
        let run_id = *run_id;

        tokio::spawn(async move {
            let result =
                run_streaming_loop(&llm, &tools, &config, &run_id, input, &tx, &hooks_snapshot)
                    .await;
            match result {
                Ok(output) => {
                    let _ = tx.send(AgentEvent::Done { output }).await;
                }
                Err(e) => {
                    let _ = tx
                        .send(AgentEvent::Error {
                            message: e.to_string(),
                        })
                        .await;
                }
            }
        });

        Ok(rx)
    }
}

fn role_label(role: &LlmRole) -> &'static str {
    match role {
        LlmRole::System => "system",
        LlmRole::User => "user",
        LlmRole::Assistant => "assistant",
        LlmRole::Tool => "tool",
    }
}

fn content_to_plain(content: &Content) -> String {
    let mut out = String::new();
    for part in &content.parts {
        match part {
            Part::Text(t) => out.push_str(t),
            Part::ToolCall { name, args, .. } => {
                out.push_str(&format!("[tool:{name} {}]", args));
            }
            Part::ToolResult {
                content: c,
                is_error,
                ..
            } => {
                out.push_str(&format!(
                    "[tool_result{}] {}",
                    if *is_error { ":error" } else { "" },
                    c
                ));
            }
            _ => {}
        }
    }
    out
}

fn extract_text_response(content: &Content) -> String {
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

/// Guard terminal completions before treating them as success.
///
/// A [`FinishReason::Length`] finish with no visible text means the whole
/// output budget was consumed before any text was emitted (on
/// adaptive-thinking Claude models, thinking tokens count against
/// `max_tokens`). Silently returning it would complete the run with empty
/// output (#12), so it is surfaced as an error instead. A `Length` finish
/// that did produce text is allowed through with a warning (truncated).
fn check_terminal_response(response: &LlmResponse) -> Result<()> {
    if response.finish_reason == FinishReason::Length {
        if extract_text_response(&response.content).trim().is_empty() {
            return Err(RusvelError::Llm(
                "model hit the max_tokens limit before producing any visible text \
                 (the output budget may have been consumed by thinking); \
                 retry with a larger max_tokens"
                    .into(),
            ));
        }
        warn!("LLM finished with max_tokens; output text is likely truncated");
    }
    Ok(())
}

/// Match a tool name against a hook pattern.
///
/// Supports: exact match, `*` (matches everything), or `prefix*` (starts-with).
fn match_tool_pattern(pattern: &str, name: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        return name.starts_with(prefix);
    }
    pattern == name
}

/// Run pre-tool-use hooks from a snapshot (for use in free functions).
fn run_hooks_pre(
    hooks: &[(ToolHookConfig, HookHandler)],
    tool_name: &str,
    args: &serde_json::Value,
) -> std::result::Result<serde_json::Value, String> {
    let mut current_args = args.clone();
    for (cfg, handler) in hooks {
        if cfg.hook_point != HookPoint::PreToolUse {
            continue;
        }
        if !match_tool_pattern(&cfg.tool_pattern, tool_name) {
            continue;
        }
        match handler(tool_name, &current_args) {
            HookDecision::Allow => {}
            HookDecision::Modify(new_args) => {
                debug!(hook_id = %cfg.id, %tool_name, "pre-hook modified args");
                current_args = new_args;
            }
            HookDecision::Deny(reason) => {
                debug!(hook_id = %cfg.id, %tool_name, %reason, "pre-hook denied tool call");
                return Err(reason);
            }
        }
    }
    Ok(current_args)
}

/// Run post-tool-use hooks from a snapshot (informational, results ignored).
fn run_hooks_post(
    hooks: &[(ToolHookConfig, HookHandler)],
    tool_name: &str,
    args: &serde_json::Value,
) {
    for (cfg, handler) in hooks {
        if cfg.hook_point != HookPoint::PostToolUse {
            continue;
        }
        if !match_tool_pattern(&cfg.tool_pattern, tool_name) {
            continue;
        }
        let _ = handler(tool_name, args);
    }
}

/// Ensures the kept suffix does not start with a lone [`LlmRole::Tool`] message (invalid ordering).
fn adjust_suffix_start(messages: &[LlmMessage], keep: usize) -> usize {
    if messages.is_empty() {
        return 0;
    }
    let mut start = messages.len().saturating_sub(keep);
    while start < messages.len() && messages[start].role == LlmRole::Tool {
        if start == 0 {
            break;
        }
        start -= 1;
    }
    start
}

/// If `messages.len() > COMPACT_THRESHOLD`, takes the oldest messages (all except the
/// last [`COMPACT_KEEP_RECENT`]), formats them as text, sends a summarization prompt to
/// the LLM with `rusvel.model_tier: fast`, and replaces those messages with a single
/// System summary message while keeping the most recent messages intact.
async fn compact_messages(llm: &dyn LlmPort, messages: &mut Vec<LlmMessage>) {
    if messages.len() <= COMPACT_THRESHOLD {
        return;
    }

    let system_len = messages
        .first()
        .is_some_and(|m| m.role == LlmRole::System)
        .then_some(1)
        .unwrap_or(0);

    let suffix_start = adjust_suffix_start(messages, COMPACT_KEEP_RECENT);
    if suffix_start <= system_len {
        return;
    }

    let mut block_text = String::new();
    for m in &messages[system_len..suffix_start] {
        block_text.push_str(&format!(
            "[{}] {}\n",
            role_label(&m.role),
            content_to_plain(&m.content)
        ));
    }

    let summarize_req = LlmRequest {
        model: ModelRef {
            provider: ModelProvider::Claude,
            model: "claude-sonnet-4-20250514".into(),
        },
        messages: vec![
            LlmMessage {
                role: LlmRole::System,
                content: Content::text(
                    "Summarize the prior conversation turns below into a concise summary for context. \
Preserve key facts, user goals, tool calls and outcomes, and decisions. \
Output plain text only, no preamble.",
                ),
            },
            LlmMessage {
                role: LlmRole::User,
                content: Content::text(block_text),
            },
        ],
        tools: vec![],
        temperature: Some(0.2),
        max_tokens: Some(2048),
        metadata: serde_json::json!({
            "rusvel.model_tier": "fast",
        }),
    };

    let summary_text = match llm.generate(summarize_req).await {
        Ok(resp) => {
            let t = extract_text_response(&resp.content);
            if t.trim().is_empty() {
                warn!("context compaction returned empty summary; using placeholder");
                "[Earlier conversation summarized (empty model output).]".to_string()
            } else {
                t
            }
        }
        Err(e) => {
            warn!(%e, "context compaction summary failed; using placeholder");
            "[Earlier messages omitted (summary unavailable).]".to_string()
        }
    };

    let mut new_messages =
        Vec::with_capacity(system_len + 1 + messages.len().saturating_sub(suffix_start));
    if system_len > 0 {
        new_messages.push(messages[0].clone());
    }
    new_messages.push(LlmMessage {
        role: LlmRole::System,
        content: Content::text(format!("[Earlier conversation summary]\n{summary_text}")),
    });
    new_messages.extend(messages[suffix_start..].iter().cloned());
    *messages = new_messages;

    debug!(new_len = messages.len(), "context compaction applied");
}

/// Inner streaming agent loop, factored out so it can be spawned as a task.
/// For each entry in `metadata.ag_ui_state_deltas`, emit [`AgentEvent::StateDelta`].
async fn emit_tool_ag_ui_state_deltas(
    tx: &tokio::sync::mpsc::Sender<AgentEvent>,
    metadata: &serde_json::Value,
) {
    if let Some(arr) = metadata
        .get("ag_ui_state_deltas")
        .and_then(|v| v.as_array())
    {
        for d in arr {
            let _ = tx.send(AgentEvent::StateDelta { delta: d.clone() }).await;
        }
    }
}

async fn run_streaming_loop(
    llm: &Arc<dyn LlmPort>,
    tools: &Arc<dyn ToolPort>,
    config: &AgentConfig,
    run_id: &RunId,
    input: Content,
    tx: &tokio::sync::mpsc::Sender<AgentEvent>,
    hooks: &[(ToolHookConfig, HookHandler)],
) -> Result<AgentOutput> {
    let mut messages: Vec<LlmMessage> = Vec::new();
    if let Some(ref instructions) = config.instructions {
        messages.push(LlmMessage {
            role: LlmRole::System,
            content: Content::text(instructions),
        });
    }
    messages.push(LlmMessage {
        role: LlmRole::User,
        content: input,
    });

    let mut total_usage = LlmUsage::default();
    let mut total_cost_usd: f64 = 0.0;
    let mut tool_calls: u32 = 0;

    // Deferred tool loading: only non-searchable tools go into the initial prompt.
    // Searchable tools are discovered via `tool_search` and added dynamically.
    let mut tool_defs: Vec<ToolDefinition> =
        tools.list().into_iter().filter(|t| !t.searchable).collect();
    filter_tool_defs_for_config(&mut tool_defs, &config.tools);
    let mut consecutive_failures: u32 = 0;

    let max_iter = config.max_iterations.unwrap_or(DEFAULT_MAX_ITERATIONS);
    for iteration in 0..max_iter {
        debug!(%run_id, iteration, "streaming agent loop iteration");

        compact_messages(llm.as_ref(), &mut messages).await;

        let request = AgentRuntime::build_request(config, &messages, &tool_defs);
        let request_model = request.model.clone();

        // Use stream() for incremental deltas
        let mut stream_rx = llm.stream(request).await?;
        let mut response: Option<LlmResponse> = None;

        while let Some(event) = stream_rx.recv().await {
            match event {
                LlmStreamEvent::Delta(text) => {
                    let _ = tx.send(AgentEvent::TextDelta { text }).await;
                }
                LlmStreamEvent::ToolUse { id, name, args } => {
                    // Tool use events from stream are informational;
                    // we handle them via the Done response below.
                    let _ = tx
                        .send(AgentEvent::ToolCall {
                            tool_call_id: id.clone(),
                            name: name.clone(),
                            args: args.clone(),
                        })
                        .await;
                }
                LlmStreamEvent::Done(resp) => {
                    response = Some(resp);
                    break;
                }
                LlmStreamEvent::Error(msg) => {
                    return Err(RusvelError::Llm(msg));
                }
            }
        }

        let response =
            response.ok_or_else(|| RusvelError::Llm("stream ended without Done event".into()))?;

        total_usage.input_tokens += response.usage.input_tokens;
        total_usage.output_tokens += response.usage.output_tokens;
        total_cost_usd += response_cost_usd(&request_model, &response);

        match response.finish_reason {
            FinishReason::Stop | FinishReason::Length | FinishReason::ContentFilter => {
                check_terminal_response(&response)?;
                return Ok(AgentOutput {
                    run_id: *run_id,
                    content: response.content,
                    tool_calls,
                    usage: total_usage,
                    cost_estimate: total_cost_usd,
                    metadata: serde_json::json!({}),
                });
            }
            FinishReason::ToolUse => {
                let (tool_call_id, tool_name, tool_args) =
                    AgentRuntime::extract_tool_call(&response).ok_or_else(|| {
                        RusvelError::Agent(
                            "ToolUse finish_reason but no Part::ToolCall found".into(),
                        )
                    })?;

                debug!(%run_id, %tool_name, "streaming: calling tool");

                messages.push(LlmMessage {
                    role: LlmRole::Assistant,
                    content: response.content,
                });

                // Run pre-tool-use hooks.
                let effective_args = match run_hooks_pre(hooks, &tool_name, &tool_args) {
                    Ok(args) => args,
                    Err(reason) => {
                        tool_calls += 1;
                        let _ = tx
                            .send(AgentEvent::ToolResult {
                                tool_call_id: tool_call_id.clone(),
                                name: tool_name.clone(),
                                output: reason.clone(),
                                is_error: true,
                            })
                            .await;
                        messages.push(LlmMessage {
                            role: LlmRole::Tool,
                            content: Content {
                                parts: vec![Part::ToolResult {
                                    tool_call_id,
                                    content: format!("Tool denied by hook: {reason}"),
                                    is_error: true,
                                }],
                            },
                        });
                        continue;
                    }
                };

                if let Some(msg) = agent_permission_blocks_tool(config, &tool_name, &tool_defs) {
                    tool_calls += 1;
                    let _ = tx
                        .send(AgentEvent::ToolResult {
                            tool_call_id: tool_call_id.clone(),
                            name: tool_name.clone(),
                            output: msg.clone(),
                            is_error: true,
                        })
                        .await;
                    messages.push(LlmMessage {
                        role: LlmRole::Tool,
                        content: Content {
                            parts: vec![Part::ToolResult {
                                tool_call_id,
                                content: msg,
                                is_error: true,
                            }],
                        },
                    });
                    continue;
                }

                let mut call_args = effective_args;
                if let serde_json::Value::Object(ref mut m) = call_args {
                    if let Some(dept) = config
                        .metadata
                        .get(RUSVEL_META_DEPARTMENT_ID)
                        .and_then(|v| v.as_str())
                    {
                        m.insert(
                            "__department_id".into(),
                            serde_json::Value::String(dept.to_string()),
                        );
                    }
                }

                let tool_result = tools.call(&tool_name, call_args).await;
                tool_calls += 1;

                if let Ok(ref r) = tool_result {
                    emit_tool_ag_ui_state_deltas(tx, &r.metadata).await;
                }

                // Run post-tool-use hooks (informational).
                run_hooks_post(hooks, &tool_name, &tool_args);

                // Deferred tool loading: when tool_search is called,
                // inject discovered tools into subsequent LLM requests.
                if tool_name == "tool_search" {
                    if let Ok(ref r) = tool_result {
                        if let Some(names) = r.metadata.get("discovered_tools") {
                            if let Some(arr) = names.as_array() {
                                let query = tool_args
                                    .get("query")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("");
                                let discovered = tools.search(query, arr.len().max(10));
                                for tool in discovered {
                                    if tool_allowed_by_dept_config(&tool.name, &config.tools)
                                        && !tool_defs.iter().any(|t| t.name == tool.name)
                                    {
                                        debug!(%run_id, tool_name = %tool.name, "discovered tool via search");
                                        tool_defs.push(tool);
                                    }
                                }
                            }
                        }
                    }
                }

                let (result_text, is_error) = match &tool_result {
                    Ok(r) => {
                        let text = r
                            .output
                            .parts
                            .iter()
                            .filter_map(|p| match p {
                                Part::Text(t) => Some(t.as_str()),
                                _ => None,
                            })
                            .collect::<Vec<_>>()
                            .join("");
                        (text, !r.success)
                    }
                    Err(e) => (format!("Tool error: {e}"), true),
                };

                let _ = tx
                    .send(AgentEvent::ToolResult {
                        tool_call_id: tool_call_id.clone(),
                        name: tool_name,
                        output: result_text.clone(),
                        is_error,
                    })
                    .await;

                messages.push(LlmMessage {
                    role: LlmRole::Tool,
                    content: Content {
                        parts: vec![Part::ToolResult {
                            tool_call_id,
                            content: result_text,
                            is_error,
                        }],
                    },
                });

                // Error recovery: inject reflection prompts on consecutive failures
                if is_error {
                    consecutive_failures += 1;
                    if consecutive_failures >= 3 {
                        messages.push(LlmMessage {
                            role: LlmRole::System,
                            content: Content::text(
                                "[REFLECTION REQUIRED] 3 consecutive tool failures. \
                                 Stop and reassess your approach. What assumption is wrong? \
                                 Try a completely different strategy.",
                            ),
                        });
                    }
                } else {
                    consecutive_failures = 0;
                }
            }
            FinishReason::Other(ref reason) => {
                return Err(RusvelError::Agent(format!(
                    "unexpected finish reason: {reason}"
                )));
            }
        }
    }

    Err(RusvelError::Agent(format!(
        "agent loop exceeded {max_iter} iterations"
    )))
}

#[async_trait]
impl AgentPort for AgentRuntime {
    async fn create(&self, config: AgentConfig) -> Result<RunId> {
        let run_id = RunId::new();
        let state = RunState {
            config,
            status: AgentStatus::Idle,
        };
        self.runs.write().await.insert(run_id, state);
        info!(%run_id, "agent run created");
        Ok(run_id)
    }

    async fn run(&self, run_id: &RunId, input: Content) -> Result<AgentOutput> {
        // Transition to Running.
        {
            let mut runs = self.runs.write().await;
            let state = runs.get_mut(run_id).ok_or_else(|| RusvelError::NotFound {
                kind: "AgentRun".into(),
                id: run_id.to_string(),
            })?;
            if state.status == AgentStatus::Stopped {
                return Err(RusvelError::Agent("run has been stopped".into()));
            }
            state.status = AgentStatus::Running;
        }

        // Snapshot config for use outside the lock.
        let config = {
            let runs = self.runs.read().await;
            runs.get(run_id)
                .ok_or_else(|| RusvelError::NotFound {
                    kind: "AgentRun".into(),
                    id: run_id.to_string(),
                })?
                .config
                .clone()
        };

        // Seed the conversation with system instructions + user input.
        let mut messages: Vec<LlmMessage> = Vec::new();
        if let Some(ref instructions) = config.instructions {
            messages.push(LlmMessage {
                role: LlmRole::System,
                content: Content::text(instructions),
            });
        }
        messages.push(LlmMessage {
            role: LlmRole::User,
            content: input,
        });

        let mut total_usage = LlmUsage::default();
        let mut total_cost_usd: f64 = 0.0;
        let mut tool_calls: u32 = 0;

        // Deferred tool loading: only non-searchable tools in the initial prompt.
        let mut tool_defs: Vec<ToolDefinition> = self
            .tools
            .list()
            .into_iter()
            .filter(|t| !t.searchable)
            .collect();
        filter_tool_defs_for_config(&mut tool_defs, &config.tools);
        let mut consecutive_failures: u32 = 0;

        // ── Agent loop ───────────────────────────────────────────────
        let max_iter = config.max_iterations.unwrap_or(DEFAULT_MAX_ITERATIONS);
        for iteration in 0..max_iter {
            debug!(%run_id, iteration, "agent loop iteration");

            // Check if stopped between iterations.
            {
                let runs = self.runs.read().await;
                if let Some(state) = runs.get(run_id)
                    && state.status == AgentStatus::Stopped
                {
                    return Err(RusvelError::Agent("run was stopped".into()));
                }
            }

            compact_messages(self.llm.as_ref(), &mut messages).await;

            let request = Self::build_request(&config, &messages, &tool_defs);
            let request_model = request.model.clone();
            let response = self.llm.generate(request).await?;

            // Accumulate usage and cost.
            total_usage.input_tokens += response.usage.input_tokens;
            total_usage.output_tokens += response.usage.output_tokens;
            total_cost_usd += response_cost_usd(&request_model, &response);

            match response.finish_reason {
                FinishReason::Stop | FinishReason::Length | FinishReason::ContentFilter => {
                    if let Err(e) = check_terminal_response(&response) {
                        self.runs.write().await.entry(*run_id).and_modify(|s| {
                            s.status = AgentStatus::Failed;
                        });
                        return Err(e);
                    }
                    // Terminal — return the final content.
                    let output = AgentOutput {
                        run_id: *run_id,
                        content: response.content,
                        tool_calls,
                        usage: total_usage,
                        cost_estimate: total_cost_usd,
                        metadata: serde_json::json!({}),
                    };

                    self.runs.write().await.entry(*run_id).and_modify(|s| {
                        s.status = AgentStatus::Completed;
                    });
                    info!(%run_id, tool_calls, "agent run completed");
                    return Ok(output);
                }
                FinishReason::ToolUse => {
                    // Update status.
                    self.runs.write().await.entry(*run_id).and_modify(|s| {
                        s.status = AgentStatus::AwaitingTool;
                    });

                    let (tool_call_id, tool_name, tool_args) = Self::extract_tool_call(&response)
                        .ok_or_else(|| {
                        RusvelError::Agent(
                            "ToolUse finish_reason but no Part::ToolCall found".into(),
                        )
                    })?;

                    debug!(%run_id, %tool_name, "calling tool");

                    // Append the assistant message (includes the ToolCall part).
                    messages.push(LlmMessage {
                        role: LlmRole::Assistant,
                        content: response.content,
                    });

                    // Run pre-tool-use hooks.
                    let effective_args = match self.run_pre_hooks(&tool_name, &tool_args).await {
                        Ok(args) => args,
                        Err(reason) => {
                            tool_calls += 1;
                            messages.push(LlmMessage {
                                role: LlmRole::Tool,
                                content: Content {
                                    parts: vec![Part::ToolResult {
                                        tool_call_id,
                                        content: format!("Tool denied by hook: {reason}"),
                                        is_error: true,
                                    }],
                                },
                            });
                            self.runs.write().await.entry(*run_id).and_modify(|s| {
                                s.status = AgentStatus::Running;
                            });
                            continue;
                        }
                    };

                    if let Some(msg) = agent_permission_blocks_tool(&config, &tool_name, &tool_defs)
                    {
                        tool_calls += 1;
                        messages.push(LlmMessage {
                            role: LlmRole::Tool,
                            content: Content {
                                parts: vec![Part::ToolResult {
                                    tool_call_id,
                                    content: msg,
                                    is_error: true,
                                }],
                            },
                        });
                        self.runs.write().await.entry(*run_id).and_modify(|s| {
                            s.status = AgentStatus::Running;
                        });
                        continue;
                    }

                    // Execute the tool.
                    let mut call_args = effective_args;
                    if let serde_json::Value::Object(ref mut m) = call_args {
                        if let Some(dept) = config
                            .metadata
                            .get(RUSVEL_META_DEPARTMENT_ID)
                            .and_then(|v| v.as_str())
                        {
                            m.insert(
                                "__department_id".into(),
                                serde_json::Value::String(dept.to_string()),
                            );
                        }
                    }
                    let tool_result = self.tools.call(&tool_name, call_args).await;
                    tool_calls += 1;

                    // Run post-tool-use hooks (informational).
                    self.run_post_hooks(&tool_name, &tool_args).await;

                    // Deferred tool loading: inject discovered tools.
                    if tool_name == "tool_search" {
                        if let Ok(ref r) = tool_result {
                            if let Some(names) = r.metadata.get("discovered_tools") {
                                if let Some(arr) = names.as_array() {
                                    let query = tool_args
                                        .get("query")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("");
                                    let discovered = self.tools.search(query, arr.len().max(10));
                                    for tool in discovered {
                                        if tool_allowed_by_dept_config(&tool.name, &config.tools)
                                            && !tool_defs.iter().any(|t| t.name == tool.name)
                                        {
                                            debug!(%run_id, tool_name = %tool.name, "discovered tool via search");
                                            tool_defs.push(tool);
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Build a ToolResult part with the proper tool_call_id.
                    let (result_text, is_error) = match &tool_result {
                        Ok(r) => {
                            let text = r
                                .output
                                .parts
                                .iter()
                                .filter_map(|p| match p {
                                    Part::Text(t) => Some(t.as_str()),
                                    _ => None,
                                })
                                .collect::<Vec<_>>()
                                .join("");
                            (text, !r.success)
                        }
                        Err(e) => (format!("Tool error: {e}"), true),
                    };

                    messages.push(LlmMessage {
                        role: LlmRole::Tool,
                        content: Content {
                            parts: vec![Part::ToolResult {
                                tool_call_id,
                                content: result_text,
                                is_error,
                            }],
                        },
                    });

                    // Error recovery: inject reflection on consecutive failures
                    if is_error {
                        consecutive_failures += 1;
                        if consecutive_failures >= 3 {
                            messages.push(LlmMessage {
                                role: LlmRole::System,
                                content: Content::text(
                                    "[REFLECTION REQUIRED] 3 consecutive tool failures. \
                                     Stop and reassess your approach. What assumption is wrong? \
                                     Try a completely different strategy.",
                                ),
                            });
                        }
                    } else {
                        consecutive_failures = 0;
                    }

                    // Transition back to Running.
                    self.runs.write().await.entry(*run_id).and_modify(|s| {
                        s.status = AgentStatus::Running;
                    });
                }
                FinishReason::Other(ref reason) => {
                    warn!(%run_id, %reason, "unexpected finish reason");
                    return Err(RusvelError::Agent(format!(
                        "unexpected finish reason: {reason}"
                    )));
                }
            }
        }

        // Exhausted iterations.
        self.runs.write().await.entry(*run_id).and_modify(|s| {
            s.status = AgentStatus::Failed;
        });
        Err(RusvelError::Agent(format!(
            "agent loop exceeded {max_iter} iterations"
        )))
    }

    async fn stop(&self, run_id: &RunId) -> Result<()> {
        let mut runs = self.runs.write().await;
        let state = runs.get_mut(run_id).ok_or_else(|| RusvelError::NotFound {
            kind: "AgentRun".into(),
            id: run_id.to_string(),
        })?;
        state.status = AgentStatus::Stopped;
        info!(%run_id, "agent run stopped");
        Ok(())
    }

    async fn status(&self, run_id: &RunId) -> Result<AgentStatus> {
        let runs = self.runs.read().await;
        let state = runs.get(run_id).ok_or_else(|| RusvelError::NotFound {
            kind: "AgentRun".into(),
            id: run_id.to_string(),
        })?;
        Ok(state.status.clone())
    }
}

// ════════════════════════════════════════════════════════════════════
//  Tests
// ════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── Mock LlmPort ─────────────────────────────────────────────

    struct MockLlm {
        /// Responses to return, consumed in order.
        responses: RwLock<Vec<LlmResponse>>,
    }

    impl MockLlm {
        fn new(responses: Vec<LlmResponse>) -> Self {
            Self {
                responses: RwLock::new(responses),
            }
        }
    }

    #[async_trait]
    impl LlmPort for MockLlm {
        async fn generate(&self, _request: LlmRequest) -> Result<LlmResponse> {
            let mut resps = self.responses.write().await;
            if resps.is_empty() {
                return Err(RusvelError::Llm("no more mock responses".into()));
            }
            Ok(resps.remove(0))
        }

        async fn embed(&self, _model: &ModelRef, _text: &str) -> Result<Vec<f32>> {
            Ok(vec![0.0; 128])
        }

        async fn list_models(&self) -> Result<Vec<ModelRef>> {
            Ok(vec![])
        }
    }

    // ── Mock ToolPort ────────────────────────────────────────────

    struct MockTool;

    #[async_trait]
    impl ToolPort for MockTool {
        async fn register(&self, _tool: ToolDefinition) -> Result<()> {
            Ok(())
        }

        async fn call(&self, _name: &str, _args: serde_json::Value) -> Result<ToolResult> {
            Ok(ToolResult {
                success: true,
                output: Content::text("tool result"),
                metadata: serde_json::json!({}),
            })
        }

        fn list(&self) -> Vec<ToolDefinition> {
            vec![]
        }

        fn search(&self, _query: &str, _limit: usize) -> Vec<ToolDefinition> {
            vec![]
        }

        fn schema(&self, _name: &str) -> Option<serde_json::Value> {
            None
        }
    }

    // ── Mock MemoryPort ──────────────────────────────────────────

    struct MockMemory;

    #[async_trait]
    impl MemoryPort for MockMemory {
        async fn store(&self, _entry: MemoryEntry) -> Result<uuid::Uuid> {
            Ok(uuid::Uuid::now_v7())
        }

        async fn recall(&self, _id: &uuid::Uuid) -> Result<Option<MemoryEntry>> {
            Ok(None)
        }

        async fn search(
            &self,
            _session_id: &SessionId,
            _query: &str,
            _limit: usize,
        ) -> Result<Vec<MemoryEntry>> {
            Ok(vec![])
        }

        async fn forget(&self, _id: &uuid::Uuid) -> Result<()> {
            Ok(())
        }
    }

    // ── Helpers ──────────────────────────────────────────────────

    fn make_config() -> AgentConfig {
        AgentConfig {
            profile_id: None,
            session_id: SessionId::new(),
            model: None,
            tools: vec![],
            instructions: Some("You are a helpful assistant.".into()),
            budget_limit: None,
            max_iterations: None,
            permission_mode: Default::default(),
            metadata: serde_json::json!({}),
        }
    }

    fn stop_response(text: &str) -> LlmResponse {
        LlmResponse {
            content: Content::text(text),
            finish_reason: FinishReason::Stop,
            usage: LlmUsage {
                input_tokens: 10,
                output_tokens: 20,
            },
            metadata: serde_json::json!({}),
        }
    }

    fn tool_use_response(tool_name: &str) -> LlmResponse {
        LlmResponse {
            content: Content {
                parts: vec![Part::ToolCall {
                    id: format!("call_{tool_name}"),
                    name: tool_name.into(),
                    args: serde_json::json!({"query": "test"}),
                }],
            },
            finish_reason: FinishReason::ToolUse,
            usage: LlmUsage {
                input_tokens: 10,
                output_tokens: 5,
            },
            metadata: serde_json::json!({}),
        }
    }

    fn make_runtime(responses: Vec<LlmResponse>) -> AgentRuntime {
        AgentRuntime::new(
            Arc::new(MockLlm::new(responses)),
            Arc::new(MockTool),
            Arc::new(MockMemory),
        )
    }

    // ── Tests ────────────────────────────────────────────────────

    #[tokio::test]
    async fn create_and_status() {
        let rt = make_runtime(vec![]);
        let run_id = rt.create(make_config()).await.unwrap();
        assert_eq!(rt.status(&run_id).await.unwrap(), AgentStatus::Idle);
    }

    #[tokio::test]
    async fn simple_run_returns_output() {
        let rt = make_runtime(vec![stop_response("Hello!")]);
        let run_id = rt.create(make_config()).await.unwrap();
        let output = rt.run(&run_id, Content::text("Hi")).await.unwrap();

        assert_eq!(output.run_id, run_id);
        assert_eq!(output.tool_calls, 0);
        assert_eq!(output.usage.input_tokens, 10);
        assert_eq!(output.usage.output_tokens, 20);
        assert_eq!(rt.status(&run_id).await.unwrap(), AgentStatus::Completed);
    }

    #[tokio::test]
    async fn tool_use_loop() {
        let rt = make_runtime(vec![
            tool_use_response("search"),
            stop_response("Here is your answer"),
        ]);
        let run_id = rt.create(make_config()).await.unwrap();
        let output = rt
            .run(&run_id, Content::text("Find something"))
            .await
            .unwrap();

        assert_eq!(output.tool_calls, 1);
        assert_eq!(output.usage.input_tokens, 20); // 10 + 10
        assert_eq!(output.usage.output_tokens, 25); // 5 + 20
    }

    #[tokio::test]
    async fn run_accumulates_cost_from_response_metadata() {
        // The cost-tracking LLM wrapper stamps per-call cost into
        // metadata.cost_usd; the loop must sum it into cost_estimate.
        let mut first = tool_use_response("search");
        first.metadata = serde_json::json!({ "cost_usd": 0.25 });
        let mut last = stop_response("done");
        last.metadata = serde_json::json!({ "cost_usd": 0.5 });

        let rt = make_runtime(vec![first, last]);
        let run_id = rt.create(make_config()).await.unwrap();
        let output = rt.run(&run_id, Content::text("Hi")).await.unwrap();

        assert!((output.cost_estimate - 0.75).abs() < 1e-12);
    }

    #[tokio::test]
    async fn run_estimates_cost_when_metadata_absent() {
        // Bare providers carry no stamped cost; fall back to list-price
        // estimate for the requested model (default: Claude sonnet, $3/$15).
        let rt = make_runtime(vec![stop_response("Hello!")]);
        let run_id = rt.create(make_config()).await.unwrap();
        let output = rt.run(&run_id, Content::text("Hi")).await.unwrap();

        let expected = 10.0 * 3.0 / 1e6 + 20.0 * 15.0 / 1e6;
        assert!(
            (output.cost_estimate - expected).abs() < 1e-12,
            "got {}",
            output.cost_estimate
        );
    }

    #[tokio::test]
    async fn stop_prevents_run() {
        let rt = make_runtime(vec![stop_response("ok")]);
        let run_id = rt.create(make_config()).await.unwrap();
        rt.stop(&run_id).await.unwrap();

        let err = rt.run(&run_id, Content::text("Hi")).await.unwrap_err();
        assert!(err.to_string().contains("stopped"));
    }

    #[tokio::test]
    async fn unknown_run_id_errors() {
        let rt = make_runtime(vec![]);
        let fake = RunId::new();
        let err = rt.status(&fake).await.unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[tokio::test]
    async fn max_iterations_exceeded() {
        // 11 tool-use responses should exceed a 10-iteration cap (default max is higher).
        let responses: Vec<LlmResponse> = (0..11).map(|_| tool_use_response("search")).collect();
        let rt = make_runtime(responses);
        let mut cfg = make_config();
        cfg.max_iterations = Some(10);
        let run_id = rt.create(cfg).await.unwrap();
        let err = rt.run(&run_id, Content::text("loop")).await.unwrap_err();
        assert!(err.to_string().contains("exceeded"));
        assert_eq!(rt.status(&run_id).await.unwrap(), AgentStatus::Failed);
    }

    fn length_response(text: &str) -> LlmResponse {
        LlmResponse {
            content: if text.is_empty() {
                Content { parts: vec![] }
            } else {
                Content::text(text)
            },
            finish_reason: FinishReason::Length,
            usage: LlmUsage {
                input_tokens: 10,
                output_tokens: 200,
            },
            metadata: serde_json::json!({}),
        }
    }

    #[tokio::test]
    async fn length_finish_with_empty_text_errors() {
        // #12: an all-thinking max_tokens completion must not be reported
        // as a clean empty success.
        let rt = make_runtime(vec![length_response("")]);
        let run_id = rt.create(make_config()).await.unwrap();
        let err = rt.run(&run_id, Content::text("Hi")).await.unwrap_err();
        assert!(err.to_string().contains("max_tokens"));
        assert_eq!(rt.status(&run_id).await.unwrap(), AgentStatus::Failed);
    }

    #[tokio::test]
    async fn length_finish_with_text_still_completes() {
        let rt = make_runtime(vec![length_response("Partial answer")]);
        let run_id = rt.create(make_config()).await.unwrap();
        let output = rt.run(&run_id, Content::text("Hi")).await.unwrap();
        assert_eq!(crate::agent_output_to_plain(&output), "Partial answer");
        assert_eq!(rt.status(&run_id).await.unwrap(), AgentStatus::Completed);
    }

    #[tokio::test]
    async fn build_request_sets_default_max_tokens() {
        let request = AgentRuntime::build_request(&make_config(), &[], &[]);
        assert_eq!(request.max_tokens, Some(DEFAULT_MAX_OUTPUT_TOKENS));
        assert!(request.max_tokens.unwrap() >= 8192);
    }

    // ── Streaming mock ─────────────────────────────────────────

    /// Mock LLM that emits multiple Delta events before Done.
    struct MockStreamingLlm {
        responses: RwLock<Vec<LlmResponse>>,
        /// Number of delta chunks to emit per response.
        deltas_per_response: usize,
    }

    impl MockStreamingLlm {
        fn new(responses: Vec<LlmResponse>, deltas_per_response: usize) -> Self {
            Self {
                responses: RwLock::new(responses),
                deltas_per_response,
            }
        }
    }

    #[async_trait]
    impl LlmPort for MockStreamingLlm {
        async fn generate(&self, _request: LlmRequest) -> Result<LlmResponse> {
            let mut resps = self.responses.write().await;
            if resps.is_empty() {
                return Err(RusvelError::Llm("no more mock responses".into()));
            }
            Ok(resps.remove(0))
        }

        async fn stream(
            &self,
            _request: LlmRequest,
        ) -> Result<tokio::sync::mpsc::Receiver<LlmStreamEvent>> {
            let mut resps = self.responses.write().await;
            if resps.is_empty() {
                return Err(RusvelError::Llm("no more mock responses".into()));
            }
            let response = resps.remove(0);
            let deltas = self.deltas_per_response;

            let (tx, rx) = tokio::sync::mpsc::channel(64);
            tokio::spawn(async move {
                // Extract full text from response
                let full_text: String = response
                    .content
                    .parts
                    .iter()
                    .filter_map(|p| match p {
                        Part::Text(t) => Some(t.as_str()),
                        _ => None,
                    })
                    .collect();

                // Split into N delta chunks
                if !full_text.is_empty() && deltas > 0 {
                    let chunk_size = full_text.len().div_ceil(deltas);
                    for chunk in full_text.as_bytes().chunks(chunk_size) {
                        let text = String::from_utf8_lossy(chunk).to_string();
                        let _ = tx.send(LlmStreamEvent::Delta(text)).await;
                    }
                }

                let _ = tx.send(LlmStreamEvent::Done(response)).await;
            });

            Ok(rx)
        }

        async fn embed(&self, _model: &ModelRef, _text: &str) -> Result<Vec<f32>> {
            Ok(vec![0.0; 128])
        }

        async fn list_models(&self) -> Result<Vec<ModelRef>> {
            Ok(vec![])
        }
    }

    fn make_streaming_runtime(responses: Vec<LlmResponse>, deltas: usize) -> AgentRuntime {
        AgentRuntime::new(
            Arc::new(MockStreamingLlm::new(responses, deltas)),
            Arc::new(MockTool),
            Arc::new(MockMemory),
        )
    }

    // ── Streaming tests ────────────────────────────────────────

    #[tokio::test]
    async fn streaming_emits_deltas_then_done() {
        let rt = make_streaming_runtime(vec![stop_response("Hello world!")], 3);
        let run_id = rt.create(make_config()).await.unwrap();
        let mut rx = rt
            .run_streaming(&run_id, Content::text("Hi"))
            .await
            .unwrap();

        let mut deltas = Vec::new();
        let mut done = false;

        while let Some(event) = rx.recv().await {
            match event {
                AgentEvent::TextDelta { text } => deltas.push(text),
                AgentEvent::Done { output } => {
                    assert_eq!(output.run_id, run_id);
                    assert_eq!(output.tool_calls, 0);
                    assert!(
                        output.cost_estimate > 0.0,
                        "streaming Done must carry a priced cost_estimate"
                    );
                    done = true;
                }
                AgentEvent::Error { message } => panic!("unexpected error: {message}"),
                _ => {}
            }
        }

        assert!(done, "should receive Done event");
        assert!(
            deltas.len() >= 2,
            "should receive multiple deltas, got {}",
            deltas.len()
        );
        let reassembled: String = deltas.into_iter().collect();
        assert_eq!(reassembled, "Hello world!");
    }

    #[tokio::test]
    async fn streaming_with_tool_use() {
        let rt = make_streaming_runtime(
            vec![tool_use_response("search"), stop_response("Answer found")],
            2,
        );
        let run_id = rt.create(make_config()).await.unwrap();
        let mut rx = rt
            .run_streaming(&run_id, Content::text("Search"))
            .await
            .unwrap();

        let mut got_tool_result = false;
        let mut got_done = false;

        while let Some(event) = rx.recv().await {
            match event {
                AgentEvent::ToolResult { name, .. } => {
                    assert_eq!(name, "search");
                    got_tool_result = true;
                }
                AgentEvent::Done { output } => {
                    assert_eq!(output.tool_calls, 1);
                    got_done = true;
                }
                AgentEvent::Error { message } => panic!("unexpected error: {message}"),
                _ => {}
            }
        }

        assert!(got_tool_result, "should receive ToolResult");
        assert!(got_done, "should receive Done");
    }

    #[tokio::test]
    async fn streaming_length_finish_with_empty_text_emits_error() {
        // #12: streaming path must surface an Error event instead of a
        // Done event with empty output when max_tokens ate the budget.
        let rt = make_streaming_runtime(vec![length_response("")], 0);
        let run_id = rt.create(make_config()).await.unwrap();
        let mut rx = rt
            .run_streaming(&run_id, Content::text("Hi"))
            .await
            .unwrap();

        let mut got_error = false;
        while let Some(event) = rx.recv().await {
            match event {
                AgentEvent::Error { message } => {
                    assert!(message.contains("max_tokens"));
                    got_error = true;
                }
                AgentEvent::Done { .. } => {
                    panic!("empty Length completion must not emit Done")
                }
                _ => {}
            }
        }
        assert!(got_error, "should receive Error event");
    }

    #[tokio::test]
    async fn streaming_batch_fallback() {
        // Using the non-streaming MockLlm (uses default stream() impl)
        let rt = make_runtime(vec![stop_response("Batch response")]);
        let run_id = rt.create(make_config()).await.unwrap();
        let mut rx = rt
            .run_streaming(&run_id, Content::text("Hi"))
            .await
            .unwrap();

        let mut got_done = false;
        while let Some(event) = rx.recv().await {
            match event {
                AgentEvent::Done { output } => {
                    assert_eq!(output.run_id, run_id);
                    got_done = true;
                }
                AgentEvent::Error { message } => panic!("unexpected error: {message}"),
                _ => {}
            }
        }
        assert!(got_done, "batch fallback should emit Done");
    }
}
