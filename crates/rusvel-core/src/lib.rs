//! # rusvel-core
//!
//! The **contract crate** for the RUSVEL system.
//!
//! This crate contains:
//!
//! - **Port traits** — the 10 hexagonal-boundary abstractions that
//!   engines program against and adapters implement.
//! - **Shared domain types** — structs and enums used across every
//!   engine (Content, Opportunity, Contact, Goal, Event, …).
//! - **ID newtypes** — strongly-typed `Uuid` wrappers that prevent
//!   mixing identifiers at compile time.
//! - **Engine base trait** — the lifecycle contract every domain engine
//!   implements.
//! - **Error type** — [`RusvelError`] with `thiserror` variants for
//!   every failure mode.
//!
//! ## Dependency policy
//!
//! `rusvel-core` depends on **nothing** beyond the Rust standard library
//! and a small set of foundational crates: `serde`, `serde_json`,
//! `async-trait`, `thiserror`, `chrono`, `uuid`.
//!
//! Zero framework dependencies. Zero IO. Pure types and traits.

// ── Modules ────────────────────────────────────────────────────────

pub mod config;
pub mod constants;
pub mod department;
pub mod domain;
pub mod engine;
pub mod error;
pub mod id;
pub mod ports;
pub mod registry;
pub mod rrf;
pub mod tenant;
pub mod terminal;

// ── Convenience re-exports ─────────────────────────────────────────
//
// So consumers can write `use rusvel_core::*;` for the most common
// types, or `use rusvel_core::ports::LlmPort;` for specific traits.

pub use department::DepartmentApp;
pub use domain::*;
pub use engine::Engine;
pub use error::{Result, RusvelError};
pub use id::*;
pub use rrf::{RRF_K_DEFAULT, reciprocal_rank_fusion};
pub use tenant::{TenantProfile, TenantRegistry};
pub use terminal::{Layout, Pane, PaneSize, PaneSource, PaneStatus, Window, WindowSource};
