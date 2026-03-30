//! Engine-specific API routes that call domain engines directly.
//!
//! These routes expose real domain logic (code analysis, content drafting,
//! harvest scoring) — not just CRUD or generic chat.

use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr as _;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use gtm_engine::events as gtm_events;
use gtm_engine::{DealId, DealStage, OutreachSequence, SequenceId, SequenceStep};
use serde::Deserialize;
use serde::Serialize;

use rusvel_core::domain::Contact;
use rusvel_core::domain::{
    CodeAnalysisSummary, ContentItem, ContentKind, Event, ExecutiveBrief, FlowExecution, JobKind,
    NewJob, Opportunity, OpportunityStage, Platform,
};
use rusvel_core::error::RusvelError;
use rusvel_core::id::{ContactId, ContentId, EventId, SessionId};

use gtm_engine::crm::CrmManager;
use gtm_engine::{InvoiceId, InvoiceManager, InvoiceStatus, LineItem};

use forge_engine::PipelineOrchestrationDef;
use harvest_engine::{HarvestScanParams, scan_from_params};

use crate::AppState;

type ApiResult<T> = Result<Json<T>, (StatusCode, String)>;

fn parse_session_id(id: &str) -> Result<SessionId, (StatusCode, String)> {
    id.parse::<uuid::Uuid>()
        .map(SessionId::from_uuid)
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid session id".into()))
}

fn parse_content_id(id: &str) -> Result<ContentId, (StatusCode, String)> {
    id.parse::<uuid::Uuid>()
        .map(ContentId::from_uuid)
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid content id".into()))
}

fn engine_err(e: impl std::fmt::Display) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

fn harvest_scan_err(e: RusvelError) -> (StatusCode, String) {
    match e {
        RusvelError::Validation(msg) => (StatusCode::BAD_REQUEST, msg),
        other => engine_err(other),
    }
}

// ── Executive brief (Forge) ────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct BriefSessionQuery {
    pub session_id: String,
}

/// GET /api/brief?session_id= — generate and return today’s executive brief.
pub async fn brief_get(
    State(state): State<Arc<AppState>>,
    Query(q): Query<BriefSessionQuery>,
) -> ApiResult<ExecutiveBrief> {
    let sid = parse_session_id(&q.session_id)?;
    let brief = state.forge.generate_brief(&sid).await.map_err(engine_err)?;
    Ok(Json(brief))
}

/// GET /api/brief/latest?session_id= — last persisted brief (no LLM); 404 if none.
pub async fn brief_latest_get(
    State(state): State<Arc<AppState>>,
    Query(q): Query<BriefSessionQuery>,
) -> Result<Json<ExecutiveBrief>, (StatusCode, String)> {
    let sid = parse_session_id(&q.session_id)?;
    match state.forge.latest_brief(&sid).await.map_err(engine_err)? {
        Some(b) => Ok(Json(b)),
        None => Err((
            StatusCode::NOT_FOUND,
            "no persisted executive brief for this session".into(),
        )),
    }
}

#[derive(Debug, Deserialize)]
pub struct BriefGenerateBody {
    pub session_id: String,
}

/// POST /api/brief/generate — same as GET; explicit trigger for clients that prefer POST.
pub async fn brief_generate(
    State(state): State<Arc<AppState>>,
    Json(body): Json<BriefGenerateBody>,
) -> ApiResult<ExecutiveBrief> {
    let sid = parse_session_id(&body.session_id)?;
    let brief = state.forge.generate_brief(&sid).await.map_err(engine_err)?;
    Ok(Json(brief))
}

#[derive(Debug, Deserialize)]
pub struct ForgePipelineBody {
    pub session_id: String,
    #[serde(default)]
    pub def: Option<PipelineOrchestrationDef>,
}

/// POST /api/forge/pipeline — cross-engine harvest → content pipeline (S-042).
pub async fn forge_pipeline_orchestrate(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ForgePipelineBody>,
) -> ApiResult<FlowExecution> {
    let sid = parse_session_id(&body.session_id)?;
    let harvest = state.harvest_engine.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Harvest engine not available".into(),
    ))?;
    let content = state.content_engine.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Content engine not available".into(),
    ))?;
    let def = body.def.unwrap_or_default();
    let scan_params = HarvestScanParams {
        sources: def.harvest_sources.clone(),
        query: def.harvest_query.clone(),
        cdp_extract_js: def.harvest_cdp_extract_js.clone(),
        cdp_endpoint: def.harvest_cdp_endpoint.clone(),
    };
    let browser = state
        .cdp
        .clone()
        .map(|c| c as std::sync::Arc<dyn rusvel_core::ports::BrowserPort>);
    let runner = crate::pipeline_runner::HarvestContentPipelineRunner::new(
        harvest.clone(),
        content.clone(),
        browser,
        scan_params,
    );
    let exec = state
        .forge
        .orchestrate_pipeline(sid, def, &runner)
        .await
        .map_err(engine_err)?;
    Ok(Json(exec))
}

#[derive(Debug, Deserialize)]
pub struct ForgeArtifactsQuery {
    pub session_id: String,
    #[serde(default = "default_forge_artifacts_limit")]
    pub limit: u32,
}

