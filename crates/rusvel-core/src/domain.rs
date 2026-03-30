//! Shared domain types used across all RUSVEL engines and adapters.
//!
//! **ADR-007:** Persisted and API-stable structs carry `metadata: serde_json::Value` for
//! schema evolution. Ephemeral LLM wire types (`Content`, `LlmRequest`, …) omit it to keep
//! token and serde overhead low.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::id::*;

// ════════════════════════════════════════════════════════════════════
//  Content — universal message type (inspired by adk-rust Content/Part)
// ════════════════════════════════════════════════════════════════════

/// A multi-part content value used as the universal message type
/// throughout the agent system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Content {
    pub parts: Vec<Part>,
}

impl Content {
    /// Convenience: create a `Content` with a single text part.
    pub fn text(s: impl Into<String>) -> Self {
        Self {
            parts: vec![Part::Text(s.into())],
        }
    }

    /// Returns `true` when there are no parts.
    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }
}

/// A single part inside a [`Content`] value.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum Part {
    Text(String),
    Image(Vec<u8>),
    /// Base64 image with explicit MIME type (Claude computer use / vision tool results).
    ImageBase64 {
        base64: String,
        media_type: String,
    },
    Audio(Vec<u8>),
    Video(Vec<u8>),
    File {
        name: String,
        data: Vec<u8>,
    },
    /// A tool invocation requested by the LLM.
    ToolCall {
        id: String,
        name: String,
        args: serde_json::Value,
    },
    /// The result of a tool invocation, sent back to the LLM.
    ToolResult {
        tool_call_id: String,
        content: String,
        is_error: bool,
    },
}

// ════════════════════════════════════════════════════════════════════
//  LLM streaming types
// ════════════════════════════════════════════════════════════════════

/// An event emitted during incremental LLM streaming.
///
/// Providers that support real streaming emit `Delta` events as text arrives,
/// followed by a final `Done` with the aggregated response. Providers without
/// native streaming use the default `LlmPort::stream()` implementation which
/// emits a single `Done`.
#[derive(Debug, Clone)]
pub enum LlmStreamEvent {
    /// Incremental text chunk from the model.
    Delta(String),
    /// A tool call the model wants to make.
    ToolUse {
        id: String,
        name: String,
        args: serde_json::Value,
    },
    /// Stream finished — contains the aggregated response.
    Done(LlmResponse),
    /// Stream error.
    Error(String),
}

// ════════════════════════════════════════════════════════════════════
//  LLM types
// ════════════════════════════════════════════════════════════════════

/// Reference to a specific model on a specific provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRef {
    pub provider: ModelProvider,
    pub model: String,
}

/// Supported LLM providers (open-ended via `Other`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModelProvider {
    Claude,
    OpenAI,
    Gemini,
    Ollama,
    Other(String),
}

/// Request sent to an LLM via [`LlmPort`](crate::ports::LlmPort).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmRequest {
    pub model: ModelRef,
    pub messages: Vec<LlmMessage>,
    pub tools: Vec<serde_json::Value>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
    pub metadata: serde_json::Value,
}

/// A single message in an LLM conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmMessage {
    pub role: LlmRole,
    pub content: Content,
}

/// Message roles in an LLM conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LlmRole {
    System,
    User,
    Assistant,
    Tool,
}

/// Response from an LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmResponse {
    pub content: Content,
    pub finish_reason: FinishReason,
    pub usage: LlmUsage,
    pub metadata: serde_json::Value,
}

/// Why the LLM stopped generating.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FinishReason {
    Stop,
    Length,
    ToolUse,
    ContentFilter,
    Other(String),
}

/// Token usage for a single LLM call.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LlmUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

/// Cost tier for routing and spend accounting (Haiku / Sonnet / Opus style).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelTier {
    Fast,
    Balanced,
    Premium,
}

impl std::fmt::Display for ModelTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModelTier::Fast => write!(f, "fast"),
            ModelTier::Balanced => write!(f, "balanced"),
            ModelTier::Premium => write!(f, "premium"),
        }
    }
}

impl std::str::FromStr for ModelTier {
    type Err = ();

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "fast" | "haiku" => Ok(ModelTier::Fast),
            "balanced" | "sonnet" | "standard" => Ok(ModelTier::Balanced),
            "premium" | "opus" | "max" => Ok(ModelTier::Premium),
            _ => Err(()),
        }
    }
}

impl ModelTier {
    /// Reads [`RUSVEL_META_MODEL_TIER`] from [`LlmRequest::metadata`](LlmRequest::metadata).
    pub fn from_request_metadata(meta: &serde_json::Value) -> Option<Self> {
        let v = meta.get(RUSVEL_META_MODEL_TIER)?;
        v.as_str()?.parse().ok()
    }
}

/// `LlmRequest.metadata` key for [`ModelTier`] (e.g. `"fast"` / `"balanced"` / `"premium"`).
pub const RUSVEL_META_MODEL_TIER: &str = "rusvel.model_tier";

/// Optional session scope for cost metrics (`SessionId` as string).
pub const RUSVEL_META_SESSION_ID: &str = "rusvel.session_id";

/// Department id for LLM spend attribution (`dept` path segment, e.g. `"harvest"`, or `"global"` for god chat).
pub const RUSVEL_META_DEPARTMENT_ID: &str = "rusvel.department_id";

