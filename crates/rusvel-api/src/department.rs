//! Department infrastructure — registry-driven config, chat, and events.
//!
//! All departments share the same 6 parameterized handlers.
//! Department definitions come from the `DepartmentRegistry`.
//! Config uses three-layer cascade: global → department → session.

use std::convert::Infallible;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, KeepAliveStream, Sse};
use chrono::Utc;
use futures::stream::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use tokio_stream::wrappers::ReceiverStream;

use rusvel_agent::{AgentEvent, ContextPack, to_prompt_section};
use rusvel_core::config::{
    ContextPackFlags, LayeredConfig, ResolvedConfig, resolve_context_pack_flags,
    tool_permission_mode_from_dept_config_str,
};
use rusvel_core::domain::{
    AgentConfig, Content, EventFilter, JobFilter, JobStatus, RUSVEL_META_DEPARTMENT_ID,
    RUSVEL_META_MODEL_TIER, SYSTEM_PROMPT_CACHE_BOUNDARY, UserProfile,
};
use rusvel_core::error::RusvelError;
use rusvel_core::id::{EventId, SessionId};
use rusvel_core::ports::{AgentPort, StoragePort};
use rusvel_core::registry::DepartmentDef;
use uuid::Uuid;

use crate::chat::{ChatMessage, ChatRequest, ConversationSummary};
use crate::sse_helpers;
use crate::{AppState, CONTEXT_PACK_CACHE_TTL};

// ── Department Config (stored per-dept as LayeredConfig) ─────

const CONFIG_STORE_KEY: &str = "dept_config";

fn msg_namespace(engine: &str) -> String {
    format!("dept_msg_{engine}")
}

async fn load_dept_config(engine: &str, state: &Arc<AppState>) -> LayeredConfig {
    state
        .storage
        .objects()
        .get(CONFIG_STORE_KEY, engine)
        .await
        .ok()
        .flatten()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default()
}

/// Resolve a department's effective config: registry defaults + stored overrides + user context.
fn resolve_dept_config(
    dept_def: &DepartmentDef,
    stored: &LayeredConfig,
    profile: Option<&UserProfile>,
) -> ResolvedConfig {
    // Start with registry defaults
    let mut base = dept_def.default_config.clone();

    // Set system prompt from registry + user context
    let user_context = profile
        .map(rusvel_core::UserProfile::to_system_prompt)
        .unwrap_or_default();
    if base.system_prompt.is_none() {
        base.system_prompt = Some(format!("{}\n\n{user_context}", dept_def.system_prompt));
    }

    // Stored overrides on top of registry defaults
    let merged = stored.overlay(&base);
    merged.resolve()
}

async fn quick_context_metrics(state: &Arc<AppState>, sid: &SessionId) -> String {
    let jobs = state
        .jobs
        .list(JobFilter {
            session_id: Some(*sid),
            statuses: vec![
                JobStatus::Queued,
                JobStatus::Running,
                JobStatus::AwaitingApproval,
            ],
            limit: Some(64),
            ..Default::default()
        })
        .await
        .unwrap_or_default();
    let mut parts = vec![format!("jobs_in_flight: {}", jobs.len())];
    if let Some(h) = state.harvest_engine.as_ref() {
        if let Ok(stats) = h.pipeline(sid).await {
            parts.push(format!("harvest_opportunities: {}", stats.total));
        }
    }
    parts.join("; ")
}

