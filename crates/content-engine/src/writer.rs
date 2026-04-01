//! AI-powered content generation, adaptation, and review.

use std::sync::Arc;

use rusvel_core::domain::*;
use rusvel_core::error::Result;
use rusvel_core::id::*;
use rusvel_core::ports::AgentPort;
use serde::{Deserialize, Serialize};

/// Quality review produced by the AI reviewer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentReview {
    /// 0.0 – 1.0 quality score.
    pub score: f64,
    /// Actionable improvement suggestions.
    pub suggestions: Vec<String>,
    /// Whether the content passes the quality bar.
    pub approved: bool,
}

/// Build a drafting prompt from a code analysis summary and target content kind.
///
/// Pure string construction — callers pass the result to [`ContentWriter::draft`] as `topic`.
pub fn build_code_prompt(summary: &CodeAnalysisSummary, kind: &ContentKind) -> String {
    let stats_line = format!(
        "{} files, {} symbols",
        summary.total_files, summary.total_symbols
    );
    let symbols = if summary.top_symbols.is_empty() {
        "(no function symbols listed)".to_string()
    } else {
        summary.top_symbols.join(", ")
    };
    let largest = summary
        .largest_function
        .as_deref()
        .unwrap_or("not identified");

    match kind {
        ContentKind::LinkedInPost => {
            format!(
                "LinkedIn post about this codebase (repo: {}).\n\n\
                 Open with one sharp hook line about what the project is.\n\
                 Then give exactly 3 bullet lines with key stats: {}, top API areas: {}, largest function: {}.\n\
                 Add 2–3 sentences on what makes the architecture or tech choices interesting for engineers.\n\
                 Close with a clear CTA (e.g. try it, read the repo, or follow for updates).\n\n\
                 Snapshot id: {}",
                summary.repo_path, stats_line, symbols, largest, summary.snapshot_id
            )
        }
        ContentKind::Thread => {
            let mut chunks = Vec::new();
            chunks.push(format!(
                "Tweet 1: Hook — what is `{}` and why it matters.",
                summary.repo_path
            ));
            chunks.push(format!("Tweet 2: Scale — {}.", stats_line));
            chunks.push(format!(
                "Tweet 3: Surface area — symbols to know: {}.",
                symbols
            ));
            chunks.push(format!(
                "Tweet 4: Complexity hotspot — largest function: {}.",
                largest
            ));
            chunks.push(format!(
                "Tweet 5: Why engineers should care — one concrete takeaway from the structure."
            ));
            chunks.push(format!(
                "Tweet 6: CTA — snapshot `{}`, invite readers to dig into the repo.",
                summary.snapshot_id
            ));
            chunks.push(
                "Tweet 7 (optional): One risk or next step you'd highlight for maintainers.".into(),
            );
            chunks.push(
                "Tweet 8 (optional): Shout-out to a pattern or tool you noticed in the codebase."
                    .into(),
            );
            format!(
                "Twitter/X thread: write 6–8 short tweets (each one insight, under ~280 chars when published).\n\n{}",
                chunks.join("\n\n")
            )
        }
        ContentKind::Blog => {
            format!(
                "Technical blog post about repository `{}` (analysis snapshot {}).\n\n\
                 Use this structure in markdown:\n\
                 1. Title (you will put it on the first line as `# ...`)\n\
                 2. Introduction — problem space and what the codebase does\n\
                 3. Repository overview — {}, {}\n\
                 4. Notable symbols and modules — {}\n\
                 5. Complexity / hotspots — discuss `{}`\n\
                 6. Engineering takeaways — patterns, testing, dependencies\n\
                 7. Conclusion — who benefits and suggested next steps\n\n\
                 Ground claims in the metrics above; do not invent file paths not implied by the summary.",
                summary.repo_path,
                summary.snapshot_id,
                stats_line,
                summary.total_symbols,
                symbols,
                largest
            )
        }
        other => {
            format!(
                "Write a {other:?} piece about this codebase.\n\
                 Repo: {}\nMetrics: {}\nTop symbols: {}\nLargest function: {}\nSnapshot: {}",
                summary.repo_path, stats_line, symbols, largest, summary.snapshot_id
            )
        }
    }
}

/// AI content writer backed by an [`AgentPort`].
pub struct ContentWriter {
    agent: Arc<dyn AgentPort>,
    voice_rules: std::sync::RwLock<Option<String>>,
}

impl ContentWriter {
    pub fn new(agent: Arc<dyn AgentPort>) -> Self {
        Self {
            agent,
            voice_rules: std::sync::RwLock::new(None),
        }
    }

    /// Set voice/style rules that will be injected into all content generation prompts.
    pub fn set_voice_rules(&self, rules: impl Into<String>) {
        *self.voice_rules.write().unwrap() = Some(rules.into());
    }

    fn system_prompt(&self, role: &str) -> String {
        let base = format!("You are {role}.");
        match self.voice_rules.read().unwrap().as_deref() {
            Some(rules) => format!("{base}\n\n--- Voice & Style Rules ---\n{rules}"),
            None => base,
        }
    }