/// Marks spend from async batch API (e.g. 50% discount vs sync).
pub const RUSVEL_META_BATCH: &str = "rusvel.batch";

/// Per-response model hint for cost when the caller did not supply a full [`LlmRequest`] (batch poll).
pub const RUSVEL_META_COST_MODEL: &str = "rusvel.cost_model";

/// Provider name for [`RUSVEL_META_COST_MODEL`] (e.g. `"Claude"`).
pub const RUSVEL_META_COST_PROVIDER: &str = "rusvel.cost_provider";

/// Batch discount multiplier applied to estimated USD (e.g. `0.5` for 50% off).
pub const RUSVEL_META_BATCH_DISCOUNT: &str = "rusvel.batch_discount";

/// Default multiplier for [`estimate_llm_cost_usd`] on async batch completions (list-price proxy).
pub const LLM_BATCH_COST_MULTIPLIER: f64 = 0.5;

// ════════════════════════════════════════════════════════════════════
//  LLM batch (LlmPort — async Message Batches / jobs layer)
// ════════════════════════════════════════════════════════════════════

/// Identifies a submitted batch job on a provider (used with [`crate::ports::LlmPort::poll_batch`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchHandle {
    pub provider: ModelProvider,
    pub id: String,
}

/// One row in [`LlmBatchRequest`]; `id` becomes the provider `custom_id`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmBatchItem {
    pub id: String,
    pub request: LlmRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmBatchRequest {
    pub items: Vec<LlmBatchItem>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmBatchSubmitResult {
    pub handle: BatchHandle,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Terminal and in-flight states for an async batch job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchJobStatus {
    InProgress,
    Ended,
    Canceling,
    Failed { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmBatchPollResult {
    pub status: BatchJobStatus,
    pub items: Vec<LlmBatchItemOutcome>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// One batch line result: either an [`LlmResponse`] or an error string from the provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmBatchItemOutcome {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<LlmResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl LlmBatchItemOutcome {
    pub fn ok(id: impl Into<String>, response: LlmResponse) -> Self {
        Self {
            id: id.into(),
            model: None,
            response: Some(response),
            error: None,
        }
    }

    pub fn ok_with_model(id: impl Into<String>, model: ModelRef, response: LlmResponse) -> Self {
        Self {
            id: id.into(),
            model: Some(model),
            response: Some(response),
            error: None,
        }
    }

    pub fn err(id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            model: None,
            response: None,
            error: Some(message.into()),
        }
    }
}

/// High-level batch lifecycle status for callers that want to track a batch
/// from submission through completion in a single enum.
///
/// Compared to [`BatchJobStatus`] (wire-level), this carries the responses
/// inline so callers can pattern-match once.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum BatchStatus {
    Pending {
        batch_id: String,
        submitted: DateTime<Utc>,
    },
    Processing {
        batch_id: String,
        completed: usize,
        total: usize,
    },
    Done {
        batch_id: String,
        responses: Vec<LlmResponse>,
    },
    Failed {
        batch_id: String,
        error: String,
    },
}

/// Heuristic USD estimate from usage (approximate public list prices; Ollama/local = 0).
pub fn estimate_llm_cost_usd(provider: &ModelProvider, model: &str, usage: &LlmUsage) -> f64 {
    let in_m = f64::from(usage.input_tokens) / 1_000_000.0;
    let out_m = f64::from(usage.output_tokens) / 1_000_000.0;
    match provider {
        ModelProvider::Claude => estimate_claude_usd(model, in_m, out_m),
        ModelProvider::OpenAI => estimate_openai_usd(model, in_m, out_m),
        ModelProvider::Ollama | ModelProvider::Gemini | ModelProvider::Other(_) => 0.0,
    }
}

fn estimate_claude_usd(model: &str, in_m: f64, out_m: f64) -> f64 {
    let m = model.to_ascii_lowercase();
    let (inp, outp) = if m.contains("haiku") {
        (1.0, 5.0)
    } else if m.contains("opus") {
        (15.0, 75.0)
    } else {
        (3.0, 15.0)
    };
    inp * in_m + outp * out_m
}

fn estimate_openai_usd(model: &str, in_m: f64, out_m: f64) -> f64 {
    let m = model.to_ascii_lowercase();
    let (inp, outp) = if m.contains("gpt-4o-mini") || m.contains("4o-mini") {
        (0.15, 0.60)
    } else if m.contains("gpt-4o") {
        (2.50, 10.0)
    } else if m.contains("o1") {
        (15.0, 60.0)
    } else {
        (0.50, 1.50)
    };
    inp * in_m + outp * out_m
}

// ════════════════════════════════════════════════════════════════════
//  Agent types
// ════════════════════════════════════════════════════════════════════

/// A reusable agent persona / profile stored in the system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProfile {
    pub id: AgentProfileId,
    pub name: String,
    pub role: String,
    pub instructions: String,
    pub default_model: ModelRef,
    pub allowed_tools: Vec<String>,
    pub capabilities: Vec<Capability>,
    pub budget_limit: Option<f64>,
    pub metadata: serde_json::Value,
}

/// Capability tags attached to agents and engines.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Capability {
    CodeAnalysis,
    ContentCreation,
    OpportunityDiscovery,
    Outreach,
    Planning,
    ToolUse,
    WebBrowsing,
    Custom(String),
}