async fn assemble_context_pack(
    state: &Arc<AppState>,
    sid: &SessionId,
    flags: rusvel_core::config::ResolvedContextPackFlags,
) -> Result<ContextPack, RusvelError> {
    let session = state.sessions.load(sid).await?;
    let session_name = if flags.session_name {
        session.name
    } else {
        String::new()
    };
    let goal_titles = if flags.goals {
        state
            .forge
            .list_goals(sid)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|g| g.title)
            .collect()
    } else {
        Vec::new()
    };
    let recent_event_summaries = if flags.events {
        let evs = state
            .events
            .query(EventFilter {
                session_id: Some(*sid),
                limit: Some(16),
                ..Default::default()
            })
            .await
            .unwrap_or_default();
        evs.into_iter()
            .rev()
            .take(10)
            .map(|e| {
                let pl = e.payload.to_string();
                let short = pl.chars().take(120).collect::<String>();
                format!("{}: {short}", e.kind)
            })
            .collect()
    } else {
        Vec::new()
    };
    let metrics_summary = if flags.metrics {
        Some(quick_context_metrics(state, sid).await)
    } else {
        None
    };
    Ok(ContextPack {
        session_name,
        goal_titles,
        recent_event_summaries,
        metrics_summary,
    })
}

async fn context_pack_for_chat(
    state: &Arc<AppState>,
    dept: &str,
    sid: &SessionId,
    stored: &LayeredConfig,
) -> Result<ContextPack, RusvelError> {
    let flags = resolve_context_pack_flags(stored);
    let key = (*sid, dept.to_string());
    {
        let guard = state.context_pack_cache.inner.lock().unwrap();
        if let Some((t, pack)) = guard.get(&key) {
            if t.elapsed() < CONTEXT_PACK_CACHE_TTL {
                return Ok(pack.clone());
            }
        }
    }
    let pack = assemble_context_pack(state, sid, flags).await?;
    {
        let mut guard = state.context_pack_cache.inner.lock().unwrap();
        guard.insert(key, (std::time::Instant::now(), pack.clone()));
    }
    Ok(pack)
}

// ── Legacy DepartmentConfig (for backward-compatible JSON responses) ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepartmentConfig {
    pub engine: String,
    pub model: String,
    pub effort: String,
    pub max_budget_usd: Option<f64>,
    pub permission_mode: String,
    pub allowed_tools: Vec<String>,
    pub disallowed_tools: Vec<String>,
    pub system_prompt: String,
    pub add_dirs: Vec<String>,
    pub max_turns: Option<u32>,
    #[serde(default)]
    pub context_pack: Option<ContextPackFlags>,
    /// `discuss` (default) or `agentic` — steers assistant behavior for this department.
    #[serde(default = "default_dept_chat_mode")]
    pub chat_mode: String,
}

fn default_dept_chat_mode() -> String {
    "discuss".into()
}

impl From<(&str, ResolvedConfig)> for DepartmentConfig {
    fn from((engine, r): (&str, ResolvedConfig)) -> Self {
        Self {
            engine: engine.into(),
            model: r.model,
            effort: r.effort,
            max_budget_usd: r.max_budget_usd,
            permission_mode: r.permission_mode,
            allowed_tools: r.allowed_tools,
            disallowed_tools: r.disallowed_tools,
            system_prompt: r.system_prompt,
            add_dirs: r.add_dirs,
            max_turns: r.max_turns,
            context_pack: None,
            chat_mode: r.chat_mode,
        }
    }
}

impl DepartmentConfig {
    /// Convert incoming `DepartmentConfig` update into a `LayeredConfig` for storage.
    fn to_layered(&self) -> LayeredConfig {
        LayeredConfig {
            model: Some(self.model.clone()),
            effort: Some(self.effort.clone()),
            max_budget_usd: self.max_budget_usd,
            permission_mode: Some(self.permission_mode.clone()),
            allowed_tools: Some(self.allowed_tools.clone()),
            disallowed_tools: Some(self.disallowed_tools.clone()),
            system_prompt: Some(self.system_prompt.clone()),
            add_dirs: Some(self.add_dirs.clone()),
            max_turns: self.max_turns,
            context_pack: self.context_pack.clone(),
            chat_mode: Some(self.chat_mode.clone()),
        }
    }
}

// ── Validate dept param against registry ─────────────────────

fn validate_dept<'a>(
    state: &'a Arc<AppState>,
    dept: &str,
) -> Result<&'a DepartmentDef, (StatusCode, String)> {
    state
        .registry
        .get(dept)
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Unknown department: {dept}")))
}

