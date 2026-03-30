//! Harvest scan → proposal persistence; mock source only (no live RSS).

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use chrono::Utc;
use forge_engine::ForgeEngine;
use harvest_engine::HarvestEngine;
use rusvel_agent::AgentRuntime;
use rusvel_api::{AppState, build_router};
use rusvel_config::TomlConfig;
use rusvel_core::domain::{
    AgentOutput, AgentStatus, Content, FinishReason, JobKind, JobResult, LlmRequest, LlmResponse,
    LlmUsage, ModelRef, Session, SessionConfig, SessionKind, ToolDefinition, ToolResult,
};
use rusvel_core::error::Result;
use rusvel_core::id::{RunId, SessionId};
use rusvel_core::ports::{
    AgentPort, AuthPort, ConfigPort, EventPort, JobPort, LlmPort, MemoryPort, SessionPort,
    StoragePort, ToolPort,
};
use rusvel_auth::InMemoryAuthAdapter;
use rusvel_core::registry::DepartmentRegistry;
use rusvel_db::Database;
use rusvel_event::EventBus;
use rusvel_memory::MemoryStore;
use serde_json::{Value, json};
use tower::{Service, ServiceExt};

struct SessionAdapter(Arc<dyn StoragePort>);

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

struct StaticForgeAgent;