fn default_forge_artifacts_limit() -> u32 {
    50
}

/// GET /api/forge/artifacts?session_id=&limit= — list persisted Forge doc artifacts (S-049).
pub async fn forge_artifacts_list(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ForgeArtifactsQuery>,
) -> ApiResult<serde_json::Value> {
    let sid = parse_session_id(&q.session_id)?;
    let items = forge_engine::list_artifacts(&state.storage, &sid, q.limit.min(200))
        .await
        .map_err(engine_err)?;
    Ok(Json(serde_json::to_value(items).map_err(engine_err)?))
}

// ── Code Engine ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct AnalyzeRequest {
    pub path: String,
}

pub async fn code_analyze(
    State(state): State<Arc<AppState>>,
    Json(body): Json<AnalyzeRequest>,
) -> ApiResult<serde_json::Value> {
    let engine = state.code_engine.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Code engine not available".into(),
    ))?;
    let analysis = engine
        .analyze(std::path::Path::new(&body.path))
        .await
        .map_err(engine_err)?;
    Ok(Json(serde_json::to_value(analysis).map_err(engine_err)?))
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: String,
    pub limit: Option<usize>,
}

pub async fn code_search(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SearchQuery>,
) -> ApiResult<serde_json::Value> {
    let engine = state.code_engine.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Code engine not available".into(),
    ))?;
    let results = engine
        .search(&params.q, params.limit.unwrap_or(20))
        .map_err(engine_err)?;
    Ok(Json(serde_json::to_value(results).map_err(engine_err)?))
}

// ── Content Engine ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct DraftRequest {
    pub session_id: String,
    pub topic: String,
    pub kind: Option<String>,
}

pub async fn content_draft(
    State(state): State<Arc<AppState>>,
    Json(body): Json<DraftRequest>,
) -> ApiResult<serde_json::Value> {
    let engine = state.content_engine.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Content engine not available".into(),
    ))?;
    let sid = parse_session_id(&body.session_id)?;
    let kind: rusvel_core::domain::ContentKind = body
        .kind
        .as_deref()
        .and_then(|k| serde_json::from_value(serde_json::json!(k)).ok())
        .unwrap_or(rusvel_core::domain::ContentKind::Blog);
    let item = engine
        .draft(&sid, &body.topic, kind)
        .await
        .map_err(engine_err)?;
    Ok(Json(serde_json::to_value(item).map_err(engine_err)?))
}

#[derive(Debug, Deserialize)]
pub struct FromCodeRequest {
    pub session_id: String,
    pub path: String,
    pub kinds: Vec<String>,
}

/// POST /api/dept/content/from-code — analyze a path, then draft one item per requested kind.
pub async fn content_from_code(
    State(state): State<Arc<AppState>>,
    Json(body): Json<FromCodeRequest>,
) -> ApiResult<Vec<ContentItem>> {
    let code_engine = state.code_engine.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Code engine not available".into(),
    ))?;
    let content_engine = state.content_engine.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Content engine not available".into(),
    ))?;
    let sid = parse_session_id(&body.session_id)?;
    let analysis = code_engine
        .analyze(Path::new(&body.path))
        .await
        .map_err(engine_err)?;
    let summary: CodeAnalysisSummary = (&analysis).into();

    let mut items = Vec::new();
    for k in &body.kinds {
        let kind: ContentKind = serde_json::from_value(serde_json::json!(k)).map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                format!("invalid content kind `{k}`: {e}"),
            )
        })?;
        let topic = content_engine::build_code_prompt(&summary, &kind);
        let item = content_engine
            .draft(&sid, &topic, kind)
            .await
            .map_err(engine_err)?;
        items.push(item);
    }
    Ok(Json(items))
}

/// PATCH /api/dept/content/{id}/approve — set content item approval to Approved (object store).
pub async fn content_approve(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<String>,
) -> ApiResult<ContentItem> {
    let engine = state.content_engine.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Content engine not available".into(),
    ))?;
    let cid = parse_content_id(&id)?;
    let item = engine.approve_content(cid).await.map_err(|e| match e {
        RusvelError::NotFound { .. } => (StatusCode::NOT_FOUND, e.to_string()),
        _ => engine_err(e),
    })?;
    Ok(Json(item))
}

#[derive(Debug, Deserialize)]
pub struct PublishRequest {
    pub session_id: String,
    pub content_id: String,
    pub platform: String,
}

pub async fn content_publish(
    State(state): State<Arc<AppState>>,
    Json(body): Json<PublishRequest>,
) -> ApiResult<serde_json::Value> {
    let engine = state.content_engine.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Content engine not available".into(),
    ))?;
    let sid = parse_session_id(&body.session_id)?;
    let cid = parse_content_id(&body.content_id)?;
    let platform: rusvel_core::domain::Platform =
        serde_json::from_value(serde_json::json!(body.platform))
            .map_err(|e| (StatusCode::BAD_REQUEST, format!("invalid platform: {e}")))?;
    let result = engine
        .publish(&sid, cid, platform)
        .await
        .map_err(engine_err)?;
    Ok(Json(serde_json::to_value(result).map_err(engine_err)?))
}

#[derive(Debug, Deserialize)]
pub struct ContentListQuery {
    pub session_id: String,
    pub status: Option<String>,
}

