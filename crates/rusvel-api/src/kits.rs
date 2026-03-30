//! Built-in starter kits — one-click bundles that seed agents, skills, rules, workflows.
//!
//! Install paths mirror [`crate::capability`] persistence (`ObjectStore` namespaces).

use std::sync::Arc;
use std::sync::LazyLock;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Serialize;

use rusvel_core::domain::{KitEntity, StarterKit};
use rusvel_core::ports::StoragePort;

use crate::AppState;

static BUILTIN_KITS: LazyLock<Vec<StarterKit>> = LazyLock::new(|| {
    vec![
        kit_indie_saas(),
        kit_freelancer(),
        kit_open_source_maintainer(),
    ]
});

#[derive(Debug, Serialize)]
pub struct StarterKitListItem {
    pub id: String,
    pub name: String,
    pub description: String,
    pub target_audience: String,
    pub departments: Vec<String>,
    pub entity_count: usize,
}

#[derive(Debug, Serialize)]
pub struct KitInstallResult {
    pub kit_id: String,
    pub installed: Vec<String>,
}

fn kit_indie_saas() -> StarterKit {
    StarterKit {
        id: "indie-saas".into(),
        name: "Indie SaaS".into(),
        description:
            "Ship and grow a SaaS: content, code quality, funnel, and product feedback loops."
                .into(),
        target_audience: "indie saas".into(),
        departments: vec![
            "content".into(),
            "code".into(),
            "growth".into(),
            "product".into(),
        ],
        entities: vec![
            KitEntity {
                kind: "agent".into(),
                department: "content".into(),
                name: "saas-content-writer".into(),
                definition: serde_json::json!({
                    "name": "saas-content-writer",
                    "role": "B2B SaaS content & changelog writer",
                    "instructions": "You write marketing copy, blog posts, and release notes for an indie SaaS. Prefer clear, benefit-led headlines, concrete examples, and a confident but honest tone. Use {{input}} context when the user describes the feature or audience.",
                    "default_model": {"provider": "Claude", "model": "sonnet"},
                    "allowed_tools": ["Read", "Write", "Glob", "Grep"],
                    "capabilities": [],
                    "budget_limit": null,
                    "metadata": {}
                }),
                metadata: serde_json::Value::default(),
            },
            KitEntity {
                kind: "agent".into(),
                department: "code".into(),
                name: "saas-code-reviewer".into(),
                definition: serde_json::json!({
                    "name": "saas-code-reviewer",
                    "role": "Pragmatic code reviewer for small teams",
                    "instructions": "Review diffs and code for security, correctness, and maintainability. Call out breaking issues first, then nits. Suggest tests when behavior is non-obvious. Match the project's style and keep feedback actionable.",
                    "default_model": {"provider": "Claude", "model": "sonnet"},
                    "allowed_tools": ["Read", "Write", "Edit", "Glob", "Grep", "Bash"],
                    "capabilities": [],
                    "budget_limit": null,
                    "metadata": {}
                }),
                metadata: serde_json::Value::default(),
            },
            KitEntity {
                kind: "skill".into(),
                department: "growth".into(),
                name: "growth-funnel-snapshot".into(),
                definition: serde_json::json!({
                    "name": "growth-funnel-snapshot",
                    "description": "Summarize funnel metrics and suggest one experiment.",
                    "prompt_template": "Given the funnel data or description below, produce: (1) a one-paragraph health summary, (2) 3 hypotheses for the biggest leak, (3) one concrete next experiment with success metric and duration. Data: {{input}}",
                    "metadata": {}
                }),
                metadata: serde_json::Value::default(),
            },
            KitEntity {
                kind: "skill".into(),
                department: "growth".into(),
                name: "growth-weekly-experiment".into(),
                definition: serde_json::json!({
                    "name": "growth-weekly-experiment",
                    "description": "Turn a goal into a weekly growth experiment.",
                    "prompt_template": "The user wants to improve: {{input}}\n\nPropose one weekly growth experiment: hypothesis, channel, audience, copy angle, metric, sample size, and stop criteria. Keep it feasible for a solo founder.",
                    "metadata": {}
                }),
                metadata: serde_json::Value::default(),
            },
            KitEntity {
                kind: "rule".into(),
                department: "product".into(),
                name: "product-feedback-loop".into(),
                definition: serde_json::json!({
                    "name": "product-feedback-loop",
                    "content": "When discussing features, tie ideas to user evidence (quotes, tickets, support patterns). Prefer small shipped experiments over big bets. Surface risks and trade-offs explicitly.",
                    "enabled": true,
                    "metadata": {}
                }),
                metadata: serde_json::Value::default(),
            },
        ],
        metadata: serde_json::Value::default(),
    }
}

