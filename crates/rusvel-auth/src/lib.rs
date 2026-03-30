//! In-memory credential store implementing [`AuthPort`] (Phase 0).
//!
//! Credentials can be pre-loaded from environment variables named
//! `RUSVEL_KEY_<NAME>` (treated as API keys for provider `"env"`).

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use rusvel_core::domain::{Credential, CredentialKind};
use rusvel_core::error::{Result, RusvelError};
use rusvel_core::ports::AuthPort;

/// Simple in-memory credential store.
pub struct InMemoryAuthAdapter {
    store: Mutex<HashMap<String, Credential>>,
}

impl InMemoryAuthAdapter {
    /// Create an empty store.
    pub fn new() -> Self {
        Self {
            store: Mutex::new(HashMap::new()),
        }
    }

    /// Create a store pre-populated with `RUSVEL_KEY_*` env vars.
    ///
    /// Each `RUSVEL_KEY_FOO` becomes a credential keyed `"foo"` (lowercased)
    /// with provider `"env"` and kind `ApiKey`.
    pub fn from_env() -> Self {
        let mut map = HashMap::new();
        for (k, _v) in std::env::vars() {
            if let Some(name) = k.strip_prefix("RUSVEL_KEY_") {
                let cred = Credential {
                    provider: "env".into(),
                    kind: CredentialKind::ApiKey,
                    expires_at: None,
                    metadata: serde_json::json!({ "source": "env" }),
                };
                map.insert(name.to_lowercase(), cred);
            }
        }
        Self {
            store: Mutex::new(map),
        }
    }
}

impl Default for InMemoryAuthAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AuthPort for InMemoryAuthAdapter {
    async fn store_credential(&self, key: &str, credential: Credential) -> Result<()> {
        self.store
            .lock()
            .unwrap()
            .insert(key.to_owned(), credential);
        Ok(())
    }

    async fn get_credential(&self, key: &str) -> Result<Option<Credential>> {
        Ok(self.store.lock().unwrap().get(key).cloned())
    }

    async fn refresh(&self, key: &str) -> Result<Credential> {
        self.store
            .lock()
            .unwrap()
            .get(key)
            .cloned()
            .ok_or_else(|| RusvelError::NotFound {
                kind: "credential".into(),
                id: key.into(),
            })
    }

    async fn delete_credential(&self, key: &str) -> Result<()> {
        self.store.lock().unwrap().remove(key);
        Ok(())
    }

    async fn list_credential_keys(&self) -> Result<Vec<String>> {
        Ok(self
            .store
            .lock()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn store_and_retrieve() {
        let auth = InMemoryAuthAdapter::new();
        let cred = Credential {
            provider: "openai".into(),
            kind: CredentialKind::ApiKey,
            expires_at: None,
            metadata: serde_json::json!({}),
        };
        auth.store_credential("openai", cred.clone()).await.unwrap();
        let got = auth.get_credential("openai").await.unwrap().unwrap();
        assert_eq!(got.provider, "openai");
    }

    #[tokio::test]
    async fn get_missing_returns_none() {
        let auth = InMemoryAuthAdapter::new();
        assert!(auth.get_credential("nope").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn refresh_missing_is_error() {
        let auth = InMemoryAuthAdapter::new();
        assert!(auth.refresh("nope").await.is_err());
    }

    #[tokio::test]
    async fn delete_credential_removes_it() {
        let auth = InMemoryAuthAdapter::new();
        let cred = Credential {
            provider: "gh".into(),
            kind: CredentialKind::Bearer,
            expires_at: None,
            metadata: serde_json::json!({}),
        };
        auth.store_credential("gh", cred).await.unwrap();
        auth.delete_credential("gh").await.unwrap();
        assert!(auth.get_credential("gh").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn from_env_returns_none_for_missing() {
        let auth = InMemoryAuthAdapter::from_env();
        let got = auth.get_credential("nonexistent_key_xyz").await.unwrap();
        assert!(got.is_none());
    }
}
