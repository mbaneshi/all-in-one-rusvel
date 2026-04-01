//! DEV.to Articles API adapter (`POST /api/articles`).

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use reqwest::StatusCode;
use rusvel_core::domain::{ContentItem, Platform};
use rusvel_core::error::{Result, RusvelError};
use rusvel_core::ports::ConfigPort;
use serde_json::json;

use crate::platform::{PlatformAdapter, PostMetrics, PublishResult};

const DEFAULT_API_BASE: &str = "https://dev.to/api";

/// Publishes articles to DEV.to using an API key from [`ConfigPort`] key `devto_api_key`.
pub struct DevToAdapter {
    config: Arc<dyn ConfigPort>,
    client: reqwest::Client,
    api_base: String,
}

impl DevToAdapter {
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

    fn api_key(&self) -> Result<String> {
        match self.config.get_value("devto_api_key")? {
            Some(v) => {
                serde_json::from_value(v).map_err(|e| RusvelError::Serialization(e.to_string()))
            }
            None => Err(RusvelError::Validation(
                "config key `devto_api_key` is not set".into(),
            )),
        }
    }

    fn extract_tags(content: &ContentItem) -> Vec<String> {
        content
            .metadata
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| t.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[async_trait]
impl PlatformAdapter for DevToAdapter {
    fn platform(&self) -> Platform {
        Platform::DevTo
    }

    async fn publish(&self, content: &ContentItem) -> Result<PublishResult> {
        let key = self.api_key()?;
        let body_md = self.format_content(&content.body_markdown);
        let tags = Self::extract_tags(content);
        let canonical_url = content
            .metadata
            .get("canonical_url")
            .and_then(|v| v.as_str());
        let mut article = json!({
            "title": content.title,
            "body_markdown": body_md,
            "published": true,
            "tags": tags,
        });
        if let Some(url) = canonical_url {
            article["canonical_url"] = json!(url);
        }
        let body = json!({ "article": article });
        let url = format!("{}/articles", self.api_base);
        let resp = self
            .client
            .post(&url)
            .header("api-key", &key)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| RusvelError::Storage(e.to_string()))?;

        let status = resp.status();
        let body_bytes = resp
            .bytes()
            .await
            .map_err(|e| RusvelError::Storage(e.to_string()))?;

        if status == StatusCode::TOO_MANY_REQUESTS {
            return Err(RusvelError::Internal(
                "DEV.to API rate limited (429)".into(),
            ));
        }
        if status == StatusCode::UNPROCESSABLE_ENTITY {
            let snippet = String::from_utf8_lossy(&body_bytes);
            let short: String = snippet.chars().take(500).collect();
            return Err(RusvelError::Validation(format!(
                "DEV.to rejected article: {short}"
            )));
        }
        if !status.is_success() {
            let snippet = String::from_utf8_lossy(&body_bytes);
            let short: String = snippet.chars().take(500).collect();
            return Err(RusvelError::Internal(format!(
                "DEV.to publish failed: {status} — {short}"
            )));
        }

        let v: serde_json::Value = serde_json::from_slice(&body_bytes)
            .map_err(|e| RusvelError::Serialization(e.to_string()))?;
        let article_id = v["id"]
            .as_u64()
            .map(|n| n.to_string())
            .unwrap_or_else(|| "unknown".into());
        let article_url = v["url"]
            .as_str()
            .map(String::from)
            .unwrap_or_else(|| format!("https://dev.to/article/{article_id}"));

        Ok(PublishResult {
            post_id: article_id,
            url: article_url,
            published_at: Utc::now(),
        })
    }

    async fn metrics(&self, post_id: &str) -> Result<PostMetrics> {
        let key = self.api_key()?;
        let url = format!("{}/articles/{post_id}", self.api_base);
        let resp = self
            .client
            .get(&url)
            .header("api-key", &key)
            .send()
            .await
            .map_err(|e| RusvelError::Storage(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(RusvelError::Internal(format!(
                "DEV.to metrics fetch failed: {}",
                resp.status()
            )));
        }

        let v: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| RusvelError::Serialization(e.to_string()))?;

        Ok(PostMetrics {
            views: v["page_views_count"].as_u64().unwrap_or(0),
            likes: v["public_reactions_count"].as_u64().unwrap_or(0),
            comments: v["comments_count"].as_u64().unwrap_or(0),
            shares: 0,
        })
    }

    fn max_length(&self) -> Option<usize> {
        Some(100_000)
    }

    fn format_content(&self, markdown: &str) -> String {
        // DEV.to accepts markdown natively
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
        key: Mutex<String>,
    }
    impl TestCfg {
        fn new(key: &str) -> Self {
            Self {
                key: Mutex::new(key.into()),
            }
        }
    }
    impl ConfigPort for TestCfg {
        fn get_value(&self, key: &str) -> rusvel_core::error::Result<Option<serde_json::Value>> {
            if key == "devto_api_key" {
                let k = self.key.lock().unwrap();
                return Ok(Some(json!(k.as_str())));
            }
            Ok(None)
        }
        fn set_value(&self, _: &str, _: serde_json::Value) -> rusvel_core::error::Result<()> {
            Ok(())
        }
    }

    fn sample_item() -> ContentItem {
        ContentItem {
            id: ContentId::new(),
            session_id: SessionId::new(),
            kind: ContentKind::Blog,
            title: "My Rust Article".into(),
            body_markdown: "# Hello\n\nThis is a test article about Rust.".into(),
            platform_targets: vec![],
            status: ContentStatus::Draft,
            approval: ApprovalStatus::Approved,
            scheduled_at: None,
            published_at: None,
            metadata: json!({"tags": ["rust", "programming"]}),
        }
    }

    #[tokio::test]
    async fn publish_success_against_mock_server() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/articles"))
            .and(header("api-key", "test-key-123"))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "id": 42,
                "url": "https://dev.to/user/my-rust-article-abc"
            })))
            .mount(&mock_server)
            .await;

        let cfg = Arc::new(TestCfg::new("test-key-123"));
        let adapter = DevToAdapter::with_base_url(cfg, mock_server.uri());

        let item = sample_item();
        let r = adapter.publish(&item).await.unwrap();
        assert_eq!(r.post_id, "42");
        assert!(r.url.contains("dev.to"));
    }

    #[tokio::test]
    async fn publish_maps_429_to_error() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/articles"))
            .respond_with(ResponseTemplate::new(429).set_body_string("slow down"))
            .mount(&mock_server)
            .await;

        let cfg = Arc::new(TestCfg::new("k"));
        let adapter = DevToAdapter::with_base_url(cfg, mock_server.uri());
        let err = adapter.publish(&sample_item()).await.unwrap_err();
        assert!(err.to_string().contains("429") || err.to_string().contains("rate"));
    }

    #[tokio::test]
    async fn publish_maps_422_to_validation_error() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/articles"))
            .respond_with(
                ResponseTemplate::new(422).set_body_string(r#"{"error":"Title is too short"}"#),
            )
            .mount(&mock_server)
            .await;

        let cfg = Arc::new(TestCfg::new("k"));
        let adapter = DevToAdapter::with_base_url(cfg, mock_server.uri());
        let err = adapter.publish(&sample_item()).await.unwrap_err();
        assert!(err.to_string().contains("Title is too short"));
    }

    #[tokio::test]
    async fn metrics_success_against_mock_server() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/articles/42"))
            .and(header("api-key", "k"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "page_views_count": 1500,
                "public_reactions_count": 42,
                "comments_count": 7
            })))
            .mount(&mock_server)
            .await;

        let cfg = Arc::new(TestCfg::new("k"));
        let adapter = DevToAdapter::with_base_url(cfg, mock_server.uri());
        let m = adapter.metrics("42").await.unwrap();
        assert_eq!(m.views, 1500);
        assert_eq!(m.likes, 42);
        assert_eq!(m.comments, 7);
    }

    #[tokio::test]
    async fn missing_api_key_returns_error() {
        struct EmptyCfg;
        impl ConfigPort for EmptyCfg {
            fn get_value(&self, _: &str) -> rusvel_core::error::Result<Option<serde_json::Value>> {
                Ok(None)
            }
            fn set_value(&self, _: &str, _: serde_json::Value) -> rusvel_core::error::Result<()> {
                Ok(())
            }
        }
        let adapter = DevToAdapter::new(Arc::new(EmptyCfg));
        let err = adapter.publish(&sample_item()).await.unwrap_err();
        assert!(err.to_string().contains("devto_api_key"));
    }
}
