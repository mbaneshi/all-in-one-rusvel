//! Operator control center: runtime snapshot, persisted prefs, graceful shutdown.
//!
//! Prefs live in [`ObjectStore`] (`system` / `operator_prefs`). Changes require process restart
//! to affect LLM registration (composition root in `rusvel-app`).

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::json;

use rusvel_core::ports::StoragePort;

use crate::AppState;
use crate::config;

pub const OPERATOR_PREFS_KIND: &str = "system";
pub const OPERATOR_PREFS_ID: &str = "operator_prefs";

/// Persisted operator preferences (ADR-007: evolve via `metadata`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OperatorRuntimePrefs {
    /// `Some(true)` forces Claude CLI for `claude/…` at boot (like `RUSVEL_USE_CLAUDE_CLI=1`).
    /// `Some(false)` uses Anthropic Messages API when `ANTHROPIC_API_KEY` is set, else CLI.
    /// `None` — env only (`RUSVEL_USE_CLAUDE_CLI` + key presence).
    #[serde(default)]
    pub force_claude_cli: Option<bool>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct OperatorCapability {
    pub id: &'static str,
    pub label: &'static str,
    pub settings_href: Option<&'static str>,
    /// Repo-relative doc path for operators.
    pub doc_path: &'static str,
}

pub const OPERATOR_CAPABILITY_REGISTRY: &[OperatorCapability] = &[
    OperatorCapability {
        id: "control",
        label: "Control center",
        settings_href: Some("/settings/control"),
        doc_path: "docs/operators/security-hardening.md",
    },
    OperatorCapability {
        id: "llm",
        label: "LLM & models",
        settings_href: Some("/settings/llm"),
        doc_path: "docs/operators/security-hardening.md",
    },
    OperatorCapability {
        id: "spend",
        label: "Spend",
        settings_href: Some("/settings/spend"),
        doc_path: "docs/status/current-state.md",
    },
    OperatorCapability {
        id: "security",
        label: "Security & env reference",
        settings_href: None,
        doc_path: "docs/operators/security-hardening.md",
    },
];

pub async fn load_operator_prefs(storage: &Arc<dyn StoragePort>) -> OperatorRuntimePrefs {
    match storage
        .objects()
        .get(OPERATOR_PREFS_KIND, OPERATOR_PREFS_ID)
        .await
    {
        Ok(Some(v)) => serde_json::from_value(v).unwrap_or_default(),
        _ => OperatorRuntimePrefs::default(),
    }
}

#[derive(Debug, Deserialize)]
pub struct PutOperatorPrefsBody {
    #[serde(default)]
    pub force_claude_cli: Option<bool>,
}

/// `PUT /api/system/operator-prefs` — save prefs; LLM stack applies after restart.
pub async fn put_operator_prefs(
    State(state): State<Arc<AppState>>,
    Json(body): Json<PutOperatorPrefsBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let mut prefs = load_operator_prefs(&state.storage).await;
    prefs.force_claude_cli = body.force_claude_cli;
    let v = serde_json::to_value(&prefs).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("serialize operator prefs: {e}"),
        )
    })?;
    state
        .storage
        .objects()
        .put(OPERATOR_PREFS_KIND, OPERATOR_PREFS_ID, v)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "requires_restart": true,
        "message": "Restart the Rusvel process to apply operator preferences (LLM registration is fixed at boot).",
        "saved": prefs,
    })))
}

/// `POST /api/system/shutdown` — graceful shutdown (pair with systemd/Docker/manual restart).
pub async fn post_shutdown(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let Some(tx) = state.shutdown_tx.as_ref() else {
        return Err((
            StatusCode::NOT_IMPLEMENTED,
            "shutdown is not wired for this server build".into(),
        ));
    };
    let _ = tx.send(true);
    Ok(Json(json!({
        "ok": true,
        "message": "Shutdown initiated; the process will exit shortly. Restart via your supervisor or terminal."
    })))
}

/// `GET /api/system/runtime` — in-process effective configuration (no secret values).
pub async fn get_runtime(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let uptime_secs = state.boot_time.elapsed().as_secs();

    let db_check = match state.database.execute_sql("SELECT 1") {
        Ok(_) => "ok".to_string(),
        Err(e) => format!("error: {e}"),
    };

    let llm = config::build_llm_providers_report_for_state(&state).await;

    let env_suggests_cli = rusvel_llm::claude_transport_is_cli();

    let telegram_configured = state.channel.is_some();
    let smtp_host_set = std::env::var("RUSVEL_SMTP_HOST")
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);
    let mcp_http_auth = std::env::var("RUSVEL_MCP_HTTP_AUTH")
        .ok()
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    let api_token_set = state.auth.token.is_some();
    let read_token_set = state.auth.read_token.is_some();

    let failed: Vec<serde_json::Value> = state
        .failed_departments
        .iter()
        .map(|(id, msg)| json!({ "id": id, "error": msg }))
        .collect();

    Ok(Json(json!({
        "process": {
            "uptime_seconds": uptime_secs,
            "data_dir": state.data_dir.display().to_string(),
            "http_listen": state.http_listen,
        },
        "auth": {
            "admin_token_configured": api_token_set,
            "read_token_configured": read_token_set,
        },
        "llm": llm,
        "drift": {
            "claude": {
                "env_suggests_cli": env_suggests_cli,
                "effective_cli": state.claude_transport_cli,
                "operator_pref_force_claude_cli": state.operator_prefs.force_claude_cli,
            }
        },
        "integrations": {
            "telegram_channel_configured": telegram_configured,
            "smtp_host_set": smtp_host_set,
            "mcp_http_auth_env": mcp_http_auth,
        },
        "subsystems": {
            "embedding": state.embedding.is_some(),
            "vector_store": state.vector_store.is_some(),
            "deploy": state.deploy.is_some(),
            "terminal": state.terminal.is_some(),
            "cdp": state.cdp.is_some(),
        },
        "health": {
            "database": db_check,
            "departments_failed": failed,
        },
        "operator_prefs_stored": state.operator_prefs,
        "capabilities": OPERATOR_CAPABILITY_REGISTRY,
        "notes": {
            "restart": "Env vars and operator prefs affecting LLM bootstrap apply only after process restart.",
            "shutdown": "POST /api/system/shutdown exits the process when wired (use admin token if API auth is enabled)."
        }
    })))
}
