//! Income/expense tracking.

use chrono::{DateTime, Utc};
use rusvel_core::domain::ObjectFilter;
use rusvel_core::error::Result;
use rusvel_core::id::SessionId;
use rusvel_core::ports::StoragePort;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TransactionId(Uuid);
impl Default for TransactionId {
    fn default() -> Self {
        Self::new()
    }
}

impl TransactionId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}
impl std::fmt::Display for TransactionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransactionKind {
    Income,
    Expense,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub id: TransactionId,
    pub session_id: SessionId,
    pub kind: TransactionKind,
    pub amount: f64,
    pub description: String,
    pub category: String,
    pub date: DateTime<Utc>,
    pub metadata: serde_json::Value,
}

const KIND: &str = "finance_transaction";

pub struct LedgerManager {
    storage: Arc<dyn StoragePort>,
}

impl LedgerManager {
    pub fn new(storage: Arc<dyn StoragePort>) -> Self {
        Self { storage }
    }

    pub async fn record(
        &self,
        session_id: SessionId,
        kind: TransactionKind,
        amount: f64,
        description: String,
        category: String,
    ) -> Result<TransactionId> {
        let tx = Transaction {
            id: TransactionId::new(),
            session_id,
            kind,
            amount,
            description,
            category,
            date: Utc::now(),
            metadata: serde_json::json!({}),
        };
        let id = tx.id;
        self.storage
            .objects()
            .put(KIND, &id.to_string(), serde_json::to_value(&tx)?)
            .await?;
        Ok(id)
    }

    pub async fn list_transactions(&self, session_id: SessionId) -> Result<Vec<Transaction>> {
        let filter = ObjectFilter {
            conversation_id: None,
            session_id: Some(session_id),
            ..Default::default()
        };
        let vals = self.storage.objects().list(KIND, filter).await?;
        vals.into_iter()
            .map(|v| Ok(serde_json::from_value(v)?))
            .collect()
    }

    pub async fn balance(&self, session_id: SessionId) -> Result<f64> {
        let txs = self.list_transactions(session_id).await?;
        let total = txs
            .iter()
            .map(|t| match t.kind {
                TransactionKind::Income => t.amount,
                TransactionKind::Expense => -t.amount,
            })
            .sum();
        Ok(total)
    }
}