fn kit_freelancer() -> StarterKit {
    StarterKit {
        id: "freelancer".into(),
        name: "Freelancer".into(),
        description:
            "Find clients, write proposals, stay paid: harvest, proposals, outreach, and invoicing."
                .into(),
        target_audience: "freelancer".into(),
        departments: vec![
            "harvest".into(),
            "gtm".into(),
            "finance".into(),
            "content".into(),
        ],
        entities: vec![
            KitEntity {
                kind: "skill".into(),
                department: "harvest".into(),
                name: "harvest-pipeline-scan".into(),
                definition: serde_json::json!({
                    "name": "harvest-pipeline-scan",
                    "description": "Turn a niche or market into a shortlist of opportunities.",
                    "prompt_template": "From this niche or market description: {{input}}\n\nPropose 5 concrete opportunity angles (client type, pain, deliverable, rough price band), ranked by fit for a solo freelancer.",
                    "metadata": {}
                }),
                metadata: serde_json::Value::default(),
            },
            KitEntity {
                kind: "agent".into(),
                department: "gtm".into(),
                name: "proposal-writer".into(),
                definition: serde_json::json!({
                    "name": "proposal-writer",
                    "role": "Freelance proposal and SOW writer",
                    "instructions": "You write concise proposals and statements of work. Include scope, deliverables, timeline, assumptions, and pricing structure. Ask clarifying questions only when blocking. Keep tone professional and confident.",
                    "default_model": {"provider": "Claude", "model": "sonnet"},
                    "allowed_tools": ["Read", "Write", "Glob", "Grep"],
                    "capabilities": [],
                    "budget_limit": null,
                    "metadata": {}
                }),
                metadata: serde_json::Value::default(),
            },
            KitEntity {
                kind: "rule".into(),
                department: "finance".into(),
                name: "invoice-hygiene".into(),
                definition: serde_json::json!({
                    "name": "invoice-hygiene",
                    "content": "When discussing client work, remind about deposits, milestones, payment terms, and scope creep. Prefer written agreements and explicit change orders.",
                    "enabled": true,
                    "metadata": {}
                }),
                metadata: serde_json::Value::default(),
            },
            KitEntity {
                kind: "workflow".into(),
                department: "gtm".into(),
                name: "client-outreach-follow-up".into(),
                definition: serde_json::json!({
                    "name": "client-outreach-follow-up",
                    "description": "Draft outreach then refine for a specific lead.",
                    "steps": [
                        {
                            "agent_name": "proposal-writer",
                            "prompt_template": "Draft a short cold outreach email (under 120 words) for this lead: {{input}}",
                            "step_type": "sequential"
                        },
                        {
                            "agent_name": "proposal-writer",
                            "prompt_template": "Now tighten the email: remove fluff, add one specific proof point, one clear CTA. Original context: {{input}}",
                            "step_type": "sequential"
                        }
                    ],
                    "metadata": {}
                }),
                metadata: serde_json::Value::default(),
            },
        ],
        metadata: serde_json::Value::default(),
    }
}

