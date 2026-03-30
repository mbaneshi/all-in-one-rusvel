use axum::{
    body::Body,
    http::{Extensions, HeaderValue, Request, Response},
    middleware::Next,
};
use tracing::Instrument;
use uuid::Uuid;

/// Stored on [`rusvel_core::domain::Job::metadata`] when a job is enqueued from an HTTP request.
pub const HTTP_REQUEST_ID_METADATA_KEY: &str = "http_request_id";

/// Merge `http_request_id` from request extensions (set by [`request_id_middleware`]) into job metadata.
pub(crate) fn merge_http_request_metadata(
    extensions: &Extensions,
    metadata: serde_json::Value,
) -> serde_json::Value {
    let rid = extensions.get::<String>().map(|s| s.as_str());
    merge_http_request_id(metadata, rid)
}

pub(crate) fn merge_http_request_id(
    metadata: serde_json::Value,
    http_request_id: Option<&str>,
) -> serde_json::Value {
    let mut obj = match metadata {
        serde_json::Value::Object(m) => m,
        _ => serde_json::Map::new(),
    };
    if let Some(rid) = http_request_id {
        if !rid.is_empty() {
            obj.insert(
                HTTP_REQUEST_ID_METADATA_KEY.to_string(),
                serde_json::Value::String(rid.to_string()),
            );
        }
    }
    serde_json::Value::Object(obj)
}

pub async fn request_id_middleware(mut req: Request<Body>, next: Next) -> Response<Body> {
    let request_id = Uuid::now_v7().to_string();
    req.extensions_mut().insert(request_id.clone());

    let span = tracing::info_span!("request", request_id = %request_id);
    let mut response = next.run(req).instrument(span).await;

    if let Ok(val) = HeaderValue::from_str(&request_id) {
        response.headers_mut().insert("x-request-id", val);
    }
    response
}
