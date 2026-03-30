//! POST /api/forge/pipeline — S-042 cross-engine pipeline (harness has Harvest + Content).

mod common;

use axum::http::StatusCode;
use rusvel_core::domain::{FlowExecution, FlowExecutionStatus};
use serde_json::json;

use common::{build_harness, json_request};

#[tokio::test]
async fn post_forge_pipeline_returns_flow_execution() {
    let mut h = build_harness().await;

    let (status, body) = json_request(
        &mut h.router,
        "POST",
        "/api/forge/pipeline",
        Some(json!({
            "session_id": h.session_id.to_string(),
            "def": {
                "draft_topic": "API integration test draft"
            }
        })),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::OK,
        "body: {}",
        String::from_utf8_lossy(&body)
    );
    let exec: FlowExecution = serde_json::from_slice(&body).expect("flow execution json");
    assert_eq!(exec.status, FlowExecutionStatus::Succeeded);
    assert!(!exec.id.to_string().is_empty());
}
