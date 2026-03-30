//! Skills CRUD — reusable prompt templates stored in `ObjectStore`.
//!
//! Skills are scoped to departments via `metadata.engine` field.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};

use crate::AppState;

const STORE_KIND: &str = "skills";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub prompt_template: String,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct SkillQuery {
    pub engine: Option<String>,
}

pub async fn list_skills(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SkillQuery>,
) -> Result<Json<Vec<SkillDefinition>>, (StatusCode, String)> {
    let all = state
        .storage
        .objects()
        .list(STORE_KIND, rusvel_core::domain::ObjectFilter::default())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut skills: Vec<SkillDefinition> = all
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect();

    if let Some(ref engine) = params.engine {
        skills.retain(|s| {
            let skill_engine = s.metadata.get("engine").and_then(|e| e.as_str());
            skill_engine == Some(engine) || skill_engine.is_none()
        });
    }

    Ok(Json(skills))
}

pub async fn create_skill(
    State(state): State<Arc<AppState>>,
    Json(mut skill): Json<SkillDefinition>,
) -> Result<(StatusCode, Json<SkillDefinition>), (StatusCode, String)> {
    if skill.id.is_empty() {
        skill.id = uuid::Uuid::now_v7().to_string();
    }
    state
        .storage
        .objects()
        .put(
            STORE_KIND,
            &skill.id,
            serde_json::to_value(&skill).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?,
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok((StatusCode::CREATED, Json(skill)))
}

pub async fn get_skill(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<SkillDefinition>, (StatusCode, String)> {
    let val = state
        .storage
        .objects()
        .get(STORE_KIND, &id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "skill not found".into()))?;
    Ok(Json(serde_json::from_value(val).map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    })?))
}

pub async fn update_skill(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(skill): Json<SkillDefinition>,
) -> Result<Json<SkillDefinition>, (StatusCode, String)> {
    state
        .storage
        .objects()
        .put(
            STORE_KIND,
            &id,
            serde_json::to_value(&skill).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?,
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(skill))
}

pub async fn delete_skill(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    state
        .storage
        .objects()
        .delete(STORE_KIND, &id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

/// Resolve a `/skill-name` invocation in a chat message.
///
/// If the message starts with `/` followed by a skill name (case-insensitive,
/// hyphens and underscores normalized), looks up the skill in `ObjectStore`,
/// interpolates `{{input}}` with the remaining text, and returns the expanded prompt.
///
/// Returns `None` if no skill matches.
pub async fn resolve_skill(state: &Arc<AppState>, engine: &str, message: &str) -> Option<String> {
    let trimmed = message.trim();
    if !trimmed.starts_with('/') || trimmed.len() < 2 {
        return None;
    }

    // Don't match common chat slash-commands that aren't skills
    let non_skill_prefixes = ["/build", "/capability", "/help", "/clear"];
    for prefix in non_skill_prefixes {
        if trimmed.starts_with(prefix) {
            return None;
        }
    }

    // Extract skill name and input: "/skill-name some input text"
    let without_slash = &trimmed[1..];
    let (skill_slug, input) = match without_slash.find(char::is_whitespace) {
        Some(pos) => (&without_slash[..pos], without_slash[pos..].trim()),
        None => (without_slash, ""),
    };

    // Normalize: lowercase, treat - and _ as equivalent
    let normalized_slug = skill_slug.to_lowercase().replace('_', "-");

    // Load skills for this engine
    let all = state
        .storage
        .objects()
        .list(STORE_KIND, rusvel_core::domain::ObjectFilter::default())
        .await
        .ok()?;

    let skills: Vec<SkillDefinition> = all
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .filter(|s: &SkillDefinition| {
            let skill_engine = s.metadata.get("engine").and_then(|e| e.as_str());
            skill_engine == Some(engine) || skill_engine.is_none()
        })
        .collect();

    // Match by normalized name (spaces/underscores → hyphens, case-insensitive)
    let skill = skills.into_iter().find(|s| {
        let normalized_name = s.name.to_lowercase().replace([' ', '_'], "-");
        normalized_name == normalized_slug
    })?;

    // Interpolate {{input}} in the prompt template
    let prompt = skill.prompt_template.replace("{{input}}", input);
    Some(prompt)
}

/// Resolve a skill by display name/slug for playbook steps (no leading `/`).
pub async fn resolve_skill_by_name_for_playbook(
    state: &Arc<AppState>,
    engine: &str,
    skill_name: &str,
    input: &str,
) -> Result<String, String> {
    let normalized_slug = skill_name
        .trim()
        .to_lowercase()
        .replace('_', "-")
        .replace(' ', "-");

    let all = state
        .storage
        .objects()
        .list(STORE_KIND, rusvel_core::domain::ObjectFilter::default())
        .await
        .map_err(|e| e.to_string())?;

    let skills: Vec<SkillDefinition> = all
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .filter(|s: &SkillDefinition| {
            let skill_engine = s.metadata.get("engine").and_then(|e| e.as_str());
            skill_engine == Some(engine) || skill_engine.is_none()
        })
        .collect();

    let skill = skills
        .into_iter()
        .find(|s| {
            let normalized_name = s.name.to_lowercase().replace([' ', '_'], "-");
            normalized_name == normalized_slug
        })
        .ok_or_else(|| format!("skill '{skill_name}' not found for engine '{engine}'"))?;

    Ok(skill.prompt_template.replace("{{input}}", input))
}
