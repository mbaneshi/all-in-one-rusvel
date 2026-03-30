//! # rusvel-api
//!
//! HTTP API surface for RUSVEL, built on Axum.
//!
//! Provides a JSON REST API for sessions, mission planning, goals,
//! and event queries. All domain logic is delegated to the Forge engine
//! and core port traits.

pub mod agents;
pub mod analytics;
pub mod approvals;
pub mod artifacts;
pub mod auth;
pub mod automation;
pub mod browser;
pub mod build_cmd;
pub mod capability;
pub mod chat;
pub mod config;
pub mod connectors;
pub mod cron;
pub mod dashboard;
pub mod db_routes;
pub mod department;
pub mod engine_routes;
pub mod flow_routes;
pub mod help;
pub mod hook_dispatch;
pub mod hooks;
pub mod jobs;
pub mod kits;
pub mod knowledge;
pub mod mcp_servers;
pub mod pipeline_runner;
pub mod playbooks;
pub(crate) mod request_id;
pub mod routes;
pub mod rules;
pub mod secrets;
pub mod skills;
pub(crate) mod sse_helpers;
pub mod system;
pub mod operator_runtime;
pub mod terminal;
pub mod visual_report;
pub mod webhooks;
pub mod workflows;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Router;
use axum::http::{HeaderValue, Method};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use tower::ServiceBuilder;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

use code_engine::CodeEngine;
use content_engine::ContentEngine;
use flow_engine::FlowEngine;
use forge_engine::ForgeEngine;
use gtm_engine::GtmEngine;
use harvest_engine::HarvestEngine;
use rusvel_agent::{AgentRuntime, ContextPack};
use rusvel_channel::ChannelPort;
use rusvel_core::domain::UserProfile;
use rusvel_core::id::SessionId;
use rusvel_core::ports::{
    AuthPort, DeployPort, EmbeddingPort, EventPort, JobPort, MemoryPort, SessionPort, StoragePort,
    TerminalPort, ToolPort, VectorStorePort,
};
use rusvel_core::registry::DepartmentRegistry;
use rusvel_cron::CronScheduler;
use rusvel_db::Database;
use rusvel_webhook::WebhookReceiver;

/// In-memory TTL cache for [`ContextPack`] assembly (S-045); keyed by session + department id.
#[derive(Default)]
pub struct ContextPackCache {
    pub(crate) inner: std::sync::Mutex<HashMap<(SessionId, String), (Instant, ContextPack)>>,
}

pub(crate) const CONTEXT_PACK_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(45);

