//! Playbooks — predefined multi-step pipelines (delegate_agent + FlowEngine).

use std::collections::HashMap;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::Utc;
use serde::Deserialize;
use serde_json::json;

use crate::AppState;
use crate::skills;
use rusvel_core::domain::ObjectFilter;
use rusvel_core::domain::{
    AgentProfile, Content, Part, Playbook, PlaybookAction, PlaybookRun, PlaybookRunStatus,
};
use rusvel_core::id::FlowId;

type ApiResult<T> = Result<Json<T>, (StatusCode, String)>;

const PLAYBOOK_OBJECT_KIND: &str = "playbooks";

fn err(status: StatusCode, msg: impl Into<String>) -> (StatusCode, String) {
    (status, msg.into())
}

// ── Store (runs in-memory; definitions persisted in ObjectStore) ─────────────

struct PlaybookRunStore {
    runs: std::sync::RwLock<HashMap<String, PlaybookRun>>,
}

impl PlaybookRunStore {
    fn new() -> Self {
        Self {
            runs: std::sync::RwLock::new(HashMap::new()),
        }
    }
}

fn run_store() -> &'static PlaybookRunStore {
    use std::sync::OnceLock;
    static S: OnceLock<PlaybookRunStore> = OnceLock::new();
    S.get_or_init(PlaybookRunStore::new)
}