// ════════════════════════════════════════════════════════════════════
//  Starter kits (one-click department bundles)
// ════════════════════════════════════════════════════════════════════

/// A curated bundle of agents, skills, rules, and workflows for a persona or use case.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StarterKit {
    pub id: String,
    pub name: String,
    pub description: String,
    pub target_audience: String,
    pub departments: Vec<String>,
    pub entities: Vec<KitEntity>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// One entity to create when a [`StarterKit`] is installed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KitEntity {
    pub kind: String,
    pub department: String,
    pub name: String,
    pub definition: serde_json::Value,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Configuration passed to [`AgentPort::create`](crate::ports::AgentPort::create).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub profile_id: Option<AgentProfileId>,
    pub session_id: SessionId,
    pub model: Option<ModelRef>,
    pub tools: Vec<String>,
    pub instructions: Option<String>,
    pub budget_limit: Option<f64>,
    /// Maximum tool-use iterations. Default: 24 in agent runtime. Set lower for simple Q&A.
    #[serde(default)]
    pub max_iterations: Option<u32>,
    /// Tool execution policy for this agent run ([`ToolPermissionMode`]).
    #[serde(default)]
    pub permission_mode: ToolPermissionMode,
    pub metadata: serde_json::Value,
}

/// Output returned after an agent run completes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentOutput {
    pub run_id: RunId,
    pub content: Content,
    pub tool_calls: u32,
    pub usage: LlmUsage,
    pub cost_estimate: f64,
    pub metadata: serde_json::Value,
}

/// Runtime status of a running agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentStatus {
    Idle,
    Running,
    AwaitingTool,
    AwaitingApproval,
    Completed,
    Failed,
    Stopped,
}

// ════════════════════════════════════════════════════════════════════
//  Session → Run → Thread hierarchy  (architecture-v2)
// ════════════════════════════════════════════════════════════════════

/// A workspace session (project, lead, campaign, …).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub name: String,
    pub kind: SessionKind,
    pub tags: Vec<String>,
    pub config: SessionConfig,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub metadata: serde_json::Value,
}

/// What kind of work a session represents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionKind {
    Project,
    Lead,
    ContentCampaign,
    General,
}

/// Per-session configuration overrides.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionConfig {
    pub default_model: Option<ModelRef>,
    pub budget_limit: Option<f64>,
    pub approval_policies: Vec<ApprovalPolicy>,
    pub metadata: serde_json::Value,
}

/// Summary returned by [`SessionPort::list`](crate::ports::SessionPort::list).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: SessionId,
    pub name: String,
    pub kind: SessionKind,
    pub tags: Vec<String>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// A single execution run within a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Run {
    pub id: RunId,
    pub session_id: SessionId,
    pub engine: String,
    pub input_summary: String,
    pub status: RunStatus,
    pub llm_budget_used: f64,
    pub tool_calls_count: u32,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub metadata: serde_json::Value,
}

/// Status of a [`Run`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunStatus {
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

/// A message thread within a run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thread {
    pub id: ThreadId,
    pub run_id: RunId,
    pub channel: ThreadChannel,
    pub messages: Vec<ThreadMessage>,
    pub metadata: serde_json::Value,
}

/// Which channel a thread belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThreadChannel {
    User,
    Agent,
    System,
    Event,
}

/// A single message inside a [`Thread`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadMessage {
    pub role: ThreadChannel,
    pub content: Content,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

// ════════════════════════════════════════════════════════════════════
//  Central Job Queue  (ADR-003)
// ════════════════════════════════════════════════════════════════════

/// An async job in the central queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: JobId,
    pub session_id: SessionId,
    pub kind: JobKind,
    pub payload: serde_json::Value,
    pub status: JobStatus,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub retries: u32,
    pub max_retries: u32,
    pub error: Option<String>,
    pub metadata: serde_json::Value,
}

/// Describes a new job to enqueue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewJob {
    pub session_id: SessionId,
    pub kind: JobKind,
    pub payload: serde_json::Value,
    pub max_retries: u32,
    pub metadata: serde_json::Value,
    /// When set, the job is not eligible for dequeue until this instant (UTC).
    #[serde(default)]
    pub scheduled_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// What kind of work a job represents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobKind {
    AgentRun,
    ContentPublish,
    OutreachSend,
    HarvestScan,
    CodeAnalyze,
    /// Generate a freelance proposal; parks at [`JobStatus::AwaitingApproval`] after generation.
    ProposalDraft,
    ScheduledCron,
    Custom(String),
}

/// Lifecycle status of a [`Job`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    /// Human-in-the-loop gate (ADR-008).
    AwaitingApproval,
}

/// Result payload attached when a job completes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobResult {
    pub output: serde_json::Value,
    pub metadata: serde_json::Value,
}

/// Filter for listing jobs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JobFilter {
    pub session_id: Option<SessionId>,
    pub kinds: Vec<JobKind>,
    pub statuses: Vec<JobStatus>,
    pub limit: Option<u32>,
}

// ════════════════════════════════════════════════════════════════════
//  Opportunity (Harvest engine domain)
// ════════════════════════════════════════════════════════════════════

