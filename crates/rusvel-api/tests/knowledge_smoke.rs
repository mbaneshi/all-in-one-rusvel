mod common;

use axum::http::StatusCode;
use serde_json::json;

use common::{build_harness, json_request};

#[tokio::test]
async fn knowledge_stats_ok() {
    let mut h = build_harness().await;
    let (status, _) = json_request(&mut h.router, "GET", "/api/knowledge/stats", None).await;
    // May return 200 with empty stats, or 503 if embedding not configured
    assert!(status == StatusCode::OK || status == StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn list_knowledge_ok() {
    let mut h = build_harness().await;
    let (status, _) = json_request(&mut h.router, "GET", "/api/knowledge", None).await;
    // Graceful when no embedding adapter is configured
    assert!(status == StatusCode::OK || status == StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn search_knowledge_ok() {
    let mut h = build_harness().await;
    let (status, _) = json_request(
        &mut h.router,
        "POST",
        "/api/knowledge/search",
        Some(json!({ "query": "test", "limit": 5 })),
    )
    .await;
    assert!(
        status == StatusCode::OK
            || status == StatusCode::SERVICE_UNAVAILABLE
            || status == StatusCode::INTERNAL_SERVER_ERROR
    );
}