fn kit_open_source_maintainer() -> StarterKit {
    StarterKit {
        id: "open-source-maintainer".into(),
        name: "Open Source Maintainer".into(),
        description: "Triage issues, ship releases, and grow community around an OSS project."
            .into(),
        target_audience: "open source maintainer".into(),
        departments: vec!["code".into(), "content".into(), "growth".into()],
        entities: vec![
            KitEntity {
                kind: "agent".into(),
                department: "code".into(),
                name: "issue-triage".into(),
                definition: serde_json::json!({
                    "name": "issue-triage",
                    "role": "OSS issue triage and prioritization",
                    "instructions": "Classify issues (bug, feature, question, duplicate), suggest labels, priority, and next action. Be kind to contributors. If information is missing, list exactly what to ask.",
                    "default_model": {"provider": "Claude", "model": "sonnet"},
                    "allowed_tools": ["Read", "Glob", "Grep", "WebFetch"],
                    "capabilities": [],
                    "budget_limit": null,
                    "metadata": {}
                }),
                metadata: serde_json::Value::default(),
            },
            KitEntity {
                kind: "skill".into(),
                department: "content".into(),
                name: "release-notes".into(),
                definition: serde_json::json!({
                    "name": "release-notes",
                    "description": "Generate user-facing release notes from a change list.",
                    "prompt_template": "From this list of changes/commits: {{input}}\n\nWrite release notes: title, short summary, breaking changes section (if any), and bullet list of improvements with user-facing wording.",
                    "metadata": {}
                }),
                metadata: serde_json::Value::default(),
            },
            KitEntity {
                kind: "rule".into(),
                department: "growth".into(),
                name: "community-growth".into(),
                definition: serde_json::json!({
                    "name": "community-growth",
                    "content": "When planning OSS growth, prioritize sustainable contributors, clear CONTRIBUTING.md, and respectful communication. Avoid growth hacks that burn maintainer time.",
                    "enabled": true,
                    "metadata": {}
                }),
                metadata: serde_json::Value::default(),
            },
        ],
        metadata: serde_json::Value::default(),
    }
}

fn kit_store_kind(kind: &str) -> Option<&'static str> {
    match kind.trim().to_ascii_lowercase().as_str() {
        "agent" => Some("agents"),
        "skill" => Some("skills"),
        "rule" => Some("rules"),
        "workflow" => Some("workflows"),
        _ => None,
    }
}

fn inject_kit_metadata(val: &mut serde_json::Value, engine: &str, kit_id: &str) {
    if let Some(obj) = val.as_object_mut() {
        let meta = obj
            .entry("metadata")
            .or_insert_with(|| serde_json::json!({}));
        if let Some(meta_obj) = meta.as_object_mut() {
            meta_obj.insert("engine".into(), engine.into());
            meta_obj.insert("created_by".into(), "starter-kit".into());
            meta_obj.insert("kit_id".into(), kit_id.into());
        }
    }
}

async fn install_entities(
    kit: &StarterKit,
    storage: &Arc<dyn StoragePort>,
) -> Result<Vec<String>, String> {
    let mut installed = vec![];

    for entity in &kit.entities {
        let Some(store) = kit_store_kind(&entity.kind) else {
            return Err(format!("unsupported entity kind: {}", entity.kind));
        };

        let id = uuid::Uuid::now_v7().to_string();
        let mut val = entity.definition.clone();
        inject_kit_metadata(&mut val, &entity.department, &kit.id);

        if let Some(obj) = val.as_object_mut() {
            obj.insert("id".into(), serde_json::Value::String(id.clone()));
        }

        storage
            .objects()
            .put(store, &id, val)
            .await
            .map_err(|e| e.to_string())?;

        installed.push(format!(
            "{}:{}",
            entity.kind.to_ascii_lowercase(),
            entity.name
        ));
    }

    Ok(installed)
}

/// `GET /api/kits` — list built-in starter kits (summary).
pub async fn list_kits() -> Json<Vec<StarterKitListItem>> {
    let items: Vec<StarterKitListItem> = BUILTIN_KITS
        .iter()
        .map(|k| StarterKitListItem {
            id: k.id.clone(),
            name: k.name.clone(),
            description: k.description.clone(),
            target_audience: k.target_audience.clone(),
            departments: k.departments.clone(),
            entity_count: k.entities.len(),
        })
        .collect();
    Json(items)
}

/// `GET /api/kits/{id}` — full kit definition.
pub async fn get_kit(Path(id): Path<String>) -> Result<Json<StarterKit>, (StatusCode, String)> {
    let kit = BUILTIN_KITS
        .iter()
        .find(|k| k.id == id)
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("unknown kit: {id}")))?;
    Ok(Json(kit.clone()))
}

/// `POST /api/kits/{id}/install` — install all entities into `ObjectStore`.
pub async fn install_kit(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<KitInstallResult>, (StatusCode, String)> {
    let kit = BUILTIN_KITS
        .iter()
        .find(|k| k.id == id)
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("unknown kit: {id}")))?;

    let installed = install_entities(kit, &state.storage)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    Ok(Json(KitInstallResult {
        kit_id: kit.id.clone(),
        installed,
    }))
}