    /// Ask the LLM to draft a new content item for the given topic and kind.
    pub async fn draft(
        &self,
        session_id: &SessionId,
        topic: &str,
        kind: ContentKind,
    ) -> Result<ContentItem> {
        let prompt = format!(
            "Write a {kind:?} article about: {topic}\n\n\
             Return the result as markdown. Include a title on the first line \
             prefixed with `# `."
        );
        let config = AgentConfig {
            profile_id: None,
            session_id: *session_id,
            model: None,
            tools: vec![],
            instructions: Some(self.system_prompt("a professional content writer")),
            budget_limit: None,
            max_iterations: None,
            permission_mode: Default::default(),
            metadata: serde_json::json!({}),
        };
        let run_id = self.agent.create(config).await?;
        let output = self.agent.run(&run_id, Content::text(prompt)).await?;

        let body = extract_text(&output.content);
        let title = body.lines().find(|l| l.starts_with("# ")).map_or_else(
            || topic.to_string(),
            |l| l.trim_start_matches("# ").to_string(),
        );

        Ok(ContentItem {
            id: ContentId::new(),
            session_id: *session_id,
            kind,
            title,
            body_markdown: body,
            platform_targets: vec![],
            status: ContentStatus::Draft,
            approval: ApprovalStatus::Pending,
            scheduled_at: None,
            published_at: None,
            metadata: serde_json::json!({}),
        })
    }

    /// Adapt content for a specific platform (respecting length, tone, format).
    pub async fn adapt(
        &self,
        content: &ContentItem,
        platform: Platform,
        max_length: Option<usize>,
    ) -> Result<String> {
        let length_hint = max_length
            .map(|n| format!("Maximum length: {n} characters. "))
            .unwrap_or_default();
        let prompt = format!(
            "Adapt the following content for {platform:?}. {length_hint}\
             Keep the core message but adjust tone and format.\n\n\
             ---\n{}\n---",
            content.body_markdown
        );
        let config = AgentConfig {
            profile_id: None,
            session_id: content.session_id,
            model: None,
            tools: vec![],
            instructions: Some(self.system_prompt("a social-media content strategist")),
            budget_limit: None,
            max_iterations: None,
            permission_mode: Default::default(),
            metadata: serde_json::json!({}),
        };
        let run_id = self.agent.create(config).await?;
        let output = self.agent.run(&run_id, Content::text(prompt)).await?;
        Ok(extract_text(&output.content))
    }

    /// Ask the LLM to review content quality and provide feedback.
    pub async fn review(&self, content: &ContentItem) -> Result<ContentReview> {
        let prompt = format!(
            "Review the following content for quality, clarity, and engagement.\n\
             Score it 0.0-1.0 and list suggestions.\n\n---\n{}\n---",
            content.body_markdown
        );
        let config = AgentConfig {
            profile_id: None,
            session_id: content.session_id,
            model: None,
            tools: vec![],
            instructions: Some(self.system_prompt("an editorial reviewer")),
            budget_limit: None,
            max_iterations: None,
            permission_mode: Default::default(),
            metadata: serde_json::json!({}),
        };
        let run_id = self.agent.create(config).await?;
        let output = self.agent.run(&run_id, Content::text(prompt)).await?;
        let text = extract_text(&output.content);

        // Parse a simple score from the response; fall back to 0.7.
        let score = text
            .lines()
            .find_map(|l| {
                l.split_whitespace().find_map(|w| {
                    w.trim_matches(|c: char| !c.is_ascii_digit() && c != '.')
                        .parse::<f64>()
                        .ok()
                        .filter(|&v| (0.0..=1.0).contains(&v))
                })
            })
            .unwrap_or(0.7);

        Ok(ContentReview {
            score,
            suggestions: text
                .lines()
                .filter(|l| l.starts_with("- "))
                .map(String::from)
                .collect(),
            approved: score >= 0.6,
        })
    }
}

/// Extract plain text from a [`Content`] value.
fn extract_text(content: &Content) -> String {
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

#[cfg(test)]
mod prompt_tests {
    use super::build_code_prompt;
    use rusvel_core::domain::{CodeAnalysisSummary, ContentKind};

    fn sample_summary() -> CodeAnalysisSummary {
        CodeAnalysisSummary {
            snapshot_id: "snap-1".into(),
            repo_path: "/repo".into(),
            total_files: 10,
            total_symbols: 200,
            top_symbols: vec!["main".into(), "run".into()],
            largest_function: Some("process_batch".into()),
            metadata: Default::default(),
        }
    }

    #[test]
    fn linkedin_prompt_includes_stats_and_cta() {
        let p = build_code_prompt(&sample_summary(), &ContentKind::LinkedInPost);
        assert!(p.contains("10 files, 200 symbols"));
        assert!(p.contains("main, run"));
        assert!(p.contains("CTA"));
        assert!(p.contains("snap-1"));
    }

    #[test]
    fn thread_prompt_has_multiple_insights() {
        let p = build_code_prompt(&sample_summary(), &ContentKind::Thread);
        assert!(p.contains("Tweet 1"));
        assert!(p.contains("Tweet 6"));
        assert!(p.contains("/repo"));
    }

    #[test]
    fn blog_prompt_has_section_structure() {
        let p = build_code_prompt(&sample_summary(), &ContentKind::Blog);
        assert!(p.contains("Introduction"));
        assert!(p.contains("Conclusion"));
        assert!(p.contains("process_batch"));
    }
}
