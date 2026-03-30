//! Platform-specific URL routing and JSON normalization for passive CDP capture.

pub mod freelancer;
pub mod upwork;

use rusvel_core::domain::BrowserEvent;

/// Map a captured JSON response to zero or more [`BrowserEvent::DataCaptured`] values.
pub fn route_json_response(url: &str, body: &serde_json::Value, tab_id: &str) -> Vec<BrowserEvent> {
    let mut out = Vec::new();
    if upwork::matches_capture_url(url) {
        out.extend(upwork::events_from_response(url, body, tab_id));
    }
    if freelancer::matches_capture_url(url) {
        out.extend(freelancer::events_from_response(url, body, tab_id));
    }
    out
}
