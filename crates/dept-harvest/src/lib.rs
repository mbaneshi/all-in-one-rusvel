//! Harvest Department — DepartmentApp implementation.
//!
//! Wraps `harvest-engine` (opportunity discovery) with the ADR-014
//! department contract: manifest declaration, subsystem registration,
//! and lifecycle.

mod manifest;
mod tools;

use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use harvest_engine::HarvestEngine;
use rusvel_core::Engine;
use rusvel_core::department::*;
use rusvel_core::error::Result;

/// The Harvest Department app.
///
/// Implements [`DepartmentApp`] to register the harvest engine's routes,
/// tools, event subscriptions, and job handlers with the host.
pub struct HarvestDepartment {
    engine: OnceLock<Arc<HarvestEngine>>,
}

impl HarvestDepartment {
    pub fn new() -> Self {
        Self {
            engine: OnceLock::new(),
        }
    }

    /// Access the inner engine (available after registration).
    pub fn engine(&self) -> Option<&Arc<HarvestEngine>> {
        self.engine.get()
    }
}

impl Default for HarvestDepartment {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DepartmentApp for HarvestDepartment {
    fn manifest(&self) -> DepartmentManifest {
        manifest::harvest_manifest()
    }

    async fn register(&self, ctx: &mut RegistrationContext) -> Result<()> {
        let harvest = HarvestEngine::new(ctx.storage.clone())
            .with_events(ctx.events.clone())
            .with_agent(ctx.agent.clone());
        harvest.configure_rag(ctx.embedding.clone(), ctx.vector_store.clone());
        let engine = Arc::new(harvest);
        let _ = self.engine.set(engine.clone());

        tools::register_tools(&mut ctx.tools, engine);

        tracing::info!("Harvest department registered");
        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        if let Some(engine) = self.engine.get() {
            engine.shutdown().await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn department_creates() {
        let dept = HarvestDepartment::new();
        let m = dept.manifest();
        assert_eq!(m.id, "harvest");
        assert_eq!(m.requires_ports.len(), 4);
        assert!(dept.engine().is_none());
    }

    #[test]
    fn default_impl_works() {
        let dept = HarvestDepartment::default();
        assert_eq!(dept.manifest().id, "harvest");
    }

    #[test]
    fn manifest_is_pure() {
        let dept = HarvestDepartment::new();
        let m1 = dept.manifest();
        let m2 = dept.manifest();
        assert_eq!(m1.id, m2.id);
        assert_eq!(m1.routes.len(), m2.routes.len());
        assert_eq!(m1.tools.len(), m2.tools.len());
    }
}
