//! Opportunity scoring — keyword-based fallback with optional LLM scoring via [`AgentPort`].
//!
//! LLM scoring uses a structured prompt (user skills, budget floor, opportunity fields) and expects
//! JSON `{"score": <0-100>, "reasoning": "..."}`. The engine stores [`Opportunity::score`] as **0.0–1.0**
//! (UI multiplies by 100 for display).

use std::sync::Arc;

use rusvel_core::ports::AgentPort;
use rusvel_core::{AgentConfig, Content, Result, RusvelError, SessionId};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::source::RawOpportunity;

/// How an opportunity was scored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScoringMethod {
    Llm,
    Keyword,
}

/// A scored opportunity ready for pipeline insertion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoredOpportunity {
    pub raw: RawOpportunity,
    /// Normalized **0.0–1.0** (LLM 0–100 is divided by 100).
    pub score: f64,
    pub reasoning: String,
    pub scoring_method: ScoringMethod,
}

/// Scores raw opportunities by skill match, budget fit, and optional LLM relevance.
pub struct OpportunityScorer {
    agent: Option<Arc<dyn AgentPort>>,
    skills: Vec<String>,
    min_budget: Option<f64>,
    scoring_session: Option<SessionId>,
    /// Recent won/lost lines appended to the LLM prompt (S-044).
    outcome_hints: Vec<String>,
    /// Optional operator context (e.g. [`rusvel_core::UserProfile::to_system_prompt`] summary).
    user_profile_prompt: Option<String>,
}

impl OpportunityScorer {
    pub fn new(
        agent: Option<Arc<dyn AgentPort>>,
        skills: Vec<String>,
        min_budget: Option<f64>,
    ) -> Self {
        Self {
            agent,
            skills,
            min_budget,
            scoring_session: None,
            outcome_hints: Vec::new(),
            user_profile_prompt: None,
        }
    }

    /// Use this session ID for the ephemeral [`AgentPort`] run (observability / billing alignment).
    pub fn with_scoring_session(mut self, session_id: SessionId) -> Self {
        self.scoring_session = Some(session_id);
        self
    }

    pub fn with_outcome_hints(mut self, hints: Vec<String>) -> Self {
        self.outcome_hints = hints;
        self
    }

    pub fn with_user_profile_prompt(mut self, prompt: Option<String>) -> Self {
        self.user_profile_prompt = prompt;
        self
    }

    /// Score a raw opportunity. Uses LLM if available, otherwise keyword matching.
    pub async fn score(&self, raw: &RawOpportunity) -> Result<ScoredOpportunity> {
        if let Some(agent) = &self.agent {
            match self.score_with_agent(agent, raw).await {
                Ok(scored) => return Ok(scored),
                Err(e) => {
                    tracing::warn!("LLM scoring failed, falling back to keywords: {e}");
                }
            }
        }

        Ok(self.score_with_keywords(raw))
    }

    /// Keyword-based fallback scoring.
    fn score_with_keywords(&self, raw: &RawOpportunity) -> ScoredOpportunity {
        if self.skills.is_empty() {
            return ScoredOpportunity {
                raw: raw.clone(),
                score: 0.5,
                reasoning: "No skills configured for matching".into(),
                scoring_method: ScoringMethod::Keyword,
            };
        }

        let text = format!("{} {}", raw.title, raw.description).to_lowercase();
        let raw_skills_lower: Vec<String> = raw.skills.iter().map(|s| s.to_lowercase()).collect();

        let mut matched = Vec::new();
        for skill in &self.skills {
            let skill_lower = skill.to_lowercase();
            if text.contains(&skill_lower) || raw_skills_lower.contains(&skill_lower) {
                matched.push(skill.clone());
            }
        }

        let skill_score = matched.len() as f64 / self.skills.len() as f64;

        let budget_bonus = if let (Some(budget_str), Some(min)) = (&raw.budget, self.min_budget) {
            let numeric: f64 = budget_str
                .chars()
                .filter(|c| c.is_ascii_digit() || *c == '.')
                .collect::<String>()
                .parse()
                .unwrap_or(0.0);
            if numeric >= min { 0.1 } else { 0.0 }
        } else {
            0.0
        };

        let score = (skill_score + budget_bonus).min(1.0);
        let reasoning = if matched.is_empty() {
            "No skill matches found".into()
        } else {
            format!("Matched skills: {}", matched.join(", "))
        };

        ScoredOpportunity {
            raw: raw.clone(),
            score,
            reasoning,
            scoring_method: ScoringMethod::Keyword,
        }
    }