// ── Registry endpoint ────────────────────────────────────────

pub async fn list_departments(State(state): State<Arc<AppState>>) -> Json<Vec<serde_json::Value>> {
    let depts: Vec<serde_json::Value> = state
        .registry
        .list()
        .iter()
        .map(|d| serde_json::to_value(d).unwrap_or_default())
        .collect();
    Json(depts)
}

// ── Profile endpoints ────────────────────────────────────────

pub async fn get_profile(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let profile = state
        .profile
        .as_ref()
        .map(|p| serde_json::to_value(p).unwrap_or_default())
        .unwrap_or(serde_json::json!(null));
    Json(profile)
}

pub async fn update_profile(
    State(state): State<Arc<AppState>>,
    Json(profile): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    // Store profile in ObjectStore for now
    state
        .storage
        .objects()
        .put("user_profile", "current", profile.clone())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(profile))
}

// ── Parameterized department handlers ────────────────────────

pub async fn dept_config_get(
    State(state): State<Arc<AppState>>,
    Path(dept): Path<String>,
) -> Result<Json<DepartmentConfig>, (StatusCode, String)> {
    let dept_def = validate_dept(&state, &dept)?;
    let stored = load_dept_config(&dept, &state).await;
    let resolved = resolve_dept_config(dept_def, &stored, state.profile.as_ref());
    let mut cfg = DepartmentConfig::from((dept.as_str(), resolved));
    cfg.context_pack = stored.context_pack.clone();
    Ok(Json(cfg))
}

pub async fn dept_config_update(
    State(state): State<Arc<AppState>>,
    Path(dept): Path<String>,
    Json(config): Json<DepartmentConfig>,
) -> Result<Json<DepartmentConfig>, (StatusCode, String)> {
    validate_dept(&state, &dept)?;
    let layered = config.to_layered();
    let value =
        serde_json::to_value(&layered).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    state
        .storage
        .objects()
        .put(CONFIG_STORE_KEY, &dept, value)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(config))
}

pub async fn dept_chat(
    State(state): State<Arc<AppState>>,
    Path(dept): Path<String>,
    Json(body): Json<ChatRequest>,
) -> Result<
    Sse<KeepAliveStream<Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>>>>,
    (StatusCode, String),