pub async fn content_list(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ContentListQuery>,
) -> ApiResult<serde_json::Value> {
    let engine = state.content_engine.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Content engine not available".into(),
    ))?;
    let sid = parse_session_id(&params.session_id)?;
    let status_filter: Option<rusvel_core::domain::ContentStatus> = params
        .status
        .as_deref()
        .and_then(|s| serde_json::from_value(serde_json::json!(s)).ok());
    let items = engine
        .list_content(&sid, status_filter)
        .await
        .map_err(engine_err)?;
    Ok(Json(serde_json::to_value(items).map_err(engine_err)?))
}

#[derive(Debug, Deserialize)]
pub struct ContentScheduleRequest {
    pub session_id: String,
    pub content_id: String,
    pub platform: String,
    /// RFC3339 UTC (or offset) datetime when the item should publish.
    pub publish_at: String,
}

/// POST /api/dept/content/schedule — persist schedule on the draft and enqueue publish job.
pub async fn content_schedule(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ContentScheduleRequest>,
) -> ApiResult<serde_json::Value> {
    let engine = state.content_engine.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Content engine not available".into(),
    ))?;
    let sid = parse_session_id(&body.session_id)?;
    let cid = parse_content_id(&body.content_id)?;
    let platform: Platform = serde_json::from_value(serde_json::json!(body.platform))
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("invalid platform: {e}")))?;
    let at = DateTime::parse_from_rfc3339(body.publish_at.trim())
        .map(|d| d.with_timezone(&Utc))
        .map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                "invalid publish_at (use RFC3339)".into(),
            )
        })?;
    engine
        .schedule(&sid, cid, platform, at)
        .await
        .map_err(engine_err)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

// ── Harvest Engine ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ScoreRequest {
    pub session_id: String,
    pub opportunity_id: String,
}

#[derive(Debug, Deserialize)]
pub struct HarvestScanRequest {
    pub session_id: String,
    pub sources: Vec<String>,
    pub query: String,
    /// Optional CDP extract script (must evaluate to a JSON array string). When omitted, the engine default is used.
    #[serde(default)]
    pub cdp_extract_js: Option<String>,
    /// Optional CDP HTTP base per Chrome profile (overrides `RUSVEL_CDP_ENDPOINT`).
    #[serde(default)]
    pub cdp_endpoint: Option<String>,
}

/// POST /api/dept/harvest/scan — run configured sources, score, persist opportunities.
pub async fn harvest_scan(
    State(state): State<Arc<AppState>>,
    Json(body): Json<HarvestScanRequest>,
) -> ApiResult<Vec<Opportunity>> {
    let engine = state.harvest_engine.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Harvest engine not available".into(),
    ))?;
    let sid = parse_session_id(&body.session_id)?;
    let params = HarvestScanParams {
        sources: body.sources.clone(),
        query: body.query.clone(),
        cdp_extract_js: body.cdp_extract_js.clone(),
        cdp_endpoint: body.cdp_endpoint.clone(),
    };
    let browser = state
        .cdp
        .clone()
        .map(|c| c as std::sync::Arc<dyn rusvel_core::ports::BrowserPort>);
    let all = scan_from_params(engine, &sid, &params, browser)
        .await
        .map_err(harvest_scan_err)?;
    crate::hook_dispatch::dispatch_hooks(
        harvest_engine::events::SCAN_COMPLETED,
        serde_json::json!({
            "session_id": body.session_id,
            "count": all.len(),
            "via": "api.scan",
        }),
        "harvest",
        state.storage.clone(),
    );
    Ok(Json(all))
}

#[derive(Debug, Deserialize)]
pub struct HarvestIngestRequest {
    pub session_id: String,
    /// Default when a job row omits `"source"` (e.g. `upwork`, `freelancer`, `manual`).
    #[serde(default = "default_ingest_platform")]
    pub platform: String,
    /// One or more job objects (`title`, `description`, `url`, optional `source` per row, enrich fields).
    pub jobs: Vec<serde_json::Value>,
}

fn default_ingest_platform() -> String {
    "manual".into()
}

/// POST /api/dept/harvest/ingest — push captured jobs (extension, webhooks, fleet); same scoring + dedup as scan.
pub async fn harvest_ingest(
    State(state): State<Arc<AppState>>,
    Json(body): Json<HarvestIngestRequest>,
) -> ApiResult<Vec<Opportunity>> {
    let engine = state.harvest_engine.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Harvest engine not available".into(),
    ))?;
    if body.jobs.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "jobs must be a non-empty array".into(),
        ));
    }
    let sid = parse_session_id(&body.session_id)?;
    let platform = body
        .platform
        .trim()
        .is_empty()
        .then_some("manual")
        .unwrap_or(body.platform.trim());
    let opps = engine
        .ingest_jobs(&sid, &body.jobs, platform, "http")
        .await
        .map_err(engine_err)?;
    crate::hook_dispatch::dispatch_hooks(
        harvest_engine::events::BATCH_INGEST_COMPLETED,
        serde_json::json!({
            "session_id": body.session_id,
            "count": opps.len(),
            "platform": platform,
        }),
        "harvest",
        state.storage.clone(),
    );
    Ok(Json(opps))
}

