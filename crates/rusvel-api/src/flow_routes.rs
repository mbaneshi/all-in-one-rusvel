//! Flow API routes — CRUD for flow definitions + execution.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Deserialize;
use uuid::Uuid;

use flow_engine::cross_engine_handoff_template;
use rusvel_core::domain::FlowDef;
use rusvel_core::id::{FlowExecutionId, FlowId};
use rusvel_core::terminal::Pane;

use crate::AppState;

type ApiResult<T> = Result<Json<T>, (StatusCode, String)>;

fn engine_err(e: impl std::fmt::Display) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

fn flow_engine(state: &Arc<AppState>) -> Result<&flow_engine::FlowEngine, (StatusCode, String)> {
    state.flow_engine.as_ref().map(|e| e.as_ref()).ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Flow engine not available".into(),
    ))
}

/// GET /api/flows/templates/cross-engine-handoff — S-042 demo DAG (save with POST /api/flows).
pub async fn get_cross_engine_handoff_template() -> ApiResult<FlowDef> {
    Ok(Json(cross_engine_handoff_template()))
}

// ── CRUD ─────────────────────────────────────────────────────────

pub async fn create_flow(
    State(state): State<Arc<AppState>>,
    Json(mut flow): Json<FlowDef>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, String)> {
    let engine = flow_engine(&state)?;
    if *flow.id.as_uuid() == Uuid::nil() {
        flow.id = FlowId::new();
    }
    let id = engine.save_flow(&flow).await.map_err(engine_err)?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "id": id.to_string() })),
    ))
}

pub async fn list_flows(State(state): State<Arc<AppState>>) -> ApiResult<Vec<FlowDef>> {
    let engine = flow_engine(&state)?;
    let flows = engine.list_flows().await.map_err(engine_err)?;
    Ok(Json(flows))
}

pub async fn get_flow(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let engine = flow_engine(&state)?;
    let flow_id = id
        .parse::<uuid::Uuid>()
        .map(FlowId::from_uuid)
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid flow id".into()))?;
    let flow = engine
        .get_flow(&flow_id)
        .await
        .map_err(engine_err)?
        .ok_or((StatusCode::NOT_FOUND, "flow not found".into()))?;
    Ok(Json(serde_json::to_value(flow).map_err(engine_err)?))
}

pub async fn update_flow(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(mut flow): Json<FlowDef>,
) -> ApiResult<serde_json::Value> {
    let engine = flow_engine(&state)?;
    let flow_id = id
        .parse::<uuid::Uuid>()
        .map(FlowId::from_uuid)
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid flow id".into()))?;
    flow.id = flow_id;
    engine.save_flow(&flow).await.map_err(engine_err)?;
    Ok(Json(serde_json::json!({ "id": flow_id.to_string() })))
}

pub async fn delete_flow(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let engine = flow_engine(&state)?;
    let flow_id = id
        .parse::<uuid::Uuid>()
        .map(FlowId::from_uuid)
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid flow id".into()))?;
    engine.delete_flow(&flow_id).await.map_err(engine_err)?;
    Ok(StatusCode::NO_CONTENT)
}

// ── Execution ────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct RunFlowRequest {
    #[serde(default)]
    pub trigger_data: serde_json::Value,
}

pub async fn run_flow(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<RunFlowRequest>,
) -> ApiResult<serde_json::Value> {
    let engine = flow_engine(&state)?;
    let flow_id = id
        .parse::<uuid::Uuid>()
        .map(FlowId::from_uuid)
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid flow id".into()))?;
    let execution = engine
        .run_flow(&flow_id, body.trigger_data)
        .await
        .map_err(engine_err)?;
    Ok(Json(serde_json::to_value(execution).map_err(engine_err)?))
}

pub async fn list_executions(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let engine = flow_engine(&state)?;
    let flow_id = id
        .parse::<uuid::Uuid>()
        .map(FlowId::from_uuid)
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid flow id".into()))?;
    let execs = engine.list_executions(&flow_id).await.map_err(engine_err)?;
    Ok(Json(serde_json::to_value(execs).map_err(engine_err)?))
}

pub async fn get_execution(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let engine = flow_engine(&state)?;
    let exec_id = id
        .parse::<uuid::Uuid>()
        .map(rusvel_core::id::FlowExecutionId::from_uuid)
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid execution id".into()))?;
    let exec = engine
        .get_execution(&exec_id)
        .await
        .map_err(engine_err)?
        .ok_or((StatusCode::NOT_FOUND, "execution not found".into()))?;
    Ok(Json(serde_json::to_value(exec).map_err(engine_err)?))
}

pub async fn list_flow_execution_panes(
    State(state): State<Arc<AppState>>,
    Path((flow_id, exec_id)): Path<(String, String)>,
) -> ApiResult<Vec<Pane>> {
    let terminal = state.terminal.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Terminal not configured".into(),
    ))?;
    let engine = flow_engine(&state)?;
    let flow_uuid = flow_id
        .parse::<uuid::Uuid>()
        .map(FlowId::from_uuid)
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid flow id".into()))?;
    let exec_uuid = exec_id
        .parse::<uuid::Uuid>()
        .map(FlowExecutionId::from_uuid)
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid execution id".into()))?;
    let exec = engine
        .get_execution(&exec_uuid)
        .await
        .map_err(engine_err)?
        .ok_or((StatusCode::NOT_FOUND, "execution not found".into()))?;
    if exec.flow_id != flow_uuid {
        return Err((StatusCode::NOT_FOUND, "execution not found".into()));
    }
    let panes = terminal
        .panes_for_flow(&exec_uuid)
        .await
        .map_err(engine_err)?;
    Ok(Json(panes))
}

pub async fn resume_flow(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let engine = flow_engine(&state)?;
    let execution = engine.resume_flow(&id).await.map_err(engine_err)?;
    Ok(Json(serde_json::to_value(execution).map_err(engine_err)?))
}

pub async fn retry_node(
    State(state): State<Arc<AppState>>,
    Path((id, node_id)): Path<(String, String)>,
) -> ApiResult<serde_json::Value> {
    let engine = flow_engine(&state)?;
    let result = engine.retry_node(&id, &node_id).await.map_err(engine_err)?;
    Ok(Json(serde_json::to_value(result).map_err(engine_err)?))
}

pub async fn get_checkpoint(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let engine = flow_engine(&state)?;
    let ck = engine.get_checkpoint(&id).await.map_err(engine_err)?;
    match ck {
        Some(c) => Ok(Json(serde_json::to_value(c).map_err(engine_err)?)),
        None => Err((
            StatusCode::NOT_FOUND,
            "no checkpoint for this execution".into(),
        )),
    }
}

// ── Node Types ───────────────────────────────────────────────────

pub async fn list_node_types(State(state): State<Arc<AppState>>) -> ApiResult<Vec<String>> {
    let engine = flow_engine(&state)?;
    let parallel_on = std::env::var("RUSVEL_FLOW_PARALLEL_EVALUATE")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let mut types = engine.node_types();
    if !parallel_on {
        types.retain(|t| t != "parallel_evaluate");
    }
    Ok(Json(types))
}
