
## Overview

RUSVEL organizes work into **14** department apps (`dept-*` crates), each with its own specialized AI agent surface. Together they form a virtual agency that a solo founder can command from a single interface.

Departments follow the **DepartmentApp pattern** (ADR-014). Each department lives in its own `dept-*` crate that implements the `DepartmentApp` trait, declares its capabilities via a `DepartmentManifest`, and registers with the host at boot. Adding a new department means adding a new `dept-*` crate -- zero changes to `rusvel-core`.

## The DepartmentApp Trait

Every department crate implements this contract:

```rust
pub trait DepartmentApp: Send + Sync {
    fn manifest(&self) -> DepartmentManifest;
    fn register(&self, ctx: &mut RegistrationContext) -> Result<()>;
}
```

The `DepartmentManifest` declares the department's ID, name, icon, color, system prompt, capabilities, tabs, quick actions, routes, tools, and CLI commands. The host collects all manifests to generate the `DepartmentRegistry`, API routes, CLI subcommands, and frontend navigation.

## The 14 dept-* Crates

| Crate | Department | Engine | Focus |
|-------|-----------|--------|-------|
| `dept-forge` | [Forge](/departments/forge/) | Forge | Agent orchestration, goal planning, mission management |
| `dept-code` | [Code](/departments/code/) | Code | Code intelligence, implementation, testing |
| `dept-content` | [Content](/departments/content/) | Content | Content creation, publishing, calendar |
| `dept-harvest` | [Harvest](/departments/harvest/) | Harvest | Opportunity discovery, proposals, pipeline |
| `dept-flow` | [Flow](/departments/flow/) | Flow | DAG workflow engine, visual workflow builder |
| `dept-gtm` | [GTM](/departments/gtm/) | GoToMarket | CRM, outreach, deals, invoicing |
| `dept-finance` | [Finance](/departments/finance/) | Finance | Revenue, expenses, runway, tax |
| `dept-product` | [Product](/departments/product/) | Product | Roadmap, pricing, feedback |
| `dept-growth` | [Growth](/departments/growth/) | Growth | Funnels, cohorts, KPIs, retention |
| `dept-distro` | [Distro](/departments/distro/) | Distribution | Marketplace, SEO, affiliates |
| `dept-legal` | [Legal](/departments/legal/) | Legal | Contracts, compliance, IP |
| `dept-support` | [Support](/departments/support/) | Support | Tickets, knowledge base, NPS |
| `dept-infra` | [Infra](/departments/infra/) | Infra | Deployments, monitoring, incidents |
| `dept-messaging` | [Messaging](/departments/messaging/) | — (shell) | Cross-channel messaging surface |

## Department UI Structure

Each department page has two panels:

### Chat Panel (right side)

The AI agent for this department. Send messages, get responses, use quick actions. The agent has access to department-specific tools via the **ScopedToolRegistry** -- each department only sees tools relevant to its domain.

### Department Panel (left side)

A tabbed panel with up to 9 tabs depending on the department:

| Tab | Purpose |
|-----|---------|
| **Actions** | Quick-action buttons for common tasks |
| **Agents** | Custom agent profiles for this department |
| **Workflows** | Multi-step agent chains (sequential, parallel, loop, graph) |
| **Skills** | Reusable prompt templates with variables |
| **Rules** | Constraints injected into the system prompt |
| **MCP** | MCP server connections (Code department) |
| **Hooks** | Event-triggered automations |
| **Dirs** | Working directories for code operations |
| **Events** | Event log for this department |

## The God Agent

The main **Chat** page (not tied to any department) runs the God Agent. This agent has full authority and visibility over all departments. Use it for cross-cutting tasks:

- "What is the status across all departments?"
- "Move the proposal from Harvest to Content for editing"
- "Generate a weekly summary of all activity"

## Quick Actions

Each department has predefined quick-action buttons that send a prompt to the agent with one click. These are declared in the department's `DepartmentManifest` and can be customized.

## Department Configuration

Each department inherits global config and can override:

- **Model** -- which LLM to use for this department's agent
- **Effort** -- low, medium, or high (controls response depth)
- **Permission mode** -- what tools the agent can use
- **Working directories** -- for code-operating departments

Configure via the Settings page or the API:

```bash
curl -X PUT http://localhost:3000/api/dept/code/config \
  -H "Content-Type: application/json" \
  -d '{"model": "claude-sonnet-5", "effort": "high"}'
```