pub async fn harvest_score(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ScoreRequest>,
) -> ApiResult<serde_json::Value> {
    let engine = state.harvest_engine.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Harvest engine not available".into(),
    ))?;
    let sid = parse_session_id(&body.session_id)?;
    let update = engine
        .score_opportunity(&sid, &body.opportunity_id)
        .await
        .map_err(engine_err)?;
    Ok(Json(serde_json::json!({
        "score": update.score,
        "reasoning": update.reasoning
    })))
}

#[derive(Debug, Deserialize)]
pub struct HarvestAdvanceRequest {
    pub session_id: String,
    pub opportunity_id: String,
    pub stage: String,
}

/// POST /api/dept/harvest/advance — move opportunity to a pipeline stage.
pub async fn harvest_advance(
    State(state): State<Arc<AppState>>,
    Json(body): Json<HarvestAdvanceRequest>,
) -> ApiResult<serde_json::Value> {
    let engine = state.harvest_engine.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Harvest engine not available".into(),
    ))?;
    let sid = parse_session_id(&body.session_id)?;
    let stage: OpportunityStage = serde_json::from_value(serde_json::json!(&body.stage))
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid opportunity stage".into()))?;
    engine
        .advance_opportunity(&body.opportunity_id, stage.clone())
        .await
        .map_err(engine_err)?;
    if matches!(stage, OpportunityStage::Won | OpportunityStage::Lost) {
        let result = match stage {
            OpportunityStage::Won => harvest_engine::HarvestDealOutcome::Won,
            OpportunityStage::Lost => harvest_engine::HarvestDealOutcome::Lost,
            _ => unreachable!(),
        };
        let _ = engine
            .record_opportunity_outcome(&sid, &body.opportunity_id, result, String::new())
            .await;
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(Debug, Deserialize)]
pub struct ProposalRequest {
    pub session_id: String,
    pub opportunity_id: String,
    pub profile: String,
    /// When `true`, run [`HarvestEngine::generate_proposal`] inline and return proposal JSON.
    /// When `false` or omitted (default), enqueue [`JobKind::ProposalDraft`] for the app worker
    /// and human approval gate (`hold_for_approval`).
    #[serde(default)]
    pub sync: bool,
}

pub async fn harvest_proposal(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ProposalRequest>,
) -> ApiResult<serde_json::Value> {
    let engine = state.harvest_engine.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Harvest engine not available".into(),
    ))?;
    let sid = parse_session_id(&body.session_id)?;
    if body.sync {
        let proposal = engine
            .generate_proposal(&sid, &body.opportunity_id, &body.profile)
            .await
            .map_err(engine_err)?;
        return Ok(Json(serde_json::to_value(proposal).map_err(engine_err)?));
    }

    let job_id = state
        .jobs
        .enqueue(NewJob {
            session_id: sid,
            kind: JobKind::ProposalDraft,
            payload: serde_json::json!({
                "opportunity_id": body.opportunity_id,
                "profile": body.profile,
            }),
            max_retries: 3,
            metadata: serde_json::json!({}),
            scheduled_at: None,
        })
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(serde_json::json!({
        "job_id": job_id.to_string(),
        "status": "queued",
        "message": "ProposalDraft job queued; the worker generates the proposal and parks it in /api/approvals when ready."
    })))
}

#[derive(Debug, Deserialize)]
pub struct PipelineQuery {
    pub session_id: String,
}

pub async fn harvest_pipeline(
    State(state): State<Arc<AppState>>,
    Query(params): Query<PipelineQuery>,
) -> ApiResult<serde_json::Value> {
    let engine = state.harvest_engine.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Harvest engine not available".into(),
    ))?;
    let sid = parse_session_id(&params.session_id)?;
    let stats = engine.pipeline(&sid).await.map_err(engine_err)?;
    Ok(Json(serde_json::to_value(stats).map_err(engine_err)?))
}

#[derive(Debug, Deserialize)]
pub struct OpportunityListQuery {
    pub session_id: String,
    pub stage: Option<String>,
}

pub async fn harvest_list(
    State(state): State<Arc<AppState>>,
    Query(params): Query<OpportunityListQuery>,
) -> ApiResult<serde_json::Value> {
    let engine = state.harvest_engine.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Harvest engine not available".into(),
    ))?;
    let sid = parse_session_id(&params.session_id)?;
    let stage: Option<rusvel_core::domain::OpportunityStage> = params
        .stage
        .as_deref()
        .and_then(|s| serde_json::from_value(serde_json::json!(s)).ok());
    let items = engine
        .list_opportunities(&sid, stage.as_ref())
        .await
        .map_err(engine_err)?;
    Ok(Json(serde_json::to_value(items).map_err(engine_err)?))
}

#[derive(Debug, Deserialize)]
pub struct HarvestOutcomeBody {
    pub session_id: String,
    pub opportunity_id: String,
    /// `won` | `lost` | `withdrawn`
    pub result: String,
    #[serde(default)]
    pub notes: String,
}

