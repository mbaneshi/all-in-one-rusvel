mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use common::build_harness_with_auth;
use rusvel_api::auth::AuthConfig;
use tower::ServiceExt;

fn req(uri: &str, token: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder()
        .uri(uri)
        .header("content-type", "application/json");
    if let Some(t) = token {
        builder = builder.header("authorization", format!("Bearer {t}"));
    }
    builder.body(Body::empty()).unwrap()
}

#[tokio::test]
async fn auth_no_env_passes_all() {
    let h = build_harness_with_auth(AuthConfig {
        token: None,
        read_token: None,
    })
    .await;
    let res = h.router.oneshot(req("/api/sessions", None)).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn auth_valid_token_200() {
    let h = build_harness_with_auth(AuthConfig {
        token: Some("test-tok-200".into()),
        read_token: None,
    })
    .await;
    let res = h
        .router
        .oneshot(req("/api/sessions", Some("test-tok-200")))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn auth_invalid_token_401() {
    let h = build_harness_with_auth(AuthConfig {
        token: Some("test-tok-401".into()),
        read_token: None,
    })
    .await;
    let res = h
        .router
        .oneshot(req("/api/sessions", Some("wrong")))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn auth_missing_token_401() {
    let h = build_harness_with_auth(AuthConfig {
        token: Some("test-tok-miss".into()),
        read_token: None,
    })
    .await;
    let res = h.router.oneshot(req("/api/sessions", None)).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn auth_health_always_exempt() {
    let h = build_harness_with_auth(AuthConfig {
        token: Some("test-tok-health".into()),
        read_token: None,
    })
    .await;
    let res = h.router.oneshot(req("/api/health", None)).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}
