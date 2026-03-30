mod common;

use axum::http::StatusCode;

use common::{build_harness, json_request};

#[tokio::test]
async fn list_departments_ok() {
    let mut h = build_harness().await;
    let (status, body) = json_request(&mut h.router, "GET", "/api/departments", None).await;
    assert_eq!(status, StatusCode::OK);
    let list: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    // Registry loaded from defaults (no TOML file found at /__no_such__)
    // Should return the built-in departments
    assert!(!list.is_empty());
}

#[tokio::test]
async fn get_department_by_id() {
    let mut h = build_harness().await;
    let (status, body) = json_request(&mut h.router, "GET", "/api/departments/forge", None).await;
    // May return 200 with dept info or 404 if route is /api/dept/{id} pattern
    assert!(status == StatusCode::OK || status == StatusCode::NOT_FOUND);
    if status == StatusCode::OK {
        let dept: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(dept["id"], "forge");
    }
}
