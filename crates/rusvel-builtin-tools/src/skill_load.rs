//! `skill.load`: pull a department's full skill prompt template on demand.
//!
//! Issue #18 (progressive disclosure): each department's `SkillContribution`
//! name + description is injected into its chat system prompt (cheap, always
//! present), while the full `prompt_template` — potentially large — is only
//! fetched when the agent actually decides to use the skill.

use std::collections::HashMap;
use std::sync::Arc;

use rusvel_core::department::DepartmentManifest;
use rusvel_core::domain::{Content, ToolDefinition, ToolResult};
use rusvel_core::error::RusvelError;
use rusvel_tool::ToolRegistry;
use serde_json::json;

pub async fn register(
    registry: &ToolRegistry,
    dept_manifests: HashMap<String, DepartmentManifest>,
) {
    registry
        .register_with_handler(
            ToolDefinition {
                name: "skill.load".into(),
                description: "Load the full prompt template for a department skill. Skill \
                    names and short descriptions are listed in the department's system prompt \
                    under '--- Skills ---'; call this to fetch the complete template (with its \
                    {{placeholder}} fields) before using it."
                    .into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "department_id": {
                            "type": "string",
                            "description": "Department id, e.g. \"content\", \"harvest\""
                        },
                        "name": {
                            "type": "string",
                            "description": "Skill name as listed in the system prompt, e.g. \"Blog Draft\""
                        }
                    },
                    "required": ["department_id", "name"]
                }),
                searchable: true,
                metadata: json!({"category": "skills", "read_only": true}),
            },
            Arc::new(move |args: serde_json::Value| {
                let dept_manifests = dept_manifests.clone();
                Box::pin(async move {
                    let department_id = args["department_id"].as_str().ok_or_else(|| {
                        RusvelError::Tool("skill.load: department_id required".into())
                    })?;
                    let name = args["name"]
                        .as_str()
                        .ok_or_else(|| RusvelError::Tool("skill.load: name required".into()))?;

                    let manifest = dept_manifests.get(department_id).ok_or_else(|| {
                        RusvelError::Tool(format!(
                            "skill.load: unknown department_id {department_id:?}"
                        ))
                    })?;
                    let skill = manifest
                        .skills
                        .iter()
                        .find(|s| s.name.eq_ignore_ascii_case(name))
                        .ok_or_else(|| {
                            RusvelError::Tool(format!(
                                "skill.load: no skill named {name:?} in department {department_id:?}"
                            ))
                        })?;

                    Ok(ToolResult {
                        success: true,
                        output: Content::text(skill.prompt_template.clone()),
                        metadata: json!({"department_id": department_id, "name": skill.name}),
                    })
                })
            }),
        )
        .await
        .unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusvel_core::department::SkillContribution;
    use rusvel_core::ports::ToolPort;

    fn manifest_with_skill(dept_id: &str, name: &str, template: &str) -> DepartmentManifest {
        let mut manifest = DepartmentManifest::new(dept_id, dept_id);
        manifest.skills = vec![SkillContribution {
            name: name.into(),
            description: "test skill".into(),
            prompt_template: template.into(),
        }];
        manifest
    }

    #[tokio::test]
    async fn loads_full_prompt_template_for_known_skill() {
        let mut manifests = HashMap::new();
        manifests.insert(
            "content".to_string(),
            manifest_with_skill("content", "Blog Draft", "Write about {{topic}}"),
        );

        let registry = ToolRegistry::new();
        register(&registry, manifests).await;

        let result = registry
            .call(
                "skill.load",
                json!({"department_id": "content", "name": "Blog Draft"}),
            )
            .await
            .unwrap();

        assert!(result.success);
        match &result.output.parts[0] {
            rusvel_core::domain::Part::Text(s) => assert_eq!(s, "Write about {{topic}}"),
            _ => panic!("expected text part"),
        }
    }

    #[tokio::test]
    async fn unknown_department_or_skill_name_errors() {
        let registry = ToolRegistry::new();
        register(&registry, HashMap::new()).await;

        let err = registry
            .call(
                "skill.load",
                json!({"department_id": "content", "name": "Blog Draft"}),
            )
            .await;
        assert!(err.is_err());
    }
}
