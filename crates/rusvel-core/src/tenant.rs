//! Tenant registry — the "which client" axis for engines that draft on a
//! client's behalf (content today; outreach later).
//!
//! Deliberately a plain in-process struct, not a port trait: nothing external
//! backs it yet, and a swappable-adapter trait would be abstraction ahead of
//! need. See `docs/superpowers/specs/2026-09-03-rusvel-as-estate-abstraction.md`
//! §4-5 for why this exists and what it deliberately doesn't do yet (no
//! `CapabilityTenantInfraPort` wiring — that's a separate, later decision).

use std::collections::HashMap;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

use crate::id::TenantId;

/// What an engine needs to know about a tenant to draft on their behalf.
///
/// ADR-007: carries `metadata` for schema evolution — a distinct AvalAI key,
/// a domestic/foreign channel tag, a brand-kernel source path, and similar
/// per-tenant needs land there before they earn a named field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantProfile {
    pub id: TenantId,
    pub name: String,
    /// Brand voice/tone instructions, injected into drafting prompts.
    /// Sourced from the tenant's own brand documentation where one exists
    /// (e.g. mbaneshi's `branding/brand-kernel.yaml`) rather than invented
    /// per engine — see the content-pipeline work this profile type backs.
    pub voice_rules: String,
    pub metadata: serde_json::Value,
}

impl TenantProfile {
    pub fn new(
        id: impl Into<TenantId>,
        name: impl Into<String>,
        voice_rules: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            voice_rules: voice_rules.into(),
            metadata: serde_json::Value::Null,
        }
    }
}

/// In-process lookup from [`TenantId`] to [`TenantProfile`].
///
/// One registry, shared across engines behind an `Arc`, so "mbaneshi" means
/// the same tenant to `content-engine` and, later, `gtm-engine` — instead of
/// each engine keeping its own copy that can drift.
#[derive(Default)]
pub struct TenantRegistry {
    profiles: RwLock<HashMap<TenantId, TenantProfile>>,
}

impl TenantRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register or replace a tenant's profile.
    pub fn register(&self, profile: TenantProfile) {
        self.profiles
            .write()
            .unwrap()
            .insert(profile.id.clone(), profile);
    }

    /// Look up a tenant's profile. `None` if never registered.
    pub fn get(&self, id: &TenantId) -> Option<TenantProfile> {
        self.profiles.read().unwrap().get(id).cloned()
    }

    /// All currently-registered tenant ids.
    pub fn list(&self) -> Vec<TenantId> {
        self.profiles.read().unwrap().keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_then_get_roundtrips() {
        let registry = TenantRegistry::new();
        let profile = TenantProfile::new(
            "mbaneshi",
            "Mehdi Baneshi",
            "Direct, concrete, evidence-driven, no-hype.",
        );
        registry.register(profile.clone());

        let found = registry.get(&TenantId::new("mbaneshi")).unwrap();
        assert_eq!(found.name, "Mehdi Baneshi");
        assert_eq!(found.voice_rules, profile.voice_rules);
    }

    #[test]
    fn unknown_tenant_returns_none() {
        let registry = TenantRegistry::new();
        assert!(registry.get(&TenantId::new("nobody")).is_none());
    }

    #[test]
    fn registering_the_same_id_twice_replaces_the_profile() {
        let registry = TenantRegistry::new();
        registry.register(TenantProfile::new("mbaneshi", "v1", "old voice"));
        registry.register(TenantProfile::new("mbaneshi", "v2", "new voice"));

        let found = registry.get(&TenantId::new("mbaneshi")).unwrap();
        assert_eq!(found.name, "v2");
        assert_eq!(found.voice_rules, "new voice");
    }

    #[test]
    fn list_returns_all_registered_tenants() {
        let registry = TenantRegistry::new();
        registry.register(TenantProfile::new("mbaneshi", "Mehdi", ""));
        registry.register(TenantProfile::new("taban", "Taban Clinic", ""));

        let mut ids: Vec<String> = registry.list().iter().map(|t| t.as_str().to_string()).collect();
        ids.sort();
        assert_eq!(ids, vec!["mbaneshi", "taban"]);
    }
}
