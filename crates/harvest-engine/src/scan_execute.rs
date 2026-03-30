//! Unified harvest scan execution for HTTP API, jobs, forge pipeline, and agent tools.

use std::sync::Arc;

use rusvel_core::id::SessionId;
use rusvel_core::ports::BrowserPort;
use rusvel_core::{Opportunity, Result, RusvelError};

use crate::HarvestEngine;
use crate::cdp_source::CdpSource;
use crate::source::{FreelancerRssSource, MockSource, UpworkRssSource};

/// Parameters for one or more harvest sources (mirrors HTTP `HarvestScanRequest` / job payload).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct HarvestScanParams {
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub cdp_extract_js: Option<String>,
    /// Override CDP HTTP base (e.g. `http://127.0.0.1:9223`) for multi-profile Chrome.
    #[serde(default)]
    pub cdp_endpoint: Option<String>,
}

impl HarvestScanParams {
    /// When `sources` is empty: use mock-only scan (job/API default).
    pub fn default_mock() -> Self {
        Self {
            sources: vec!["mock".into()],
            query: String::new(),
            cdp_extract_js: None,
            cdp_endpoint: None,
        }
    }

    /// Parse job `payload` with the same defaults as `HarvestScan` worker.
    pub fn from_job_payload(payload: &serde_json::Value) -> Self {
        let sources: Vec<String> = payload
            .get("sources")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect::<Vec<_>>()
            })
            .filter(|v: &Vec<String>| !v.is_empty())
            .unwrap_or_else(|| vec!["mock".into()]);
        let query = payload
            .get("query")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let cdp_extract_js = payload
            .get("cdp_extract_js")
            .and_then(|v| {
                if v.is_null() {
                    None
                } else {
                    v.as_str().map(String::from)
                }
            })
            .or_else(|| {
                payload
                    .get("cdpExtractJs")
                    .and_then(|v| v.as_str().map(String::from))
            });
        let cdp_endpoint = payload
            .get("cdp_endpoint")
            .or_else(|| payload.get("cdpEndpoint"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from);
        Self {
            sources,
            query,
            cdp_extract_js,
            cdp_endpoint,
        }
    }
}

/// Run all configured sources and concatenate persisted opportunities (same semantics as HTTP harvest scan).
pub async fn scan_from_params(
    engine: &HarvestEngine,
    session_id: &SessionId,
    params: &HarvestScanParams,
    browser: Option<Arc<dyn BrowserPort>>,
) -> Result<Vec<Opportunity>> {
    let skills: Vec<String> = engine.harvest_skills().iter().cloned().collect();
    let mut all = Vec::new();

    for s in &params.sources {
        match s.to_lowercase().as_str() {
            "mock" => {
                let src = MockSource::new();
                let mut v = engine.scan(session_id, &src).await?;
                all.append(&mut v);
            }
            "cdp" => {
                if params.query.trim().is_empty() {
                    return Err(RusvelError::Validation(
                        "cdp source requires `query` (listing page URL)".into(),
                    ));
                }
                let endpoint = params
                    .cdp_endpoint
                    .clone()
                    .or_else(|| std::env::var("RUSVEL_CDP_ENDPOINT").ok())
                    .unwrap_or_else(|| "http://127.0.0.1:9222".into());
                let mut src = CdpSource::new(browser.clone(), endpoint, params.query.clone());
                if let Some(js) = params
                    .cdp_extract_js
                    .as_deref()
                    .map(str::trim)
                    .filter(|x| !x.is_empty())
                {
                    src = src.with_extract_js(js.to_string());
                }
                let mut v = engine.scan(session_id, &src).await?;
                all.append(&mut v);
            }
            "upwork" => {
                let src = UpworkRssSource::new(params.query.clone(), skills.clone())
                    .map_err(|e| RusvelError::Validation(e.to_string()))?;
                let mut v = engine.scan(session_id, &src).await?;
                all.append(&mut v);
            }
            "freelancer" => {
                let src = FreelancerRssSource::new(params.query.clone(), skills.clone())
                    .map_err(|e| RusvelError::Validation(e.to_string()))?;
                let mut v = engine.scan(session_id, &src).await?;
                all.append(&mut v);
            }
            other => {
                return Err(RusvelError::Validation(format!(
                    "unknown harvest source: {other}"
                )));
            }
        }
    }

    Ok(all)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_job_payload_empty_defaults_to_mock() {
        let p = HarvestScanParams::from_job_payload(&serde_json::json!({}));
        assert_eq!(p.sources, vec!["mock".to_string()]);
        assert!(p.query.is_empty());
        assert!(p.cdp_extract_js.is_none());
        assert!(p.cdp_endpoint.is_none());
    }

    #[test]
    fn from_job_payload_reads_sources_and_query() {
        let p = HarvestScanParams::from_job_payload(&serde_json::json!({
            "sources": ["upwork", "mock"],
            "query": "rust",
            "cdp_extract_js": "return '[]';"
        }));
        assert_eq!(p.sources, vec!["upwork", "mock"]);
        assert_eq!(p.query, "rust");
        assert_eq!(p.cdp_extract_js.as_deref(), Some("return '[]';"));
    }

    #[test]
    fn from_job_payload_reads_cdp_endpoint() {
        let p = HarvestScanParams::from_job_payload(&serde_json::json!({
            "cdp_endpoint": "http://127.0.0.1:9223"
        }));
        assert_eq!(p.cdp_endpoint.as_deref(), Some("http://127.0.0.1:9223"));
    }
}