/// A discovered opportunity (gig, job, project lead).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Opportunity {
    pub id: OpportunityId,
    pub session_id: SessionId,
    pub source: OpportunitySource,
    pub title: String,
    pub url: Option<String>,
    pub description: String,
    /// RUSVEL authoritative score (0.0–1.0 in engine; UI may show ×100).
    pub score: f64,
    pub stage: OpportunityStage,
    pub value_estimate: Option<f64>,
    /// Stable per-platform id for deduplication (e.g. Upwork ciphertext job id).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform_job_key: Option<String>,
    /// Hash of normalized capture fields for change detection (see harvest-engine normalize).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    /// Optional pre-filter score from upstream capture (extension, external tool). RUSVEL `score` is always recomputed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream_score: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget_min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget_max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget_currency: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hourly: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_hire_rate: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_total_spent: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payment_verified: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposal_count: Option<u32>,
    pub metadata: serde_json::Value,
}

/// Where the opportunity was found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpportunitySource {
    Upwork,
    Freelancer,
    LinkedIn,
    GitHub,
    Manual,
    Other(String),
}

/// Pipeline stage for an opportunity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpportunityStage {
    Cold,
    Contacted,
    Qualified,
    ProposalSent,
    Won,
    Lost,
}

// ════════════════════════════════════════════════════════════════════
//  Content item (Content engine domain)
// ════════════════════════════════════════════════════════════════════

/// A piece of authored content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentItem {
    pub id: ContentId,
    pub session_id: SessionId,
    pub kind: ContentKind,
    pub title: String,
    pub body_markdown: String,
    pub platform_targets: Vec<Platform>,
    pub status: ContentStatus,
    pub approval: ApprovalStatus,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub published_at: Option<DateTime<Utc>>,
    pub metadata: serde_json::Value,
}

/// Genre / format of a content item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContentKind {
    LongForm,
    Tweet,
    Thread,
    LinkedInPost,
    Blog,
    VideoScript,
    Email,
    Proposal,
}

/// Lifecycle status of a content item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContentStatus {
    Draft,
    Adapted,
    Scheduled,
    Published,
    Archived,
}

/// Publishing platform.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Platform {
    Twitter,
    LinkedIn,
    DevTo,
    Medium,
    YouTube,
    Substack,
    Email,
    Custom(String),
}

// ════════════════════════════════════════════════════════════════════
//  Contact (GoToMarket engine domain)
// ════════════════════════════════════════════════════════════════════

/// A CRM contact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contact {
    pub id: ContactId,
    pub session_id: SessionId,
    pub name: String,
    pub emails: Vec<String>,
    pub links: Vec<String>,
    pub company: Option<String>,
    pub role: Option<String>,
    pub tags: Vec<String>,
    pub last_contacted_at: Option<DateTime<Utc>>,
    pub metadata: serde_json::Value,
}

// ════════════════════════════════════════════════════════════════════
//  Goal + Task (Mission sub-domain inside Forge)
// ════════════════════════════════════════════════════════════════════

/// A goal tracked by the mission sub-system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Goal {
    pub id: GoalId,
    pub session_id: SessionId,
    pub title: String,
    pub description: String,
    pub timeframe: Timeframe,
    pub status: GoalStatus,
    pub progress: f64,
    pub metadata: serde_json::Value,
}

/// Duration bucket for a goal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Timeframe {
    Day,
    Week,
    Month,
    Quarter,
}

/// Lifecycle status of a goal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GoalStatus {
    Active,
    Completed,
    Abandoned,
    Deferred,
}

/// An actionable task, optionally linked to a goal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub session_id: SessionId,
    pub goal_id: Option<GoalId>,
    pub title: String,
    pub status: TaskStatus,
    pub due_at: Option<DateTime<Utc>>,
    pub priority: Priority,
    pub metadata: serde_json::Value,
}

/// Lifecycle status of a task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    Todo,
    InProgress,
    Done,
    Cancelled,
}

/// Task priority.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Priority {
    Low,
    Medium,
    High,
    Urgent,
}

// ════════════════════════════════════════════════════════════════════
//  Executive brief (Forge — cross-department daily digest)
// ════════════════════════════════════════════════════════════════════

/// Cross-department executive digest from the Forge mission pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutiveBrief {
    pub id: String,
    pub date: NaiveDate,
    pub sections: Vec<BriefSection>,
    pub summary: String,
    pub action_items: Vec<String>,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// One department’s slice of an [`ExecutiveBrief`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BriefSection {
    pub department: String,
    pub status: String,
    pub highlights: Vec<String>,
    pub metrics: serde_json::Value,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

// ════════════════════════════════════════════════════════════════════
//  Event  (ADR-005: kind is String, not enum)
// ════════════════════════════════════════════════════════════════════

/// A domain event emitted by engines and adapters.
///
/// `kind` is a **free-form string** (ADR-005) so that rusvel-core does
/// not need to know every possible event type. Engines define their own
/// constants, e.g. `forge::events::AGENT_CREATED`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: EventId,
    pub session_id: Option<SessionId>,
    pub run_id: Option<RunId>,
    pub source: String,
    pub kind: String,
    pub payload: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub metadata: serde_json::Value,
}

