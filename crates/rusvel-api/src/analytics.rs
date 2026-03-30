//! Analytics endpoint — aggregate counts across all `ObjectStore` kinds.
//!
//! `GET /api/analytics` returns a JSON object with counts of agents, skills,
//! rules, MCP servers, hooks, conversations, events, and departments.
//!
//! `GET /api/analytics/spend` returns LLM spend from SQLite `cost_events` rollups when the table
//! has rows, else from [`MetricStore`] (`llm.cost_usd`), optionally filtered by department and
//! scoped to a session for budget warnings.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};

use chrono::{DateTime, Utc};
use rusvel_core::domain::{CostEvent, EventFilter, MetricFilter, ObjectFilter};
use rusvel_core::id::SessionId;
use rusvel_core::ports::MetricStore;
use rusvel_llm::cost::{LLM_COST_METRIC_NAME, aggregate_spend};
use uuid::Uuid;

use crate::AppState;

#[derive(Debug, Serialize)]
pub struct AnalyticsResponse {
    pub agents: usize,
    pub skills: usize,
    pub rules: usize,
    pub mcp_servers: usize,
    pub hooks: usize,
    pub conversations: usize,
    pub events: usize,
    pub departments: usize,
}

async fn collect_analytics_counts(
    state: &Arc<AppState>,
) -> Result<AnalyticsResponse, (StatusCode, String)> {
    let objects = state.storage.objects();
    let filter = ObjectFilter::default();

    let agents = objects
        .list("agents", filter.clone())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .len();

    let skills = objects
        .list("skills", filter.clone())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .len();

    let rules = objects
        .list("rules", filter.clone())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .len();

    let mcp_servers = objects
        .list("mcp_servers", filter.clone())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .len();

    let hooks = objects
        .list("hooks", filter.clone())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .len();

    let chat_messages = objects
        .list("chat_message", filter.clone())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let conversations: usize = chat_messages
        .iter()
        .filter_map(|v| v.get("conversation_id").and_then(|c| c.as_str()))
        .collect::<HashSet<_>>()
        .len();

    let events = state
        .events
        .query(EventFilter::default())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .len();

    let departments = state.registry.list().len();

    Ok(AnalyticsResponse {
        agents,
        skills,
        rules,
        mcp_servers,
        hooks,
        conversations,
        events,
        departments,
    })
}

/// `GET /api/analytics` — return aggregate counts.
pub async fn get_analytics(
    State(state): State<Arc<AppState>>,
) -> Result<Json<AnalyticsResponse>, (StatusCode, String)> {
    let counts = collect_analytics_counts(&state).await?;
    Ok(Json(counts))
}

/// Unified snapshot for the home dashboard (S-047): object counts + spend in one round trip.
#[derive(Debug, Serialize)]
pub struct DashboardResponse {
    #[serde(flatten)]
    pub counts: AnalyticsResponse,
    pub spend: SpendResponse,
}

/// `GET /api/analytics/dashboard` — [`AnalyticsResponse`] + [`SpendResponse`] (same query params as spend).
pub async fn get_dashboard(
    State(state): State<Arc<AppState>>,
    Query(q): Query<SpendQuery>,
) -> Result<Json<DashboardResponse>, (StatusCode, String)> {
    let counts = collect_analytics_counts(&state).await?;
    let spend = collect_spend(&state, &q).await?;
    Ok(Json(DashboardResponse { counts, spend }))
}

// ── Spend (S-035) ─────────────────────────────────────────────

const BUDGET_WARN_RATIO: f64 = 0.8;

#[derive(Debug, Serialize)]
pub struct ModelSpend {
    pub usd: f64,
    pub tokens: u64,
}

