//! Department Registry — declarative department definitions.
//!
//! Instead of hardcoding department configs in match arms and routes,
//! departments are defined in a TOML file and loaded at startup.
//! Adding a department = adding a TOML block. Zero code changes.

use crate::config::LayeredConfig;
use crate::department::DepartmentTerminalPrefs;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// A quick-action button shown in the department panel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuickAction {
    pub label: String,
    pub prompt: String,
}

/// A single department definition — everything needed to wire a department.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepartmentDef {
    pub id: String,
    pub name: String,
    pub title: String,
    pub icon: String,
    pub color: String,
    pub system_prompt: String,
    pub capabilities: Vec<String>,
    pub tabs: Vec<String>,
    pub quick_actions: Vec<QuickAction>,
    #[serde(default)]
    pub default_config: LayeredConfig,
    /// PTY defaults for `/api/terminal/dept/:id` (merged with env; see operator docs).
    #[serde(default)]
    pub terminal: DepartmentTerminalPrefs,
}

/// Registry of all departments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepartmentRegistry {
    #[serde(rename = "department")]
    pub departments: Vec<DepartmentDef>,
}

impl DepartmentRegistry {
    /// Load from a TOML file, falling back to built-in defaults if file missing.
    pub fn load(path: &Path) -> Self {
        if path.exists()
            && let Ok(contents) = std::fs::read_to_string(path)
            && let Ok(reg) = toml::from_str(&contents)
        {
            return reg;
        }
        Self::defaults()
    }

    /// Look up a department by its id.
    pub fn get(&self, id: &str) -> Option<&DepartmentDef> {
        self.departments.iter().find(|d| d.id == id)
    }

    /// All department definitions.
    pub fn list(&self) -> &[DepartmentDef] {
        &self.departments
    }

