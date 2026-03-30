//! S-044: harvest outcomes list + record API.

mod common;

use axum::http::StatusCode;
use chrono::Utc;
use common::{build_harness, build_harness_with_gtm, json_request};
use gtm_engine::crm::CrmManager;
use gtm_engine::{Deal, DealId, DealStage};
use rusvel_core::domain::{Opportunity, OpportunitySource, OpportunityStage};
use rusvel_core::id::{ContactId, OpportunityId};
use serde_json::json;

#[tokio::test]
async fn get_harvest_outcomes_empty_array() {
    let mut h = build_harness_with_gtm().await;
    let sid = h.session_id.to_string();
    let uri = format!("/api/dept/harvest/outcomes?session_id={sid}");
    let (st, body) = json_request(&mut h.router, "GET", &uri, None).await;
    assert_eq!(st, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(v.is_array());
    assert_eq!(v.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn post_harvest_outcome_404_unknown_opportunity() {
    let mut h = build_harness_with_gtm().await;
    let sid = h.session_id.to_string();
    let (st, _) = json_request(
        &mut h.router,
        "POST",
        "/api/dept/harvest/outcome",
        Some(json!({
            "session_id": sid,
            "opportunity_id": "00000000-0000-0000-0000-000000000099",
            "result": "lost",
            "notes": "test"
        })),
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn harvest_advance_won_records_outcome() {
    let mut h = build_harness().await;
    let sid = h.session_id;
    let opp_id = OpportunityId::new();
    let opp = Opportunity {
        id: opp_id,
        session_id: sid,
        source: OpportunitySource::Manual,
        title: "t".into(),
        url: None,
        description: "d".into(),
        score: 0.5,
        stage: OpportunityStage::Cold,
        value_estimate: None,
        platform_job_key: None,
        content_hash: None,
        upstream_score: None,
        budget_min: None,
        budget_max: None,
        budget_currency: None,
        hourly: None,
        client_hire_rate: None,
        client_total_spent: None,
        payment_verified: None,
        proposal_count: None,
        metadata: json!({}),
    };
    h.storage
        .objects()
        .put(
            "opportunity",
            &opp_id.to_string(),
            serde_json::to_value(&opp).unwrap(),
        )
        .await
        .unwrap();

    let (st, _) = json_request(
        &mut h.router,
        "POST",
        "/api/dept/harvest/advance",
        Some(json!({
            "session_id": sid.to_string(),
            "opportunity_id": opp_id.to_string(),
            "stage": "Won"
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);

    let uri = format!("/api/dept/harvest/outcomes?session_id={}", sid);
    let (st2, body) = json_request(&mut h.router, "GET", &uri, None).await;
    assert_eq!(st2, StatusCode::OK);
    let v: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    assert_eq!(v.len(), 1);
}

#[tokio::test]
async fn gtm_deal_won_with_opportunity_id_records_harvest_outcome() {
    let mut h = build_harness_with_gtm().await;
    let sid = h.session_id;
    let opp_id = OpportunityId::new();
    let opp = Opportunity {
        id: opp_id,
        session_id: sid,
        source: OpportunitySource::Manual,
        title: "t".into(),
        url: None,
        description: "d".into(),
        score: 0.5,
        stage: OpportunityStage::Cold,
        value_estimate: None,
        platform_job_key: None,
        content_hash: None,
        upstream_score: None,
        budget_min: None,
        budget_max: None,
        budget_currency: None,
        hourly: None,
        client_hire_rate: None,
        client_total_spent: None,
        payment_verified: None,
        proposal_count: None,
        metadata: json!({}),
    };
    h.storage
        .objects()
        .put(
            "opportunity",
            &opp_id.to_string(),
            serde_json::to_value(&opp).unwrap(),
        )
        .await
        .unwrap();

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
    let contact_id =
        ContactId::from_uuid(uuid::Uuid::parse_str(contact_json["id"].as_str().unwrap()).unwrap());

    let deal_id = DealId::new();
    let deal = Deal {
        id: deal_id,
        session_id: sid,
        contact_id,
        title: "Deal".into(),
        value: 1.0,
        stage: DealStage::Lead,
        notes: String::new(),
        created_at: Utc::now(),
        metadata: json!({}),
    };
    let crm = CrmManager::new(h.storage.clone());
    crm.add_deal(sid, deal).await.unwrap();

    let (st, _) = json_request(
        &mut h.router,
        "POST",
        "/api/dept/gtm/deals/advance",
        Some(json!({
            "session_id": sid.to_string(),
            "deal_id": deal_id.to_string(),
            "stage": "Won",
            "opportunity_id": opp_id.to_string(),
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);

    let uri = format!("/api/dept/harvest/outcomes?session_id={}", sid);
    let (st2, body) = json_request(&mut h.router, "GET", &uri, None).await;
    assert_eq!(st2, StatusCode::OK);
    let v: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    assert_eq!(v.len(), 1);
}