/// Shared application state injected into all handlers.
pub struct AppState {
    pub forge: Arc<ForgeEngine>,
    pub code_engine: Option<Arc<CodeEngine>>,
    pub content_engine: Option<Arc<ContentEngine>>,
    pub harvest_engine: Option<Arc<HarvestEngine>>,
    pub gtm_engine: Option<Arc<GtmEngine>>,
    pub flow_engine: Option<Arc<FlowEngine>>,
    pub sessions: Arc<dyn SessionPort>,
    pub events: Arc<dyn EventPort>,
    /// Same SQLite job queue as [`StoragePort::jobs`] when using `rusvel_db::Database`.
    pub jobs: Arc<dyn JobPort>,
    /// Same database as `storage` when using `rusvel_db::Database` — health, analytics, direct SQL.
    pub database: Arc<Database>,
    /// RusvelBase browser routes (`/api/db/*`); implemented by [`rusvel_db::RusvelBaseAdapter`].
    pub rusvel_base: Arc<dyn rusvel_core::ports::RusvelBasePort>,
    pub storage: Arc<dyn StoragePort>,
    pub profile: Option<UserProfile>,
    pub registry: DepartmentRegistry,
    pub embedding: Option<Arc<dyn EmbeddingPort>>,
    pub vector_store: Option<Arc<dyn VectorStorePort>>,
    /// Session-scoped FTS memory (SQLite + FTS5); required for hybrid search.
    pub memory: Arc<dyn MemoryPort>,
    pub deploy: Option<Arc<dyn DeployPort>>,
    pub agent_runtime: Arc<AgentRuntime>,
    pub tools: Arc<dyn ToolPort>,
    pub terminal: Option<Arc<dyn TerminalPort>>,
    /// Chrome CDP client (passive capture + actions); `None` when not wired.
    pub cdp: Option<Arc<rusvel_cdp::CdpClient>>,
    /// Bearer token auth (opt-in via `RUSVEL_API_TOKEN`); see [`auth::AuthConfig`].
    pub auth: auth::AuthConfig,
    /// Credential / secret registry ([`AuthPort`]); same instance as department boot.
    pub credentials: Arc<dyn AuthPort>,
    /// Incoming webhooks (HMAC + object-store registrations).
    pub webhook_receiver: Arc<WebhookReceiver>,
    /// Cron schedules; tick enqueues [`rusvel_core::domain::JobKind::ScheduledCron`] jobs.
    pub cron_scheduler: Arc<CronScheduler>,
    pub context_pack_cache: Arc<ContextPackCache>,
    /// Outbound notifications (e.g. Telegram); `None` when not configured.
    pub channel: Option<Arc<dyn ChannelPort>>,
    pub boot_time: Instant,
    pub failed_departments: Vec<(String, String)>,
    /// Resolved data directory (`~/.rusvel` by default).
    pub data_dir: std::path::PathBuf,
    /// Effective HTTP bind this process uses (e.g. `127.0.0.1:3000`).
    pub http_listen: String,
    /// True when Claude is served via `claude -p` at boot (matches `compose_llm_multi` choice).
    pub claude_transport_cli: bool,
    /// Snapshot of operator prefs loaded at boot (ObjectStore `system`/`operator_prefs`).
    pub operator_prefs: operator_runtime::OperatorRuntimePrefs,
    /// When set, `POST /api/system/shutdown` signals graceful exit (same channel as Ctrl+C).
    pub shutdown_tx: Option<tokio::sync::watch::Sender<bool>>,
}

/// Build the Axum router with all routes, CORS, and tracing middleware.
///
/// If `frontend_dir` is provided and exists, the frontend SPA is served
/// at `/` with SPA fallback (all non-API routes serve `index.html`).
pub fn build_router(state: AppState) -> Router {
    build_router_with_frontend(state, None)
}

