//! RusvelBase DB browser API — schema introspection and read-focused queries.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;

use rusvel_core::error::RusvelError;
use rusvel_core::ports::{RusvelBasePagedRows, RusvelBaseSqlExecute};

use crate::AppState;

#[derive(Debug, serde::Deserialize)]
pub struct RowsQuery {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub order: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct SqlBody {
    pub query: String,
    #[serde(default = "default_read_only")]
    pub read_only: bool,
}

fn default_read_only() -> bool {
    true
}

/// When `RUSVEL_DB_SQL_WRITE` is `0`, `false`, or `off`, POST `/api/db/sql` always runs with
/// `PRAGMA query_only = ON` (writes blocked) regardless of client `read_only: false`.
fn env_disallows_sql_writes() -> bool {
    std::env::var("RUSVEL_DB_SQL_WRITE")
        .map(|v| {
            let v = v.trim();
            v == "0" || v.eq_ignore_ascii_case("false") || v.eq_ignore_ascii_case("off")
        })
        .unwrap_or(false)
}

pub type RowsResponse = RusvelBasePagedRows;
pub type SqlExecuteResponse = RusvelBaseSqlExecute;

fn map_err(e: RusvelError) -> (StatusCode, String) {
    match e {
        RusvelError::NotFound { .. } => (StatusCode::NOT_FOUND, e.to_string()),
        RusvelError::Validation(_) => (StatusCode::BAD_REQUEST, e.to_string()),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn list_tables(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<rusvel_schema::TableSummary>>, (StatusCode, String)> {
    let rows = state
        .rusvel_base
        .list_table_summaries()
        .await
        .map_err(map_err)?;
    let out = rows
        .into_iter()
        .map(|r| rusvel_schema::TableSummary {
            name: r.name,
            row_count: r.row_count,
        })
        .collect();
    Ok(Json(out))
}

pub async fn get_table_schema(
    State(state): State<Arc<AppState>>,
    Path(table): Path<String>,
) -> Result<Json<rusvel_schema::TableInfo>, (StatusCode, String)> {
    let v = state
        .rusvel_base
        .get_table_schema_json(&table)
        .await
        .map_err(map_err)?;
    let info: rusvel_schema::TableInfo = serde_json::from_value(v).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("schema json: {e}"),
        )
    })?;
    Ok(Json(info))
}

pub async fn get_table_rows(
    State(state): State<Arc<AppState>>,
    Path(table): Path<String>,
    Query(q): Query<RowsQuery>,
) -> Result<Json<RowsResponse>, (StatusCode, String)> {
    let limit = q.limit.unwrap_or(50).clamp(1, 500);
    let offset = q.offset.unwrap_or(0);
    state
        .rusvel_base
        .query_table_rows(&table, limit, offset, q.order.as_deref())
        .await
        .map_err(map_err)
        .map(Json)
}

pub async fn post_sql(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SqlBody>,
) -> Result<Json<SqlExecuteResponse>, (StatusCode, String)> {
    let force_read_only = env_disallows_sql_writes();
    let read_only = force_read_only || body.read_only;
    state
        .rusvel_base
        .execute_sql(&body.query, read_only)
        .await
        .map_err(map_err)
        .map(Json)
}
