//! LinkedIn REST Posts API adapter (`POST /rest/posts`).
//!
//! Migrated from deprecated `/v2/ugcPosts` to the current REST Posts API.
//! See: <https://learn.microsoft.com/en-us/linkedin/marketing/community-management/shares/posts-api>

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use reqwest::StatusCode;
use rusvel_core::domain::{ContentItem, Platform};
use rusvel_core::error::{Result, RusvelError};
use rusvel_core::ports::ConfigPort;
use serde_json::json;

use crate::platform::{PlatformAdapter, PostMetrics, PublishResult};

const DEFAULT_API_BASE: &str = "https://api.linkedin.com";

/// Publishes posts via LinkedIn's REST Posts API.
///
/// Required config keys:
/// - `linkedin_token`: OAuth 2.0 access token with `w_member_social` scope
/// - `linkedin_person_id`: Your LinkedIn person URN ID (from `/v2/userinfo` `sub` field)
pub struct LinkedInAdapter {
    config: Arc<dyn ConfigPort>,
    client: reqwest::Client,
    api_base: String,
}

impl LinkedInAdapter {
    pub fn new(config: Arc<dyn ConfigPort>) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
            api_base: DEFAULT_API_BASE.to_string(),
        }
    }

    /// Override API base URL (for tests with mock HTTP).
    pub fn with_base_url(config: Arc<dyn ConfigPort>, base_url: impl Into<String>) -> Self {
        let base = base_url.into();
        Self {
            config,
            client: reqwest::Client::new(),
            api_base: base.trim_end_matches('/').to_string(),
        }
    }

    fn bearer(&self) -> Result<String> {
        match self.config.get_value("linkedin_token")? {
            Some(v) => {
                serde_json::from_value(v).map_err(|e| RusvelError::Serialization(e.to_string()))
            }
            None => Err(RusvelError::Validation(
                "config key `linkedin_token` is not set".into(),
            )),
        }
    }

    fn person_urn(&self) -> Result<String> {
        match self.config.get_value("linkedin_person_id")? {
            Some(v) => {
                let id: String = serde_json::from_value(v)
                    .map_err(|e| RusvelError::Serialization(e.to_string()))?;
                Ok(format!("urn:li:person:{id}"))
            }
            None => Err(RusvelError::Validation(
                "config key `linkedin_person_id` is not set — get it from GET /v2/userinfo `sub` field".into(),
            )),
        }
    }

    fn api_version() -> String {
        let now = Utc::now();
        format!("{}{:02}", now.format("%Y"), now.format("%m"))
    }
}

#[async_trait]
impl PlatformAdapter for LinkedInAdapter {
    fn platform(&self) -> Platform {
        Platform::LinkedIn
    }

    async fn publish(&self, content: &ContentItem) -> Result<PublishResult> {
        let token = self.bearer()?;
        let author = self.person_urn()?;
        let text = self.format_content(&content.body_markdown);
        let body = json!({
            "author": author,
            "commentary": text,
            "visibility": "PUBLIC",
            "distribution": {
                "feedDistribution": "MAIN_FEED",
                "targetEntities": [],
                "thirdPartyDistributionChannels": []
            },
            "lifecycleState": "PUBLISHED",
            "isReshareDisabledByAuthor": false
        });
        let url = format!("{}/rest/posts", self.api_base);
        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {token}"))
            .header("X-Restli-Protocol-Version", "2.0.0")
            .header("LinkedIn-Version", Self::api_version())
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| RusvelError::Storage(e.to_string()))?;

        let status = resp.status();
        // Post ID comes from x-restli-id response header on the new API
        let post_id = resp
            .headers()
            .get("x-restli-id")
            .and_then(|v| v.to_str().ok())
            .map(String::from);

        let body_bytes = resp
            .bytes()
            .await
            .map_err(|e| RusvelError::Storage(e.to_string()))?;

        if status == StatusCode::TOO_MANY_REQUESTS {
            return Err(RusvelError::Internal(
                "LinkedIn API rate limited (429)".into(),
            ));
        }
        if !status.is_success() {
            let snippet = String::from_utf8_lossy(&body_bytes);
            let short: String = snippet.chars().take(500).collect();
            return Err(RusvelError::Internal(format!(
                "LinkedIn publish failed: {status} — {short}"
            )));
        }

        // Fallback: try response body if header missing
        let post_id = post_id
            .or_else(|| {
                serde_json::from_slice::<serde_json::Value>(&body_bytes)
                    .ok()
                    .and_then(|v| v["id"].as_str().map(String::from))
            })
            .unwrap_or_else(|| "unknown".into());