> {
    let dept_def = validate_dept(&state, &dept)?;
    let stored = load_dept_config(&dept, &state).await;
    let mut resolved = resolve_dept_config(dept_def, &stored, state.profile.as_ref());
    let namespace = msg_namespace(&dept);

    let conversation_id = body
        .conversation_id
        .unwrap_or_else(|| uuid::Uuid::now_v7().to_string());

    // Load history
    let history = load_namespaced_history(&state.storage, &namespace, &conversation_id, 50)
        .await
        .unwrap_or_default();

    // Store user message
    let user_msg = ChatMessage {
        id: uuid::Uuid::now_v7().to_string(),
        conversation_id: conversation_id.clone(),
        role: "user".into(),
        content: body.message.clone(),
        created_at: Utc::now().to_rfc3339(),
    };
    let _ = store_namespaced_message(&state.storage, &namespace, &user_msg).await;

    // !build interceptor
    if let Some(build_cmd) = crate::build_cmd::parse_build_command(&body.message) {
        let storage = state.storage.clone();
        let engine_owned = dept.clone();
        let conv_id = conversation_id.clone();
        let ns = namespace.clone();

        let (tx, rx) = tokio::sync::mpsc::channel::<Event>(8);
        tokio::spawn(async move {
            let _ = tx
                .send(
                    Event::default().event("delta").data(
                        serde_json::json!({
                            "text": format!("Building {}...\n\n", build_cmd.entity_type.label()),
                            "conversation_id": conv_id,
                        })
                        .to_string(),
                    ),
                )
                .await;

            let result = crate::build_cmd::execute_build(&build_cmd, &engine_owned, &storage).await;
            let response_text = match result {
                Ok(confirmation) => confirmation,
                Err(e) => format!("**Build failed:** {e}"),
            };

            let assistant_msg = ChatMessage {
                id: uuid::Uuid::now_v7().to_string(),
                conversation_id: conv_id.clone(),
                role: "assistant".into(),
                content: response_text.clone(),
                created_at: Utc::now().to_rfc3339(),
            };
            let _ = store_namespaced_message(&storage, &ns, &assistant_msg).await;
            let _ = tx
                .send(
                    Event::default().event("done").data(
                        serde_json::json!({
                            "text": response_text,
                            "cost_usd": 0.0,
                            "conversation_id": conv_id,
                        })
                        .to_string(),
                    ),
                )
                .await;
        });

        let stream: Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>> =
            Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx).map(Ok::<_, Infallible>));
        return Ok(Sse::new(stream).keep_alive(KeepAlive::default()));
    }

    // /skill-name interceptor
    let effective_message =
        if let Some(expanded) = crate::skills::resolve_skill(&state, &dept, &body.message).await {
            expanded
        } else {
            body.message.clone()
        };

    // @agent-name mention override
    if let Some(agent_name) = extract_agent_mention(&body.message)
        && let Ok(agents) = state
            .storage
            .objects()
            .list("agents", rusvel_core::domain::ObjectFilter::default())
            .await
    {
        let found = agents
            .into_iter()
            .filter_map(|v| serde_json::from_value::<rusvel_core::domain::AgentProfile>(v).ok())
            .find(|a| a.name.eq_ignore_ascii_case(&agent_name));
        if let Some(agent) = found {
            resolved.system_prompt = agent.instructions.clone();
            resolved.model = agent.default_model.model.clone();
            if !agent.allowed_tools.is_empty() {
                resolved.allowed_tools = agent.allowed_tools.clone();
            }
        }
    }

    // ═══ Cache boundary ═══
    // Everything above is stable per-department (persona, capabilities, agent
    // override). Everything below is dynamic per-session (rules, context pack,
    // RAG, chat mode). The Claude adapter splits on this marker and applies
    // cache_control: { type: "ephemeral" } to the static prefix.
    resolved
        .system_prompt
        .push_str(&format!("\n{SYSTEM_PROMPT_CACHE_BOUNDARY}\n"));

    // Load enabled rules and append to system prompt
    let rules = crate::rules::load_rules_for_engine(&state, &dept).await;
    if !rules.is_empty() {
        resolved.system_prompt.push_str("\n\n--- Rules ---\n");
        for rule in &rules {
            resolved
                .system_prompt
                .push_str(&format!("[{}]: {}\n", rule.name, rule.content));
        }
    }

    // Workspace chat mode (discuss vs agentic) — maps to Claude-style Chat / Cowork-style execution hints
    match resolved.chat_mode.to_ascii_lowercase().as_str() {
        "agentic" | "run" => {
            resolved.system_prompt.push_str(
                "\n\n--- Mode: Agentic ---\n\
                 Use available tools proactively to fulfill requests. For multi-step work, state a brief plan first, then execute.\n",
            );
        }
        _ => {
            resolved.system_prompt.push_str(
                "\n\n--- Mode: Discuss ---\n\
                 Prefer concise explanations. Offer to use tools when the user wants actions taken on their behalf.\n",
            );
        }
    }

    if let Ok(Some(conn)) = state
        .storage
        .objects()
        .get("github_connector", "default")
        .await
    {
        if conn
            .get("token")
            .and_then(|t| t.as_str())
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false)
        {
            resolved.system_prompt.push_str(
                "\n\n--- GitHub connector ---\n\
                 A GitHub personal access token is configured on this Rusvel instance. Prefer the GitHub API or `gh` for repository operations when relevant.\n",
            );
        }
    }

    if let Some(sid_str) = body.session_id.as_ref()
        && let Ok(u) = Uuid::parse_str(sid_str)
    {
        let sid = SessionId::from_uuid(u);
        if let Ok(pack) = context_pack_for_chat(&state, &dept, &sid, &stored).await {
            resolved.system_prompt.push_str(&to_prompt_section(&pack));
        }
    }

    resolved.system_prompt.push_str(
        "\n\n--- Platform APIs ---\n\
         - Webhooks: POST /api/webhooks {name, event_kind} → id + secret; POST /api/webhooks/{id} with body + X-Rusvel-Signature (HMAC-SHA256)\n\
         - Cron: GET/POST /api/cron; GET/PUT/DELETE /api/cron/{id}; POST /api/cron/tick (manual scheduler tick). Schedules use presets hourly|daily|weekly or cron expressions; jobs enqueue as ScheduledCron. Use event_kind \"forge.daily_briefing\" with schedule daily (or custom) to run the executive daily briefing for that session (S-043).\n\
         - Jobs: GET /api/jobs?session_id=<uuid>&kinds=<comma-separated>&status=<Queued|Running|...>&limit=<n>\n",
    );

    // Inject engine-specific capabilities into system prompt
    match dept.as_str() {
        "code" => {
            resolved.system_prompt.push_str(
                "\n\n--- Department Actions ---\n\
                 This department has a wired Code Engine with real capabilities:\n\
                 - Analyze code: POST /api/dept/code/analyze {\"path\": \"<repo_path>\"}\n\
                 - Search symbols: GET /api/dept/code/search?q=<query>&limit=20\n\
                 You can direct users to these endpoints or describe the analysis results.",
            );
        }
        "content" => {
            resolved.system_prompt.push_str(
                "\n\n--- Department Actions ---\n\
                 This department has a wired Content Engine with real capabilities:\n\
                 - Draft content: POST /api/dept/content/draft {session_id, topic, kind}\n\
                 - Publish content: POST /api/dept/content/publish {session_id, content_id, platform}\n\
                 - List content: GET /api/dept/content/list?session_id=<id>&status=<filter>\n\
                 - Schedule draft: POST /api/dept/content/schedule {session_id, content_id, platform, publish_at (RFC3339)}\n\
                 - Scheduled posts: GET /api/dept/content/scheduled?session_id=<id>&from=<RFC3339>&to=<RFC3339>\n\
                 Content kinds: LongForm, Tweet, Thread, LinkedInPost, Blog, VideoScript, Email, Proposal\n\
                 Platforms: DevTo, Twitter, LinkedIn, Mastodon, Bluesky, Medium",
            );
        }
        "harvest" => {
            resolved.system_prompt.push_str(
                "\n\n--- Department Actions ---\n\
                 This department has a wired Harvest Engine with real capabilities:\n\
                 - Scan sources: POST /api/dept/harvest/scan {session_id, sources: [mock|upwork|freelancer|cdp], query, cdp_endpoint?, cdp_extract_js?}\n\
                 - Ingest push (extension/fleet): POST /api/dept/harvest/ingest {session_id, platform: upwork|freelancer|..., jobs: [{title, description, url?, ...}]}\n\
                 - CDP scan: use source \"cdp\" and query = listing page URL; default base RUSVEL_CDP_ENDPOINT; optional cdp_endpoint per Chrome profile\n\
                 - Passive CDP → harvest: set RUSVEL_HARVEST_CDP_SESSION_ID=<session uuid> to attach captures to that session\n\
                 - Cron auto-scan: schedule event_kind \"harvest.auto_scan\" with payload {sources, query, cdp_extract_js?, cdp_endpoint?} (same as HarvestScan job)\n\
                 - Score opportunity: POST /api/dept/harvest/score {session_id, opportunity_id} → score + reasoning\n\
                 - Advance stage: POST /api/dept/harvest/advance {session_id, opportunity_id, stage} — Won/Lost also records outcome for learning (S-044)\n\
                 - Generate proposal: POST /api/dept/harvest/proposal {session_id, opportunity_id, profile} — default queues ProposalDraft for worker + approval; add \"sync\": true for immediate JSON response\n\
                 - Pipeline stats: GET /api/dept/harvest/pipeline?session_id=<id>\n\
                 - List opportunities: GET /api/dept/harvest/list?session_id=<id>&stage=<filter>\n\
                 - Record outcome (won/lost): POST /api/dept/harvest/outcome {session_id, opportunity_id, result, notes?}\n\
                 - List outcomes: GET /api/dept/harvest/outcomes?session_id=<id>&limit=<n>",
            );
        }
        "forge" => {
            resolved.system_prompt.push_str(
                "\n\n--- Department Actions ---\n\
                 Forge mission + briefings:\n\
                 - Executive brief: GET /api/brief?session_id=<id> or POST /api/brief/generate {session_id}\n\
                 - Autonomous daily briefing (S-043): POST /api/cron with {session_id, name, schedule: \"daily\", event_kind: \"forge.daily_briefing\", enabled: true} — worker runs generate_brief, persists brief, emits forge.brief.generated.\n",
            );
        }
        "gtm" => {
            resolved.system_prompt.push_str(
                "\n\n--- Department Actions ---\n\
                 GTM CRM (contacts, deals, invoicing) uses shared storage:\n\
                 - List contacts: GET /api/dept/gtm/contacts?session_id=<id>\n\
                 - Create contact: POST /api/dept/gtm/contacts {session_id, name, email, company?, role?, tags?, links?}\n\
                 - List deals (with contact names for UI): GET /api/dept/gtm/deals?session_id=<id>&stage=<optional>\n\
                 - Advance deal stage: POST /api/dept/gtm/deals/advance {session_id, deal_id, stage, opportunity_id?} — Won/Lost + opportunity_id (or deal.metadata.harvest_opportunity_id) records Harvest outcome (S-044)\n\
                 - List outreach sequences: GET /api/dept/gtm/outreach/sequences?session_id=<id>\n\
                 - Create sequence (draft): POST /api/dept/gtm/outreach/sequences {session_id, name, steps[{delay_days, channel, template}]}\n\
                 - Activate sequence: POST /api/dept/gtm/outreach/sequences/{id}/activate {session_id}\n\
                 - Run sequence on contact: POST /api/dept/gtm/outreach/execute {session_id, sequence_id, contact_id}\n\
                 - List invoices: GET /api/dept/gtm/invoices?session_id=<id>&status=<optional Draft|Sent|Paid|Overdue|Cancelled>\n\
                 - Create invoice: POST /api/dept/gtm/invoices {session_id, contact_id, items[{description, quantity, unit_price}], due_date RFC3339}\n\
                 - Invoice detail: GET /api/dept/gtm/invoices/{id}?session_id=<id>\n\
                 - Set invoice status: POST /api/dept/gtm/invoices/{id}/status {session_id, status}",
            );
        }
        _ => {}
    }

    // RAG: retrieve relevant knowledge
    if let (Some(embed_port), Some(vector_store)) = (&state.embedding, &state.vector_store)
        && let Ok(query_emb) = embed_port.embed_one(&body.message).await
    {
        let results = vector_store.search(&query_emb, 5).await.unwrap_or_default();
        if !results.is_empty() {
            resolved
                .system_prompt
                .push_str("\n\n--- Relevant Knowledge ---\n");
            for r in &results {
                resolved
                    .system_prompt
                    .push_str(&format!("[score: {:.2}] {}\n", r.score, r.entry.content));
            }
        }
    }

    // Build the user input (system prompt is passed via AgentConfig.instructions)
    let _prompt = build_dept_prompt(&resolved.system_prompt, &history, &effective_message);

    // Build AgentConfig for the runtime
    let model_ref = sse_helpers::parse_model_ref(&resolved.model);
    let mut meta = serde_json::Map::new();
    if let Some(t) = &body.model_tier {
        meta.insert(RUSVEL_META_MODEL_TIER.into(), serde_json::json!(t));
    }
    meta.insert(
        RUSVEL_META_DEPARTMENT_ID.into(),
        serde_json::json!(dept.as_str()),
    );
    let sid = body
        .session_id
        .as_ref()
        .and_then(|s| Uuid::parse_str(s).ok())
        .map(SessionId::from)
        .unwrap_or_else(SessionId::new);
    let agent_config = AgentConfig {
        profile_id: None,
        session_id: sid,
        model: Some(model_ref),
        tools: resolved.allowed_tools.clone(),
        instructions: Some(resolved.system_prompt.clone()),
        budget_limit: resolved.max_budget_usd,
        max_iterations: None,
        permission_mode: tool_permission_mode_from_dept_config_str(&resolved.permission_mode),
        metadata: serde_json::Value::Object(meta),
    };

    // Build the user message with conversation history context
    let mut user_input = String::new();
    for msg in &history {
        match msg.role.as_str() {
            "user" => user_input.push_str(&format!("User: {}\n\n", msg.content)),
            "assistant" => user_input.push_str(&format!("Assistant: {}\n\n", msg.content)),
            _ => {}
        }
    }
    user_input.push_str(&effective_message);

    // Create agent run
    let run_id = state
        .agent_runtime
        .create(agent_config)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Stream via AgentRuntime
    let input = Content::text(&user_input);
    let rx = state
        .agent_runtime
        .run_streaming(&run_id, input)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let storage = state.storage.clone();
    let events_port = state.events.clone();
    let conv_id = conversation_id.clone();
    let ns = namespace.clone();
    let run_id_str = run_id.to_string();

    let prelude = sse_helpers::prelude_stream(run_id_str.clone(), conv_id.clone());

    let main = ReceiverStream::new(rx).map(move |event| {
        Ok::<Event, Infallible>(match event {
            AgentEvent::Done { output } => {
                let full_text = sse_helpers::extract_done_text(&output);
                let cost = output.cost_estimate;

                let storage = storage.clone();
                let events_port = events_port.clone();
                let conv_id_inner = conv_id.clone();
                let ns_inner = ns.clone();
                let text = full_text.clone();
                let eng = dept.clone();
                tokio::spawn(async move {
                    let msg = ChatMessage {
                        id: uuid::Uuid::now_v7().to_string(),
                        conversation_id: conv_id_inner.clone(),
                        role: "assistant".into(),
                        content: text,
                        created_at: Utc::now().to_rfc3339(),
                    };
                    let _ = store_namespaced_message(&storage, &ns_inner, &msg).await;
                    let _ = events_port
                        .emit(rusvel_core::domain::Event {
                            id: EventId::new(),
                            session_id: None,
                            run_id: None,
                            source: eng.clone(),
                            kind: format!("{eng}.chat.completed"),
                            payload: serde_json::json!({
                                "conversation_id": conv_id_inner,
                                "cost_usd": cost,
                                "response_length": msg.content.len(),
                            }),
                            created_at: Utc::now(),
                            metadata: serde_json::json!({}),
                        })
                        .await;
                    crate::hook_dispatch::dispatch_hooks(
                        &format!("{eng}.chat.completed"),
                        serde_json::json!({
                            "conversation_id": conv_id_inner,
                            "cost_usd": cost,
                            "response_length": msg.content.len(),
                        }),
                        &eng.to_string(),
                        storage.clone(),
                    );
                });

                sse_helpers::run_completed_sse(&run_id_str, full_text, cost, &conv_id)
            }
            other => sse_helpers::other_event_sse(&run_id_str, other, &conv_id),
        })
    });

    let stream: Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>> =
        Box::pin(prelude.chain(main));

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

