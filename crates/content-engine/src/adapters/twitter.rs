//! Twitter / X API v2 adapter (`POST /2/tweets`).
//!
//! IMPORTANT: The `twitter_token` config key must be an OAuth 2.0 **user access token**
//! obtained via the PKCE flow with `tweet.write` scope. A bearer-only (app-only) token
//! cannot create tweets and will get HTTP 403.
//!
//! Free tier allows 17 tweets/24h. Basic ($100/mo) allows 100/user/24h.
//! See: <https://docs.x.com/x-api/fundamentals/rate-limits>

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use reqwest::StatusCode;
use rusvel_core::domain::{ContentItem, ContentKind, Platform};
use rusvel_core::error::{Result, RusvelError};
use rusvel_core::ports::ConfigPort;
use serde_json::json;

use crate::platform::{PlatformAdapter, PostMetrics, PublishResult};

const DEFAULT_API_BASE: &str = "https://api.twitter.com/2";

/// Posts tweets and threads via X API v2.
///
/// Required config key:
/// - `twitter_token`: OAuth 2.0 user access token with `tweet.write` scope
pub struct TwitterAdapter {
    config: Arc<dyn ConfigPort>,
    client: reqwest::Client,
    api_base: String,
}

impl TwitterAdapter {
    pub fn new(config: Arc<dyn ConfigPort>) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
            api_base: DEFAULT_API_BASE.to_string(),
        }
    }

    pub fn with_base_url(config: Arc<dyn ConfigPort>, base_url: impl Into<String>) -> Self {
        let base = base_url.into();
        Self {
            config,
            client: reqwest::Client::new(),
            api_base: base.trim_end_matches('/').to_string(),
        }
    }

    fn bearer(&self) -> Result<String> {
        match self.config.get_value("twitter_token")? {
            Some(v) => {
                serde_json::from_value(v).map_err(|e| RusvelError::Serialization(e.to_string()))
            }
            None => Err(RusvelError::Validation(
                "config key `twitter_token` is not set — must be OAuth 2.0 user token with tweet.write scope".into(),
            )),
        }
    }

    async fn post_tweet(&self, token: &str, text: &str, reply_to: Option<&str>) -> Result<String> {
        let mut body = json!({ "text": text });
        if let Some(parent_id) = reply_to {
            body["reply"] = json!({ "in_reply_to_tweet_id": parent_id });
        }
        let url = format!("{}/tweets", self.api_base);
        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {token}"))
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
                "Twitter/X API rate limited (429)".into(),
            ));
        }
        if status == StatusCode::FORBIDDEN {
            return Err(RusvelError::Unauthorized(
                "Twitter 403 — token likely app-only. Need OAuth 2.0 user token with tweet.write scope.".into(),
            ));
        }
        if !status.is_success() {
            let snippet = String::from_utf8_lossy(&body_bytes);
            let short: String = snippet.chars().take(500).collect();
            return Err(RusvelError::Internal(format!(
                "Twitter publish failed: {status} — {short}"
            )));
        }

        let v: serde_json::Value = serde_json::from_slice(&body_bytes)
            .map_err(|e| RusvelError::Serialization(e.to_string()))?;
        Ok(v["data"]["id"]
            .as_str()
            .map(String::from)
            .unwrap_or_else(|| "unknown".into()))
    }

    /// Split thread content into individual tweets.
    /// Expects numbered format: "1/ First tweet\n\n2/ Second tweet\n\n3/ Third tweet"
    /// Falls back to splitting on double newlines if no numbering found.
    fn split_thread(text: &str) -> Vec<String> {
        // Try numbered format: "1/ ...", "2/ ...", etc.
        let mut tweets: Vec<String> = Vec::new();
        let mut current = String::new();
        for line in text.lines() {
            let trimmed = line.trim();
            // Check if line starts with "N/" pattern
            if let Some(rest) = trimmed.strip_prefix(|c: char| c.is_ascii_digit()).and_then(|s| {
                // Handle multi-digit: consume more digits then "/"
                let s = s.trim_start_matches(|c: char| c.is_ascii_digit());
                s.strip_prefix('/')
            }) {
                if !current.is_empty() {
                    tweets.push(current.trim().to_string());
                }
                current = rest.trim().to_string();
            } else if trimmed.is_empty() && !current.is_empty() {
                current.push('\n');
            } else {
                if !current.is_empty() {
                    current.push('\n');
                }
                current.push_str(trimmed);
            }
        }
        if !current.is_empty() {
            tweets.push(current.trim().to_string());
        }

        // If we only got one chunk, try splitting on double newlines
        if tweets.len() <= 1 {
            tweets = text
                .split("\n\n")
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }

        // Enforce 280 char limit per tweet
        tweets.iter().map(|t| t.chars().take(280).collect()).collect()
    }
}

#[async_trait]
impl PlatformAdapter for TwitterAdapter {
    fn platform(&self) -> Platform {
        Platform::Twitter
    }

