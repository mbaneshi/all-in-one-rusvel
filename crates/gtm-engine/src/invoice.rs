//! Simple invoice management.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use rusvel_core::domain::ObjectFilter;
use rusvel_core::error::{Result, RusvelError};
use rusvel_core::id::{ContactId, SessionId};
use rusvel_core::ports::StoragePort;

// ── Local ID + domain types ────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct InvoiceId(Uuid);

impl Default for InvoiceId {
    fn default() -> Self {
        Self::new()
    }
}

impl InvoiceId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl std::fmt::Display for InvoiceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::str::FromStr for InvoiceId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InvoiceStatus {
    Draft,
    Sent,
    Paid,
    Overdue,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineItem {
    pub description: String,
    pub quantity: f64,
    pub unit_price: f64,
}

impl LineItem {
    pub fn subtotal(&self) -> f64 {
        self.quantity * self.unit_price
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Invoice {
    pub id: InvoiceId,
    pub session_id: SessionId,
    pub contact_id: ContactId,
    pub items: Vec<LineItem>,
    pub total: f64,
    pub status: InvoiceStatus,
    pub due_date: DateTime<Utc>,
    pub paid_at: Option<DateTime<Utc>>,
    pub metadata: serde_json::Value,
}

// ── InvoiceManager ─────────────────────────────────────────────────

const KIND_INVOICE: &str = "invoice";

pub struct InvoiceManager {
    storage: Arc<dyn StoragePort>,
}

impl InvoiceManager {
    pub fn new(storage: Arc<dyn StoragePort>) -> Self {
        Self { storage }
    }

    pub async fn create_invoice(
        &self,
        session_id: SessionId,
        contact_id: ContactId,
        items: Vec<LineItem>,
        due_date: DateTime<Utc>,
    ) -> Result<InvoiceId> {
        let total: f64 = items.iter().map(LineItem::subtotal).sum();
        let inv = Invoice {
            id: InvoiceId::new(),
            session_id,
            contact_id,
            items,
            total,
            status: InvoiceStatus::Draft,
            due_date,
            paid_at: None,
            metadata: serde_json::json!({}),
        };
        let id = inv.id;
        let json = serde_json::to_value(&inv)?;
        self.storage
            .objects()
            .put(KIND_INVOICE, &id.to_string(), json)
            .await?;
        Ok(id)
    }

    pub async fn get_invoice(&self, id: &InvoiceId) -> Result<Invoice> {
        let val = self
            .storage
            .objects()
            .get(KIND_INVOICE, &id.to_string())
            .await?;
        match val {
            Some(v) => Ok(serde_json::from_value(v)?),
            None => Err(RusvelError::NotFound {
                kind: KIND_INVOICE.into(),
                id: id.to_string(),
            }),
        }
    }

    /// Updates lifecycle status. Sets `paid_at` when moving to [`InvoiceStatus::Paid`].
    pub async fn set_invoice_status(
        &self,
        id: &InvoiceId,
        new_status: InvoiceStatus,
    ) -> Result<()> {
        let val = self
            .storage
            .objects()
            .get(KIND_INVOICE, &id.to_string())
            .await?;
        match val {
            Some(v) => {
                let mut inv: Invoice = serde_json::from_value(v)?;
                inv.status = new_status.clone();
                match new_status {
                    InvoiceStatus::Paid => {
                        if inv.paid_at.is_none() {
                            inv.paid_at = Some(Utc::now());
                        }
                    }
                    _ => {}
                }
                let json = serde_json::to_value(&inv)?;
                self.storage
                    .objects()
                    .put(KIND_INVOICE, &id.to_string(), json)
                    .await
            }
            None => Err(RusvelError::NotFound {
                kind: KIND_INVOICE.into(),
                id: id.to_string(),
            }),
        }
    }

    pub async fn list_invoices(
        &self,
        session_id: SessionId,
        status_filter: Option<InvoiceStatus>,
    ) -> Result<Vec<Invoice>> {
        let filter = ObjectFilter {
            conversation_id: None,
            session_id: Some(session_id),
            ..Default::default()
        };
        let vals = self.storage.objects().list(KIND_INVOICE, filter).await?;
        let invoices: Vec<Invoice> = vals
            .into_iter()
            .map(serde_json::from_value)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        match status_filter {
            Some(s) => Ok(invoices.into_iter().filter(|i| i.status == s).collect()),
            None => Ok(invoices),
        }
    }

    pub async fn mark_paid(&self, id: &InvoiceId) -> Result<()> {
        self.set_invoice_status(id, InvoiceStatus::Paid).await
    }

    pub async fn total_revenue(&self, session_id: SessionId) -> Result<f64> {
        let paid = self
            .list_invoices(session_id, Some(InvoiceStatus::Paid))
            .await?;
        Ok(paid.iter().map(|i| i.total).sum())
    }
}
