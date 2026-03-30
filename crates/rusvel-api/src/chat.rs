//! Chat API handler — streaming conversation with the god agent.
//!
//! `POST /api/chat` accepts a message and streams the response via SSE.
//! Conversation history is persisted in `ObjectStore`.

use std::convert::Infallible;
use std::pin::Pin;
use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, KeepAliveStream, Sse};
use chrono::Utc;
use futures::stream::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use tokio_stream::wrappers::ReceiverStream;
use uuid::Uuid;

use rusvel_agent::AgentEvent;
use rusvel_core::domain::{
    AgentConfig, Content, RUSVEL_META_DEPARTMENT_ID, RUSVEL_META_MODEL_TIER,
};
use rusvel_core::id::SessionId;
use rusvel_core::ports::{AgentPort, StoragePort};

use crate::AppState;
use crate::config::load_and_migrate_chat_config;
use crate::sse_helpers;

// ── Request / Response types ─────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub message: String,
    #[serde(default)]
    pub conversation_id: Option<String>,
    /// Overrides persisted chat config when set (`fast` | `balanced` | `premium`).
    #[serde(default)]
    pub model_tier: Option<String>,
    /// Active workspace session (UUID); attributes LLM spend metrics to this session.
    #[serde(default)]
    pub session_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChatMessage {
    pub id: String,
    pub conversation_id: String,
    pub role: String, // "user" | "assistant" | "system"
    pub content: String,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct ConversationSummary {
    pub id: String,
    pub title: String,
    pub updated_at: String,
    pub message_count: usize,
}

// ── Handlers ─────────────────────────────────────────────────

/// `POST /api/chat` — stream a response via SSE.
pub async fn chat_handler(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ChatRequest>,
) -> Result<
    Sse<KeepAliveStream<Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>>>>,
    (StatusCode, String),
> {
    let profile = state.profile.as_ref().ok_or((
        StatusCode::INTERNAL_SERVER_ERROR,
        "no profile loaded".into(),
    ))?;

    let conversation_id = body
        .conversation_id
        .unwrap_or_else(|| uuid::Uuid::now_v7().to_string());

    // Load conversation history (last 50 messages)
    let history = load_history(&state.storage, &conversation_id, 50)
        .await
        .unwrap_or_default();

    // Store the user message
    let user_msg = ChatMessage {
        id: uuid::Uuid::now_v7().to_string(),
        conversation_id: conversation_id.clone(),
        role: "user".into(),
        content: body.message.clone(),
        created_at: Utc::now().to_rfc3339(),
    };
    let _ = store_message(&state.storage, &user_msg).await;

    // Load chat config (model, effort, tools, etc.) — migrates legacy bare `opus`/`sonnet`/`haiku` to Cursor.
    let chat_config = load_and_migrate_chat_config(&state.storage)
        .await
        .map_err(|e| (e.0, e.1))?;

    // Build AgentConfig from chat config + profile + department capabilities
    let mut system_prompt = profile.to_system_prompt();
    system_prompt.push_str(&build_capabilities_overview(&state));
    let model_ref = sse_helpers::parse_model_ref(&chat_config.model);
    let tier = body
        .model_tier
        .as_deref()
        .or(chat_config.model_tier.as_deref());
    let mut meta = serde_json::Map::new();
    if let Some(t) = tier {
        meta.insert(RUSVEL_META_MODEL_TIER.into(), serde_json::json!(t));
    }
    meta.insert(
        RUSVEL_META_DEPARTMENT_ID.into(),
        serde_json::json!("global"),
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
        tools: chat_config.allowed_tools.clone(),
        instructions: Some(system_prompt),
        budget_limit: chat_config.max_budget_usd,
        max_iterations: None,
        permission_mode: Default::default(),
        metadata: serde_json::Value::Object(meta),
    };

    // Build user input with conversation history context
    let mut user_input = String::new();
    for msg in &history {
        match msg.role.as_str() {
            "user" => user_input.push_str(&format!("User: {}\n\n", msg.content)),
            "assistant" => user_input.push_str(&format!("Assistant: {}\n\n", msg.content)),
            _ => {}
        }
    }
    user_input.push_str(&body.message);

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
    let conv_id = conversation_id.clone();
    let run_id_str = run_id.to_string();

    let prelude = sse_helpers::prelude_stream(run_id_str.clone(), conv_id.clone());

    let main = ReceiverStream::new(rx).map(move |event| {
        Ok::<Event, Infallible>(match event {
            AgentEvent::Done { output } => {
                let full_text = sse_helpers::extract_done_text(&output);
                let cost = output.cost_estimate;

                let storage = storage.clone();
                let conv_id_inner = conv_id.clone();
                let text = full_text.clone();
                tokio::spawn(async move {
                    let msg = ChatMessage {
                        id: uuid::Uuid::now_v7().to_string(),
                        conversation_id: conv_id_inner,
                        role: "assistant".into(),
                        content: text,
                        created_at: Utc::now().to_rfc3339(),
                    };
                    let _ = store_message(&storage, &msg).await;
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

/// `GET /api/chat/conversations` — list all conversations.
pub async fn list_conversations(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<ConversationSummary>>, (StatusCode, String)> {
    let mut conv_filter = rusvel_core::domain::ObjectFilter::default();
    conv_filter.limit = Some(5000);
    let all = state
        .storage
        .objects()
        .list("chat_message", conv_filter)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Group by conversation_id
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
            let title = msgs
                .iter()
                .find(|m| m.role == "user")
                .map(|m| {
                    if m.content.len() > 60 {
                        format!("{}...", &m.content[..57])
                    } else {
                        m.content.clone()
                    }
                })
                .unwrap_or_else(|| "New conversation".into());
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

/// `GET /api/chat/conversations/{id}` — get message history.
pub async fn get_history(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<Vec<ChatMessage>>, (StatusCode, String)> {
    let msgs = load_history(&state.storage, &id, 200)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(msgs))
}

// ── Internal helpers ─────────────────────────────────────────

async fn load_history(
    storage: &Arc<dyn StoragePort>,
    conversation_id: &str,
    limit: usize,
) -> rusvel_core::error::Result<Vec<ChatMessage>> {
    let mut filter = rusvel_core::domain::ObjectFilter::default();
    filter.conversation_id = Some(conversation_id.to_string());
    filter.limit = Some((limit as u32).saturating_mul(4).max(100).min(10_000));
    let rows = storage.objects().list("chat_message", filter).await?;

    let mut msgs: Vec<ChatMessage> = rows
        .into_iter()
        .filter_map(|v| serde_json::from_value::<ChatMessage>(v).ok())
        .filter(|m| m.conversation_id == conversation_id)
        .collect();

    msgs.sort_by(|a, b| a.created_at.cmp(&b.created_at));

    if msgs.len() > limit {
        msgs = msgs[msgs.len() - limit..].to_vec();
    }

    Ok(msgs)
}

async fn store_message(
    storage: &Arc<dyn StoragePort>,
    msg: &ChatMessage,
) -> rusvel_core::error::Result<()> {
    storage
        .objects()
        .put("chat_message", &msg.id, serde_json::to_value(msg)?)
        .await
}

/// Build a capabilities overview from the department registry for the God Agent.
fn build_capabilities_overview(state: &AppState) -> String {
    let mut out = String::from("\n\n--- RUSVEL Capabilities ---\n");
    out.push_str("You are the God Agent — the central orchestrator across all departments.\n");
    out.push_str("When asked what you can do, describe these capabilities:\n\n");

    // Departments
    out.push_str("DEPARTMENTS:\n");
    for d in state.registry.list() {
        out.push_str(&format!(
            "- {} ({}): {}\n",
            d.name,
            d.id,
            d.capabilities.join(", ")
        ));
    }

    // Wired engines with real actions
    out.push_str("\nWIRED ENGINE ACTIONS:\n");
    out.push_str("- Code: analyze repos (POST /api/dept/code/analyze), search symbols (GET /api/dept/code/search)\n");
    out.push_str("- Content: draft articles, publish to LinkedIn/Twitter/DEV.to, manage content calendar, schedule posts\n");
    out.push_str("- Harvest: scan opportunity sources (Upwork, freelancer, CDP), score leads, generate proposals, track pipeline\n");
    out.push_str("- GTM: manage CRM contacts, deals, outreach sequences, invoices\n");
    out.push_str("- Forge: mission planning, daily plans, goal tracking, weekly reviews, executive briefings\n");
    out.push_str("- Flow: DAG workflow engine with code/condition/agent nodes\n");
    out.push_str("- Finance: ledger entries, runway forecast, tax estimation\n");
    out.push_str("- Product: roadmap items, pricing tiers, feedback tracking\n");
    out.push_str("- Growth: funnel analysis, cohort tracking, KPI dashboards\n");
    out.push_str("- Distro: SEO, marketplace listings, affiliate programs\n");
    out.push_str("- Legal: contracts, compliance checks, IP management\n");
    out.push_str("- Support: ticket queue, knowledge base, NPS surveys\n");
    out.push_str("- Infra: deployments, monitoring, incident response\n");

    // Platform-wide capabilities
    out.push_str("\nPLATFORM CAPABILITIES:\n");
    out.push_str("- Chat with any department via /dept/{id}/chat\n");
    out.push_str("- Agents: create custom AI agents with specific personas and tools\n");
    out.push_str("- Skills: reusable prompt templates with {{input}} interpolation\n");
    out.push_str("- Rules: auto-injected system prompt rules per department\n");
    out.push_str("- Hooks: event-triggered actions on chat completion\n");
    out.push_str("- Workflows: visual DAG builder for multi-step automations\n");
    out.push_str("- Knowledge base: vector-backed RAG for semantic search\n");
    out.push_str("- Webhooks: register incoming webhooks with HMAC verification\n");
    out.push_str("- Cron: schedule recurring jobs (daily briefings, scans, etc.)\n");
    out.push_str("- Terminal: browser-based PTY terminal\n");
    out.push_str("- Database browser: schema introspection, SQL runner\n");
    out.push_str("- MCP: connect to external MCP servers for tool discovery\n");
    out.push_str(
        "- Approval queue: human-in-the-loop approval for content publishing and outreach\n",
    );

    // Tools summary
    let tool_count = state.tools.list().len();
    out.push_str(&format!(
        "\nTOOLS: {} registered (use tool_search to discover available tools)\n",
        tool_count
    ));

    out
}
