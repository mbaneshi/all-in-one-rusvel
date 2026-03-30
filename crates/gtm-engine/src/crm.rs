//! Contact and deal management (CRM).

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use rusvel_core::domain::{Contact, ObjectFilter};
use rusvel_core::error::{Result, RusvelError};
use rusvel_core::id::{ContactId, SessionId};
use rusvel_core::ports::StoragePort;

// ── Local ID + domain types ────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DealId(Uuid);

impl Default for DealId {
    fn default() -> Self {
        Self::new()
    }
}

impl DealId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl std::str::FromStr for DealId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl std::fmt::Display for DealId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DealStage {
    Lead,
    Qualified,
    Proposal,
    Negotiation,
    Won,
    Lost,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deal {
    pub id: DealId,
    pub session_id: SessionId,
    pub contact_id: ContactId,
    pub title: String,
    pub value: f64,
    pub stage: DealStage,
    pub notes: String,
    pub created_at: DateTime<Utc>,
    pub metadata: serde_json::Value,
}

// ── CrmManager ─────────────────────────────────────────────────────

const KIND_CONTACT: &str = "contact";
const KIND_DEAL: &str = "deal";

pub struct CrmManager {
    storage: Arc<dyn StoragePort>,
}

impl CrmManager {
    pub fn new(storage: Arc<dyn StoragePort>) -> Self {
        Self { storage }
    }

    pub async fn add_contact(
        &self,
        session_id: SessionId,
        mut contact: Contact,
    ) -> Result<ContactId> {
        contact.session_id = session_id;
        let id = contact.id;
        let json = serde_json::to_value(&contact)?;
        self.storage
            .objects()
            .put(KIND_CONTACT, &id.to_string(), json)
            .await?;
        Ok(id)
    }

    pub async fn get_contact(&self, id: &ContactId) -> Result<Contact> {
        let val = self
            .storage
            .objects()
            .get(KIND_CONTACT, &id.to_string())
            .await?;
        match val {
            Some(v) => Ok(serde_json::from_value(v)?),
            None => Err(RusvelError::NotFound {
                kind: KIND_CONTACT.into(),
                id: id.to_string(),
            }),
        }
    }

    pub async fn list_contacts(&self, session_id: SessionId) -> Result<Vec<Contact>> {
        let filter = ObjectFilter {
            conversation_id: None,
            session_id: Some(session_id),
            ..Default::default()
        };
        let vals = self.storage.objects().list(KIND_CONTACT, filter).await?;
        vals.into_iter()
            .map(|v| Ok(serde_json::from_value(v)?))
            .collect()
    }

    pub async fn update_contact(&self, contact: &Contact) -> Result<()> {
        let json = serde_json::to_value(contact)?;
        self.storage
            .objects()
            .put(KIND_CONTACT, &contact.id.to_string(), json)
            .await
    }

    pub async fn add_deal(&self, session_id: SessionId, mut deal: Deal) -> Result<DealId> {
        deal.session_id = session_id;
        let id = deal.id;
        let json = serde_json::to_value(&deal)?;
        self.storage
            .objects()
            .put(KIND_DEAL, &id.to_string(), json)
            .await?;
        Ok(id)
    }

    pub async fn list_deals(
        &self,
        session_id: SessionId,
        stage_filter: Option<DealStage>,
    ) -> Result<Vec<Deal>> {
        let filter = ObjectFilter {
            conversation_id: None,
            session_id: Some(session_id),
            ..Default::default()
        };
        let vals = self.storage.objects().list(KIND_DEAL, filter).await?;
        let deals: Vec<Deal> = vals
            .into_iter()
            .map(serde_json::from_value)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        match stage_filter {
            Some(stage) => Ok(deals.into_iter().filter(|d| d.stage == stage).collect()),
            None => Ok(deals),
        }
    }

    pub async fn advance_deal(&self, id: &DealId, new_stage: DealStage) -> Result<()> {
        let val = self
            .storage
            .objects()
            .get(KIND_DEAL, &id.to_string())
            .await?;
        match val {
            Some(v) => {
                let mut deal: Deal = serde_json::from_value(v)?;
                deal.stage = new_stage;
                let json = serde_json::to_value(&deal)?;
                self.storage
                    .objects()
                    .put(KIND_DEAL, &id.to_string(), json)
                    .await
            }
            None => Err(RusvelError::NotFound {
                kind: KIND_DEAL.into(),
                id: id.to_string(),
            }),
        }
    }
}
