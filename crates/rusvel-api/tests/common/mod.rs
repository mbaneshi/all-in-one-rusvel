//! Shared Axum test harness: temp DB, stub LLM, Forge + content engines.

pub mod outreach;

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use chrono::Utc;
use code_engine::CodeEngine;
use content_engine::{ContentEngine, MockPlatformAdapter};
use flow_engine::FlowEngine;
use forge_engine::ForgeEngine;
use gtm_engine::GtmEngine;
use harvest_engine::HarvestEngine;
use rusvel_agent::AgentRuntime;
use rusvel_api::operator_runtime::OperatorRuntimePrefs;
use rusvel_api::{AppState, build_router};
use rusvel_auth::InMemoryAuthAdapter;
use rusvel_config::TomlConfig;
use rusvel_core::domain::{
    AgentOutput, AgentStatus, Content, FinishReason, LlmRequest, LlmResponse, LlmUsage, ModelRef,
    Session, SessionConfig, SessionKind, ToolDefinition, ToolResult,
};
use rusvel_core::error::Result;
use rusvel_core::id::{RunId, SessionId};
use rusvel_core::ports::{
    AgentPort, AuthPort, ConfigPort, EventPort, JobPort, LlmPort, MemoryPort, SessionPort,
    StoragePort, ToolPort,
};
use rusvel_core::registry::DepartmentRegistry;
use rusvel_db::Database;
use rusvel_event::EventBus;
use rusvel_llm::claude_transport_is_cli;
use rusvel_memory::MemoryStore;
use serde_json::{Value, json};
use tower::ServiceExt;

pub struct SessionAdapter(pub Arc<dyn StoragePort>);

#[async_trait]
impl SessionPort for SessionAdapter {
    async fn create(&self, session: Session) -> Result<SessionId> {
        let id = session.id;
        self.0.sessions().put_session(&session).await?;
        Ok(id)
    }
    async fn load(&self, id: &SessionId) -> Result<Session> {
        self.0.sessions().get_session(id).await?.ok_or_else(|| {
            rusvel_core::error::RusvelError::NotFound {
                kind: "session".into(),
                id: id.to_string(),
            }
        })
    }
    async fn save(&self, session: &Session) -> Result<()> {
        self.0.sessions().put_session(session).await
    }
    async fn list(&self) -> Result<Vec<rusvel_core::domain::SessionSummary>> {
        self.0.sessions().list_sessions().await
    }
}

struct StubLlm;

#[async_trait]
impl LlmPort for StubLlm {
    async fn generate(&self, _: LlmRequest) -> Result<LlmResponse> {
        Ok(LlmResponse {
            content: Content::text("stub"),
            finish_reason: FinishReason::Stop,
            usage: LlmUsage::default(),
            metadata: json!({}),
        })
    }
    async fn embed(&self, _: &ModelRef, _: &str) -> Result<Vec<f32>> {
        Ok(vec![])
    }
    async fn list_models(&self) -> Result<Vec<ModelRef>> {
        Ok(vec![])
    }
}

struct StubTool;

#[async_trait]
impl ToolPort for StubTool {
    async fn register(&self, _: ToolDefinition) -> Result<()> {
        Ok(())
    }
    async fn call(&self, _: &str, _: Value) -> Result<ToolResult> {
        Ok(ToolResult {
            success: true,
            output: Content::text("ok"),
            metadata: json!({}),
        })
    }
    fn list(&self) -> Vec<ToolDefinition> {
        vec![]
    }
    fn search(&self, _: &str, _: usize) -> Vec<ToolDefinition> {
        vec![]
    }
    fn schema(&self, _: &str) -> Option<Value> {
        None
    }
}

struct StaticDraftAgent;