/// POST /api/dept/harvest/outcome — record won/lost for learning (S-044).
pub async fn harvest_record_outcome(
    State(state): State<Arc<AppState>>,
    Json(body): Json<HarvestOutcomeBody>,
) -> ApiResult<serde_json::Value> {
    let engine = state.harvest_engine.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Harvest engine not available".into(),
    ))?;
    let sid = parse_session_id(&body.session_id)?;
    let result = match body.result.to_lowercase().as_str() {
        "won" => harvest_engine::HarvestDealOutcome::Won,
        "lost" => harvest_engine::HarvestDealOutcome::Lost,
        "withdrawn" => harvest_engine::HarvestDealOutcome::Withdrawn,
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                "result must be won, lost, or withdrawn".into(),
            ));
        }
    };
    match engine
        .record_opportunity_outcome(&sid, &body.opportunity_id, result, body.notes)
        .await
    {
        Ok(record) => Ok(Json(serde_json::to_value(record).map_err(engine_err)?)),
        Err(RusvelError::NotFound { .. }) => {
            Err((StatusCode::NOT_FOUND, "opportunity not found".into()))
        }
        Err(e) => Err(engine_err(e)),
    }
}

#[derive(Debug, Deserialize)]
pub struct HarvestOutcomesQuery {
    pub session_id: String,
    #[serde(default = "default_harvest_outcomes_limit")]
    pub limit: u32,
}

fn default_harvest_outcomes_limit() -> u32 {
    50
}

/// GET /api/dept/harvest/outcomes?session_id=&limit=
pub async fn harvest_outcomes_list(
    State(state): State<Arc<AppState>>,
    Query(q): Query<HarvestOutcomesQuery>,
) -> ApiResult<serde_json::Value> {
    let engine = state.harvest_engine.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Harvest engine not available".into(),
    ))?;
    let sid = parse_session_id(&q.session_id)?;
    let items = engine
        .list_harvest_outcomes(&sid, q.limit.min(200))
        .await
        .map_err(engine_err)?;
    Ok(Json(serde_json::to_value(items).map_err(engine_err)?))
}

// ── GTM CRM (S-036) — contacts + deals via [`CrmManager`] on shared storage ──

#[derive(Debug, Deserialize)]
pub struct GtmSessionQuery {
    pub session_id: String,
}

/// GET /api/dept/gtm/contacts — list CRM contacts for the session.
pub async fn gtm_contacts_list(
    State(state): State<Arc<AppState>>,
    Query(q): Query<GtmSessionQuery>,
) -> ApiResult<Vec<Contact>> {
    let sid = parse_session_id(&q.session_id)?;
    let crm = CrmManager::new(state.storage.clone());
    let contacts = crm.list_contacts(sid).await.map_err(engine_err)?;
    Ok(Json(contacts))
}