fn builtin_playbooks() -> Vec<Playbook> {
    vec![
        Playbook {
            id: "builtin-content-from-code".into(),
            name: "Content from Code".into(),
            description: "Analyze the codebase, draft a blog post, then review.".into(),
            category: "content".into(),
            steps: vec![
                rusvel_core::domain::PlaybookStep {
                    name: "Analyze".into(),
                    description: "Survey the repo for themes worth writing about.".into(),
                    action: PlaybookAction::Agent {
                        persona: Some("CodeAnalyst".into()),
                        prompt_template: "Analyze the current repository for 3–5 concrete themes suitable for a technical blog. Focus on architecture, interesting patterns, and lessons learned. Summarize in bullet points.\n\nContext:\n{{last_output}}".into(),
                        tools: vec![
                            "read_file".into(),
                            "glob".into(),
                            "grep".into(),
                        ],
                    },
                    metadata: serde_json::Value::default(),
                },
                rusvel_core::domain::PlaybookStep {
                    name: "Draft".into(),
                    description: "Turn the analysis into a draft post.".into(),
                    action: PlaybookAction::Agent {
                        persona: Some("TechnicalWriter".into()),
                        prompt_template: "Write a concise technical blog draft (with title and sections) based on this analysis. Audience: senior engineers.\n\nAnalysis:\n{{last_output}}".into(),
                        tools: vec!["read_file".into(), "write_file".into()],
                    },
                    metadata: serde_json::Value::default(),
                },
                rusvel_core::domain::PlaybookStep {
                    name: "Review".into(),
                    description: "Review for clarity and accuracy.".into(),
                    action: PlaybookAction::Agent {
                        persona: Some("Editor".into()),
                        prompt_template: "Review the draft below. List concrete improvements (max 5) and a short revised summary.\n\nDraft:\n{{last_output}}".into(),
                        tools: vec![],
                    },
                    metadata: serde_json::Value::default(),
                },
            ],
            metadata: json!({"builtin": true}),
        },
        Playbook {
            id: "builtin-opportunity-pipeline".into(),
            name: "Opportunity Pipeline".into(),
            description: "Scan sources, score opportunities, draft proposals.".into(),
            category: "harvest".into(),
            steps: vec![
                rusvel_core::domain::PlaybookStep {
                    name: "Scan".into(),
                    description: "Identify candidate opportunities from configured sources.".into(),
                    action: PlaybookAction::Agent {
                        persona: Some("Sourcer".into()),
                        prompt_template: "Use harvest_scan with the current session_id and configured sources (e.g. mock, upwork, cdp) plus query when required; then harvest_list to confirm stored opportunities. If tools are unavailable, outline the scan you would run.\n\nPrior context:\n{{last_output}}".into(),
                        tools: vec![
                            "harvest_scan".into(),
                            "harvest_list".into(),
                            "read_file".into(),
                            "grep".into(),
                        ],
                    },
                    metadata: serde_json::Value::default(),
                },
                rusvel_core::domain::PlaybookStep {
                    name: "Score".into(),
                    description: "Score and prioritize leads.".into(),
                    action: PlaybookAction::Agent {
                        persona: Some("Scorer".into()),
                        prompt_template: "Use harvest_score on specific opportunity IDs from harvest_list when needed. Given the scan summary below, propose a scoring rubric (0–10) and rank the top 3 opportunities with rationale.\n\n{{last_output}}".into(),
                        tools: vec!["harvest_list".into(), "harvest_score".into()],
                    },
                    metadata: serde_json::Value::default(),
                },
                rusvel_core::domain::PlaybookStep {
                    name: "Proposals".into(),
                    description: "Draft outreach or proposal snippets.".into(),
                    action: PlaybookAction::Agent {
                        persona: Some("BizDev".into()),
                        prompt_template: "Use harvest_propose for the top opportunity when session_id and opportunity_id are known. Otherwise draft short proposal or outreach snippets (2–3 paragraphs total) from:\n\n{{last_output}}".into(),
                        tools: vec!["harvest_propose".into(), "harvest_list".into()],
                    },
                    metadata: serde_json::Value::default(),
                },
            ],
            metadata: json!({"builtin": true}),
        },
        Playbook {
            id: "builtin-harvest-capability-hint".into(),
            name: "Harvest → capability gap".into(),
            description: "After a harvest pass, spot tooling gaps and point to capability build when useful.".into(),
            category: "harvest".into(),
            steps: vec![rusvel_core::domain::PlaybookStep {
                name: "Review".into(),
                description: "List opportunities and suggest capability follow-up.".into(),
                action: PlaybookAction::Agent {
                    persona: Some("Sourcer".into()),
                    prompt_template: "Call harvest_list for the session. From the highest-score rows, infer tech/skill gaps vs a lean solo stack. If a gap is worth automating, say the operator can run chat command !capability or POST /api/capability/build (see API help).\n\nContext:\n{{last_output}}"
                        .into(),
                    tools: vec!["harvest_list".into()],
                },
                metadata: serde_json::Value::default(),
            }],
            metadata: json!({"builtin": true, "triggers": ["post-harvest", "high-score-opportunity"]}),
        },
        Playbook {
            id: "builtin-daily-brief".into(),
            name: "Daily Brief".into(),
            description: "Query department context, summarize, present an executive brief.".into(),
            category: "forge".into(),
            steps: vec![
                rusvel_core::domain::PlaybookStep {
                    name: "Gather".into(),
                    description: "Collect signals across departments (as available).".into(),
                    action: PlaybookAction::Agent {
                        persona: Some("ChiefOfStaff".into()),
                        prompt_template: "Summarize what each RUSVEL department (code, content, harvest, finance, growth, gtm, product, support, infra, legal, distro, forge) would report as its top 1–2 items today. If unknown, say 'no signal' for that dept.\n\nNotes:\n{{last_output}}".into(),
                        tools: vec!["read_file".into(), "grep".into()],
                    },
                    metadata: serde_json::Value::default(),
                },
                rusvel_core::domain::PlaybookStep {
                    name: "Summarize".into(),
                    description: "Compress into themes and risks.".into(),
                    action: PlaybookAction::Agent {
                        persona: Some("Analyst".into()),
                        prompt_template: "From the department notes below, extract themes, risks, and dependencies (bullet list).\n\n{{last_output}}".into(),
                        tools: vec![],
                    },
                    metadata: serde_json::Value::default(),
                },
                rusvel_core::domain::PlaybookStep {
                    name: "Present".into(),
                    description: "Executive one-pager.".into(),
                    action: PlaybookAction::Agent {
                        persona: Some("ExecutiveBrief".into()),
                        prompt_template: "Produce a tight executive brief (max 200 words): headline, 3 priorities, 1 risk, 1 ask.\n\nInput:\n{{last_output}}".into(),
                        tools: vec![],
                    },
                    metadata: serde_json::Value::default(),
                },
            ],
            metadata: json!({"builtin": true}),
        },
    ]
}

async fn list_stored_playbooks(state: &AppState) -> Result<Vec<Playbook>, String> {
    let rows = state
        .storage
        .objects()
        .list(PLAYBOOK_OBJECT_KIND, ObjectFilter::default())
        .await
        .map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .filter_map(|v| serde_json::from_value::<Playbook>(v).ok())
        .collect())
}