/// Build the router with optional frontend static file serving.
pub fn build_router_with_frontend(
    state: AppState,
    frontend_dir: Option<std::path::PathBuf>,
) -> Router {
    let shared = Arc::new(state);
    let _ = automation::register_app_state_for_worker(shared.clone());

    let api = Router::new()
        .route("/api/health", get(routes::health))
        .route(
            "/api/webhooks",
            get(webhooks::list_webhooks).post(webhooks::create_webhook),
        )
        .route("/api/webhooks/{id}", post(webhooks::receive_webhook))
        .route(
            "/api/cron",
            get(cron::list_schedules).post(cron::create_schedule),
        )
        .route("/api/cron/tick", post(cron::tick_now))
        .route(
            "/api/cron/{id}",
            get(cron::get_schedule)
                .put(cron::update_schedule)
                .delete(cron::delete_schedule),
        )
        .route("/api/brief", get(engine_routes::brief_get))
        .route("/api/brief/latest", get(engine_routes::brief_latest_get))
        .route("/api/brief/generate", post(engine_routes::brief_generate))
        .route(
            "/api/forge/pipeline",
            post(engine_routes::forge_pipeline_orchestrate),
        )
        .route(
            "/api/forge/artifacts",
            get(engine_routes::forge_artifacts_list),
        )
        .route("/api/sessions", get(routes::list_sessions))
        .route("/api/sessions", post(routes::create_session))
        .route("/api/sessions/{id}", get(routes::get_session))
        .route(
            "/api/sessions/{id}/mission/today",
            get(routes::mission_today),
        )
        .route("/api/sessions/{id}/mission/goals", get(routes::list_goals))
        .route(
            "/api/sessions/{id}/mission/goals",
            post(routes::create_goal),
        )
        .route("/api/sessions/{id}/events", get(routes::query_events))
        // Chat (god agent)
        .route("/api/chat", post(chat::chat_handler))
        .route("/api/chat/conversations", get(chat::list_conversations))
        .route("/api/chat/conversations/{id}", get(chat::get_history))
        // Config
        .route("/api/config", get(config::get_config))
        .route("/api/config", axum::routing::put(config::update_config))
        .route("/api/config/models", get(config::list_models))
        .route("/api/config/tools", get(config::list_tools))
        .route(
            "/api/config/llm-providers",
            get(config::llm_providers_status),
        )
        // Department Registry
        .route("/api/departments", get(department::list_departments))
        // RusvelBase — DB browser (schema, rows, SQL)
        .route("/api/db/tables", get(db_routes::list_tables))
        .route(
            "/api/db/tables/{table}/schema",
            get(db_routes::get_table_schema),
        )
        .route(
            "/api/db/tables/{table}/rows",
            get(db_routes::get_table_rows),
        )
        .route("/api/db/sql", axum::routing::post(db_routes::post_sql))
        // Profile
        .route("/api/profile", get(department::get_profile))
        .route(
            "/api/profile",
            axum::routing::put(department::update_profile),
        )
        // Departments — 6 parameterized routes replace 72 hardcoded ones
        .route("/api/dept/{dept}/chat", post(department::dept_chat))
        .route(
            "/api/dept/{dept}/chat/conversations",
            get(department::dept_conversations),
        )
        .route(
            "/api/dept/{dept}/chat/conversations/{id}",
            get(department::dept_history),
        )
        .route("/api/dept/{dept}/config", get(department::dept_config_get))
        .route(
            "/api/dept/{dept}/config",
            axum::routing::put(department::dept_config_update),
        )
        .route("/api/dept/{dept}/events", get(department::dept_events))
        // Agents CRUD
        .route(
            "/api/agents",
            get(agents::list_agents).post(agents::create_agent),
        )
        .route(
            "/api/agents/{id}",
            get(agents::get_agent)
                .put(agents::update_agent)
                .delete(agents::delete_agent),
        )
        // Skills CRUD
        .route(
            "/api/skills",
            get(skills::list_skills).post(skills::create_skill),
        )
        .route(
            "/api/skills/{id}",
            get(skills::get_skill)
                .put(skills::update_skill)
                .delete(skills::delete_skill),
        )
        // Rules CRUD
        .route(
            "/api/rules",
            get(rules::list_rules).post(rules::create_rule),
        )
        .route(
            "/api/rules/{id}",
            get(rules::get_rule)
                .put(rules::update_rule)
                .delete(rules::delete_rule),
        )
        // MCP Servers CRUD
        .route(
            "/api/mcp-servers",
            get(mcp_servers::list_mcp_servers).post(mcp_servers::create_mcp_server),
        )
        .route(
            "/api/mcp-servers/{id}",
            get(mcp_servers::get_mcp_server)
                .put(mcp_servers::update_mcp_server)
                .delete(mcp_servers::delete_mcp_server),
        )
        // Workflows CRUD + execution
        .route(
            "/api/workflows",
            get(workflows::list_workflows).post(workflows::create_workflow),
        )
        .route(
            "/api/workflows/{id}",
            get(workflows::get_workflow)
                .put(workflows::update_workflow)
                .delete(workflows::delete_workflow),
        )
        .route("/api/workflows/{id}/run", post(workflows::run_workflow))
        // Engine-specific routes (Code, Content, Harvest)
        .route("/api/dept/code/analyze", post(engine_routes::code_analyze))
        .route("/api/dept/code/search", get(engine_routes::code_search))
        .route(
            "/api/dept/content/draft",
            post(engine_routes::content_draft),
        )
        .route(
            "/api/dept/content/from-code",
            post(engine_routes::content_from_code),
        )
        .route(
            "/api/dept/content/{id}/approve",
            axum::routing::patch(engine_routes::content_approve),
        )
        .route(
            "/api/dept/content/publish",
            post(engine_routes::content_publish),
        )
        .route(
            "/api/dept/content/schedule",
            post(engine_routes::content_schedule),
        )
        .route("/api/dept/content/list", get(engine_routes::content_list))
        .route(
            "/api/dept/content/scheduled",
            get(engine_routes::content_scheduled),
        )
        .route(
            "/api/dept/harvest/score",
            post(engine_routes::harvest_score),
        )
        .route("/api/dept/harvest/scan", post(engine_routes::harvest_scan))
        .route(
            "/api/dept/harvest/ingest",
            post(engine_routes::harvest_ingest),
        )
        .route(
            "/api/dept/harvest/proposal",
            post(engine_routes::harvest_proposal),
        )
        .route(
            "/api/dept/harvest/pipeline",
            get(engine_routes::harvest_pipeline),
        )
        .route("/api/dept/harvest/list", get(engine_routes::harvest_list))
        .route(
            "/api/dept/harvest/outcomes",
            get(engine_routes::harvest_outcomes_list),
        )
        .route(
            "/api/dept/harvest/outcome",
            post(engine_routes::harvest_record_outcome),
        )
        .route(
            "/api/dept/harvest/advance",
            post(engine_routes::harvest_advance),
        )
        .route(
            "/api/dept/gtm/contacts",
            get(engine_routes::gtm_contacts_list).post(engine_routes::gtm_contacts_create),
        )
        .route("/api/dept/gtm/deals", get(engine_routes::gtm_deals_list))
        .route(
            "/api/dept/gtm/deals/advance",
            post(engine_routes::gtm_deal_advance),
        )
        .route(
            "/api/dept/gtm/outreach/sequences",
            get(engine_routes::gtm_outreach_sequences_list)
                .post(engine_routes::gtm_outreach_sequences_create),
        )
        .route(
            "/api/dept/gtm/outreach/sequences/{id}/activate",
            post(engine_routes::gtm_outreach_sequences_activate),
        )
        .route(
            "/api/dept/gtm/outreach/execute",
            post(engine_routes::gtm_outreach_execute),
        )
        .route(
            "/api/dept/gtm/invoices",
            get(engine_routes::gtm_invoices_list).post(engine_routes::gtm_invoices_create),
        )
        .route(
            "/api/dept/gtm/invoices/{id}",
            get(engine_routes::gtm_invoice_get),
        )
        .route(
            "/api/dept/gtm/invoices/{id}/status",
            post(engine_routes::gtm_invoice_set_status),
        )
        // Flow Engine (DAG workflows)
        .route(
            "/api/flows",
            get(flow_routes::list_flows).post(flow_routes::create_flow),
        )
        .route(
            "/api/flows/templates/cross-engine-handoff",
            get(flow_routes::get_cross_engine_handoff_template),
        )
        .route(
            "/api/flows/{id}",
            get(flow_routes::get_flow)
                .put(flow_routes::update_flow)
                .delete(flow_routes::delete_flow),
        )
        .route("/api/flows/{id}/run", post(flow_routes::run_flow))
        .route(
            "/api/flows/{id}/executions",
            get(flow_routes::list_executions),
        )
        .route(
            "/api/flows/{id}/executions/{exec_id}/panes",
            get(flow_routes::list_flow_execution_panes),
        )
        .route(
            "/api/flows/executions/{id}",
            get(flow_routes::get_execution),
        )
        .route(
            "/api/flows/executions/{id}/resume",
            post(flow_routes::resume_flow),
        )
        .route(
            "/api/flows/executions/{id}/retry/{node_id}",
            post(flow_routes::retry_node),
        )
        .route(
            "/api/flows/executions/{id}/checkpoint",
            get(flow_routes::get_checkpoint),
        )
        .route("/api/flows/node-types", get(flow_routes::list_node_types))
        // Playbooks (multi-step pipelines)
        .route("/api/playbooks/runs", get(playbooks::list_runs))
        .route("/api/playbooks/runs/{run_id}", get(playbooks::get_run))
        .route(
            "/api/playbooks",
            get(playbooks::list_playbooks).post(playbooks::create_playbook),
        )
        .route("/api/playbooks/{id}", get(playbooks::get_playbook))
        .route("/api/playbooks/{id}/run", post(playbooks::run_playbook))
        // Secrets registry (AuthPort; values redacted in list/get)
        .route("/api/secrets", get(secrets::list_secrets))
        .route(
            "/api/secrets/{key}",
            get(secrets::get_secret_meta)
                .put(secrets::upsert_secret)
                .delete(secrets::delete_secret),
        )
        // Starter kits
        .route("/api/kits", get(kits::list_kits))
        .route("/api/kits/{id}", get(kits::get_kit))
        .route("/api/kits/{id}/install", post(kits::install_kit))
        // Capability Engine
        .route("/api/capability/build", post(capability::build_capability))
        // Analytics
        .route("/api/analytics", get(analytics::get_analytics))
        .route("/api/analytics/dashboard", get(analytics::get_dashboard))
        .route("/api/analytics/spend", get(analytics::get_spend))
        .route("/api/analytics/costs", get(analytics::get_costs))
        .route(
            "/api/analytics/costs/summary",
            get(analytics::get_costs_summary),
        )
        // Help (AI-powered)
        .route("/api/help", post(help::help_handler))
        // Artifacts (saved outputs)
        .route(
            "/api/artifacts",
            get(artifacts::list_artifacts).post(artifacts::create_artifact),
        )
        .route(
            "/api/artifacts/{id}",
            get(artifacts::get_artifact).delete(artifacts::delete_artifact),
        )
        // Connectors (GitHub PAT)
        .route(
            "/api/connectors/github/status",
            get(connectors::github_status),
        )
        .route(
            "/api/connectors/github/pat",
            axum::routing::post(connectors::github_set_pat).delete(connectors::github_clear_pat),
        )
        // Active tasks dashboard
        .route("/api/dashboard/active", get(dashboard::active_dashboard))
        // Job queue (ADR-003)
        .route("/api/jobs", get(jobs::list_jobs))
        // Approvals (human-in-the-loop, ADR-008)
        .route("/api/approvals", get(approvals::list_pending))
        .route("/api/approvals/{id}/approve", post(approvals::approve_job))
        .route("/api/approvals/{id}/reject", post(approvals::reject_job))
        // Hooks CRUD
        .route(
            "/api/hooks",
            get(hooks::list_hooks).post(hooks::create_hook),
        )
        .route(
            "/api/hooks/{id}",
            get(hooks::get_hook)
                .put(hooks::update_hook)
                .delete(hooks::delete_hook),
        )
        .route("/api/hooks/events", get(hooks::list_hook_events))
        // Knowledge (RAG)
        .route("/api/knowledge", get(knowledge::list_knowledge))
        .route("/api/knowledge/ingest", post(knowledge::ingest_knowledge))
        .route("/api/knowledge/search", post(knowledge::search_knowledge))
        .route(
            "/api/knowledge/hybrid-search",
            post(knowledge::hybrid_search_knowledge),
        )
        .route("/api/knowledge/stats", get(knowledge::knowledge_stats))
        .route("/api/knowledge/related", get(knowledge::related_knowledge))
        .route(
            "/api/knowledge/{id}",
            axum::routing::delete(knowledge::delete_knowledge),
        )
        // System self-improvement
        .route("/api/system/test", post(system::run_tests))
        .route("/api/system/build", post(system::run_build))
        .route("/api/system/status", get(system::get_status))
        .route("/api/system/runtime", get(operator_runtime::get_runtime))
        .route(
            "/api/system/operator-prefs",
            axum::routing::put(operator_runtime::put_operator_prefs),
        )
        .route("/api/system/shutdown", post(operator_runtime::post_shutdown))
        .route("/api/system/notify", post(system::notify))
        .route("/api/system/fix", post(system::self_fix))
        .route("/api/system/ingest-docs", post(system::ingest_docs))
        // Visual regression testing
        .route(
            "/api/system/visual-report",
            get(visual_report::get_reports).post(visual_report::store_report),
        )
        .route(
            "/api/system/visual-report/self-correct",
            post(visual_report::self_correct),
        )
        .route(
            "/api/system/visual-test",
            post(visual_report::run_visual_tests),
        )
        // Terminal: dept pane + WebSocket + run-scoped panes (delegation visibility)
        .route(
            "/api/terminal/session/snapshot",
            get(terminal::terminal_session_snapshot),
        )
        .route(
            "/api/terminal/dept/{dept_id}",
            get(terminal::terminal_dept_pane),
        )
        .route(
            "/api/terminal/dept/{dept_id}/trace",
            axum::routing::post(terminal::terminal_dept_trace),
        )
        .route(
            "/api/terminal/runs/{run_id}/panes",
            get(terminal::terminal_run_panes),
        )
        .route("/api/terminal/ws", get(terminal::terminal_ws))
        .route(
            "/api/terminal/pane/{pane_id}/resize",
            axum::routing::post(terminal::terminal_resize_pane),
        )
        .route(
            "/api/terminal/window/{window_id}/pane",
            axum::routing::post(terminal::terminal_window_add_pane),
        )
        .route(
            "/api/terminal/window/{window_id}/layout",
            axum::routing::post(terminal::terminal_window_set_layout),
        )
        // Browser (CDP)
        .route("/api/browser/status", get(browser::browser_status))
        .route(
            "/api/browser/connect",
            axum::routing::post(browser::browser_connect),
        )
        .route("/api/browser/tabs", get(browser::browser_tabs))
        .route(
            "/api/browser/observe/{tab}",
            axum::routing::post(browser::browser_observe),
        )
        .route("/api/browser/captures", get(browser::browser_captures))
        .route(
            "/api/browser/captures/stream",
            get(browser::browser_captures_stream),
        )
        .route(
            "/api/browser/act",
            axum::routing::post(browser::browser_act),
        )
        .with_state(shared.clone());

    // Serve frontend SPA if build directory exists.
    let app = if let Some(dir) = frontend_dir.filter(|d| d.exists()) {
        let index_html = std::fs::read_to_string(dir.join("index.html"))
            .unwrap_or_else(|_| "<h1>Frontend not found</h1>".into());
        let spa_fallback = {
            let html = index_html.clone();
            move |req: axum::extract::Request| {
                let html = html.clone();
                async move {
                    let path = req.uri().path();
                    if path.starts_with("/_app") || path == "/favicon.png" {
                        axum::response::Html("").into_response()
                    } else {
                        axum::response::Html(html).into_response()
                    }
                }
            }
        };
        api.nest_service("/_app", ServeDir::new(dir.join("_app")))
            .nest_service("/favicon.png", ServeDir::new(dir.join("favicon.png")))
            .fallback(spa_fallback)
    } else {
        api
    };

    let allowed_origins = [
        "http://localhost:5173".parse::<HeaderValue>().unwrap(),
        "http://localhost:3000".parse::<HeaderValue>().unwrap(),
    ];
    let rate_limit: u64 = std::env::var("RUSVEL_RATE_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100);

    app.layer(
        CorsLayer::new()
            .allow_origin(allowed_origins)
            .allow_methods([
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::DELETE,
                Method::OPTIONS,
            ])
            .allow_headers([
                axum::http::header::CONTENT_TYPE,
                axum::http::header::AUTHORIZATION,
            ]),
    )
    .layer(
        ServiceBuilder::new()
            .layer(axum::error_handling::HandleErrorLayer::new(|_| async {
                axum::http::StatusCode::TOO_MANY_REQUESTS
            }))
            .buffer(64)
            .rate_limit(rate_limit, Duration::from_secs(1)),
    )
    .layer(axum::middleware::from_fn_with_state(
        shared.clone(),
        auth::bearer_auth,
    ))
    .layer(TraceLayer::new_for_http())
    .layer(axum::middleware::from_fn(request_id::request_id_middleware))
}

/// Start the HTTP server on the given address with graceful shutdown.
///
/// After the shutdown signal fires, in-flight connections (e.g. SSE streams)
/// get 5 seconds to finish before the process force-exits.
pub async fn start_server(
    router: Router,
    addr: SocketAddr,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> Result<(), Box<dyn std::error::Error>> {
    tracing::info!("RUSVEL API listening on {addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;

    let (graceful_tx, graceful_rx) = tokio::sync::oneshot::channel::<()>();

    // When the shutdown signal fires, trigger graceful shutdown then start a
    // hard-exit timer on a real OS thread (immune to tokio runtime shutdown).
    tokio::spawn(async move {
        shutdown.await;
        let _ = graceful_tx.send(());
        tracing::info!("Graceful shutdown started — force exit in 3s");
        // OS thread guarantees the timer fires even if tokio is shutting down
        std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_secs(3));
            std::process::exit(0);
        });
    });

    let shutdown_signal = async move {
        let _ = graceful_rx.await;
    };

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal)
        .await?;
    Ok(())
}