#[derive(Debug, Deserialize)]
pub struct GtmContactCreateBody {
    pub session_id: String,
    pub name: String,
    pub email: String,
    #[serde(default)]
    pub company: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub links: Vec<String>,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

/// POST /api/dept/gtm/contacts — create a contact (`email` becomes the primary entry in `emails`).
pub async fn gtm_contacts_create(
    State(state): State<Arc<AppState>>,
    Json(body): Json<GtmContactCreateBody>,
) -> ApiResult<serde_json::Value> {
    let sid = parse_session_id(&body.session_id)?;
    let crm = CrmManager::new(state.storage.clone());
    let contact = Contact {
        id: ContactId::new(),
        session_id: sid,
        name: body.name,
        emails: vec![body.email],
        links: body.links,
        company: body.company,
        role: body.role,
        tags: body.tags,
        last_contacted_at: None,
        metadata: body.metadata.unwrap_or_else(|| serde_json::json!({})),
    };
    let id = crm.add_contact(sid, contact).await.map_err(engine_err)?;

    let ev = Event {
        id: EventId::new(),
        session_id: Some(sid),
        run_id: None,
        source: "gtm".into(),
        kind: gtm_events::CONTACT_ADDED.into(),
        payload: serde_json::json!({ "contact_id": id.to_string() }),
        created_at: Utc::now(),
        metadata: serde_json::json!({}),
    };
    let _ = state.events.emit(ev).await;

    Ok(Json(serde_json::json!({ "id": id.to_string() })))
}

#[derive(Debug, Deserialize)]
pub struct GtmDealsQuery {
    pub session_id: String,
    pub stage: Option<String>,
}

#[derive(Serialize)]
pub struct GtmDealRow {
    pub id: String,
    pub contact_id: String,
    pub title: String,
    pub value: f64,
    pub stage: DealStage,
    pub notes: String,
    pub created_at: DateTime<Utc>,
    pub contact_name: Option<String>,
    pub last_activity: DateTime<Utc>,
}

/// GET /api/dept/gtm/deals — list deals with contact names for the Kanban UI (S-037).
pub async fn gtm_deals_list(
    State(state): State<Arc<AppState>>,
    Query(q): Query<GtmDealsQuery>,
) -> ApiResult<Vec<GtmDealRow>> {
    let sid = parse_session_id(&q.session_id)?;
    let crm = CrmManager::new(state.storage.clone());
    let stage_filter = q
        .stage
        .as_deref()
        .and_then(|s| serde_json::from_value::<DealStage>(serde_json::json!(s)).ok());
    let deals = crm
        .list_deals(sid, stage_filter)
        .await
        .map_err(engine_err)?;
    let contacts = crm.list_contacts(sid).await.map_err(engine_err)?;
    let contact_map: HashMap<String, rusvel_core::domain::Contact> = contacts
        .into_iter()
        .map(|c| (c.id.to_string(), c))
        .collect();

    let mut rows = Vec::with_capacity(deals.len());
    for d in deals {
        let contact_name = contact_map
            .get(&d.contact_id.to_string())
            .map(|c| c.name.clone());
        let last_activity = contact_map
            .get(&d.contact_id.to_string())
            .and_then(|c| c.last_contacted_at)
            .map(|lc| lc.max(d.created_at))
            .unwrap_or(d.created_at);
        rows.push(GtmDealRow {
            id: d.id.to_string(),
            contact_id: d.contact_id.to_string(),
            title: d.title,
            value: d.value,
            stage: d.stage,
            notes: d.notes,
            created_at: d.created_at,
            contact_name,
            last_activity,
        });
    }
    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
pub struct GtmDealAdvanceBody {
    pub session_id: String,
    pub deal_id: String,
    pub stage: String,
    /// When set and stage is Won/Lost, records a Harvest outcome for this opportunity id (S-044).
    #[serde(default)]
    pub opportunity_id: Option<String>,
}

/// POST /api/dept/gtm/deals/advance — move a deal to a new stage (drag-and-drop from the UI).
pub async fn gtm_deal_advance(
    State(state): State<Arc<AppState>>,
    Json(body): Json<GtmDealAdvanceBody>,
) -> ApiResult<serde_json::Value> {
    let sid = parse_session_id(&body.session_id)?;
    let deal_id = DealId::from_str(body.deal_id.trim()).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            "invalid deal_id (expected UUID)".into(),
        )
    })?;
    let new_stage: DealStage =
        serde_json::from_value(serde_json::json!(&body.stage)).map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                "invalid stage (use Lead, Qualified, Proposal, Negotiation, Won, Lost)".into(),
            )
        })?;

    let crm = CrmManager::new(state.storage.clone());
    let deals = crm.list_deals(sid, None).await.map_err(engine_err)?;
    let deal = deals
        .iter()
        .find(|d| d.id == deal_id)
        .cloned()
        .ok_or((StatusCode::NOT_FOUND, "deal not found for session".into()))?;

    let outcome_stage = new_stage.clone();
    crm.advance_deal(&deal_id, new_stage)
        .await
        .map_err(engine_err)?;

    if matches!(outcome_stage, DealStage::Won | DealStage::Lost) {
        let oid = body.opportunity_id.clone().or_else(|| {
            deal.metadata
                .get("harvest_opportunity_id")
                .and_then(|v| v.as_str())
                .map(String::from)
        });
        if let (Some(opp), Some(he)) = (oid, state.harvest_engine.as_ref()) {
            let result = match outcome_stage {
                DealStage::Won => harvest_engine::HarvestDealOutcome::Won,
                DealStage::Lost => harvest_engine::HarvestDealOutcome::Lost,
                _ => unreachable!(),
            };
            let _ = he
                .record_opportunity_outcome(&sid, &opp, result, format!("gtm deal {}", deal_id))
                .await;
        }
    }

    let ev = Event {
        id: EventId::new(),
        session_id: Some(sid),
        run_id: None,
        source: "gtm".into(),
        kind: gtm_events::DEAL_UPDATED.into(),
        payload: serde_json::json!({
            "deal_id": deal_id.to_string(),
            "stage": body.stage,
        }),
        created_at: Utc::now(),
        metadata: serde_json::json!({}),
    };
    let _ = state.events.emit(ev).await;

    Ok(Json(serde_json::json!({ "ok": true })))
}

// ── GTM Invoices (S-038) ────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct GtmInvoicesQuery {
    pub session_id: String,
    pub status: Option<String>,
}

#[derive(Serialize)]
pub struct GtmInvoiceRow {
    pub id: String,
    pub contact_id: String,
    pub contact_name: Option<String>,
    pub items: Vec<LineItem>,
    pub total: f64,
    pub status: InvoiceStatus,
    pub due_date: DateTime<Utc>,
    pub paid_at: Option<DateTime<Utc>>,
    pub metadata: serde_json::Value,
}