async fn all_playbooks(state: &AppState) -> Result<Vec<Playbook>, String> {
    let mut out = builtin_playbooks();
    out.extend(list_stored_playbooks(state).await?);
    Ok(out)
}

async fn resolve_playbook(state: &AppState, id: &str) -> Option<Playbook> {
    if let Some(p) = builtin_playbooks().into_iter().find(|b| b.id == id) {
        return Some(p);
    }
    let val = state
        .storage
        .objects()
        .get(PLAYBOOK_OBJECT_KIND, id)
        .await
        .ok()??;
    serde_json::from_value(val).ok()
}

fn content_to_string(content: &Content) -> String {
    content
        .parts
        .iter()
        .filter_map(|p| match p {
            Part::Text(t) => Some(t.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn interpolate(template: &str, ctx: &serde_json::Value) -> String {
    let mut s = template.to_string();
    if let Some(obj) = ctx.as_object() {
        for (k, v) in obj {
            let mut pat = String::from("{{");
            pat.push_str(k);
            pat.push_str("}}");
            let rep = match v {
                serde_json::Value::String(x) => x.clone(),
                serde_json::Value::Null => String::new(),
                x => x.to_string(),
            };
            s = s.replace(&pat, &rep);
        }
    }
    s
}

fn playbook_department(playbook: &Playbook) -> &str {
    playbook
        .metadata
        .get("department")
        .and_then(|v| v.as_str())
        .unwrap_or("forge")
}

async fn load_agent_by_name(state: &AppState, name: &str) -> Option<AgentProfile> {
    let all = state
        .storage
        .objects()
        .list("agents", ObjectFilter::default())
        .await
        .ok()?;
    all.into_iter()
        .filter_map(|v| serde_json::from_value::<AgentProfile>(v).ok())
        .find(|a| a.name.eq_ignore_ascii_case(name))
}

async fn run_step(
    state: &Arc<AppState>,
    playbook: &Playbook,
    step: &rusvel_core::domain::PlaybookStep,
    ctx: &mut serde_json::Value,
) -> Result<serde_json::Value, String> {
    match &step.action {
        PlaybookAction::Agent {
            persona,
            prompt_template,
            tools,
        } => {
            let mut prompt = interpolate(prompt_template, ctx);
            if let Some(re) = ctx.get("rules_excerpt").and_then(|v| v.as_str()) {
                if !re.is_empty() {
                    prompt = format!("--- Rules context ---\n{re}\n---\n{prompt}");
                }
            }
            let mut args = json!({
                "prompt": prompt,
                "tools": tools,
            });
            if let Some(p) = persona {
                args["persona"] = json!(p);
            }
            let tr = state
                .tools
                .call("delegate_agent", args)
                .await
                .map_err(|e| e.to_string())?;
            let text = content_to_string(&tr.output);
            ctx["last_output"] = json!(text);
            Ok(json!({
                "step": step.name,
                "kind": "agent",
                "success": tr.success,
                "output": text,
            }))
        }
        PlaybookAction::Flow {
            flow_id,
            input_mapping,
        } => {
            let engine = state
                .flow_engine
                .as_ref()
                .map(|e| e.as_ref())
                .ok_or_else(|| "Flow engine not available".to_string())?;
            let fid = flow_id
                .parse::<uuid::Uuid>()
                .map(FlowId::from_uuid)
                .map_err(|_| "invalid flow_id".to_string())?;
            let trigger = if let Some(raw) = input_mapping {
                serde_json::from_str(raw).unwrap_or_else(|_| {
                    json!({ "context": ctx.clone(), "last_output": ctx.get("last_output").cloned().unwrap_or(json!(null)) })
                })
            } else {
                json!({ "context": ctx.clone() })
            };
            let execution = engine
                .run_flow(&fid, trigger)
                .await
                .map_err(|e| e.to_string())?;
            let v = serde_json::to_value(&execution).map_err(|e| e.to_string())?;
            ctx["last_output"] = v.clone();
            Ok(json!({
                "step": step.name,
                "kind": "flow",
                "execution": v,
            }))
        }
        PlaybookAction::Approval { message } => Ok(json!({
            "step": step.name,
            "kind": "approval",
            "message": message,
            "status": "awaiting_approval",
        })),
        PlaybookAction::Skill {
            skill_name,
            input,
            department,
        } => {
            let dept = if department.is_empty() {
                playbook_department(playbook)
            } else {
                department.as_str()
            };
            let mut prompt = skills::resolve_skill_by_name_for_playbook(
                state,
                dept,
                skill_name.as_str(),
                input.as_str(),
            )
            .await?;
            prompt = interpolate(&prompt, ctx);
            if let Some(re) = ctx.get("rules_excerpt").and_then(|v| v.as_str()) {
                if !re.is_empty() {
                    prompt = format!("--- Rules context ---\n{re}\n---\n{prompt}");
                }
            }
            let args = json!({
                "prompt": prompt,
                "tools": Vec::<String>::new() as Vec<String>,
            });
            let tr = state
                .tools
                .call("delegate_agent", args)
                .await
                .map_err(|e| e.to_string())?;
            let text = content_to_string(&tr.output);
            ctx["last_output"] = json!(text);
            Ok(json!({
                "step": step.name,
                "kind": "skill",
                "success": tr.success,
                "output": text,
            }))
        }
        PlaybookAction::AgentProfile {
            agent_name,
            prompt_template,
            tools,
        } => {
            let agent = load_agent_by_name(state, agent_name.as_str())
                .await
                .ok_or_else(|| format!("agent '{agent_name}' not found"))?;
            let mut body = interpolate(prompt_template, ctx);
            if let Some(re) = ctx.get("rules_excerpt").and_then(|v| v.as_str()) {
                if !re.is_empty() {
                    body = format!("--- Rules context ---\n{re}\n---\n{body}");
                }
            }
            let full_prompt = if agent.instructions.is_empty() {
                body
            } else {
                format!("<system>\n{}\n</system>\n\n{}", agent.instructions, body)
            };
            let tool_list = if tools.is_empty() {
                agent.allowed_tools.clone()
            } else {
                tools.clone()
            };
            let mut args = json!({
                "prompt": full_prompt,
                "tools": tool_list,
            });
            if let Some(p) = agent.metadata.get("persona").and_then(|v| v.as_str()) {
                args["persona"] = json!(p);
            }
            let tr = state
                .tools
                .call("delegate_agent", args)
                .await
                .map_err(|e| e.to_string())?;
            let text = content_to_string(&tr.output);
            ctx["last_output"] = json!(text);
            Ok(json!({
                "step": step.name,
                "kind": "agent_profile",
                "success": tr.success,
                "output": text,
            }))
        }
        PlaybookAction::RulesAppend {
            department,
            rule_names,
        } => {
            let mut rules = crate::rules::load_rules_for_engine(state, department.as_str()).await;
            if !rule_names.is_empty() {
                rules.retain(|r| {
                    rule_names
                        .iter()
                        .any(|n| r.name.eq_ignore_ascii_case(n.as_str()))
                });
            }
            let text: String = rules.iter().fold(String::new(), |mut acc, r| {
                use std::fmt::Write as _;
                let _ = writeln!(acc, "[{}]: {}", r.name, r.content);
                acc
            });
            ctx["rules_excerpt"] = json!(text);
            Ok(json!({
                "step": step.name,
                "kind": "rules_append",
                "rule_count": rules.len(),
            }))
        }
    }
}

// ── Handlers ──────────────────────────────────────────────────────

/// `GET /api/playbooks` — list built-in and user playbooks.
pub async fn list_playbooks(State(state): State<Arc<AppState>>) -> ApiResult<Vec<Playbook>> {
    let list = all_playbooks(state.as_ref())
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(list))
}

/// `GET /api/playbooks/:id` — get one playbook.
pub async fn get_playbook(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<Playbook> {
    let p = resolve_playbook(state.as_ref(), &id)
        .await
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "playbook not found"))?;
    Ok(Json(p))
}

#[derive(Debug, Deserialize)]
pub struct CreatePlaybookBody {
    pub name: String,
    pub description: String,
    pub category: String,
    pub steps: Vec<rusvel_core::domain::PlaybookStep>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// `POST /api/playbooks` — create a user playbook (persisted in ObjectStore).
pub async fn create_playbook(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreatePlaybookBody>,
) -> ApiResult<serde_json::Value> {
    let id = uuid::Uuid::now_v7().to_string();
    if builtin_playbooks().iter().any(|b| b.id == id) {
        return Err(err(StatusCode::CONFLICT, "id collision"));
    }
    let pb = Playbook {
        id: id.clone(),
        name: body.name,
        description: body.description,
        category: body.category,
        steps: body.steps,
        metadata: if body.metadata.is_null() {
            json!({})
        } else {
            body.metadata
        },
    };
    let val = serde_json::to_value(&pb).map_err(|e| err(StatusCode::BAD_REQUEST, e.to_string()))?;
    state
        .storage
        .objects()
        .put(PLAYBOOK_OBJECT_KIND, &id, val)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(json!({ "id": id })))
}

#[derive(Debug, Deserialize)]
pub struct RunPlaybookBody {
    #[serde(default)]
    pub variables: serde_json::Value,
}

/// Start a playbook run (cron, webhook, MCP). Returns `run_id`.
pub async fn start_playbook_run(
    state: Arc<AppState>,
    playbook_id: &str,
    variables: serde_json::Value,
) -> Result<String, String> {
    let playbook = resolve_playbook(state.as_ref(), playbook_id)
        .await
        .ok_or_else(|| format!("playbook not found: {playbook_id}"))?;
    let run_id = uuid::Uuid::now_v7().to_string();

    let mut initial = variables;
    if !initial.is_object() {
        initial = json!({});
    }

    let mut run_meta = json!({});
    if let Some(rv) = initial.get("rusvel") {
        run_meta["rusvel"] = rv.clone();
    }

    let run = PlaybookRun {
        id: run_id.clone(),
        playbook_id: playbook.id.clone(),
        status: PlaybookRunStatus::Running,
        step_results: vec![],
        started_at: Utc::now(),
        completed_at: None,
        error: None,
        metadata: run_meta,
    };
    run_store()
        .runs
        .write()
        .map_err(|_| "playbook run store poisoned".to_string())?
        .insert(run_id.clone(), run.clone());

    spawn_run_with_context(state, run, playbook, initial);
    Ok(run_id)
}

/// `POST /api/playbooks/:id/run` — start a run (async).
pub async fn run_playbook(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<RunPlaybookBody>,
) -> ApiResult<serde_json::Value> {
    let run_id = start_playbook_run(state.clone(), &id, body.variables)
        .await
        .map_err(|e| err(StatusCode::BAD_REQUEST, e))?;
    Ok(Json(json!({ "run_id": run_id })))
}

fn spawn_run_with_context(
    state: Arc<AppState>,
    mut run: PlaybookRun,
    playbook: Playbook,
    ctx: serde_json::Value,
) {
    tokio::spawn(async move {
        let mut ctx = ctx;
        if !ctx.is_object() {
            ctx = json!({});
        }
        for step in &playbook.steps {
            match run_step(&state, &playbook, step, &mut ctx).await {
                Ok(mut result) => {
                    if let Some(obj) = result.as_object_mut() {
                        if obj.get("kind").and_then(|k| k.as_str()) == Some("approval") {
                            run.status = PlaybookRunStatus::Paused;
                            run.step_results.push(result);
                            run.completed_at = Some(Utc::now());
                            if let Ok(mut g) = run_store().runs.write() {
                                g.insert(run.id.clone(), run);
                            }
                            return;
                        }
                    }
                    run.step_results.push(result);
                }
                Err(e) => {
                    run.status = PlaybookRunStatus::Failed;
                    run.error = Some(e);
                    run.completed_at = Some(Utc::now());
                    if let Ok(mut g) = run_store().runs.write() {
                        g.insert(run.id.clone(), run);
                    }
                    return;
                }
            }
        }
        run.status = PlaybookRunStatus::Completed;
        run.completed_at = Some(Utc::now());
        if let Ok(mut g) = run_store().runs.write() {
            g.insert(run.id.clone(), run);
        }
    });
}

/// `GET /api/playbooks/runs/:run_id` — status and step results.
pub async fn get_run(Path(run_id): Path<String>) -> ApiResult<PlaybookRun> {
    let g = run_store()
        .runs
        .read()
        .map_err(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "lock poisoned"))?;
    let r = g
        .get(&run_id)
        .cloned()
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "run not found"))?;
    Ok(Json(r))
}

/// `GET /api/playbooks/runs` — recent runs (newest first).
pub async fn list_runs() -> ApiResult<Vec<PlaybookRun>> {
    let g = run_store()
        .runs
        .read()
        .map_err(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "lock poisoned"))?;
    let mut runs: Vec<PlaybookRun> = g.values().cloned().collect();
    runs.sort_by_key(|a| std::cmp::Reverse(a.started_at));
    runs.truncate(100);
    Ok(Json(runs))
}