    /// Built-in defaults for all 12 departments.
    pub fn defaults() -> Self {
        Self {
            departments: vec![
                DepartmentDef {
                    id: "forge".into(),
                    name: "Forge".into(),
                    title: "Forge Department".into(),
                    icon: "=".into(),
                    color: "indigo".into(),
                    system_prompt: "You are the Forge department of RUSVEL.\n\nFocus: agent orchestration, goal planning, mission management, daily plans, reviews.".into(),
                    capabilities: vec!["planning".into(), "orchestration".into()],
                    tabs: vec!["actions".into(), "agents".into(), "workflows".into(), "skills".into(), "rules".into(), "events".into()],
                    quick_actions: vec![
                        QuickAction { label: "Daily plan".into(), prompt: "Generate today's mission plan based on active goals and priorities.".into() },
                        QuickAction { label: "Review progress".into(), prompt: "Review progress on all active goals. Summarize completed, in-progress, and blocked items.".into() },
                        QuickAction { label: "Set new goal".into(), prompt: "Help me define a new strategic goal. Ask me for context and desired outcome.".into() },
                    ],
                    default_config: LayeredConfig::default(),
                    terminal: DepartmentTerminalPrefs::default(),
                },
                DepartmentDef {
                    id: "code".into(),
                    name: "Code".into(),
                    title: "Code Department".into(),
                    icon: "#".into(),
                    color: "emerald".into(),
                    system_prompt: "You are the Code department of RUSVEL.\n\nYou have full access to Claude Code tools:\n- Read, Write, Edit files across all project directories\n- Run shell commands (build, test, git, pnpm, cargo, etc.)\n- Search codebases with grep and glob\n- Fetch web content and search the web\n- Spawn sub-agents for parallel work\n- Manage background tasks\n\nFocus: code intelligence, implementation, debugging, testing, refactoring.\nWhen writing code, follow existing patterns. Be thorough.".into(),
                    capabilities: vec!["code_analysis".into(), "tool_use".into()],
                    tabs: vec!["actions".into(), "engine".into(), "agents".into(), "workflows".into(), "skills".into(), "rules".into(), "mcp".into(), "hooks".into(), "dirs".into(), "events".into()],
                    quick_actions: vec![
                        QuickAction { label: "Analyze codebase".into(), prompt: "Analyze the codebase structure, dependencies, and code quality.".into() },
                        QuickAction { label: "Run tests".into(), prompt: "Run `cargo test` and report results. If any fail, show the errors.".into() },
                        QuickAction { label: "Find TODOs".into(), prompt: "Find all TODO, FIXME, and HACK comments across the codebase.".into() },
                        QuickAction { label: "Self-improve".into(), prompt: "Read docs/status/current-state.md and docs/status/gap-analysis.md. Identify the highest-impact fix you can make right now. Implement it, run tests, and verify.".into() },
                        QuickAction { label: "Fix build warnings".into(), prompt: "Run `cargo build` and fix any warnings. Then run `cargo test` to verify nothing broke.".into() },
                    ],
                    default_config: LayeredConfig {
                        model: None, effort: Some("high".into()),
                        permission_mode: Some("default".into()),
                        add_dirs: Some(vec![".".into()]),
                        ..Default::default()
                    },
                    terminal: DepartmentTerminalPrefs::default(),
                },
                DepartmentDef {
                    id: "harvest".into(),
                    name: "Harvest".into(),
                    title: "Harvest Department".into(),
                    icon: "$".into(),
                    color: "amber".into(),
                    system_prompt: "You are the Harvest department of RUSVEL.\n\nFocus: finding opportunities, scoring gigs, drafting proposals.\nSources: Upwork, LinkedIn, GitHub.".into(),
                    capabilities: vec!["opportunity_discovery".into()],
                    tabs: vec!["actions".into(), "engine".into(), "agents".into(), "skills".into(), "rules".into(), "events".into()],
                    quick_actions: vec![
                        QuickAction { label: "Scan opportunities".into(), prompt: "Scan for new freelance opportunities on Upwork, LinkedIn, and GitHub.".into() },
                        QuickAction { label: "Score pipeline".into(), prompt: "Score all opportunities in the pipeline by fit, budget, and probability.".into() },
                        QuickAction { label: "Draft proposal".into(), prompt: "Draft a proposal for an opportunity. Ask me for the gig details.".into() },
                    ],
                    default_config: LayeredConfig::default(),
                    terminal: DepartmentTerminalPrefs::default(),
                },
                DepartmentDef {
                    id: "content".into(),
                    name: "Content".into(),
                    title: "Content Department".into(),
                    icon: "*".into(),
                    color: "purple".into(),
                    system_prompt: "You are the Content department of RUSVEL.\n\nFocus: content creation, platform adaptation, publishing strategy.\nDraft in Markdown. Adapt for LinkedIn, Twitter/X, DEV.to, Substack.".into(),
                    capabilities: vec!["content_creation".into()],
                    tabs: vec!["actions".into(), "engine".into(), "agents".into(), "skills".into(), "rules".into(), "events".into()],
                    quick_actions: vec![
                        QuickAction { label: "Draft blog post".into(), prompt: "Draft a blog post. Ask me for the topic, audience, and key points.".into() },
                        QuickAction { label: "Adapt for Twitter".into(), prompt: "Adapt the latest content piece into a Twitter/X thread.".into() },
                        QuickAction { label: "Content calendar".into(), prompt: "Show the content calendar for this week with scheduled and draft posts.".into() },
                    ],
                    default_config: LayeredConfig::default(),
                    terminal: DepartmentTerminalPrefs::default(),
                },
                DepartmentDef {
                    id: "gtm".into(),
                    name: "GTM".into(),
                    title: "GoToMarket Department".into(),
                    icon: "^".into(),
                    color: "cyan".into(),
                    system_prompt: "You are the GoToMarket department of RUSVEL.\n\nFocus: CRM, outreach sequences, deal management, invoicing.".into(),
                    capabilities: vec!["outreach".into(), "crm".into(), "invoicing".into()],
                    tabs: vec!["actions".into(), "agents".into(), "workflows".into(), "skills".into(), "rules".into(), "events".into()],
                    quick_actions: vec![
                        QuickAction { label: "List contacts".into(), prompt: "List all contacts in the CRM. Show name, company, status, and last interaction.".into() },
                        QuickAction { label: "Draft outreach".into(), prompt: "Draft a multi-step outreach sequence for a prospect.".into() },
                        QuickAction { label: "Deal pipeline".into(), prompt: "Show the current deal pipeline with stages, values, and next actions.".into() },
                        QuickAction { label: "Generate invoice".into(), prompt: "Generate an invoice. Ask me for client details and line items.".into() },
                    ],
                    default_config: LayeredConfig::default(),
                    terminal: DepartmentTerminalPrefs::default(),
                },
                DepartmentDef {
                    id: "finance".into(),
                    name: "Finance".into(),
                    title: "Finance Department".into(),
                    icon: "%".into(),
                    color: "green".into(),
                    system_prompt: "You are the Finance department of RUSVEL.\n\nFocus: revenue tracking, expense management, tax optimization, runway forecasting, P&L reports, unit economics.".into(),
                    capabilities: vec!["ledger".into(), "tax".into(), "runway".into()],
                    tabs: vec!["actions".into(), "agents".into(), "rules".into(), "events".into()],
                    quick_actions: vec![
                        QuickAction { label: "Record income".into(), prompt: "Record a new income transaction. Ask me for amount, source, category, and date.".into() },
                        QuickAction { label: "Log expense".into(), prompt: "Log a new expense. Ask me for amount, description, category, and date.".into() },
                        QuickAction { label: "Calculate runway".into(), prompt: "Calculate the current runway based on cash on hand and burn rate.".into() },
                        QuickAction { label: "P&L report".into(), prompt: "Generate a profit & loss report by category.".into() },
                    ],
                    default_config: LayeredConfig::default(),
                    terminal: DepartmentTerminalPrefs::default(),
                },
                DepartmentDef {
                    id: "product".into(),
                    name: "Product".into(),
                    title: "Product Department".into(),
                    icon: "@".into(),
                    color: "rose".into(),
                    system_prompt: "You are the Product department of RUSVEL.\n\nFocus: product roadmaps, feature prioritization, pricing strategy, user feedback analysis, A/B testing.".into(),
                    capabilities: vec!["roadmap".into(), "pricing".into(), "feedback".into()],
                    tabs: vec!["actions".into(), "agents".into(), "skills".into(), "rules".into(), "events".into()],
                    quick_actions: vec![
                        QuickAction { label: "View roadmap".into(), prompt: "Show the current product roadmap with features, milestones, and priorities.".into() },
                        QuickAction { label: "Add feature".into(), prompt: "Add a new feature to the roadmap. Ask me for name, description, and priority.".into() },
                        QuickAction { label: "Pricing analysis".into(), prompt: "Analyze current pricing tiers and suggest optimizations.".into() },
                    ],
                    default_config: LayeredConfig::default(),
                    terminal: DepartmentTerminalPrefs::default(),
                },
                DepartmentDef {
                    id: "growth".into(),
                    name: "Growth".into(),
                    title: "Growth Department".into(),
                    icon: "&".into(),
                    color: "orange".into(),
                    system_prompt: "You are the Growth department of RUSVEL.\n\nFocus: funnel optimization, conversion tracking, cohort analysis, churn prediction, retention strategies, KPI dashboards.".into(),
                    capabilities: vec!["funnel".into(), "cohort".into(), "kpi".into()],
                    tabs: vec!["actions".into(), "agents".into(), "rules".into(), "events".into()],
                    quick_actions: vec![
                        QuickAction { label: "Funnel analysis".into(), prompt: "Analyze the conversion funnel with drop-off rates and recommendations.".into() },
                        QuickAction { label: "KPI dashboard".into(), prompt: "Show the current KPI dashboard with MRR, DAU, churn rate, and growth rate.".into() },
                        QuickAction { label: "Cohort report".into(), prompt: "Generate a cohort retention report with weekly/monthly trends.".into() },
                    ],
                    default_config: LayeredConfig::default(),
                    terminal: DepartmentTerminalPrefs::default(),
                },
                DepartmentDef {
                    id: "distro".into(),
                    name: "Distro".into(),
                    title: "Distribution Department".into(),
                    icon: "!".into(),
                    color: "teal".into(),
                    system_prompt: "You are the Distribution department of RUSVEL.\n\nFocus: marketplace listings, SEO optimization, affiliate programs, partnerships, API distribution channels.".into(),
                    capabilities: vec!["marketplace".into(), "seo".into(), "affiliate".into()],
                    tabs: vec!["actions".into(), "agents".into(), "rules".into(), "events".into()],
                    quick_actions: vec![
                        QuickAction { label: "SEO audit".into(), prompt: "Run an SEO audit. Check keyword rankings and page performance.".into() },
                        QuickAction { label: "Marketplace listings".into(), prompt: "Review all marketplace listings with status, downloads, and revenue.".into() },
                        QuickAction { label: "Distribution strategy".into(), prompt: "Analyze distribution channels and recommend new ones.".into() },
                    ],
                    default_config: LayeredConfig::default(),
                    terminal: DepartmentTerminalPrefs::default(),
                },
                DepartmentDef {
                    id: "legal".into(),
                    name: "Legal".into(),
                    title: "Legal Department".into(),
                    icon: "\u{00a7}".into(), // §
                    color: "slate".into(),
                    system_prompt: "You are the Legal department of RUSVEL.\n\nFocus: contracts, IP protection, terms of service, GDPR compliance, licensing, privacy policies.".into(),
                    capabilities: vec!["contract".into(), "compliance".into(), "ip".into()],
                    tabs: vec!["actions".into(), "agents".into(), "rules".into(), "events".into()],
                    quick_actions: vec![
                        QuickAction { label: "Draft contract".into(), prompt: "Draft a contract. Ask me for type, parties, and terms.".into() },
                        QuickAction { label: "Compliance check".into(), prompt: "Run a compliance check for GDPR and privacy policy.".into() },
                        QuickAction { label: "IP review".into(), prompt: "Review intellectual property assets.".into() },
                    ],
                    default_config: LayeredConfig::default(),
                    terminal: DepartmentTerminalPrefs::default(),
                },
                DepartmentDef {
                    id: "support".into(),
                    name: "Support".into(),
                    title: "Support Department".into(),
                    icon: "?".into(),
                    color: "yellow".into(),
                    system_prompt: "You are the Support department of RUSVEL.\n\nFocus: customer support tickets, knowledge base, NPS tracking, auto-triage, customer success.".into(),
                    capabilities: vec!["ticket".into(), "knowledge".into(), "nps".into()],
                    tabs: vec!["actions".into(), "agents".into(), "skills".into(), "rules".into(), "events".into()],
                    quick_actions: vec![
                        QuickAction { label: "Open tickets".into(), prompt: "Show all open support tickets prioritized by urgency.".into() },
                        QuickAction { label: "Write KB article".into(), prompt: "Write a knowledge base article. Ask me for the topic.".into() },
                        QuickAction { label: "NPS survey".into(), prompt: "Analyze recent NPS survey results with score breakdown and themes.".into() },
                    ],
                    default_config: LayeredConfig::default(),
                    terminal: DepartmentTerminalPrefs::default(),
                },
                DepartmentDef {
                    id: "infra".into(),
                    name: "Infra".into(),
                    title: "Infra Department".into(),
                    icon: ">".into(),
                    color: "red".into(),
                    system_prompt: "You are the Infrastructure department of RUSVEL.\n\nFocus: CI/CD pipelines, deployments, monitoring, incident response, performance, cost analysis.".into(),
                    capabilities: vec!["deploy".into(), "monitor".into(), "incident".into()],
                    tabs: vec!["actions".into(), "agents".into(), "rules".into(), "events".into()],
                    quick_actions: vec![
                        QuickAction { label: "Deploy status".into(), prompt: "Show current deployment status across all services and environments.".into() },
                        QuickAction { label: "Health check".into(), prompt: "Run health checks on all monitored services.".into() },
                        QuickAction { label: "Incident report".into(), prompt: "Show open incidents with severity, timeline, and resolution status.".into() },
                    ],
                    default_config: LayeredConfig::default(),
                    terminal: DepartmentTerminalPrefs::default(),
                },
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_has_12_departments() {
        let reg = DepartmentRegistry::defaults();
        assert_eq!(reg.departments.len(), 12);
    }

    #[test]
    fn lookup_by_id() {
        let reg = DepartmentRegistry::defaults();
        let code = reg.get("code").unwrap();
        assert_eq!(code.id, "code");
        assert_eq!(code.color, "emerald");
    }

    #[test]
    fn lookup_unknown_returns_none() {
        let reg = DepartmentRegistry::defaults();
        assert!(reg.get("nonexistent").is_none());
    }

    #[test]
    fn lookup_by_id_string_ids() {
        let reg = DepartmentRegistry::defaults();
        assert_eq!(reg.get("finance").map(|d| d.id.as_str()), Some("finance"));
        assert_eq!(reg.get("gtm").map(|d| d.id.as_str()), Some("gtm"));
        assert!(reg.get("nope").is_none());
    }

    #[test]
    fn code_dept_has_high_effort_default() {
        let reg = DepartmentRegistry::defaults();
        let code = reg.get("code").unwrap();
        assert_eq!(code.default_config.effort, Some("high".into()));
    }

    #[test]
    fn each_dept_has_quick_actions() {
        let reg = DepartmentRegistry::defaults();
        for dept in &reg.departments {
            assert!(
                !dept.quick_actions.is_empty(),
                "{} has no quick actions",
                dept.id
            );
        }
    }
}