pub async fn dept_conversations(
    State(state): State<Arc<AppState>>,
    Path(dept): Path<String>,
) -> Result<Json<Vec<ConversationSummary>>, (StatusCode, String)> {
    validate_dept(&state, &dept)?;
    let namespace = msg_namespace(&dept);
    let df = rusvel_core::domain::ObjectFilter {
        limit: Some(5000),
        ..Default::default()
    };
    let all = state
        .storage
        .objects()
        .list(&namespace, df)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut convos: std::collections::HashMap<String, Vec<ChatMessage>> =
        std::collections::HashMap::new();
    for val in all {
        if let Ok(msg) = serde_json::from_value::<ChatMessage>(val) {
            convos
                .entry(msg.conversation_id.clone())
                .or_default()
                .push(msg);
        }
    }

    let mut summaries: Vec<ConversationSummary> = convos
        .into_iter()
        .map(|(id, mut msgs)| {
            msgs.sort_by(|a, b| a.created_at.cmp(&b.created_at));
            let title = msgs.iter().find(|m| m.role == "user").map_or_else(
                || "New conversation".into(),
                |m| {
                    if m.content.len() > 60 {
                        format!("{}...", &m.content[..57])
                    } else {
                        m.content.clone()
                    }
                },
            );
            let updated_at = msgs
                .last()
                .map(|m| m.created_at.clone())
                .unwrap_or_default();
            ConversationSummary {
                id,
                title,
                updated_at,
                message_count: msgs.len(),
            }
        })
        .collect();
    summaries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(Json(summaries))
}

