mod common;

use axum::http::StatusCode;
use serde_json::Value;

use common::{build_harness, json_request};

#[tokio::test]
async fn get_system_runtime_has_expected_shape() {
    let mut h = build_harness().await;
    let (status, body) = json_request(&mut h.router, "GET", "/api/system/runtime", None).await;
    assert_eq!(status, StatusCode::OK);
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert!(v.get("process").is_some());
    assert!(v["process"].get("uptime_seconds").is_some());
    assert!(v["process"].get("data_dir").is_some());
    assert!(v["process"].get("http_listen").is_some());
    assert!(v.get("auth").is_some());
    assert!(v.get("llm").is_some());
    assert!(v["llm"].get("claude_effective_transport").is_some());
    assert!(v["llm"].get("providers").is_some());
    assert!(v.get("drift").is_some());
    assert!(v.get("integrations").is_some());
    assert!(v.get("subsystems").is_some());
    assert!(v.get("health").is_some());
    assert!(v.get("capabilities").is_some());
    assert!(v["capabilities"].is_array());
}