/// GET /api/dept/gtm/invoices — list invoices for the session (optional `status` filter).
pub async fn gtm_invoices_list(
    State(state): State<Arc<AppState>>,
    Query(q): Query<GtmInvoicesQuery>,
) -> ApiResult<Vec<GtmInvoiceRow>> {
    let sid = parse_session_id(&q.session_id)?;
    let invm = InvoiceManager::new(state.storage.clone());
    let crm = CrmManager::new(state.storage.clone());
    let status_filter = q
        .status
        .as_deref()
        .and_then(|s| serde_json::from_value::<InvoiceStatus>(serde_json::json!(s)).ok());
    let invoices = invm
        .list_invoices(sid, status_filter)
        .await
        .map_err(engine_err)?;
    let contacts = crm.list_contacts(sid).await.map_err(engine_err)?;
    let contact_map: HashMap<String, rusvel_core::domain::Contact> = contacts
        .into_iter()
        .map(|c| (c.id.to_string(), c))
        .collect();

    let mut rows = Vec::with_capacity(invoices.len());
    for inv in invoices {
        let contact_name = contact_map
            .get(&inv.contact_id.to_string())
            .map(|c| c.name.clone());
        rows.push(GtmInvoiceRow {
            id: inv.id.to_string(),
            contact_id: inv.contact_id.to_string(),
            contact_name,
            items: inv.items,
            total: inv.total,
            status: inv.status,
            due_date: inv.due_date,
            paid_at: inv.paid_at,
            metadata: inv.metadata,
        });
    }
    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
pub struct GtmInvoiceCreateBody {
    pub session_id: String,
    pub contact_id: String,
    pub items: Vec<LineItem>,
    pub due_date: String,
}

/// POST /api/dept/gtm/invoices — create invoice (starts as Draft).
pub async fn gtm_invoices_create(
    State(state): State<Arc<AppState>>,
    Json(body): Json<GtmInvoiceCreateBody>,
) -> ApiResult<serde_json::Value> {
    let sid = parse_session_id(&body.session_id)?;
    let cid = uuid::Uuid::parse_str(body.contact_id.trim())
        .map(ContactId::from_uuid)
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid contact_id (UUID)".into()))?;
    let due = DateTime::parse_from_rfc3339(&body.due_date)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid due_date (RFC3339)".into()))?;
    if body.items.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "at least one line item is required".into(),
        ));
    }
    let invm = InvoiceManager::new(state.storage.clone());
    let id = invm
        .create_invoice(sid, cid, body.items, due)
        .await
        .map_err(engine_err)?;

    let ev = Event {
        id: EventId::new(),
        session_id: Some(sid),
        run_id: None,
        source: "gtm".into(),
        kind: gtm_events::INVOICE_CREATED.into(),
        payload: serde_json::json!({ "invoice_id": id.to_string() }),
        created_at: Utc::now(),
        metadata: serde_json::json!({}),
    };
    let _ = state.events.emit(ev).await;

    Ok(Json(serde_json::json!({ "id": id.to_string() })))
}

#[derive(Serialize)]
pub struct GtmInvoiceDetail {
    pub id: String,
    pub session_id: String,
    pub contact_id: String,
    pub contact_name: Option<String>,
    pub items: Vec<LineItem>,
    pub total: f64,
    pub status: InvoiceStatus,
    pub due_date: DateTime<Utc>,
    pub paid_at: Option<DateTime<Utc>>,
    pub metadata: serde_json::Value,
}

/// GET /api/dept/gtm/invoices/{id} — single invoice detail (`session_id` query).
pub async fn gtm_invoice_get(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<String>,
    Query(q): Query<GtmSessionQuery>,
) -> ApiResult<GtmInvoiceDetail> {
    let sid = parse_session_id(&q.session_id)?;
    let iid = InvoiceId::from_str(id.trim())
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid invoice id (UUID)".into()))?;
    let invm = InvoiceManager::new(state.storage.clone());
    let inv = invm.get_invoice(&iid).await.map_err(engine_err)?;
    if inv.session_id != sid {
        return Err((
            StatusCode::NOT_FOUND,
            "invoice not found for session".into(),
        ));
    }
    let crm = CrmManager::new(state.storage.clone());
    let contacts = crm.list_contacts(sid).await.map_err(engine_err)?;
    let contact_name = contacts
        .into_iter()
        .find(|c| c.id == inv.contact_id)
        .map(|c| c.name);
    Ok(Json(GtmInvoiceDetail {
        id: inv.id.to_string(),
        session_id: inv.session_id.to_string(),
        contact_id: inv.contact_id.to_string(),
        contact_name,
        items: inv.items,
        total: inv.total,
        status: inv.status,
        due_date: inv.due_date,
        paid_at: inv.paid_at,
        metadata: inv.metadata,
    }))
}

#[derive(Debug, Deserialize)]
pub struct GtmInvoiceStatusBody {
    pub session_id: String,
    pub status: InvoiceStatus,
}

/// POST /api/dept/gtm/invoices/{id}/status — update lifecycle status (Draft, Sent, Paid, Overdue, Cancelled).
pub async fn gtm_invoice_set_status(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<String>,
    Json(body): Json<GtmInvoiceStatusBody>,
) -> ApiResult<serde_json::Value> {
    let sid = parse_session_id(&body.session_id)?;
    let iid = InvoiceId::from_str(id.trim())
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid invoice id (UUID)".into()))?;
    let invm = InvoiceManager::new(state.storage.clone());
    let inv = invm.get_invoice(&iid).await.map_err(engine_err)?;
    if inv.session_id != sid {
        return Err((
            StatusCode::NOT_FOUND,
            "invoice not found for session".into(),
        ));
    }
    invm.set_invoice_status(&iid, body.status.clone())
        .await
        .map_err(engine_err)?;
    if body.status == InvoiceStatus::Paid {
        let ev = Event {
            id: EventId::new(),
            session_id: Some(sid),
            run_id: None,
            source: "gtm".into(),
            kind: gtm_events::INVOICE_PAID.into(),
            payload: serde_json::json!({ "invoice_id": iid.to_string() }),
            created_at: Utc::now(),
            metadata: serde_json::json!({}),
        };
        let _ = state.events.emit(ev).await;
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(Debug, Deserialize)]
pub struct GtmOutreachExecuteBody {
    pub session_id: String,
    pub sequence_id: String,
    pub contact_id: String,
}

/// POST /api/dept/gtm/outreach/execute — enqueue staggered outreach send jobs (S-033).
pub async fn gtm_outreach_execute(
    State(state): State<Arc<AppState>>,
    Json(body): Json<GtmOutreachExecuteBody>,
) -> ApiResult<serde_json::Value> {
    let engine = state.gtm_engine.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "GTM engine not available".into(),
    ))?;
    let sid = parse_session_id(&body.session_id)?;
    let seq_id: SequenceId = body
        .sequence_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid sequence_id (UUID)".into()))?;
    let cid = uuid::Uuid::parse_str(body.contact_id.trim())
        .map(ContactId::from_uuid)
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid contact_id (UUID)".into()))?;
    let job_id = engine
        .outreach()
        .execute_sequence(sid, seq_id, cid)
        .await
        .map_err(engine_err)?;
    Ok(Json(serde_json::json!({
        "job_id": job_id.to_string(),
        "job_ids": [job_id.to_string()],
        "count": 1,
    })))
}