    async fn publish(&self, content: &ContentItem) -> Result<PublishResult> {
        let token = self.bearer()?;

        // Thread: post multiple tweets chained via reply_to
        if content.kind == ContentKind::Thread {
            let tweets = Self::split_thread(&content.body_markdown);
            if tweets.is_empty() {
                return Err(RusvelError::Validation("Thread has no content".into()));
            }

            let first_id = self.post_tweet(&token, &tweets[0], None).await?;
            let mut last_id = first_id.clone();

            for tweet_text in &tweets[1..] {
                last_id = self.post_tweet(&token, tweet_text, Some(&last_id)).await?;
            }

            return Ok(PublishResult {
                post_id: first_id.clone(),
                url: format!("https://x.com/i/web/status/{first_id}"),
                published_at: Utc::now(),
            });
        }

        // Single tweet
        let text = self.format_content(&content.body_markdown);
        let post_id = self.post_tweet(&token, &text, None).await?;
        Ok(PublishResult {
            post_id: post_id.clone(),
            url: format!("https://x.com/i/web/status/{post_id}"),
            published_at: Utc::now(),
        })
    }

    async fn metrics(&self, _post_id: &str) -> Result<PostMetrics> {
        Err(RusvelError::Internal(
            "Twitter metrics require Basic tier ($100/mo) API access".into(),
        ))
    }

    fn max_length(&self) -> Option<usize> {
        Some(280)
    }

    fn format_content(&self, markdown: &str) -> String {
        markdown.chars().take(280).collect()
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
        token: Mutex<String>,
    }
    impl TestCfg {
        fn new(token: &str) -> Self {
            Self {
                token: Mutex::new(token.into()),
            }
        }
    }
    impl ConfigPort for TestCfg {
        fn get_value(&self, key: &str) -> rusvel_core::error::Result<Option<serde_json::Value>> {
            if key == "twitter_token" {
                let t = self.token.lock().unwrap();
                return Ok(Some(json!(t.as_str())));
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
            kind: ContentKind::Tweet,
            title: "x".into(),
            body_markdown: "short tweet body".into(),
            platform_targets: vec![],
            status: ContentStatus::Draft,
            approval: ApprovalStatus::Approved,
            scheduled_at: None,
            published_at: None,
            metadata: json!({}),
        }
    }

    fn thread_item() -> ContentItem {
        ContentItem {
            id: ContentId::new(),
            session_id: SessionId::new(),
            kind: ContentKind::Thread,
            title: "thread".into(),
            body_markdown: "1/ First tweet about Rust agents\n\n2/ The key insight is deferred tool loading\n\n3/ This saves 85% of tokens".into(),
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
            .and(path("/tweets"))
            .and(header("Authorization", "Bearer tw-secret"))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "data": { "id": "1999888777666555444", "text": "hi" }
            })))
            .mount(&mock_server)
            .await;

        let cfg = Arc::new(TestCfg::new("tw-secret"));
        let adapter = TwitterAdapter::with_base_url(cfg, mock_server.uri());

        let r = adapter.publish(&sample_item()).await.unwrap();
        assert_eq!(r.post_id, "1999888777666555444");
    }

    #[tokio::test]
    async fn publish_thread_chains_replies() {
        let mock_server = MockServer::start().await;
        // Each POST /tweets returns a unique ID; wiremock returns same response
        // but we verify 3 calls are made
        Mock::given(method("POST"))
            .and(path("/tweets"))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "data": { "id": "100", "text": "t" }
            })))
            .expect(3)
            .mount(&mock_server)
            .await;

        let cfg = Arc::new(TestCfg::new("tw-secret"));
        let adapter = TwitterAdapter::with_base_url(cfg, mock_server.uri());

        let r = adapter.publish(&thread_item()).await.unwrap();
        assert_eq!(r.post_id, "100");
    }

    #[tokio::test]
    async fn publish_maps_403_to_auth_error() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/tweets"))
            .respond_with(ResponseTemplate::new(403).set_body_string("forbidden"))
            .mount(&mock_server)
            .await;

        let cfg = Arc::new(TestCfg::new("app-only-token"));
        let adapter = TwitterAdapter::with_base_url(cfg, mock_server.uri());
        let err = adapter.publish(&sample_item()).await.unwrap_err();
        assert!(err.to_string().contains("403") || err.to_string().contains("user token"));
    }

    #[tokio::test]
    async fn publish_maps_429_to_error() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/tweets"))
            .respond_with(ResponseTemplate::new(429))
            .mount(&mock_server)
            .await;

        let cfg = Arc::new(TestCfg::new("t"));
        let adapter = TwitterAdapter::with_base_url(cfg, mock_server.uri());
        let err = adapter.publish(&sample_item()).await.unwrap_err();
        assert!(err.to_string().contains("429") || err.to_string().contains("rate"));
    }

    #[test]
    fn split_thread_numbered_format() {
        let text = "1/ First tweet\n\n2/ Second tweet\n\n3/ Third tweet";
        let tweets = TwitterAdapter::split_thread(text);
        assert_eq!(tweets.len(), 3);
        assert_eq!(tweets[0], "First tweet");
        assert_eq!(tweets[1], "Second tweet");
        assert_eq!(tweets[2], "Third tweet");
    }

    #[test]
    fn split_thread_double_newline_fallback() {
        let text = "First part here\n\nSecond part here\n\nThird part";
        let tweets = TwitterAdapter::split_thread(text);
        assert_eq!(tweets.len(), 3);
    }
}
