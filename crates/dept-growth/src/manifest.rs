//! Static manifest for the Growth Department.

use rusvel_core::config::LayeredConfig;
use rusvel_core::constants::DEPT_GROWTH;
use rusvel_core::department::*;

pub fn growth_manifest() -> DepartmentManifest {
    DepartmentManifest {
        id: DEPT_GROWTH.into(),
        name: "Growth Department".into(),
        description: "Funnel optimization, conversion tracking, cohort analysis, churn prediction, retention strategies, KPI dashboards".into(),
        icon: "&".into(),
        color: "orange".into(),

        system_prompt: "You are the Growth department of RUSVEL.\n\nFocus: funnel optimization, conversion tracking, cohort analysis, churn prediction, retention strategies, KPI dashboards.".into(),

        capabilities: vec!["funnel".into(), "cohort".into(), "kpi".into()],

        quick_actions: vec![
            QuickAction {
                label: "Funnel analysis".into(),
                prompt: "Analyze the conversion funnel with drop-off rates and recommendations.".into(),
            },
            QuickAction {
                label: "KPI dashboard".into(),
                prompt: "Show the current KPI dashboard with MRR, DAU, churn rate, and growth rate.".into(),
            },
            QuickAction {
                label: "Cohort report".into(),
                prompt: "Generate a cohort retention report with weekly/monthly trends.".into(),
            },
        ],

        routes: vec![],
        commands: vec![],
        tools: vec![],
        personas: vec![],
        skills: vec![],
        rules: vec![],
        jobs: vec![],

        ui: UiContribution {
            tabs: vec![
                "actions".into(),
                "agents".into(),
                "rules".into(),
                "events".into(),
            ],
            dashboard_cards: vec![],
            has_settings: false,
            custom_components: vec![],
        },

        events_produced: vec![],
        events_consumed: vec![],

        requires_ports: vec![
            PortRequirement { port: "StoragePort".into(), optional: false },
            PortRequirement { port: "EventPort".into(), optional: false },
            PortRequirement { port: "AgentPort".into(), optional: false },
            PortRequirement { port: "JobPort".into(), optional: false },
        ],

        depends_on: vec![],

        config_schema: serde_json::json!({}),
        default_config: LayeredConfig::default(),
        terminal: DepartmentTerminalPrefs::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_has_correct_id() {
        let m = growth_manifest();
        assert_eq!(m.id, "growth");
        assert_eq!(m.icon, "&");
        assert_eq!(m.color, "orange");
    }

    #[test]
    fn manifest_requires_4_ports() {
        let m = growth_manifest();
        assert_eq!(m.requires_ports.len(), 4);
        assert!(m.requires_ports.iter().all(|p| !p.optional));
    }

    #[test]
    fn manifest_has_no_deps() {
        let m = growth_manifest();
        assert!(m.depends_on.is_empty());
    }
}
