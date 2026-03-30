mod common;

use axum::http::StatusCode;
use serde_json::json;

use common::{build_harness, json_request};

#[tokio::test]
async fn list_flows_empty() {
    let mut h = build_harness().await;
    let sid = h.session_id;
    let (status, body) = json_request(
        &mut h.router,
        "GET",
        &format!("/api/flows?session_id={sid}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let list: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    assert!(list.is_empty());
}

#[tokio::test]
async fn list_node_types() {
    let mut h = build_harness().await;
    let (status, body) = json_request(&mut h.router, "GET", "/api/flows/node-types", None).await;
    assert_eq!(status, StatusCode::OK);
    let types: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    assert!(!types.is_empty());
}

#[tokio::test]
async fn create_get_flow_roundtrip() {
    let mut h = build_harness().await;
    let sid = h.session_id;

    let (status, body) = json_request(
        &mut h.router,
        "POST",
        "/api/flows",
        Some(json!({
            "id": "00000000-0000-0000-0000-000000000000",
            "session_id": sid.to_string(),
            "name": "test-flow",
            "description": "",
            "nodes": [
                {
                    "id": "00000000-0000-0000-0000-000000000001",
                    "node_type": "code",
                    "name": "echo",
                    "parameters": {"command": "echo hi"},
                    "position": [0.0, 0.0],
                    "metadata": {}
                }
            ],
            "connections": [],
            "metadata": {}
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let id = created["id"].as_str().unwrap();

    let (st2, b2) = json_request(&mut h.router, "GET", &format!("/api/flows/{id}"), None).await;
    assert_eq!(st2, StatusCode::OK);
    let got: serde_json::Value = serde_json::from_slice(&b2).unwrap();
    assert_eq!(got["name"], "test-flow");
}

#[tokio::test]
async fn get_cross_engine_template() {
    let mut h = build_harness().await;
    let (status, body) = json_request(
        &mut h.router,
        "GET",
        "/api/flows/templates/cross-engine-handoff",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let tmpl: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(tmpl["nodes"].is_array());
}
