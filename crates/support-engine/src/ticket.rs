//! Support ticket management.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use rusvel_core::domain::ObjectFilter;
use rusvel_core::error::Result;
use rusvel_core::id::SessionId;
use rusvel_core::ports::StoragePort;

const KIND: &str = "support_ticket";

// ── Domain types ──────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TicketId(Uuid);

impl Default for TicketId {
    fn default() -> Self {
        Self::new()
    }
}

impl TicketId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl std::fmt::Display for TicketId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TicketPriority {
    Low,
    Medium,
    High,
    Urgent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TicketStatus {
    Open,
    InProgress,
    Resolved,
    Closed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ticket {
    pub id: TicketId,
    pub session_id: SessionId,
    pub subject: String,
    pub description: String,
    pub priority: TicketPriority,
    pub status: TicketStatus,
    pub requester_email: String,
    pub created_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub metadata: serde_json::Value,
}

// ── Manager ───────────────────────────────────────────────────────

pub struct TicketManager {
    storage: Arc<dyn StoragePort>,
}

impl TicketManager {
    pub fn new(storage: Arc<dyn StoragePort>) -> Self {
        Self { storage }
    }

    pub async fn create_ticket(
        &self,
        session_id: SessionId,
        subject: String,
        description: String,
        priority: TicketPriority,
        requester_email: String,
    ) -> Result<Ticket> {
        let ticket = Ticket {
            id: TicketId::new(),
            session_id,
            subject,
            description,
            priority,
            status: TicketStatus::Open,
            requester_email,
            created_at: Utc::now(),
            resolved_at: None,
            metadata: serde_json::json!({}),
        };
        let json = serde_json::to_value(&ticket)?;
        self.storage
            .objects()
            .put(KIND, &ticket.id.to_string(), json)
            .await?;
        Ok(ticket)
    }

    pub async fn list_tickets(&self, session_id: SessionId) -> Result<Vec<Ticket>> {
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
}