#[async_trait]
impl AgentPort for StaticForgeAgent {
    async fn create(&self, _: rusvel_core::domain::AgentConfig) -> Result<RunId> {
        Ok(RunId::new())
    }
    async fn run(&self, _: &RunId, _: Content) -> Result<AgentOutput> {
        Ok(AgentOutput {
            run_id: RunId::new(),
            content: Content::text("ok"),
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

/// Returns JSON matching [`harvest_engine::proposal::ProposalGenerator`] expectations.
struct StaticProposalAgent;

#[async_trait]
impl AgentPort for StaticProposalAgent {
    async fn create(&self, _: rusvel_core::domain::AgentConfig) -> Result<RunId> {
        Ok(RunId::new())
    }
    async fn run(&self, _: &RunId, _: Content) -> Result<AgentOutput> {
        Ok(AgentOutput {
            run_id: RunId::new(),
            content: Content::text(
                r#"{"body":"Tailored proposal for the gig.","estimated_value":5000.0,"tone":"professional"}"#,
            ),
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

async fn json_request(
    router: &mut Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Vec<u8>) {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    let req: Request<Body> = if let Some(b) = body {
        builder
            .body(Body::from(serde_json::to_vec(&b).unwrap()))
            .unwrap()
    } else {
        builder.body(Body::empty()).unwrap()
    };
    let res = ServiceExt::<Request<Body>>::ready(&mut *router)
        .await
        .unwrap()
        .call(req)
        .await
        .unwrap();
    let status = res.status();
    let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    (status, bytes.to_vec())
}

/// Same branch as `rusvel-app` for [`JobKind::ProposalDraft`]: generate, then park for approval.
async fn run_proposal_draft_job(
    harvest: &HarvestEngine,
    jobs: &dyn JobPort,
) -> rusvel_core::error::Result<()> {
    let Some(job) = jobs.dequeue(&[JobKind::ProposalDraft]).await? else {
        return Err(rusvel_core::error::RusvelError::Internal(
            "expected a ProposalDraft job".into(),
        ));
    };
    let job_id = job.id;
    let sid = job.session_id;
    let opp = job
        .payload
        .get("opportunity_id")
        .and_then(|x| x.as_str())
        .ok_or_else(|| {
            rusvel_core::error::RusvelError::Validation(
                "ProposalDraft job missing opportunity_id".into(),
            )
        })?;
    let profile = job
        .payload
        .get("profile")
        .and_then(|x| x.as_str())
        .unwrap_or("");
    let proposal = harvest.generate_proposal(&sid, opp, profile).await?;
    let job_result = JobResult {
        output: serde_json::to_value(&proposal).unwrap_or_default(),
        metadata: json!({"engine": "harvest"}),
    };
    jobs.hold_for_approval(&job_id, job_result).await?;
    Ok(())
}

async fn test_router() -> (
    Router,
    SessionId,
    SessionId,
    Arc<HarvestEngine>,
    Arc<dyn JobPort>,
) {
    let base = std::env::temp_dir().join(format!("rusvel-harvest-ict-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&base).expect("temp dir");
    let db_path = base.join("rusvel.db");
    let db: Arc<Database> = Arc::new(Database::open(&db_path).expect("db"));
    let storage: Arc<dyn StoragePort> = db.clone();
    let config: Arc<dyn ConfigPort> =
        Arc::new(TomlConfig::load(base.join("config.toml")).expect("config"));
    let events: Arc<dyn EventPort> = Arc::new(EventBus::new(
        db.clone() as Arc<dyn rusvel_core::ports::EventStore>
    ));
    let memory: Arc<dyn MemoryPort> =
        Arc::new(MemoryStore::open(base.join("memory.db").to_str().unwrap()).expect("memory"));
    let jobs: Arc<dyn JobPort> = db.clone() as Arc<dyn JobPort>;
    let sessions: Arc<dyn SessionPort> = Arc::new(SessionAdapter(storage.clone()));

    let tools: Arc<dyn ToolPort> = Arc::new(StubTool);
    let agent_runtime = Arc::new(AgentRuntime::new(
        Arc::new(StubLlm),
        tools.clone(),
        memory.clone(),
    ));

    let forge = Arc::new(ForgeEngine::new(
        Arc::new(StaticForgeAgent),
        events.clone(),
        memory.clone(),
        storage.clone(),
        jobs.clone(),
        sessions.clone(),
        config.clone(),
    ));

    let harvest_engine = Arc::new(
        HarvestEngine::new(storage.clone())
            .with_events(events.clone())
            .with_agent(Arc::new(StaticProposalAgent))
            .with_config(harvest_engine::HarvestConfig {
                skills: vec!["rust".into()],
                min_budget: None,
            }),
    );
    let harvest_for_tests = harvest_engine.clone();

    let now = Utc::now();
    let session_a = SessionId::new();
    let session_b = SessionId::new();
    for (id, name) in [(session_a, "a"), (session_b, "b")] {
        let session = Session {
            id,
            name: name.into(),
            kind: SessionKind::General,
            tags: vec![],
            config: SessionConfig::default(),
            created_at: now,
            updated_at: now,
            metadata: json!({}),
        };
        sessions.create(session).await.expect("session");
    }

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
        code_engine: None,
        content_engine: None,
        harvest_engine: Some(harvest_engine),
        gtm_engine: None,
        flow_engine: None,
        sessions,
        events,
        jobs: jobs.clone(),
        database: db.clone(),
        rusvel_base,
        storage,
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
        auth: rusvel_api::auth::AuthConfig::from_env(),
        credentials,
        webhook_receiver,
        cron_scheduler,
        context_pack_cache: Arc::new(rusvel_api::ContextPackCache::default()),
        channel: None,
        boot_time: std::time::Instant::now(),
        failed_departments: Vec::new(),
        data_dir: std::env::temp_dir().join("rusvel-harvest-test"),
        http_listen: "127.0.0.1:3000".into(),
        claude_transport_cli: rusvel_llm::claude_transport_is_cli(),
        operator_prefs: rusvel_api::operator_runtime::OperatorRuntimePrefs::default(),
        shutdown_tx: None,
        reexec_pending: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    };

    (
        build_router(state),
        session_a,
        session_b,
        harvest_for_tests,
        jobs,
    )
}

#[tokio::test]
async fn post_harvest_scan_mock_persists_opportunities() {
    let (mut router, session_a, _, _, _) = test_router().await;
    let (status, body) = json_request(
        &mut router,
        "POST",
        "/api/dept/harvest/scan",
        Some(json!({
            "session_id": session_a.to_string(),
            "sources": ["mock"],
            "query": "rust",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let ops: Vec<Value> = serde_json::from_slice(&body).expect("json");
    assert!(!ops.is_empty(), "expected opportunities from MockSource");

    let (st2, list_body) = json_request(
        &mut router,
        "GET",
        &format!("/api/dept/harvest/list?session_id={session_a}"),
        None,
    )
    .await;
    assert_eq!(st2, StatusCode::OK);
    let listed: Vec<Value> = serde_json::from_slice(&list_body).unwrap();
    assert_eq!(listed.len(), ops.len());
}

#[tokio::test]
async fn post_harvest_ingest_persists_opportunities() {
    let (mut router, session_a, _, _, _) = test_router().await;
    let (status, body) = json_request(
        &mut router,
        "POST",
        "/api/dept/harvest/ingest",
        Some(json!({
            "session_id": session_a.to_string(),
            "jobs": [{
                "title": "Ingested gig",
                "description": "Rust systems programming",
                "url": "https://example.com/j/ingest-1",
                "platform_job_key": "ingest-test-1",
                "source": "manual"
            }]
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let ops: Vec<Value> = serde_json::from_slice(&body).expect("json");
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0]["title"], "Ingested gig");

    let (st2, list_body) = json_request(
        &mut router,
        "GET",
        &format!("/api/dept/harvest/list?session_id={session_a}"),
        None,
    )
    .await;
    assert_eq!(st2, StatusCode::OK);
    let listed: Vec<Value> = serde_json::from_slice(&list_body).unwrap();
    assert!(
        listed.iter().any(|o| o["title"] == "Ingested gig"),
        "list should include ingested opportunity"
    );
}

#[tokio::test]
async fn post_harvest_proposal_persists_proposal() {
    let (mut router, session_a, _, harvest, _) = test_router().await;
    let (st, scan_body) = json_request(
        &mut router,
        "POST",
        "/api/dept/harvest/scan",
        Some(json!({
            "session_id": session_a.to_string(),
            "sources": ["mock"],
            "query": "rust",
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let ops: Vec<Value> = serde_json::from_slice(&scan_body).unwrap();
    let opp_id = ops[0]["id"].as_str().unwrap();

    let (st2, prop_body) = json_request(
        &mut router,
        "POST",
        "/api/dept/harvest/proposal",
        Some(json!({
            "session_id": session_a.to_string(),
            "opportunity_id": opp_id,
            "profile": "Senior Rust engineer",
            "sync": true,
        })),
    )
    .await;
    assert_eq!(st2, StatusCode::OK);
    let proposal: Value = serde_json::from_slice(&prop_body).unwrap();
    assert_eq!(
        proposal["body"].as_str().unwrap(),
        "Tailored proposal for the gig."
    );

    let stored = harvest.get_proposals(&session_a).await.unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].body, "Tailored proposal for the gig.");
}

#[tokio::test]
async fn harvest_session_isolation() {
    let (mut router, session_a, session_b, _, _) = test_router().await;
    let (st, _) = json_request(
        &mut router,
        "POST",
        "/api/dept/harvest/scan",
        Some(json!({
            "session_id": session_a.to_string(),
            "sources": ["mock"],
            "query": "rust",
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);

    let (st2, list_b) = json_request(
        &mut router,
        "GET",
        &format!("/api/dept/harvest/list?session_id={session_b}"),
        None,
    )
    .await;
    assert_eq!(st2, StatusCode::OK);
    let listed: Vec<Value> = serde_json::from_slice(&list_b).unwrap();
    assert!(listed.is_empty());
}

#[tokio::test]
async fn post_harvest_proposal_default_queues_job_without_sync() {
    let (mut router, session_a, _, _, _) = test_router().await;
    let (st, scan_body) = json_request(
        &mut router,
        "POST",
        "/api/dept/harvest/scan",
        Some(json!({
            "session_id": session_a.to_string(),
            "sources": ["mock"],
            "query": "rust",
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let ops: Vec<Value> = serde_json::from_slice(&scan_body).unwrap();
    let opp_id = ops[0]["id"].as_str().unwrap();

    let (st2, body) = json_request(
        &mut router,
        "POST",
        "/api/dept/harvest/proposal",
        Some(json!({
            "session_id": session_a.to_string(),
            "opportunity_id": opp_id,
            "profile": "Senior Rust engineer",
        })),
    )
    .await;
    assert_eq!(st2, StatusCode::OK);
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["status"].as_str().unwrap(), "queued");
    assert!(v["job_id"].as_str().is_some());
}

/// Sprint S-032: mock scan → score → ProposalDraft job → worker hold → pending approval visible via API.
#[tokio::test]
async fn harvest_e2e_proposal_draft_appears_in_approval_queue() {
    let (mut router, session_a, _, harvest, jobs) = test_router().await;

    let (st, scan_body) = json_request(
        &mut router,
        "POST",
        "/api/dept/harvest/scan",
        Some(json!({
            "session_id": session_a.to_string(),
            "sources": ["mock"],
            "query": "rust",
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let ops: Vec<Value> = serde_json::from_slice(&scan_body).unwrap();
    let opp_id = ops[0]["id"].as_str().unwrap();

    let (st_sc, score_body) = json_request(
        &mut router,
        "POST",
        "/api/dept/harvest/score",
        Some(json!({
            "session_id": session_a.to_string(),
            "opportunity_id": opp_id,
        })),
    )
    .await;
    assert_eq!(st_sc, StatusCode::OK);
    let score_json: Value = serde_json::from_slice(&score_body).unwrap();
    assert!(score_json.get("score").is_some());
    assert!(score_json.get("reasoning").is_some());

    let (st_pr, prop_queue_body) = json_request(
        &mut router,
        "POST",
        "/api/dept/harvest/proposal",
        Some(json!({
            "session_id": session_a.to_string(),
            "opportunity_id": opp_id,
            "profile": "Senior Rust engineer",
        })),
    )
    .await;
    assert_eq!(st_pr, StatusCode::OK);
    let queued: Value = serde_json::from_slice(&prop_queue_body).unwrap();
    assert_eq!(queued["status"].as_str().unwrap(), "queued");
    assert!(queued["job_id"].as_str().is_some());

    run_proposal_draft_job(&harvest, jobs.as_ref())
        .await
        .expect("proposal draft worker");

    let (st_ap, body_ap) = json_request(&mut router, "GET", "/api/approvals", None).await;
    assert_eq!(st_ap, StatusCode::OK);
    let pending: Vec<Value> = serde_json::from_slice(&body_ap).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0]["status"].as_str().unwrap(), "AwaitingApproval");
    let kind = &pending[0]["kind"];
    let kind_str = if kind.is_string() {
        kind.as_str().unwrap().to_string()
    } else {
        kind.to_string()
    };
    assert!(
        kind_str.contains("ProposalDraft"),
        "expected ProposalDraft, got {kind_str}"
    );
}