    /// LLM-based scoring via [`AgentPort`].
    async fn score_with_agent(
        &self,
        agent: &Arc<dyn AgentPort>,
        raw: &RawOpportunity,
    ) -> Result<ScoredOpportunity> {
        let prompt = build_llm_scoring_prompt(
            raw,
            &self.skills,
            self.min_budget,
            &self.outcome_hints,
            self.user_profile_prompt.as_deref(),
        );

        let session_id = self.scoring_session.unwrap_or_else(SessionId::new);
        let config = AgentConfig {
            profile_id: None,
            session_id,
            model: None,
            tools: vec![],
            instructions: Some(
                "You evaluate freelance job listings for fit with the user's skills. Reply with JSON only."
                    .into(),
            ),
            budget_limit: Some(0.05),
            max_iterations: None,
            permission_mode: Default::default(),
        metadata: serde_json::json!({ "task": "harvest_opportunity_score" }),
        };

        let run_id = agent.create(config).await?;
        let output = agent.run(&run_id, Content::text(prompt)).await?;

        let text = output
            .content
            .parts
            .iter()
            .filter_map(|p| match p {
                rusvel_core::Part::Text(t) => Some(t.as_str()),
                _ => None,
            })
            .collect::<String>();

        let (score_0_1, reasoning) = parse_llm_score_response(&text)?;
        Ok(ScoredOpportunity {
            raw: raw.clone(),
            score: score_0_1,
            reasoning,
            scoring_method: ScoringMethod::Llm,
        })
    }
}

/// Structured prompt for LLM scoring (S-027).
fn build_llm_scoring_prompt(
    raw: &RawOpportunity,
    user_skills: &[String],
    min_budget: Option<f64>,
    outcome_hints: &[String],
    user_profile: Option<&str>,
) -> String {
    let skills_line = if user_skills.is_empty() {
        "(none configured)".to_string()
    } else {
        user_skills.join(", ")
    };
    let budget_line = min_budget.map_or_else(|| "none".to_string(), |b| format!("${b:.0}"));
    let posted = raw.posted_at.as_deref().unwrap_or("unknown");
    let past = if outcome_hints.is_empty() {
        String::new()
    } else {
        format!(
            "\n## Past outcomes in this workspace (calibrate — similar gigs may repeat)\n{}\n\n",
            outcome_hints.join("\n")
        )
    };
    let profile_block = user_profile
        .filter(|s| !s.is_empty())
        .map(|p| format!("## Operator profile (score for this builder)\n{p}\n\n"))
        .unwrap_or_default();

    format!(
        "{profile_block}## User skills (relevance target)\n{skills_line}\n\n\
         ## Minimum budget preference\n{budget_line}\n\n\
         {past}\
         ## Opportunity\n\
         Title: {}\n\
         Description:\n{}\n\
         Stated budget: {}\n\
         Listing skills/tags: {}\n\
         Posted: {posted}\n\
         URL: {}\n\n\
         ## Task\n\
         Score how well this opportunity matches the user's skills and budget preference.\n\
         Return **only** a JSON object (no markdown fences, no prose) with this shape:\n\
         {{\"score\": <integer 0-100>, \"reasoning\": \"<one or two sentences>\"}}\n\
         Use the full 0-100 range; 100 = ideal fit.",
        raw.title,
        raw.description,
        raw.budget.as_deref().unwrap_or("not specified"),
        if raw.skills.is_empty() {
            "(none)".to_string()
        } else {
            raw.skills.join(", ")
        },
        raw.url.as_deref().unwrap_or("none"),
    )
}

/// Extract JSON from model output; parse `score` (0-100 or 0-1) and `reasoning`.
fn parse_llm_score_response(text: &str) -> Result<(f64, String)> {
    let blob = extract_json_blob(text);
    let v: Value = serde_json::from_str(&blob).map_err(|e| {
        RusvelError::Internal(format!(
            "Failed to parse LLM score JSON: {e}; snippet: {}",
            {
                let t = text.trim();
                t.chars().take(200).collect::<String>()
            }
        ))
    })?;

    let raw_score = v
        .get("score")
        .and_then(|x| x.as_f64())
        .ok_or_else(|| RusvelError::Internal("LLM JSON missing numeric \"score\"".into()))?;

    let score_0_1 = normalize_llm_score_to_unit(raw_score);

    let reasoning = v
        .get("reasoning")
        .and_then(|x| x.as_str())
        .unwrap_or("LLM scored")
        .to_string();

    Ok((score_0_1.clamp(0.0, 1.0), reasoning))
}

fn normalize_llm_score_to_unit(raw: f64) -> f64 {
    if raw.is_nan() || raw < 0.0 {
        return 0.0;
    }
    if raw <= 1.0 {
        raw
    } else {
        (raw / 100.0).min(1.0)
    }
}

