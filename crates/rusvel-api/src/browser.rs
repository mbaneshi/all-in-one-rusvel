//! HTTP routes for Chrome CDP / [`rusvel_cdp::CdpClient`] (browser bridge).
//!
//! Optional allowlist: `RUSVEL_BROWSER_ALLOWED_ORIGINS` — comma-separated hostnames
//! (e.g. `example.com,docs.rusvel.io`). When set, `navigate` actions must target
//! an allowed host (Claude-for-Chrome-style safety).

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, Sse};
use chrono::Utc;
use futures::Stream;
use rusvel_core::domain::Event as RusvelEvent;
use rusvel_core::id::EventId;
use rusvel_core::ports::BrowserPort;
use serde::Deserialize;
use tokio_stream::StreamExt as _;
use tokio_stream::wrappers::BroadcastStream;
use tracing::warn;

use crate::AppState;

fn browser_allowed_hosts() -> Option<Vec<String>> {
    std::env::var("RUSVEL_BROWSER_ALLOWED_ORIGINS")
        .ok()
        .map(|s| {
            s.split(',')
                .map(|x| x.trim().to_ascii_lowercase())
                .filter(|x| !x.is_empty())
                .collect::<Vec<_>>()
        })
        .filter(|v| !v.is_empty())
}

fn navigate_url_host(url: &str) -> Option<String> {
    let u = url.trim();
    let rest = u
        .strip_prefix("https://")
        .or_else(|| u.strip_prefix("http://"))?;
    let host = rest.split('/').next()?;
    Some(host.split(':').next()?.to_ascii_lowercase())
}

fn navigate_url_allowed(url: &str) -> bool {
    let Some(list) = browser_allowed_hosts() else {
        return true;
    };
    let Some(host) = navigate_url_host(url) else {
        return false;
    };
    list.iter()
        .any(|allowed| allowed == &host || host.ends_with(&format!(".{allowed}")))
}

#[derive(Deserialize)]
pub struct ConnectBody {
    pub endpoint: String,
}

#[derive(Deserialize)]
pub struct ActBody {
    pub tab_id: String,
    pub action: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub script: Option<String>,
}

pub async fn browser_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let Some(cdp) = state.cdp.as_ref() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };
    let connected = cdp.is_connected().await;
    Ok(Json(serde_json::json!({ "connected": connected })))
}

pub async fn browser_connect(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ConnectBody>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let Some(cdp) = state.cdp.as_ref() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };
    cdp.connect(&body.endpoint)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(Json(
        serde_json::json!({ "ok": true, "endpoint": body.endpoint }),
    ))
}

pub async fn browser_tabs(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let Some(cdp) = state.cdp.as_ref() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };
    let tabs = cdp.tabs().await.map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(Json(serde_json::to_value(tabs).unwrap_or_default()))
}

pub async fn browser_observe(
    Path(tab_id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let Some(cdp) = state.cdp.as_ref() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };
    let _rx = cdp
        .observe(&tab_id)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(Json(serde_json::json!({ "ok": true, "tab_id": tab_id })))
}

pub async fn browser_captures(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let Some(cdp) = state.cdp.as_ref() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };
    let list = cdp.captures_snapshot().await;
    Ok(Json(serde_json::json!({ "captures": list })))
}

pub async fn browser_act(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ActBody>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let Some(cdp) = state.cdp.as_ref() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };
    let out = match body.action.as_str() {
        "navigate" => {
            let url = body.url.as_deref().ok_or(StatusCode::BAD_REQUEST)?;
            if !navigate_url_allowed(url) {
                warn!(target: "rusvel::browser", %url, "navigate blocked by RUSVEL_BROWSER_ALLOWED_ORIGINS");
                return Err(StatusCode::FORBIDDEN);
            }
            cdp.navigate(&body.tab_id, url)
                .await
                .map_err(|_| StatusCode::BAD_REQUEST)?;
            let events = state.events.clone();
            let url_owned = url.to_string();
            let tab = body.tab_id.clone();
            tokio::spawn(async move {
                let _ = events
                    .emit(RusvelEvent {
                        id: EventId::new(),
                        session_id: None,
                        run_id: None,
                        source: "browser".into(),
                        kind: "browser.navigate".into(),
                        payload: serde_json::json!({
                            "tab_id": tab,
                            "url": url_owned,
                        }),
                        created_at: Utc::now(),
                        metadata: serde_json::json!({}),
                    })
                    .await;
            });
            serde_json::json!({ "ok": true })
        }
        "evaluate" | "evaluate_js" => {
            let script = body.script.as_deref().ok_or(StatusCode::BAD_REQUEST)?;
            let v = cdp
                .evaluate_js(&body.tab_id, script)
                .await
                .map_err(|_| StatusCode::BAD_REQUEST)?;
            let events = state.events.clone();
            let tab = body.tab_id.clone();
            let script_len = script.len();
            tokio::spawn(async move {
                let _ = events
                    .emit(RusvelEvent {
                        id: EventId::new(),
                        session_id: None,
                        run_id: None,
                        source: "browser".into(),
                        kind: "browser.evaluate_js".into(),
                        payload: serde_json::json!({
                            "tab_id": tab,
                            "script_len": script_len,
                        }),
                        created_at: Utc::now(),
                        metadata: serde_json::json!({}),
                    })
                    .await;
            });
            serde_json::json!({ "ok": true, "result": v })
        }
        _ => return Err(StatusCode::BAD_REQUEST),
    };
    Ok(Json(out))
}

pub async fn browser_captures_stream(
    State(state): State<Arc<AppState>>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, StatusCode> {
    let Some(cdp) = state.cdp.as_ref() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };
    let rx = cdp.subscribe_captures().await;
    let stream = BroadcastStream::new(rx).filter_map(|item| {
        let ev = item.ok()?;
        let json = serde_json::to_string(&ev).ok()?;
        Some(Ok(Event::default().data(json)))
    });
    Ok(Sse::new(stream)
        .keep_alive(axum::response::sse::KeepAlive::new().interval(Duration::from_secs(20))))
}
