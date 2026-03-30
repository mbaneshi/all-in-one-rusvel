//! GTM outreach: sequences CRUD + POST `/api/dept/gtm/outreach/execute` — S-033 wiring tests.

mod common;

use axum::http::StatusCode;
use common::{build_harness, build_harness_with_gtm, json_request};
use gtm_engine::SequenceStep;
use serde_json::json;

#[tokio::test]
async fn post_outreach_execute_without_gtm_engine_returns_503() {
    let mut h = build_harness().await;
    let (status, _) = json_request(
        &mut h.router,
        "POST",
        "/api/dept/gtm/outreach/execute",
        Some(json!({
            "session_id": h.session_id.to_string(),
            "sequence_id": uuid::Uuid::now_v7().to_string(),
            "contact_id": uuid::Uuid::now_v7().to_string(),
        })),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn post_outreach_execute_invalid_session_returns_400() {
    let mut h = build_harness_with_gtm().await;
    let (status, _) = json_request(
        &mut h.router,
        "POST",
        "/api/dept/gtm/outreach/execute",
        Some(json!({
            "session_id": "not-a-uuid",
            "sequence_id": uuid::Uuid::now_v7().to_string(),
            "contact_id": uuid::Uuid::now_v7().to_string(),
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn post_outreach_execute_invalid_sequence_id_returns_400() {
    let mut h = build_harness_with_gtm().await;
    let (status, _) = json_request(
        &mut h.router,
        "POST",
        "/api/dept/gtm/outreach/execute",
        Some(json!({
            "session_id": h.session_id.to_string(),
            "sequence_id": "nope",
            "contact_id": uuid::Uuid::now_v7().to_string(),
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn post_outreach_execute_invalid_contact_id_returns_400() {
    let mut h = build_harness_with_gtm().await;
    let (status, _) = json_request(
        &mut h.router,
        "POST",
        "/api/dept/gtm/outreach/execute",
        Some(json!({
            "session_id": h.session_id.to_string(),
            "sequence_id": uuid::Uuid::now_v7().to_string(),
            "contact_id": "bad",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn post_outreach_execute_unknown_sequence_returns_500() {
    let mut h = build_harness_with_gtm().await;
    let sid = h.session_id;

    let (st, contact_bytes) = json_request(
        &mut h.router,
        "POST",
        "/api/dept/gtm/contacts",
        Some(json!({
            "session_id": sid.to_string(),
            "name": "Zed",
            "email": "zed@example.com",
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let contact_json: serde_json::Value = serde_json::from_slice(&contact_bytes).unwrap();
    let contact_id = contact_json["id"].as_str().unwrap();

    let unknown_seq = uuid::Uuid::now_v7().to_string();
    let (status, _) = json_request(
        &mut h.router,
        "POST",
        "/api/dept/gtm/outreach/execute",
        Some(json!({
            "session_id": sid.to_string(),
            "sequence_id": unknown_seq,
            "contact_id": contact_id,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn post_outreach_execute_unknown_contact_returns_500() {
    let mut h = build_harness_with_gtm().await;
    let sid = h.session_id;

    let ge = h.gtm_engine.as_ref().expect("gtm wired");
    let steps = vec![SequenceStep {
        delay_days: 0,
        channel: "email".into(),
        template: "a".into(),
    }];
    let seq_id = ge
        .outreach()
        .create_sequence(sid, "ghost".into(), steps)
        .await
        .unwrap();
    ge.outreach().activate_sequence(&seq_id).await.unwrap();

    let ghost_contact = uuid::Uuid::now_v7().to_string();
    let (status, _) = json_request(
        &mut h.router,
        "POST",
        "/api/dept/gtm/outreach/execute",
        Some(json!({
            "session_id": sid.to_string(),
            "sequence_id": seq_id.to_string(),
            "contact_id": ghost_contact,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn post_outreach_execute_active_sequence_returns_job_id() {
    let mut h = build_harness_with_gtm().await;
    let sid = h.session_id;

    let (st, contact_bytes) = json_request(
        &mut h.router,
        "POST",
        "/api/dept/gtm/contacts",
        Some(json!({
            "session_id": sid.to_string(),
            "name": "Bob",
            "email": "bob@example.com",
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let contact_json: serde_json::Value = serde_json::from_slice(&contact_bytes).unwrap();
    let contact_id = contact_json["id"].as_str().unwrap();

    let ge = h.gtm_engine.as_ref().expect("gtm wired");
    let steps = vec![SequenceStep {
        delay_days: 0,
        channel: "email".into(),
        template: "intro".into(),
    }];
    let seq_id = ge
        .outreach()
        .create_sequence(sid, "welcome".into(), steps)
        .await
        .unwrap();
    ge.outreach().activate_sequence(&seq_id).await.unwrap();

    let (status, body) = json_request(
        &mut h.router,
        "POST",
        "/api/dept/gtm/outreach/execute",
        Some(json!({
            "session_id": sid.to_string(),
            "sequence_id": seq_id.to_string(),
            "contact_id": contact_id,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["count"], 1);
    assert_eq!(v["job_id"], v["job_ids"][0]);
    assert!(!v["job_id"].as_str().unwrap().is_empty());
}

#[tokio::test]
async fn get_outreach_sequences_without_gtm_engine_returns_503() {
    let mut h = build_harness().await;
    let (status, _) = json_request(
        &mut h.router,
        "GET",
        &format!(
            "/api/dept/gtm/outreach/sequences?session_id={}",
            h.session_id
        ),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn get_outreach_sequences_empty_with_gtm() {
    let mut h = build_harness_with_gtm().await;
    let (status, body) = json_request(
        &mut h.router,
        "GET",
        &format!(
            "/api/dept/gtm/outreach/sequences?session_id={}",
            h.session_id
        ),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v.as_array().map(|a| a.len()), Some(0));
}

#[tokio::test]
async fn post_outreach_sequences_create_and_activate() {
    let mut h = build_harness_with_gtm().await;
    let sid = h.session_id;
    let steps = vec![SequenceStep {
        delay_days: 0,
        channel: "email".into(),
        template: "Hello".into(),
    }];
    let (st, bytes) = json_request(
        &mut h.router,
        "POST",
        "/api/dept/gtm/outreach/sequences",
        Some(json!({
            "session_id": sid.to_string(),
            "name": " drip ",
            "steps": steps,
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let created: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let seq_id = created["id"].as_str().unwrap();

    let (st2, _) = json_request(
        &mut h.router,
        "POST",
        &format!("/api/dept/gtm/outreach/sequences/{seq_id}/activate"),
        Some(json!({ "session_id": sid.to_string() })),
    )
    .await;
    assert_eq!(st2, StatusCode::OK);

    let other_session = uuid::Uuid::now_v7().to_string();
    let (st3, _) = json_request(
        &mut h.router,
        "POST",
        &format!("/api/dept/gtm/outreach/sequences/{seq_id}/activate"),
        Some(json!({ "session_id": other_session })),
    )
    .await;
    assert_eq!(st3, StatusCode::NOT_FOUND);
}
