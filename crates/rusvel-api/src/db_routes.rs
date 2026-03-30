//! RusvelBase DB browser API — schema introspection and read-focused queries.

use std::sync::Arc;
use std::time::Instant;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use rusqlite::Connection;
use rusqlite::Statement;
use rusqlite::types::ValueRef;
use serde::Serialize;
use serde_json::{Number, Value};

use rusvel_core::error::RusvelError;
use rusvel_schema::SchemaIntrospector;

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

#[derive(Serialize)]
pub struct SqlColumnMeta {
    pub name: String,
    #[serde(rename = "type")]
    pub col_type: String,
}

#[derive(Serialize)]
pub struct RowsResponse {
    pub columns: Vec<SqlColumnMeta>,
    pub rows: Vec<Vec<Value>>,
    /// Number of rows returned in this page.
    pub row_count: usize,
    /// Total rows in table (from schema introspection).
    pub table_row_count: u64,
}

#[derive(Serialize)]
pub struct SqlExecuteResponse {
    pub columns: Vec<SqlColumnMeta>,
    pub rows: Vec<Vec<Value>>,
    pub row_count: usize,
    pub duration_ms: u64,
}

fn map_err(e: RusvelError) -> (StatusCode, String) {
    match e {
        RusvelError::NotFound { .. } => (StatusCode::NOT_FOUND, e.to_string()),
        RusvelError::Validation(_) => (StatusCode::BAD_REQUEST, e.to_string()),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

fn parse_order(s: &str) -> rusvel_core::Result<(String, bool)> {
    let s = s.trim();
    if s.is_empty() {
        return Err(RusvelError::Validation("empty order".into()));
    }
    if let Some((col, dir)) = s.rsplit_once('.') {
        match dir.to_ascii_lowercase().as_str() {
            "asc" => {
                if SchemaIntrospector::validate_column_name(col) {
                    return Ok((col.to_string(), false));
                }
            }
            "desc" => {
                if SchemaIntrospector::validate_column_name(col) {
                    return Ok((col.to_string(), true));
                }
            }
            _ => {}
        }
    }
    if SchemaIntrospector::validate_column_name(s) {
        return Ok((s.to_string(), false));
    }
    Err(RusvelError::Validation(format!("invalid order: {s}")))
}

fn statement_columns(stmt: &Statement<'_>) -> Vec<SqlColumnMeta> {
    stmt.columns()
        .into_iter()
        .map(|c| SqlColumnMeta {
            name: c.name().to_string(),
            col_type: c.decl_type().unwrap_or("").to_string(),
        })
        .collect()
}

fn value_ref_to_json(v: ValueRef<'_>) -> Value {
    match v {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(i) => Value::Number(Number::from(i)),
        ValueRef::Real(f) => Number::from_f64(f)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        ValueRef::Text(t) => Value::String(String::from_utf8_lossy(t).into_owned()),
        ValueRef::Blob(b) => Value::String(format!("<blob {} bytes>", b.len())),
    }
}

fn run_sql(
    conn: &Connection,
    query: &str,
) -> rusvel_core::Result<(Vec<SqlColumnMeta>, Vec<Vec<Value>>, usize)> {
    let query = query.trim();
    if query.is_empty() {
        return Err(RusvelError::Validation("empty query".into()));
    }
    let mut stmt = conn
        .prepare(query)
        .map_err(|e| RusvelError::Storage(e.to_string()))?;
    let ncols = stmt.column_count();
    if ncols == 0 {
        let n = stmt
            .execute([])
            .map_err(|e| RusvelError::Storage(e.to_string()))?;
        return Ok((vec![], vec![], n));
    }
    let cols = statement_columns(&stmt);
    let mut rows = Vec::new();
    let mut rows_iter = stmt
        .query([])
        .map_err(|e| RusvelError::Storage(e.to_string()))?;
    while let Some(row) = rows_iter
        .next()
        .map_err(|e| RusvelError::Storage(e.to_string()))?
    {
        let mut r = Vec::with_capacity(ncols);
        for i in 0..ncols {
            let cell = row
                .get_ref(i)
                .map_err(|e| RusvelError::Storage(e.to_string()))?;
            r.push(value_ref_to_json(cell));
        }
        rows.push(r);
    }
    let n = rows.len();
    Ok((cols, rows, n))
}

pub async fn list_tables(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<rusvel_schema::TableSummary>>, (StatusCode, String)> {
    let db = state.database.clone();
    tokio::task::spawn_blocking(move || {
        db.with_connection(|conn| SchemaIntrospector::list_tables(conn))
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(map_err)
    .map(Json)
}

pub async fn get_table_schema(
    State(state): State<Arc<AppState>>,
    Path(table): Path<String>,
) -> Result<Json<rusvel_schema::TableInfo>, (StatusCode, String)> {
    let db = state.database.clone();
    tokio::task::spawn_blocking(move || {
        db.with_connection(|conn| SchemaIntrospector::get_table(conn, &table))
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(map_err)
    .map(Json)
}

pub async fn get_table_rows(
    State(state): State<Arc<AppState>>,
    Path(table): Path<String>,
    Query(q): Query<RowsQuery>,
) -> Result<Json<RowsResponse>, (StatusCode, String)> {
    let limit = q.limit.unwrap_or(50).clamp(1, 500);
    let offset = q.offset.unwrap_or(0);
    let order = q.order;
    let db = state.database.clone();
    tokio::task::spawn_blocking(move || {
        db.with_connection(|conn| {
            if !SchemaIntrospector::validate_table_name(&table) {
                return Err(RusvelError::Validation(format!("invalid table: {table}")));
            }
            let info = SchemaIntrospector::get_table(conn, &table)?;
            let order_clause = if let Some(ref o) = order {
                let (col, desc) = parse_order(o)?;
                if !SchemaIntrospector::validate_column_for_table(conn, &table, &col)? {
                    return Err(RusvelError::Validation(format!("unknown column: {col}")));
                }
                let dir = if desc { "DESC" } else { "ASC" };
                format!(r#" ORDER BY "{col}" {dir}"#)
            } else {
                String::new()
            };
            let sql = format!(r#"SELECT * FROM "{table}"{order_clause} LIMIT ? OFFSET ?"#);
            let mut stmt = conn
                .prepare(&sql)
                .map_err(|e| RusvelError::Storage(e.to_string()))?;
            let cols = statement_columns(&stmt);
            let ncols = cols.len();
            let limit_i = i64::from(limit);
            let offset_i = i64::from(offset);
            let mut rows = Vec::new();
            let mut rows_iter = stmt
                .query(rusqlite::params![limit_i, offset_i])
                .map_err(|e| RusvelError::Storage(e.to_string()))?;
            while let Some(row) = rows_iter
                .next()
                .map_err(|e| RusvelError::Storage(e.to_string()))?
            {
                let mut r = Vec::with_capacity(ncols);
                for i in 0..ncols {
                    let cell = row
                        .get_ref(i)
                        .map_err(|e| RusvelError::Storage(e.to_string()))?;
                    r.push(value_ref_to_json(cell));
                }
                rows.push(r);
            }
            let row_count = rows.len();
            Ok(RowsResponse {
                columns: cols,
                rows,
                row_count,
                table_row_count: info.row_count,
            })
        })
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(map_err)
    .map(Json)
}

pub async fn post_sql(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SqlBody>,
) -> Result<Json<SqlExecuteResponse>, (StatusCode, String)> {
    let db = state.database.clone();
    let force_read_only = env_disallows_sql_writes();
    let read_only = force_read_only || body.read_only;
    let query = body.query;
    tokio::task::spawn_blocking(move || {
        db.with_connection(|conn| {
            let start = Instant::now();
            if read_only {
                conn.execute_batch("PRAGMA query_only = ON;")
                    .map_err(|e| RusvelError::Storage(e.to_string()))?;
            }
            let res = run_sql(conn, &query);
            if read_only {
                let _ = conn.execute_batch("PRAGMA query_only = OFF;");
            }
            let (columns, rows, row_count) = res?;
            let duration_ms = start.elapsed().as_millis() as u64;
            Ok(SqlExecuteResponse {
                columns,
                rows,
                row_count,
                duration_ms,
            })
        })
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(map_err)
    .map(Json)
}
