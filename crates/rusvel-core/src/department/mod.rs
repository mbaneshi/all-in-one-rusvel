//! Department-as-App architecture (ADR-014).
//!
//! Each department is a self-contained unit that implements [`DepartmentApp`],
//! declares its contributions via [`DepartmentManifest`], and registers with
//! host subsystems via [`RegistrationContext`] during boot.
//!
//! ## Lifecycle
//!
//! 1. Host reads [`DepartmentApp::manifest()`] — no side effects, fast.
//! 2. Host validates dependencies and version compatibility.
//! 3. Host calls [`DepartmentApp::register()`] in dependency order.
//! 4. Host calls [`DepartmentApp::shutdown()`] on graceful exit.
//!
//! ## Inspired by
//!
//! - Django `AppConfig` + `INSTALLED_APPS`
//! - Linux kernel `module_init()` / `module_exit()`
//! - `VSCode` `package.json#contributes` + `activate()` / `deactivate()`

mod app;
mod context;
mod manifest;

pub use app::DepartmentApp;
pub use context::*;
pub use manifest::*;
