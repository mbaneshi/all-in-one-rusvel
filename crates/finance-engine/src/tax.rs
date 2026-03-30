//! Tax estimation.

use rusvel_core::domain::ObjectFilter;
use rusvel_core::error::Result;
use rusvel_core::id::SessionId;
use rusvel_core::ports::StoragePort;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TaxEstimateId(Uuid);
impl Default for TaxEstimateId {
    fn default() -> Self {
        Self::new()
    }
}

impl TaxEstimateId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}
impl std::fmt::Display for TaxEstimateId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaxCategory {
    Income,
    SelfEmployment,
    Sales,
    Deduction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxEstimate {
    pub id: TaxEstimateId,
    pub session_id: SessionId,
    pub category: TaxCategory,
    pub amount: f64,
    pub period: String,
    pub metadata: serde_json::Value,
}

const KIND: &str = "finance_tax_estimate";

pub struct TaxManager {
    storage: Arc<dyn StoragePort>,
}

impl TaxManager {
    pub fn new(storage: Arc<dyn StoragePort>) -> Self {
        Self { storage }
    }

    pub async fn add_estimate(
        &self,
        session_id: SessionId,
        category: TaxCategory,
        amount: f64,
        period: String,
    ) -> Result<TaxEstimateId> {
        let est = TaxEstimate {
            id: TaxEstimateId::new(),
            session_id,
            category,
            amount,
            period,
            metadata: serde_json::json!({}),
        };
        let id = est.id;
        self.storage
            .objects()
            .put(KIND, &id.to_string(), serde_json::to_value(&est)?)
            .await?;
        Ok(id)
    }

    pub async fn list_estimates(&self, session_id: SessionId) -> Result<Vec<TaxEstimate>> {
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

    pub async fn total_liability(&self, session_id: SessionId) -> Result<f64> {
        let ests = self.list_estimates(session_id).await?;
        Ok(ests
            .iter()
            .filter(|e| e.category != TaxCategory::Deduction)
            .map(|e| e.amount)
            .sum::<f64>()
            - ests
                .iter()
                .filter(|e| e.category == TaxCategory::Deduction)
                .map(|e| e.amount)
                .sum::<f64>())
    }
}