/// Filter used when querying events.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EventFilter {
    pub session_id: Option<SessionId>,
    pub run_id: Option<RunId>,
    pub source: Option<String>,
    pub kind: Option<String>,
    pub since: Option<DateTime<Utc>>,
    pub limit: Option<u32>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Subscribes to event patterns and starts an agent run or flow execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventTrigger {
    pub id: String,
    pub name: String,
    /// Glob-style pattern for [`Event::kind`] (e.g. `browser.data.*`, `content.published`, `*`).
    pub event_pattern: String,
    /// Action to run when the pattern matches.
    pub action: TriggerAction,
    /// When set, only [`Event::source`] must equal this department id.
    pub department_id: Option<String>,
    pub enabled: bool,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// What to run when an [`EventTrigger`] fires.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TriggerAction {
    /// Run an agent with the given config knobs.
    RunAgent {
        persona: Option<String>,
        prompt_template: String,
        tools: Vec<String>,
    },
    /// Execute a flow by id (UUID string).
    RunFlow { flow_id: String },
}

// ════════════════════════════════════════════════════════════════════
//  Code intelligence references
// ════════════════════════════════════════════════════════════════════

/// Reference to a local (or remote) repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoRef {
    pub local_path: PathBuf,
    pub remote_url: Option<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Reference to a point-in-time code snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeSnapshotRef {
    pub id: SnapshotId,
    pub repo: RepoRef,
    pub analyzed_at: DateTime<Utc>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Compact summary of a code analysis run for content / social prompts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeAnalysisSummary {
    pub snapshot_id: String,
    pub repo_path: String,
    pub total_files: usize,
    pub total_symbols: usize,
    pub top_symbols: Vec<String>,
    pub largest_function: Option<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Result of a successful deploy (URL + provider-specific id).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployedUrl {
    pub url: String,
    pub deployment_id: String,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Observed state of a deployment on the provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployStatus {
    pub id: String,
    pub state: String,
    pub url: Option<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

// ════════════════════════════════════════════════════════════════════
//  Human-in-the-loop approval model  (ADR-008)
// ════════════════════════════════════════════════════════════════════

/// Status of a human approval gate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApprovalStatus {
    NotRequired,
    Pending,
    Approved,
    Rejected,
    AutoApproved,
}

/// Policy that controls whether an action requires human approval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalPolicy {
    /// Department ID string (e.g. `"content"`, `"gtm"`).
    pub engine: String,
    /// Action key, e.g. `"publish"`, `"send_outreach"`, `"spend > $1"`.
    pub action: String,
    pub requires_approval: bool,
    /// Auto-approve if estimated cost is below this threshold.
    pub auto_approve_below: Option<f64>,
}

// ════════════════════════════════════════════════════════════════════
//  Tool types (used by ToolPort)
// ════════════════════════════════════════════════════════════════════

/// Describes a tool that agents can invoke.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    /// JSON Schema for the tool's parameters.
    pub parameters: serde_json::Value,
    /// When true, this tool is discovered via `tool_search` instead of
    /// being included in every LLM prompt. Saves tokens. (P1)
    #[serde(default)]
    pub searchable: bool,
    pub metadata: serde_json::Value,
}

/// The result of executing a tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub success: bool,
    pub output: Content,
    pub metadata: serde_json::Value,
}

/// When in the tool lifecycle a hook fires.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum HookPoint {
    PreToolUse,
    PostToolUse,
}

/// Decision returned by a tool hook.
#[derive(Debug, Clone)]
pub enum HookDecision {
    Allow,
    Modify(serde_json::Value),
    Deny(String),
}

/// Configuration for a registered tool hook.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolHookConfig {
    pub id: String,
    pub hook_point: HookPoint,
    pub tool_pattern: String,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

// ════════════════════════════════════════════════════════════════════
//  Tool permission types (per-department tool access control)
// ════════════════════════════════════════════════════════════════════

/// How a tool invocation is handled when permission is checked.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolPermissionMode {
    /// Execute immediately without human intervention.
    #[default]
    Auto,
    /// Pause and require human approval before execution.
    Supervised,
    /// Block execution entirely.
    Locked,
}

/// A permission rule controlling tool access, optionally scoped to a department.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPermission {
    /// Tool name pattern: exact name, prefix glob (`"harvest_*"`), or wildcard (`"*"`).
    pub tool_pattern: String,
    pub mode: ToolPermissionMode,
    /// When `Some`, this rule only applies to the given department.
    /// When `None`, it is a global rule.
    pub department_id: Option<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

// ════════════════════════════════════════════════════════════════════
//  Memory types (used by MemoryPort)
// ════════════════════════════════════════════════════════════════════

/// An entry stored in the memory system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: Option<uuid::Uuid>,
    pub session_id: SessionId,
    pub kind: MemoryKind,
    pub content: String,
    pub embedding: Option<Vec<f32>>,
    pub created_at: DateTime<Utc>,
    pub metadata: serde_json::Value,
}

/// What type of knowledge a memory entry represents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoryKind {
    Fact,
    Conversation,
    Decision,
    Preference,
    Custom(String),
}

// ════════════════════════════════════════════════════════════════════
//  Credential type (used by AuthPort)
// ════════════════════════════════════════════════════════════════════

/// An opaque credential handle. Engines never see the raw secret.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credential {
    pub provider: String,
    pub kind: CredentialKind,
    pub expires_at: Option<DateTime<Utc>>,
    pub metadata: serde_json::Value,
}

