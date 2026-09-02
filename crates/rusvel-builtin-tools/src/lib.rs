//! Built-in tools for RUSVEL agents.
//!
//! Registers file operations, shell execution, web fetching, and git tools
//! into a [`ToolRegistry`](rusvel_tool::ToolRegistry).

pub mod artifacts;
pub mod browser;
pub mod delegate;
mod file_ops;
pub mod flow;
mod git;
pub mod memory;
mod shell;
mod skill_load;
pub mod terminal_tools;
pub mod tool_search;

use std::collections::HashMap;
use std::sync::Arc;

use rusvel_core::department::DepartmentManifest;
use rusvel_core::ports::{
    AgentPort, BrowserPort, EventPort, MemoryPort, StoragePort, TerminalPort,
};
use rusvel_tool::ToolRegistry;

/// Register all built-in tools into the given registry.
pub async fn register_all(registry: &ToolRegistry) {
    file_ops::register(registry).await;
    shell::register(registry).await;
    git::register(registry).await;
}

/// Register memory tools (memory_write, memory_read, memory_search, memory_delete).
pub async fn register_memory_tools(registry: &ToolRegistry, memory_port: Arc<dyn MemoryPort>) {
    memory::register(registry, memory_port).await;
}

/// Register delegate_agent tool for spawning sub-agents (streams to a delegation pane when terminal is set).
pub async fn register_delegate_tool(
    registry: &ToolRegistry,
    agent: Arc<rusvel_agent::AgentRuntime>,
    terminal: Option<Arc<dyn TerminalPort>>,
) {
    delegate::register(registry, agent, terminal).await;
}

/// Register terminal_open and terminal_watch.
pub async fn register_terminal_tools(
    registry: &ToolRegistry,
    terminal: Option<Arc<dyn TerminalPort>>,
) {
    terminal_tools::register(registry, terminal).await;
}

/// Register flow tools (invoke_flow).
pub async fn register_flow_tools(
    registry: &ToolRegistry,
    storage: Arc<dyn StoragePort>,
    events: Arc<dyn EventPort>,
    agent: Arc<dyn AgentPort>,
) {
    flow::register(registry, storage, events, agent).await;
}

/// Register `browser_observe`, `browser_search`, and `browser_act` (CDP / [`BrowserPort`]).
pub async fn register_browser_tools(registry: &ToolRegistry, browser_port: Arc<dyn BrowserPort>) {
    browser::register(registry, browser_port).await;
}

/// Register `forge_save_artifact` (S-049).
pub async fn register_artifact_tools(registry: &ToolRegistry, storage: Arc<dyn StoragePort>) {
    let _ = artifacts::register(registry, storage).await;
}

/// Register `skill.load` — fetches a department manifest skill's full prompt
/// template on demand (issue #18: progressive disclosure). `dept_manifests`
/// is keyed by department id.
pub async fn register_skill_tools(
    registry: &ToolRegistry,
    dept_manifests: HashMap<String, DepartmentManifest>,
) {
    skill_load::register(registry, dept_manifests).await;
}