#[derive(Debug, Deserialize)]
pub struct SpendQuery {
    /// Filter totals to this department id (e.g. `harvest`, `content`). Omit for all departments.
    pub dept: Option<String>,
    /// When set, include session budget from [`Session::config`] and session-wide spend for warnings.
    pub session_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SpendResponse {
    /// Total USD for the selected department, or all departments if `dept` omitted (`cost_events` or metrics).
    pub total_usd: f64,
    /// Per-department USD (`cost_events` rollups when present, else `llm.cost_usd` tags).
    pub by_department: HashMap<String, f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Total LLM spend attributed to this session (any department), when `session_id` is set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_total_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_budget_limit_usd: Option<f64>,
    /// True when session spend is at or above 80% of the configured budget.
    pub budget_warning: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget_usage_ratio: Option<f64>,
    /// Per-department token totals when spend is sourced from `cost_events` (keys match `by_department`).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub department_tokens: HashMap<String, u64>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub by_model: HashMap<String, ModelSpend>,
}

async fn spend_from_session_query(
    state: &Arc<AppState>,
    q: &SpendQuery,
    session_totals: &HashMap<String, f64>,
) -> (Option<String>, Option<f64>, Option<f64>, bool, Option<f64>) {
    let mut session_id_out: Option<String> = None;
    let mut session_total_usd: Option<f64> = None;
    let mut session_budget_limit_usd: Option<f64> = None;
    let mut budget_warning = false;
    let mut budget_usage_ratio: Option<f64> = None;

    if let Some(ref sid_str) = q.session_id {
        if let Ok(uuid) = Uuid::parse_str(sid_str) {
            let sid = SessionId::from(uuid);
            session_id_out = Some(sid_str.clone());
            let sid_key = sid.to_string();
            session_total_usd = session_totals.get(&sid_key).copied().or(Some(0.0));

            if let Ok(session) = state.sessions.load(&sid).await {
                session_budget_limit_usd = session.config.budget_limit;
            }

            if let (Some(spent), Some(limit)) = (session_total_usd, session_budget_limit_usd) {
                if limit > 0.0 {
                    let ratio = spent / limit;
                    budget_usage_ratio = Some(ratio);
                    if spent >= limit * BUDGET_WARN_RATIO {
                        budget_warning = true;
                    }
                }
            }
        }
    }

    (
        session_id_out,
        session_total_usd,
        session_budget_limit_usd,
        budget_warning,
        budget_usage_ratio,
    )
}

async fn collect_spend(
    state: &Arc<AppState>,
    q: &SpendQuery,
) -> Result<SpendResponse, (StatusCode, String)> {
    let snapshot = state
        .database
        .cost_events_spend_snapshot()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if snapshot.event_row_count == 0 {
        let filter = MetricFilter {
            name: Some(LLM_COST_METRIC_NAME.into()),
            limit: Some(50_000),
            ..Default::default()
        };
        let points = state
            .database
            .query(filter)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        let agg = aggregate_spend(&points);
        let by_department = agg.by_department;
        let total_usd = if let Some(ref d) = q.dept {
            *by_department.get(d.as_str()).unwrap_or(&0.0)
        } else {
            agg.total_usd
        };

        let (
            session_id,
            session_total_usd,
            session_budget_limit_usd,
            budget_warning,
            budget_usage_ratio,
        ) = spend_from_session_query(state, q, &agg.by_session).await;

        return Ok(SpendResponse {
            total_usd,
            by_department,
            session_id,
            session_total_usd,
            session_budget_limit_usd,
            budget_warning,
            budget_usage_ratio,
            department_tokens: HashMap::new(),
            by_model: HashMap::new(),
        });
    }

    let by_department = snapshot.by_department_usd.clone();
    let total_usd = if let Some(ref d) = q.dept {
        *by_department.get(d.as_str()).unwrap_or(&0.0)
    } else {
        snapshot.total_usd_all
    };

    let department_tokens = snapshot.by_department_tokens.clone();
    let by_model: HashMap<String, ModelSpend> = snapshot
        .by_model
        .into_iter()
        .map(|(k, (usd, tokens))| (k, ModelSpend { usd, tokens }))
        .collect();

    let (
        session_id,
        session_total_usd,
        session_budget_limit_usd,
        budget_warning,
        budget_usage_ratio,
    ) = spend_from_session_query(state, q, &snapshot.by_session_usd).await;

    Ok(SpendResponse {
        total_usd,
        by_department,
        session_id,
        session_total_usd,
        session_budget_limit_usd,
        budget_warning,
        budget_usage_ratio,
        department_tokens,
        by_model,
    })
}

/// `GET /api/analytics/spend` — LLM spend breakdown by department; optional session budget warning.
pub async fn get_spend(
    State(state): State<Arc<AppState>>,
    Query(q): Query<SpendQuery>,
) -> Result<Json<SpendResponse>, (StatusCode, String)> {
    let spend = collect_spend(&state, &q).await?;
    Ok(Json(spend))
}

// ── Cost events (structured rows) ────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CostsQuery {
    pub session_id: Option<String>,
    pub department_id: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CostSummaryQuery {
    pub group_by: String,
    pub session_id: Option<String>,
    pub department_id: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CostSummaryRow {
    pub key: String,
    pub total_usd: f64,
    pub total_tokens: u64,
}

fn parse_costs_rfc3339(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

async fn query_cost_events(
    state: &Arc<AppState>,
    q: &CostsQuery,
) -> Result<Vec<CostEvent>, (StatusCode, String)> {
    let session_id = q
        .session_id
        .as_ref()
        .and_then(|s| Uuid::parse_str(s).ok())
        .map(SessionId::from);
    let from = q.from.as_deref().and_then(parse_costs_rfc3339);
    let to = q.to.as_deref().and_then(parse_costs_rfc3339);
    state
        .database
        .query_costs(session_id.as_ref(), q.department_id.as_deref(), from, to)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

/// `GET /api/analytics/costs` — structured cost rows; query params all optional.
pub async fn get_costs(
    State(state): State<Arc<AppState>>,
    Query(q): Query<CostsQuery>,
) -> Result<Json<Vec<CostEvent>>, (StatusCode, String)> {
    let events = query_cost_events(&state, &q).await?;
    Ok(Json(events))
}

/// `GET /api/analytics/costs/summary?group_by=department|model|operation`
pub async fn get_costs_summary(
    State(state): State<Arc<AppState>>,
    Query(q): Query<CostSummaryQuery>,
) -> Result<Json<Vec<CostSummaryRow>>, (StatusCode, String)> {
    let gb = q.group_by.to_ascii_lowercase();
    if !matches!(gb.as_str(), "department" | "model" | "operation") {
        return Err((
            StatusCode::BAD_REQUEST,
            "group_by must be department, model, or operation".into(),
        ));
    }
    let cq = CostsQuery {
        session_id: q.session_id.clone(),
        department_id: q.department_id.clone(),
        from: q.from.clone(),
        to: q.to.clone(),
    };
    let events = query_cost_events(&state, &cq).await?;
    let mut acc: HashMap<String, (f64, u64)> = HashMap::new();
    for e in events {
        let key = match gb.as_str() {
            "department" => e
                .department_id
                .clone()
                .unwrap_or_else(|| "_none".to_string()),
            "model" => e.model.clone(),
            "operation" => e.operation.clone(),
            _ => unreachable!(),
        };
        let ent = acc.entry(key).or_insert((0.0, 0));
        ent.0 += e.cost_usd;
        ent.1 += u64::from(e.input_tokens) + u64::from(e.output_tokens);
    }
    let mut rows: Vec<CostSummaryRow> = acc
        .into_iter()
        .map(|(key, (total_usd, total_tokens))| CostSummaryRow {
            key,
            total_usd,
            total_tokens,
        })
        .collect();
    rows.sort_by(|a, b| {
        b.total_usd
            .partial_cmp(&a.total_usd)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(Json(rows))
}