#[async_trait]
impl AgentPort for StaticDraftAgent {
    async fn create(&self, _: rusvel_core::domain::AgentConfig) -> Result<RunId> {
        Ok(RunId::new())
    }
    async fn run(&self, _: &RunId, _: Content) -> Result<AgentOutput> {
        Ok(AgentOutput {
            run_id: RunId::new(),
            content: Content::text("# Generated Title\n\nBody from mock agent."),
            tool_calls: 0,
            usage: rusvel_core::domain::LlmUsage::default(),
            cost_estimate: 0.0,
            metadata: json!({}),
        })
    }
    async fn stop(&self, _: &RunId) -> Result<()> {
        Ok(())
    }
    async fn status(&self, _: &RunId) -> Result<AgentStatus> {
        Ok(AgentStatus::Idle)
    }
}

#[allow(dead_code)]
pub struct TestHarness {
    pub router: Router,
    pub session_id: SessionId,
    pub mock_twitter: Arc<MockPlatformAdapter>,
    pub jobs: Arc<dyn JobPort>,
    /// Shared with the router [`AppState`] (events, outreach, approvals).
    pub events: Arc<dyn EventPort>,
    /// Same [`EventBus`] as the router — use [`EventBus::subscribe`] for trigger tests (S-046).
    pub event_bus: Arc<EventBus>,
    pub storage: Arc<dyn StoragePort>,
    pub agent_port: Arc<dyn AgentPort>,
    /// Same engine instance as [`AppState::gtm_engine`] when the harness was built with GTM.
    pub gtm_engine: Option<Arc<GtmEngine>>,
}

#[allow(dead_code)]
pub async fn json_request(
    router: &mut Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Vec<u8>) {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    let req = if let Some(b) = body {
        builder
            .body(Body::from(serde_json::to_vec(&b).unwrap()))
            .unwrap()
    } else {
        builder.body(Body::empty()).unwrap()
    };
    let res = router.oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    (status, bytes.to_vec())
}

#[allow(dead_code)]
pub async fn build_harness() -> TestHarness {
    build_harness_with_auth(rusvel_api::auth::AuthConfig::from_env()).await
}

/// API harness with [`GtmEngine`] wired (same storage/jobs as the router).
#[allow(dead_code)]
pub async fn build_harness_with_gtm() -> TestHarness {
    build_harness_with_auth_and_gtm(rusvel_api::auth::AuthConfig::from_env(), true).await
}

pub async fn build_harness_with_auth(auth: rusvel_api::auth::AuthConfig) -> TestHarness {
    build_harness_with_auth_and_gtm(auth, false).await
}