/// What form the credential takes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CredentialKind {
    ApiKey,
    OAuth2,
    Bearer,
    Basic,
    Custom(String),
}

// ════════════════════════════════════════════════════════════════════
//  Health status (used by Engine trait)
// ════════════════════════════════════════════════════════════════════

/// Health check result returned by [`Engine::health`](crate::engine::Engine::health).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub healthy: bool,
    pub message: Option<String>,
    pub metadata: serde_json::Value,
}

// ════════════════════════════════════════════════════════════════════
//  Storage sub-store query types  (ADR-004)
// ════════════════════════════════════════════════════════════════════

/// Filter for querying the object store.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ObjectFilter {
    pub session_id: Option<SessionId>,
    /// When set, restricts `chat_message` (and similar) rows where `data.conversation_id` matches.
    pub conversation_id: Option<String>,
    pub tags: Vec<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

/// A single time-series metric data point.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricPoint {
    pub name: String,
    pub value: f64,
    pub tags: Vec<String>,
    pub recorded_at: DateTime<Utc>,
    pub metadata: serde_json::Value,
}

/// Filter for querying the metric store.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetricFilter {
    pub name: Option<String>,
    pub tags: Vec<String>,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
    pub limit: Option<u32>,
}

/// Structured LLM / agent cost row for analytics (stored separately from [`MetricPoint`]).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostEvent {
    pub id: String,
    pub session_id: Option<SessionId>,
    pub department_id: Option<String>,
    pub model: String,
    pub provider: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cost_usd: f64,
    pub operation: String,
    pub created_at: DateTime<Utc>,
    pub metadata: serde_json::Value,
}

// ════════════════════════════════════════════════════════════════════
//  User Profile (god agent identity)
// ════════════════════════════════════════════════════════════════════

