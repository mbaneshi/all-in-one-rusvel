mod common;

use axum::http::StatusCode;
use serde_json::json;

use common::{build_harness, json_request};

#[tokio::test]
async fn list_workflows_empty() {
    let mut h = build_harness().await;
    let (status, body) = json_request(&mut h.router, "GET", "/api/workflows", None).await;
    assert_eq!(status, StatusCode::OK);
    let list: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    assert!(list.is_empty());
}

#[tokio::test]
async fn create_get_workflow_roundtrip() {
    let mut h = build_harness().await;

    let (status, body) = json_request(
        &mut h.router,
        "POST",
        "/api/workflows",
        Some(json!({
            "name": "test-workflow",
            "description": "A test workflow",
            "steps": [
                {
                    "agent_name": "default",
                    "prompt_template": "Echo: {{input}}",
                    "step_type": "sequential"
                }
            ],
            "metadata": {}
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let id = created["id"].as_str().unwrap();

    let (st2, b2) = json_request(&mut h.router, "GET", &format!("/api/workflows/{id}"), None).await;
    assert_eq!(st2, StatusCode::OK);
    let got: serde_json::Value = serde_json::from_slice(&b2).unwrap();
    assert_eq!(got["name"], "test-workflow");

    // Delete
    let (st3, _) = json_request(
        &mut h.router,
        "DELETE",
        &format!("/api/workflows/{id}"),
        None,
    )
    .await;
    assert_eq!(st3, StatusCode::NO_CONTENT);
}