async fn build_harness_with_auth_and_gtm(
    auth: rusvel_api::auth::AuthConfig,
    include_gtm: bool,
) -> TestHarness {
    let base = std::env::temp_dir().join(format!("rusvel-api-ict-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&base).expect("temp dir");
    let db_path = base.join("rusvel.db");
    let db: Arc<Database> = Arc::new(Database::open(&db_path).expect("db"));
    let storage: Arc<dyn StoragePort> = db.clone();
    let config: Arc<dyn ConfigPort> =
        Arc::new(TomlConfig::load(base.join("config.toml")).expect("config"));
    let event_bus: Arc<EventBus> = Arc::new(EventBus::new(
        db.clone() as Arc<dyn rusvel_core::ports::EventStore>
    ));
    let events: Arc<dyn EventPort> = event_bus.clone();
    let memory: Arc<dyn MemoryPort> =
        Arc::new(MemoryStore::open(base.join("memory.db").to_str().unwrap()).expect("memory"));
    let jobs: Arc<dyn JobPort> = db.clone() as Arc<dyn JobPort>;
    let jobs_for_harness = jobs.clone();
    let events_for_harness = events.clone();
    let sessions: Arc<dyn SessionPort> = Arc::new(SessionAdapter(storage.clone()));
    let agent: Arc<dyn AgentPort> = Arc::new(StaticDraftAgent);
    let tools: Arc<dyn ToolPort> = Arc::new(StubTool);
    let agent_runtime = Arc::new(AgentRuntime::new(
        Arc::new(StubLlm),
        tools.clone(),
        memory.clone(),
    ));

    let forge = Arc::new(ForgeEngine::new(
        agent.clone(),
        events.clone(),
        memory.clone(),
        storage.clone(),
        jobs.clone(),
        sessions.clone(),
        config.clone(),
    ));

    let code_engine = Arc::new(CodeEngine::new(storage.clone(), events.clone()));
    let content_engine = Arc::new(ContentEngine::new(
        storage.clone(),
        events.clone(),
        agent.clone(),
        jobs.clone(),
    ));

    let flow_engine = Arc::new(FlowEngine::new(
        storage.clone(),
        events.clone(),
        agent.clone(),
        None,
        None,
    ));
    let harvest_engine = Arc::new(
        HarvestEngine::new(storage.clone())
            .with_events(events.clone())
            .with_agent(agent.clone()),
    );
    let mock_tw = Arc::new(MockPlatformAdapter::new(
        rusvel_core::domain::Platform::Twitter,
    ));
    content_engine.register_platform(mock_tw.clone());

    let now = Utc::now();
    let session_id = SessionId::new();
    let session = Session {
        id: session_id,
        name: "test".into(),
        kind: SessionKind::General,
        tags: vec![],
        config: SessionConfig::default(),
        created_at: now,
        updated_at: now,
        metadata: json!({}),
    };
    sessions.create(session).await.expect("session");

    let gtm_engine: Option<Arc<GtmEngine>> = if include_gtm {
        Some(Arc::new(GtmEngine::new(
            storage.clone(),
            events.clone(),
            agent_runtime.clone(),
            jobs.clone(),
        )))
    } else {
        None
    };

    let webhook_receiver = Arc::new(rusvel_webhook::WebhookReceiver::new(
        storage.clone(),
        events.clone(),
    ));
    let cron_scheduler = Arc::new(rusvel_cron::CronScheduler::new(
        storage.clone(),
        jobs.clone(),
    ));

    let rusvel_base: Arc<dyn rusvel_core::ports::RusvelBasePort> =
        Arc::new(rusvel_db::RusvelBaseAdapter(db.clone()));

    let credentials: Arc<dyn AuthPort> = Arc::new(InMemoryAuthAdapter::new());

    let state = AppState {
        forge,
        code_engine: Some(code_engine),
        content_engine: Some(content_engine),
        harvest_engine: Some(harvest_engine),
        gtm_engine: gtm_engine.clone(),
        flow_engine: Some(flow_engine),
        sessions,
        events,
        jobs,
        database: db.clone(),
        rusvel_base,
        storage: storage.clone(),
        profile: None,
        registry: DepartmentRegistry::load(
            PathBuf::from("/__no_such__/departments.toml").as_path(),
        ),
        embedding: None,
        vector_store: None,
        memory: memory.clone(),
        deploy: None,
        agent_runtime,
        tools,
        terminal: None,
        cdp: None,
        auth,
        credentials,
        webhook_receiver,
        cron_scheduler,
        context_pack_cache: Arc::new(rusvel_api::ContextPackCache::default()),
        channel: None,
        boot_time: std::time::Instant::now(),
        failed_departments: Vec::new(),
        data_dir: std::env::temp_dir().join("rusvel-api-test"),
        http_listen: "127.0.0.1:3000".into(),
        llm_multi: Arc::new(rusvel_llm::MultiProvider::new()),
        claude_transport_cli: Arc::new(std::sync::atomic::AtomicBool::new(
            claude_transport_is_cli(),
        )),
        operator_prefs: OperatorRuntimePrefs::default(),
        shutdown_tx: None,
        reexec_pending: Arc::new(std::sync::atomic::AtomicBool::new(false)),
    };

    let router = build_router(state);
    TestHarness {
        router,
        session_id,
        mock_twitter: mock_tw,
        jobs: jobs_for_harness,
        events: events_for_harness,
        event_bus,
        storage,
        agent_port: agent,
        gtm_engine,
    }
}