fn extract_json_blob(text: &str) -> String {
    let t = text.trim();
    if let Some(start_fence) = t.find("```") {
        let after = &t[start_fence + 3..];
        let after = after
            .strip_prefix("json")
            .or_else(|| after.strip_prefix("JSON"))
            .unwrap_or(after)
            .trim_start();
        if let Some(end) = after.find("```") {
            return after[..end].trim().to_string();
        }
    }
    if let (Some(a), Some(b)) = (t.find('{'), t.rfind('}')) {
        if a <= b {
            return t[a..=b].to_string();
        }
    }
    t.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusvel_core::domain::{AgentOutput, AgentStatus, LlmUsage};
    use rusvel_core::id::RunId;
    use std::sync::Mutex;

    #[tokio::test]
    async fn keyword_scoring_matches_skills() {
        let scorer = OpportunityScorer::new(
            None,
            vec!["rust".into(), "axum".into(), "python".into()],
            Some(2000.0),
        );

        let raw = RawOpportunity {
            title: "Build a REST API in Rust".into(),
            description: "Need Rust and Axum experience".into(),
            url: None,
            budget: Some("$5000".into()),
            skills: vec!["rust".into(), "axum".into()],
            posted_at: None,
            source_data: serde_json::json!({}),
            ..Default::default()
        };

        let scored = scorer.score(&raw).await.unwrap();
        assert!(scored.score > 0.7, "score was {}", scored.score);
        assert!(scored.reasoning.contains("rust"));
        assert_eq!(scored.scoring_method, ScoringMethod::Keyword);
    }

    #[tokio::test]
    async fn keyword_scoring_no_match() {
        let scorer = OpportunityScorer::new(None, vec!["golang".into()], None);

        let raw = RawOpportunity {
            title: "React Native App".into(),
            description: "Need React Native experience".into(),
            url: None,
            budget: None,
            skills: vec!["react-native".into()],
            posted_at: None,
            source_data: serde_json::json!({}),
            ..Default::default()
        };

        let scored = scorer.score(&raw).await.unwrap();
        assert!(scored.score < 0.1, "score was {}", scored.score);
    }

    #[test]
    fn parse_accepts_0_100_and_0_1() {
        let (a, _) = parse_llm_score_response(r#"{"score": 85, "reasoning": "x"}"#).unwrap();
        assert!((a - 0.85).abs() < 0.001);
        let (b, _) = parse_llm_score_response(r#"{"score": 0.72, "reasoning": "y"}"#).unwrap();
        assert!((b - 0.72).abs() < 0.001);
    }

    #[test]
    fn parse_strips_json_fence() {
        let text = "```json\n{\"score\": 60, \"reasoning\": \"ok\"}\n```";
        let (s, r) = parse_llm_score_response(text).unwrap();
        assert!((s - 0.6).abs() < 0.001);
        assert_eq!(r, "ok");
    }

    struct CapturingAgent {
        last_prompt: Arc<Mutex<String>>,
        response: String,
    }

    #[async_trait::async_trait]
    impl AgentPort for CapturingAgent {
        async fn create(&self, _: AgentConfig) -> Result<RunId> {
            Ok(RunId::new())
        }

        async fn run(&self, _: &RunId, input: Content) -> Result<AgentOutput> {
            let text = input
                .parts
                .iter()
                .filter_map(|p| match p {
                    rusvel_core::Part::Text(t) => Some(t.as_str()),
                    _ => None,
                })
                .collect::<String>();
            *self.last_prompt.lock().unwrap() = text;
            Ok(AgentOutput {
                run_id: RunId::new(),
                content: Content::text(self.response.clone()),
                tool_calls: 0,
                usage: LlmUsage::default(),
                cost_estimate: 0.0,
                metadata: serde_json::json!({}),
            })
        }

        async fn stop(&self, _: &RunId) -> Result<()> {
            Ok(())
        }

        async fn status(&self, _: &RunId) -> Result<AgentStatus> {
            Ok(AgentStatus::Idle)
        }
    }

    #[tokio::test]
    async fn llm_scoring_uses_structured_prompt_and_persists_parse() {
        let last = Arc::new(Mutex::new(String::new()));
        let agent = Arc::new(CapturingAgent {
            last_prompt: last.clone(),
            response: r#"{"score": 88, "reasoning": "Strong Rust overlap."}"#.into(),
        });

        let scorer = OpportunityScorer::new(
            Some(agent),
            vec!["rust".into(), "tokio".into()],
            Some(3000.0),
        );

        let raw = RawOpportunity {
            title: "Rust microservice".into(),
            description: "Async web work".into(),
            url: Some("https://jobs.example/1".into()),
            budget: Some("$4000".into()),
            skills: vec!["rust".into()],
            posted_at: Some("2026-03-01".into()),
            source_data: serde_json::json!({}),
            ..Default::default()
        };

        let scored = scorer.score(&raw).await.unwrap();
        assert_eq!(scored.scoring_method, ScoringMethod::Llm);
        assert!((scored.score - 0.88).abs() < 0.001);
        assert!(scored.reasoning.contains("Rust"));

        let prompt = last.lock().unwrap().clone();
        assert!(prompt.contains("## User skills"), "prompt: {prompt}");
        assert!(prompt.contains("rust"), "expected skills in prompt");
        assert!(
            prompt.contains("## Opportunity"),
            "expected opportunity section"
        );
        assert!(prompt.contains("0-100"), "expected score range in prompt");
        assert!(prompt.contains("https://jobs.example/1"));
    }
}