        Ok(PublishResult {
            post_id: post_id.clone(),
            url: format!("https://www.linkedin.com/feed/update/{post_id}"),
            published_at: Utc::now(),
        })
    }

    async fn metrics(&self, _post_id: &str) -> Result<PostMetrics> {
        // LinkedIn member post analytics require `r_member_social` scope which
        // needs separate LinkedIn approval. Use Shield ($25/mo) for analytics.
        Err(RusvelError::Internal(
            "LinkedIn metrics require r_member_social scope (restricted). Use Shield for analytics.".into(),
        ))
    }

    fn max_length(&self) -> Option<usize> {
        Some(3000)
    }

    fn format_content(&self, markdown: &str) -> String {
        markdown.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusvel_core::domain::{ApprovalStatus, ContentKind, ContentStatus};
    use rusvel_core::id::{ContentId, SessionId};
    use serde_json::json;
    use std::sync::Mutex;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    struct TestCfg {
        values: Mutex<std::collections::HashMap<String, serde_json::Value>>,
    }
    impl TestCfg {
        fn new(token: &str, person_id: &str) -> Self {
            let mut m = std::collections::HashMap::new();
            m.insert("linkedin_token".into(), json!(token));
            m.insert("linkedin_person_id".into(), json!(person_id));
            Self {
                values: Mutex::new(m),
            }
        }
    }
    impl ConfigPort for TestCfg {
        fn get_value(&self, key: &str) -> rusvel_core::error::Result<Option<serde_json::Value>> {
            let map = self.values.lock().unwrap();
            Ok(map.get(key).cloned())
        }
        fn set_value(&self, _: &str, _: serde_json::Value) -> rusvel_core::error::Result<()> {
            Ok(())
        }
    }

    fn sample_item() -> ContentItem {
        ContentItem {
            id: ContentId::new(),
            session_id: SessionId::new(),
            kind: ContentKind::LinkedInPost,
            title: "Hi".into(),
            body_markdown: "Hello **world**".into(),
            platform_targets: vec![],
            status: ContentStatus::Draft,
            approval: ApprovalStatus::Approved,
            scheduled_at: None,
            published_at: None,
            metadata: json!({}),
        }
    }

    #[tokio::test]
    async fn publish_success_against_mock_server() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/rest/posts"))
            .and(header("Authorization", "Bearer test-token"))
            .respond_with(
                ResponseTemplate::new(201)
                    .append_header("x-restli-id", "urn:li:share:123456")
                    .set_body_string(""),
            )
            .mount(&mock_server)
            .await;

        let cfg = Arc::new(TestCfg::new("test-token", "abc123"));
        let adapter = LinkedInAdapter::with_base_url(cfg, mock_server.uri());

        let item = sample_item();
        let r = adapter.publish(&item).await.unwrap();
        assert!(r.post_id.contains("123456"));
        assert!(r.url.contains("linkedin.com"));
    }

    #[tokio::test]
    async fn publish_falls_back_to_body_id() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/rest/posts"))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "id": "urn:li:share:789"
            })))
            .mount(&mock_server)
            .await;

        let cfg = Arc::new(TestCfg::new("t", "person1"));
        let adapter = LinkedInAdapter::with_base_url(cfg, mock_server.uri());
        let r = adapter.publish(&sample_item()).await.unwrap();
        assert!(r.post_id.contains("789"));
    }

    #[tokio::test]
    async fn publish_maps_429_to_error() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/rest/posts"))
            .respond_with(ResponseTemplate::new(429).set_body_string("slow down"))
            .mount(&mock_server)
            .await;

        let cfg = Arc::new(TestCfg::new("t", "p"));
        let adapter = LinkedInAdapter::with_base_url(cfg, mock_server.uri());
        let err = adapter.publish(&sample_item()).await.unwrap_err();
        assert!(err.to_string().contains("429") || err.to_string().contains("rate"));
    }

    #[tokio::test]
    async fn missing_person_id_returns_error() {
        struct TokenOnlyCfg;
        impl ConfigPort for TokenOnlyCfg {
            fn get_value(
                &self,
                key: &str,
            ) -> rusvel_core::error::Result<Option<serde_json::Value>> {
                if key == "linkedin_token" {
                    return Ok(Some(json!("tok")));
                }
                Ok(None)
            }
            fn set_value(&self, _: &str, _: serde_json::Value) -> rusvel_core::error::Result<()> {
                Ok(())
            }
        }
        let adapter = LinkedInAdapter::new(Arc::new(TokenOnlyCfg));
        let err = adapter.publish(&sample_item()).await.unwrap_err();
        assert!(err.to_string().contains("linkedin_person_id"));
    }
}
