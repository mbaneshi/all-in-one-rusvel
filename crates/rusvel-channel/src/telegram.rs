//! Telegram Bot API (`sendMessage`) adapter.
//!
//! **Payload** for [`super::ChannelPort::send_message`]:
//! - `text` (string, required) — message body; also accepts `message` as an alias.
//! - `chat_id` (string or number, optional) — overrides [`Self::default_chat_id`].
//!
//! Environment:
//! - `RUSVEL_TELEGRAM_BOT_TOKEN` — required for [`Self::from_env`] to return `Some`.
//! - `RUSVEL_TELEGRAM_CHAT_ID` — default destination when `chat_id` is omitted from payload.

use std::sync::Arc;

use async_trait::async_trait;
use rusvel_core::error::{Result, RusvelError};
use rusvel_core::id::SessionId;
use serde_json::Value;

use rusvel_core::ports::ChannelPort;

/// Sends messages via `https://api.telegram.org/bot<token>/sendMessage`.
pub struct TelegramChannel {
    token: String,
    default_chat_id: Option<String>,
    client: reqwest::Client,
}

impl TelegramChannel {
    /// Returns `None` when `RUSVEL_TELEGRAM_BOT_TOKEN` is unset or empty.
    #[must_use]
    pub fn from_env() -> Option<Arc<Self>> {
        let token = std::env::var("RUSVEL_TELEGRAM_BOT_TOKEN")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())?;
        let default_chat_id = std::env::var("RUSVEL_TELEGRAM_CHAT_ID")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        Some(Arc::new(Self {
            token,
            default_chat_id,
            client: reqwest::Client::new(),
        }))
    }
}

fn extract_text(payload: &Value) -> Result<String> {
    if let Some(s) = payload.get("text").and_then(|v| v.as_str()) {
        return Ok(s.to_string());
    }
    if let Some(s) = payload.get("message").and_then(|v| v.as_str()) {
        return Ok(s.to_string());
    }
    Err(RusvelError::Validation(
        "telegram payload needs `text` or `message`".into(),
    ))
}

fn extract_chat_id(payload: &Value, default: Option<&str>) -> Result<String> {
    if let Some(v) = payload.get("chat_id") {
        if let Some(s) = v.as_str() {
            return Ok(s.to_string());
        }
        if let Some(n) = v.as_i64() {
            return Ok(n.to_string());
        }
        if let Some(n) = v.as_u64() {
            return Ok(n.to_string());
        }
    }
    default.map(String::from).ok_or_else(|| {
        RusvelError::Validation("telegram: missing chat_id and RUSVEL_TELEGRAM_CHAT_ID".into())
    })
}

#[async_trait]
impl ChannelPort for TelegramChannel {
    fn channel_kind(&self) -> &'static str {
        "telegram"
    }

    async fn send_message(&self, session_id: &SessionId, payload: Value) -> Result<()> {
        let text = extract_text(&payload)?;
        let chat_id = extract_chat_id(&payload, self.default_chat_id.as_deref())?;
        let url = format!("https://api.telegram.org/bot{}/sendMessage", self.token);
        let body = serde_json::json!({
            "chat_id": chat_id,
            "text": format!("[session {session_id}] {text}"),
        });
        let resp = self
            .client
            .post(url)
            .json(&body)
            .send()
            .await
            .map_err(|e| RusvelError::Storage(e.to_string()))?;
        if !resp.status().is_success() {
            let txt = resp.text().await.unwrap_or_default();
            return Err(RusvelError::Validation(format!("telegram API: {txt}")));
        }
        Ok(())
    }
}
