//! S-046: cron tick → `ScheduledCron` job; webhook HMAC → persisted event; event trigger → flow run.

mod common;

use std::time::Duration;

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use common::{build_harness, build_harness_with_gtm, json_request};
use hmac::{Hmac, Mac};
use rusvel_core::domain::{EventTrigger, TriggerAction};
use rusvel_core::id::EventId;
use rusvel_event::TriggerManager;
use serde_json::json;
use sha2::Sha256;
use tower::ServiceExt;

type HmacSha256 = Hmac<Sha256>;

const S046_FLOW_ID: &str = "00000000-0000-4000-8000-000000000042";

#[tokio::test]
async fn webhook_hmac_emits_persisted_event() {
    let mut h = build_harness().await;

    let (st, bytes) = json_request(
        &mut h.router,
        "POST",
        "/api/webhooks",
        Some(json!({
            "name": "s046",
            "event_kind": "webhook.s046.persist"
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let wid = v["id"].as_str().unwrap();
    let secret = v["secret"].as_str().unwrap();

    let body = br#"{"ping":true}"#;
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(body);
    let sig = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));

    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/webhooks/{wid}"))
        .header("content-type", "application/json")
        .header("x-rusvel-signature", &sig)
        .body(Body::from(body.as_slice()))
        .unwrap();
    let res = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let out = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let ev: serde_json::Value = serde_json::from_slice(&out).unwrap();
    let eid_str = ev["event_id"].as_str().unwrap();
    let eid = EventId::from_uuid(uuid::Uuid::parse_str(eid_str).unwrap());

    let stored = h.events.get(&eid).await.unwrap().expect("persisted");
    assert_eq!(stored.kind, "webhook.s046.persist");
    assert_eq!(stored.source, "webhook");
}

#[tokio::test]
async fn cron_tick_enqueues_scheduled_cron_job() {
    let mut h = build_harness().await;
    let sid = h.session_id;

    let (st, body) = json_request(
        &mut h.router,
        "POST",
        "/api/cron",
        Some(json!({
            "name": "s046 every second",
            "session_id": sid.to_string(),
            "schedule": "*/1 * * * * * *",
            "payload": { "s046": true },
            "event_kind": "s046.cron.fired",
            "enabled": true,
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let schedule_id = created["id"].as_str().unwrap().to_string();

    let mut found = false;
    for _ in 0..30 {
        let (tick_st, _) = json_request(&mut h.router, "POST", "/api/cron/tick", None).await;
        assert_eq!(tick_st, StatusCode::OK);

        let uri = format!("/api/jobs?session_id={sid}&kinds=ScheduledCron&status=Queued&limit=20");
        let (st2, list_bytes) = json_request(&mut h.router, "GET", &uri, None).await;
        assert_eq!(st2, StatusCode::OK);
        let arr: Vec<serde_json::Value> = serde_json::from_slice(&list_bytes).unwrap();
        if arr.iter().any(|row| {
            row["kind"].as_str() == Some("ScheduledCron")
                && row["payload"]["schedule_id"].as_str() == Some(&schedule_id)
        }) {
            found = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(15)).await;
    }

    assert!(
        found,
        "expected POST /api/cron/tick to enqueue ScheduledCron for schedule {schedule_id}"
    );
}

#[tokio::test]
async fn webhook_event_triggers_flow_execution_via_trigger_manager() {
    let mut h = build_harness_with_gtm().await;

    let (st_tpl, tpl_bytes) = json_request(
        &mut h.router,
        "GET",
        "/api/flows/templates/cross-engine-handoff",
        None,
    )
    .await;
    assert_eq!(st_tpl, StatusCode::OK);
    let flow_def: serde_json::Value = serde_json::from_slice(&tpl_bytes).unwrap();

    let (st_save, _) = json_request(&mut h.router, "POST", "/api/flows", Some(flow_def)).await;
    assert_eq!(st_save, StatusCode::CREATED);

    let tm = TriggerManager::new(h.agent_port.clone(), h.storage.clone(), h.events.clone());
    tm.register_trigger(EventTrigger {
        id: "s046-webhook-flow".into(),
        name: "s046 webhook → flow".into(),
        event_pattern: "webhook.s046.flow_trigger".into(),
        action: TriggerAction::RunFlow {
            flow_id: S046_FLOW_ID.into(),
        },
        department_id: None,
        enabled: true,
        metadata: json!({}),
    });
    tm.start(h.event_bus.subscribe());

    let (st, bytes) = json_request(
        &mut h.router,
        "POST",
        "/api/webhooks",
        Some(json!({
            "name": "s046 flow hook",
            "event_kind": "webhook.s046.flow_trigger"
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let wid = v["id"].as_str().unwrap();
    let secret = v["secret"].as_str().unwrap();

    let body = br#"{"repo":"rusvel"}"#;
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(body);
    let sig = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));

    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/webhooks/{wid}"))
        .header("content-type", "application/json")
        .header("x-rusvel-signature", &sig)
        .body(Body::from(body.as_slice()))
        .unwrap();
    let res = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let mut exec_count = 0usize;
    for _ in 0..40 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let uri = format!("/api/flows/{S046_FLOW_ID}/executions");
        let (st_ex, ex_bytes) = json_request(&mut h.router, "GET", &uri, None).await;
        assert_eq!(st_ex, StatusCode::OK);
        let arr: Vec<serde_json::Value> = serde_json::from_slice(&ex_bytes).unwrap();
        exec_count = arr.len();
        if exec_count > 0 {
            break;
        }
    }

    assert!(
        exec_count > 0,
        "expected event trigger to run flow {S046_FLOW_ID} after webhook emit"
    );
}