/// The user's persistent identity, loaded from `~/.rusvel/profile.toml`.
/// Injected into every agent's system prompt so all agents know who they serve.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    pub identity: ProfileIdentity,
    pub skills: ProfileSkills,
    pub products: ProfileProducts,
    pub mission: ProfileMission,
    pub preferences: ProfilePreferences,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileIdentity {
    pub name: String,
    pub role: String,
    pub tagline: String,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileSkills {
    pub primary: Vec<String>,
    #[serde(default)]
    pub secondary: Vec<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileProducts {
    pub names: Vec<String>,
    pub description: String,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileMission {
    pub vision: String,
    #[serde(default)]
    pub values: Vec<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfilePreferences {
    #[serde(default = "default_style")]
    pub style: String,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

fn default_style() -> String {
    "Direct and technical".into()
}

impl UserProfile {
    /// Load from a TOML file.
    pub fn load(path: impl AsRef<std::path::Path>) -> std::result::Result<Self, String> {
        let content = std::fs::read_to_string(path.as_ref())
            .map_err(|e| format!("cannot read profile: {e}"))?;
        toml::from_str(&content).map_err(|e| format!("invalid profile TOML: {e}"))
    }

    /// Generate the system prompt preamble that gives agents full context about the user.
    pub fn to_system_prompt(&self) -> String {
        format!(
            "You are the RUSVEL AI assistant for {name}, {role}.\n\
             {tagline}\n\n\
             Primary skills: {skills}\n\
             Products: {products} — {desc}\n\
             Mission: {mission}\n\
             Values: {values}\n\
             Communication style: {style}\n\n\
             You help {name} with planning, strategy, development, content creation, \
             finding opportunities, and business operations. Be {style}.",
            name = self.identity.name,
            role = self.identity.role,
            tagline = self.identity.tagline,
            skills = self.skills.primary.join(", "),
            products = self.products.names.join(", "),
            desc = self.products.description,
            mission = self.mission.vision,
            values = self.mission.values.join(", "),
            style = self.preferences.style.to_lowercase(),
        )
    }
}

// ════════════════════════════════════════════════════════════════════
//  Vector / RAG types (used by EmbeddingPort + VectorStorePort)
// ════════════════════════════════════════════════════════════════════

/// A document stored in the vector knowledge base.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorEntry {
    pub id: String,
    pub content: String,
    pub source: String,
    pub created_at: DateTime<Utc>,
    pub metadata: serde_json::Value,
}

/// A search result from a vector similarity query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorSearchResult {
    pub entry: VectorEntry,
    /// Cosine similarity score (higher = more relevant).
    pub score: f64,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Single hit from hybrid FTS + vector fusion ([`reciprocal_rank_fusion`](crate::rrf::reciprocal_rank_fusion)).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridSearchHit {
    pub fusion_key: String,
    pub rrf_score: f64,
    pub source: HybridHitSource,
    pub content: String,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HybridHitSource {
    Fts,
    Vector,
}

// ════════════════════════════════════════════════════════════════════
//  Browser / CDP (BrowserPort — passive observation foundation)
// ════════════════════════════════════════════════════════════════════

/// Open browser tab as seen via Chrome `DevTools` Protocol (`/json/list`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabInfo {
    pub id: String,
    pub url: String,
    pub title: String,
    pub platform: Option<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Streamed observation events from a tab (network, navigation, lifecycle).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BrowserEvent {
    DataCaptured {
        platform: String,
        kind: String,
        data: serde_json::Value,
        tab_id: String,
    },
    Navigation {
        tab_id: String,
        url: String,
    },
    TabChanged {
        tab_id: String,
        opened: bool,
    },
}

impl BrowserEvent {
    /// One-line log for terminal / TUI (CDP capture).
    ///
    /// Example: `[14:32:01] upwork | job_listing | "Senior Rust Developer - Remote" | score: 8.5`
    #[must_use]
    pub fn terminal_log_line(&self) -> Option<String> {
        match self {
            BrowserEvent::DataCaptured {
                platform,
                kind,
                data,
                ..
            } => {
                let title_part = data
                    .get("title")
                    .or_else(|| data.get("name"))
                    .and_then(|v| v.as_str())
                    .map_or_else(|| "\"—\"".to_string(), |s| format!("{s:?}"));
                let score_part = data
                    .get("score")
                    .and_then(serde_json::Value::as_f64)
                    .map(|s| format!(" | score: {s:.1}"))
                    .unwrap_or_default();
                let time = Utc::now().format("%H:%M:%S");
                Some(format!(
                    "[{time}] {platform} | {kind} | {title_part}{score_part}"
                ))
            }
            _ => None,
        }
    }
}

/// How aggressively the system may drive or read the browser session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BrowsingMode {
    Passive,
    Assisted,
    Autonomous,
    Vision,
}

// ════════════════════════════════════════════════════════════════════
//  Flow Engine — DAG workflow definitions and execution
// ════════════════════════════════════════════════════════════════════

/// A flow definition: a DAG of nodes connected by edges.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowDef {
    pub id: crate::id::FlowId,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub nodes: Vec<FlowNodeDef>,
    pub connections: Vec<FlowConnectionDef>,
    #[serde(default)]
    pub variables: std::collections::HashMap<String, String>,
    #[serde(default = "default_metadata")]
    pub metadata: serde_json::Value,
}

/// A single node in a flow DAG.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowNodeDef {
    pub id: crate::id::FlowNodeId,
    /// Node type: "agent", "code", "condition", "http", etc. (ADR-005: String, not enum)
    pub node_type: String,
    pub name: String,
    /// Node-specific configuration (JSON).
    #[serde(default)]
    pub parameters: serde_json::Value,
    /// Canvas position for visual builder.
    #[serde(default)]
    pub position: (f64, f64),
    /// Error handling behavior for this node.
    #[serde(default)]
    pub on_error: FlowErrorBehavior,
    #[serde(default = "default_metadata")]
    pub metadata: serde_json::Value,
}

fn default_metadata() -> serde_json::Value {
    serde_json::json!({})
}

/// An edge connecting two nodes in a flow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowConnectionDef {
    pub source_node: crate::id::FlowNodeId,
    /// Output port name: "main", "true", "false", "error".
    #[serde(default = "default_output")]
    pub source_output: String,
    pub target_node: crate::id::FlowNodeId,
    /// Input port name: "main".
    #[serde(default = "default_output")]
    pub target_input: String,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

fn default_output() -> String {
    "main".into()
}

/// How a node handles errors.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum FlowErrorBehavior {
    /// Stop the entire flow on failure (default).
    #[default]
    StopFlow,
    /// Continue executing downstream nodes despite failure.
    ContinueOnFail,
    /// Route to the "error" output instead of "main".
    UseErrorOutput,
}

/// The status of a flow execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FlowExecutionStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

/// The status of a single node within an execution.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum FlowNodeStatus {
    #[default]
    Pending,
    Running,
    Succeeded,
    Failed,
    Skipped,
}

/// A complete flow execution record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowExecution {
    pub id: crate::id::FlowExecutionId,
    pub flow_id: crate::id::FlowId,
    pub status: FlowExecutionStatus,
    pub trigger_data: serde_json::Value,
    pub node_results: std::collections::HashMap<String, FlowNodeResult>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
    pub metadata: serde_json::Value,
}

/// Result of executing a single node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowNodeResult {
    pub status: FlowNodeStatus,
    pub output: Option<serde_json::Value>,
    pub error: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

fn default_empty_object() -> serde_json::Value {
    serde_json::json!({})
}

/// Persisted progress for durable flow execution (checkpoint / resume).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowCheckpoint {
    pub flow_id: String,
    pub execution_id: String,
    pub completed_nodes: Vec<String>,
    pub node_outputs: std::collections::HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub node_results: std::collections::HashMap<String, FlowNodeResult>,
    pub failed_node: Option<String>,
    pub error: Option<String>,
    #[serde(default = "default_empty_object")]
    pub trigger_data: serde_json::Value,
    #[serde(default)]
    pub started_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

// ════════════════════════════════════════════════════════════════════
//  Automation plane — cron/webhook → job worker → flow or playbook (native, no sidecar)
// ════════════════════════════════════════════════════════════════════

/// Cron `event_kind` for [`crate::domain::JobKind::ScheduledCron`]: payload is [`AutomationTriggerPayload`] JSON inside job `payload.payload`.
pub const AUTOMATION_CRON_EVENT_KIND: &str = "rusvel.automation.v1";

/// Register webhooks with this `event_kind`; POST body must include `session_id` plus automation fields.
pub const AUTOMATION_WEBHOOK_EVENT_KIND: &str = "rusvel.automation.trigger";

