//! Engine-specific tools for RUSVEL agent execution.
//!
//! Wraps harvest, content, and code engine methods as agent-callable tools
//! registered into a [`ToolRegistry`](rusvel_tool::ToolRegistry).

mod code;
mod content;
mod flow;
mod harvest;

use std::sync::Arc;

use rusvel_core::ports::BrowserPort;
use rusvel_tool::ToolRegistry;

pub async fn register_harvest_tools(
    registry: &ToolRegistry,
    engine: Arc<harvest_engine::HarvestEngine>,
    browser: Option<Arc<dyn BrowserPort>>,
) {
    harvest::register(registry, engine, browser).await;
}

pub async fn register_content_tools(
    registry: &ToolRegistry,
    engine: Arc<content_engine::ContentEngine>,
) {
    content::register(registry, engine).await;
}

pub async fn register_code_tools(registry: &ToolRegistry, engine: Arc<code_engine::CodeEngine>) {
    code::register(registry, engine).await;
}

pub async fn register_flow_tools(registry: &ToolRegistry, engine: Arc<flow_engine::FlowEngine>) {
    flow::register(registry, engine).await;
}
