mod common;

use axum::http::StatusCode;
use serde_json::json;

use common::{build_harness, json_request};

#[tokio::test]
async fn list_mcp_servers_empty() {
    let mut h = build_harness().await;
    let (status, body) = json_request(&mut h.router, "GET", "/api/mcp-servers", None).await;
    assert_eq!(status, StatusCode::OK);
    let list: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    assert!(list.is_empty());
}

#[tokio::test]
async fn create_get_delete_mcp_server_roundtrip() {
    let mut h = build_harness().await;

    let (status, body) = json_request(
        &mut h.router,
        "POST",
        "/api/mcp-servers",
        Some(json!({
            "id": "",
            "name": "test-mcp",
            "description": "",
            "server_type": "stdio",
            "command": "echo",
            "args": ["hello"],
            "url": null,
            "env": {},
            "enabled": true,
            "metadata": {}
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let id = created["id"].as_str().unwrap();

    let (st2, b2) = json_request(
        &mut h.router,
        "GET",
        &format!("/api/mcp-servers/{id}"),
        None,
    )
    .await;
    assert_eq!(st2, StatusCode::OK);
    let got: serde_json::Value = serde_json::from_slice(&b2).unwrap();
    assert_eq!(got["name"], "test-mcp");

    let (st3, _) = json_request(
        &mut h.router,
        "DELETE",
        &format!("/api/mcp-servers/{id}"),
        None,
    )
    .await;
    assert_eq!(st3, StatusCode::NO_CONTENT);
}