#[derive(Debug, Deserialize)]
pub struct GtmOutreachSequencesQuery {
    pub session_id: String,
}

/// GET /api/dept/gtm/outreach/sequences?session_id= — list outreach sequences for the session (S-033).
pub async fn gtm_outreach_sequences_list(
    State(state): State<Arc<AppState>>,
    Query(q): Query<GtmOutreachSequencesQuery>,
) -> ApiResult<Vec<OutreachSequence>> {
    let engine = state.gtm_engine.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "GTM engine not available".into(),
    ))?;
    let sid = parse_session_id(&q.session_id)?;
    let rows = engine
        .outreach()
        .list_sequences(sid)
        .await
        .map_err(engine_err)?;
    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
pub struct GtmOutreachSequenceCreateBody {
    pub session_id: String,
    pub name: String,
    pub steps: Vec<SequenceStep>,
}

/// POST /api/dept/gtm/outreach/sequences — create a draft sequence (S-033).
pub async fn gtm_outreach_sequences_create(
    State(state): State<Arc<AppState>>,
    Json(body): Json<GtmOutreachSequenceCreateBody>,
) -> ApiResult<serde_json::Value> {
    let engine = state.gtm_engine.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "GTM engine not available".into(),
    ))?;
    let sid = parse_session_id(&body.session_id)?;
    if body.name.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "name is required".into()));
    }
    let id = engine
        .outreach()
        .create_sequence(sid, body.name.trim().to_string(), body.steps)
        .await
        .map_err(engine_err)?;
    Ok(Json(serde_json::json!({ "id": id.to_string() })))
}

#[derive(Debug, Deserialize)]
pub struct GtmOutreachSequenceActivateBody {
    pub session_id: String,
}

/// POST /api/dept/gtm/outreach/sequences/{id}/activate — set sequence status to Active (S-033).
pub async fn gtm_outreach_sequences_activate(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<String>,
    Json(body): Json<GtmOutreachSequenceActivateBody>,
) -> ApiResult<serde_json::Value> {
    let engine = state.gtm_engine.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "GTM engine not available".into(),
    ))?;
    let sid = parse_session_id(&body.session_id)?;
    let seq_id: SequenceId = id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid sequence id (UUID)".into()))?;
    let seq = engine
        .outreach()
        .get_sequence(&seq_id)
        .await
        .map_err(engine_err)?;
    if seq.session_id != sid {
        return Err((
            StatusCode::NOT_FOUND,
            "sequence not found for session".into(),
        ));
    }
    engine
        .outreach()
        .activate_sequence(&seq_id)
        .await
        .map_err(engine_err)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(Debug, Deserialize)]
pub struct ContentScheduledQuery {
    pub session_id: String,
    pub from: Option<String>,
    pub to: Option<String>,
}

/// GET /api/dept/content/scheduled — list scheduled posts (optional RFC3339 `from` / `to` window).
pub async fn content_scheduled(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ContentScheduledQuery>,
) -> ApiResult<serde_json::Value> {
    let engine = state.content_engine.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Content engine not available".into(),
    ))?;
    let sid = parse_session_id(&params.session_id)?;
    let items = match (&params.from, &params.to) {
        (Some(fs), Some(ts)) => {
            let from = DateTime::parse_from_rfc3339(fs)
                .map(|d| d.with_timezone(&Utc))
                .map_err(|_| {
                    (
                        StatusCode::BAD_REQUEST,
                        "invalid `from` (use RFC3339)".into(),
                    )
                })?;
            let to = DateTime::parse_from_rfc3339(ts)
                .map(|d| d.with_timezone(&Utc))
                .map_err(|_| (StatusCode::BAD_REQUEST, "invalid `to` (use RFC3339)".into()))?;
            engine
                .list_scheduled_in_range(&sid, from, to)
                .await
                .map_err(engine_err)?
        }
        _ => engine.list_scheduled(&sid).await.map_err(engine_err)?,
    };
    Ok(Json(serde_json::to_value(items).map_err(engine_err)?))
}
