mod common;

use axum::http::StatusCode;

use common::{build_harness, json_request};

#[tokio::test]
async fn list_conversations_empty() {
    let mut h = build_harness().await;
    let (status, body) = json_request(&mut h.router, "GET", "/api/chat/conversations", None).await;
    assert_eq!(status, StatusCode::OK);
    let list: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    assert!(list.is_empty());
}

#[tokio::test]
async fn get_nonexistent_conversation() {
    let mut h = build_harness().await;
    let (status, body) = json_request(
        &mut h.router,
        "GET",
        "/api/chat/conversations/00000000-0000-0000-0000-000000000000",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    // Should return empty message list for unknown conversation
    let msgs: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    assert!(msgs.is_empty());
}
