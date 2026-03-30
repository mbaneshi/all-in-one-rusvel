mod common;

use axum::http::StatusCode;
use serde_json::json;

use common::{build_harness, json_request};

#[tokio::test]
async fn list_rules_empty() {
    let mut h = build_harness().await;
    let (status, body) = json_request(&mut h.router, "GET", "/api/rules", None).await;
    assert_eq!(status, StatusCode::OK);
    let list: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    assert!(list.is_empty());
}

#[tokio::test]
async fn create_get_delete_rule_roundtrip() {
    let mut h = build_harness().await;

    let (status, body) = json_request(
        &mut h.router,
        "POST",
        "/api/rules",
        Some(json!({
            "id": "",
            "name": "no-mocks",
            "content": "Never use mocks in integration tests.",
            "enabled": true,
            "metadata": { "engine": "code" }
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let id = created["id"].as_str().unwrap();

    let (st2, b2) = json_request(&mut h.router, "GET", &format!("/api/rules/{id}"), None).await;
    assert_eq!(st2, StatusCode::OK);
    let got: serde_json::Value = serde_json::from_slice(&b2).unwrap();
    assert_eq!(got["name"], "no-mocks");

    let (st3, b3) = json_request(&mut h.router, "GET", "/api/rules?engine=code", None).await;
    assert_eq!(st3, StatusCode::OK);
    let list: Vec<serde_json::Value> = serde_json::from_slice(&b3).unwrap();
    assert!(list.iter().any(|r| r["name"] == "no-mocks"));

    let (st4, _) = json_request(&mut h.router, "DELETE", &format!("/api/rules/{id}"), None).await;
    assert_eq!(st4, StatusCode::NO_CONTENT);

    let (st5, _) = json_request(&mut h.router, "GET", &format!("/api/rules/{id}"), None).await;
    assert_eq!(st5, StatusCode::NOT_FOUND);
}
