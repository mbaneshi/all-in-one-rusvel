mod common;

use axum::http::StatusCode;
use serde_json::json;

use common::{build_harness, json_request};

#[tokio::test]
async fn list_hooks_empty() {
    let mut h = build_harness().await;
    let (status, body) = json_request(&mut h.router, "GET", "/api/hooks", None).await;
    assert_eq!(status, StatusCode::OK);
    let list: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    assert!(list.is_empty());
}

#[tokio::test]
async fn create_get_delete_hook_roundtrip() {
    let mut h = build_harness().await;

    let (status, body) = json_request(
        &mut h.router,
        "POST",
        "/api/hooks",
        Some(json!({
            "id": "",
            "name": "notify-on-done",
            "event": "Notification",
            "matcher": ".*",
            "hook_type": "command",
            "action": "echo done",
            "enabled": true,
            "metadata": { "engine": "forge" }
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let id = created["id"].as_str().unwrap();

    let (st2, b2) = json_request(&mut h.router, "GET", &format!("/api/hooks/{id}"), None).await;
    assert_eq!(st2, StatusCode::OK);
    let got: serde_json::Value = serde_json::from_slice(&b2).unwrap();
    assert_eq!(got["name"], "notify-on-done");

    let (st3, _) = json_request(&mut h.router, "DELETE", &format!("/api/hooks/{id}"), None).await;
    assert_eq!(st3, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn list_hook_events() {
    let mut h = build_harness().await;
    let (status, body) = json_request(&mut h.router, "GET", "/api/hooks/events", None).await;
    assert_eq!(status, StatusCode::OK);
    let events: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    assert!(!events.is_empty());
}