/// Versioned payload stored in [`CronScheduleRecord::payload`] or webhook body (alongside `session_id`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AutomationTriggerPayload {
    #[serde(default = "automation_trigger_version_default")]
    pub version: u32,
    pub action: AutomationTriggerAction,
    pub target_id: String,
    #[serde(default)]
    pub variables: serde_json::Value,
    #[serde(default)]
    pub department_id: Option<String>,
}

fn automation_trigger_version_default() -> u32 {
    1
}

/// What the automation worker runs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutomationTriggerAction {
    RunFlow,
    RunPlaybook,
}

// ════════════════════════════════════════════════════════════════════
//  Playbooks — predefined multi-step pipelines (FlowEngine + delegate_agent)
// ════════════════════════════════════════════════════════════════════

/// A named playbook: sequential steps over agent delegation and/or flow invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Playbook {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub steps: Vec<PlaybookStep>,
    #[serde(default = "default_metadata")]
    pub metadata: serde_json::Value,
}

/// One step in a [`Playbook`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybookStep {
    pub name: String,
    pub description: String,
    pub action: PlaybookAction,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// What a playbook step executes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlaybookAction {
    Agent {
        #[serde(default)]
        persona: Option<String>,
        prompt_template: String,
        #[serde(default)]
        tools: Vec<String>,
    },
    Flow {
        flow_id: String,
        #[serde(default)]
        input_mapping: Option<String>,
    },
    Approval {
        message: String,
    },
    /// Resolve a stored skill by name (department-scoped like chat `/skill`).
    Skill {
        skill_name: String,
        #[serde(default)]
        input: String,
        /// Empty string means use playbook `metadata.department` or `"forge"`.
        #[serde(default)]
        department: String,
    },
    /// Load [`AgentProfile`] from object store and run `delegate_agent` with merged instructions.
    AgentProfile {
        agent_name: String,
        prompt_template: String,
        #[serde(default)]
        tools: Vec<String>,
    },
    /// Append matching rules text into `ctx["rules_excerpt"]` for subsequent steps (no LLM call).
    RulesAppend {
        department: String,
        /// Empty = all enabled rules for the department.
        #[serde(default)]
        rule_names: Vec<String>,
    },
}

/// A single execution of a playbook.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybookRun {
    pub id: String,
    pub playbook_id: String,
    pub status: PlaybookRunStatus,
    pub step_results: Vec<serde_json::Value>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Lifecycle of a [`PlaybookRun`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaybookRunStatus {
    Running,
    Paused,
    Completed,
    Failed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_text_shorthand() {
        let c = Content::text("hello");
        assert!(!c.is_empty());
        match &c.parts[0] {
            Part::Text(s) => assert_eq!(s, "hello"),
            _ => panic!("expected text part"),
        }
    }

    #[test]
    fn event_kind_is_free_string() {
        let e = Event {
            id: EventId::new(),
            session_id: None,
            run_id: None,
            source: "forge".into(),
            kind: "agent.created".into(),
            payload: serde_json::json!({}),
            created_at: Utc::now(),
            metadata: serde_json::json!({}),
        };
        assert_eq!(e.kind, "agent.created");
    }

    #[test]
    fn domain_types_roundtrip_serde() {
        let opp = Opportunity {
            id: OpportunityId::new(),
            session_id: SessionId::new(),
            source: OpportunitySource::Upwork,
            title: "Rust CLI tool".into(),
            url: Some("https://example.com".into()),
            description: "Build a CLI".into(),
            score: 0.85,
            stage: OpportunityStage::Cold,
            value_estimate: Some(5000.0),
            platform_job_key: None,
            content_hash: None,
            upstream_score: None,
            budget_min: None,
            budget_max: None,
            budget_currency: None,
            hourly: None,
            client_hire_rate: None,
            client_total_spent: None,
            payment_verified: None,
            proposal_count: None,
            metadata: serde_json::json!({"skills": ["rust"]}),
        };
        let json = serde_json::to_string(&opp).unwrap();
        let back: Opportunity = serde_json::from_str(&json).unwrap();
        assert_eq!(back.title, "Rust CLI tool");
    }

    #[test]
    fn priority_ordering() {
        assert!(Priority::Urgent > Priority::High);
        assert!(Priority::High > Priority::Medium);
        assert!(Priority::Medium > Priority::Low);
    }

    #[test]
    fn model_tier_parses_from_metadata() {
        let meta = serde_json::json!({ RUSVEL_META_MODEL_TIER: "fast" });
        assert_eq!(
            ModelTier::from_request_metadata(&meta),
            Some(ModelTier::Fast)
        );
        let meta = serde_json::json!({ RUSVEL_META_MODEL_TIER: "opus" });
        assert_eq!(
            ModelTier::from_request_metadata(&meta),
            Some(ModelTier::Premium)
        );
    }

    #[test]
    fn estimate_claude_haiku_nonzero() {
        let u = LlmUsage {
            input_tokens: 1_000_000,
            output_tokens: 500_000,
        };
        let c = estimate_llm_cost_usd(&ModelProvider::Claude, "claude-haiku-4-20250414", &u);
        assert!(c > 0.0);
        assert!((c - 3.5).abs() < 0.01);
    }
}