pub async fn dept_history(
    State(state): State<Arc<AppState>>,
    Path((dept, id)): Path<(String, String)>,
) -> Result<Json<Vec<ChatMessage>>, (StatusCode, String)> {
    validate_dept(&state, &dept)?;
    let namespace = msg_namespace(&dept);
    load_namespaced_history(&state.storage, &namespace, &id, 200)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

pub async fn dept_events(
    State(state): State<Arc<AppState>>,
    Path(dept): Path<String>,
) -> Result<Json<Vec<rusvel_core::domain::Event>>, (StatusCode, String)> {
    let dept_def = validate_dept(&state, &dept)?;
    state
        .events
        .query(EventFilter {
            source: Some(dept_def.id.clone()),
            limit: Some(50),
            ..Default::default()
        })
        .await
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

// ── Helpers ──────────────────────────────────────────────────

fn build_dept_prompt(system_prompt: &str, history: &[ChatMessage], user_message: &str) -> String {
    let mut parts = Vec::new();
    parts.push(format!("<system>\n{system_prompt}\n</system>"));
    for msg in history {
        match msg.role.as_str() {
            "user" => parts.push(msg.content.clone()),
            "assistant" => parts.push(format!("<assistant>\n{}\n</assistant>", msg.content)),
            _ => {}
        }
    }
    parts.push(user_message.to_string());
    parts.join("\n\n")
}

async fn load_namespaced_history(
    storage: &Arc<dyn StoragePort>,
    namespace: &str,
    conversation_id: &str,
    limit: usize,
) -> rusvel_core::error::Result<Vec<ChatMessage>> {
    let filter = rusvel_core::domain::ObjectFilter {
        conversation_id: Some(conversation_id.to_string()),
        limit: Some((limit as u32).saturating_mul(4).clamp(100, 10_000)),
        ..Default::default()
    };
    let rows = storage.objects().list(namespace, filter).await?;
    let mut msgs: Vec<ChatMessage> = rows
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .filter(|m: &ChatMessage| m.conversation_id == conversation_id)
        .collect();
    msgs.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    if msgs.len() > limit {
        msgs = msgs[msgs.len() - limit..].to_vec();
    }
    Ok(msgs)
}

async fn store_namespaced_message(
    storage: &Arc<dyn StoragePort>,
    namespace: &str,
    msg: &ChatMessage,
) -> rusvel_core::error::Result<()> {
    storage
        .objects()
        .put(namespace, &msg.id, serde_json::to_value(msg)?)
        .await
}

/// Extract @agent-name from a message.
fn extract_agent_mention(message: &str) -> Option<String> {
    for word in message.split_whitespace() {
        if word.starts_with('@') && word.len() > 1 {
            return Some(word[1..].to_string());
        }
    }
    None
}
