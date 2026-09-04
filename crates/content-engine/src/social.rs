//! One concept → a full social bundle (caption, hashtags, carousel slides,
//! optional cover image, optional video) — end-to-end content creation for
//! a single idea.
//!
//! Stays inside the same ports-and-adapters boundary as the rest of
//! `content-engine`: text comes from [`AgentPort`] (ADR-009 — never
//! `LlmPort` directly), media comes from [`MediaGenPort`]. This module adds
//! no new port; it's an orchestrating layer over the two that already exist.

use std::sync::Arc;

use rusvel_core::domain::{AgentConfig, Content};
use rusvel_core::error::{Result, RusvelError};
use rusvel_core::id::SessionId;
use rusvel_core::ports::AgentPort;
use serde::{Deserialize, Serialize};

use crate::media_gen::{GeneratedImage, GeneratedVideo};

/// One slide of a multi-image carousel post. `image` is filled in separately
/// (a [`crate::media_gen::MediaGenPort`] call per slide) — drafting the text
/// and generating slide art are different concerns, different costs, and a
/// caller may want text-only slides.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CarouselSlide {
    pub heading: String,
    pub text: String,
    pub image: Option<GeneratedImage>,
}

/// Everything generated for one concept.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocialBundle {
    pub concept: String,
    pub caption: String,
    pub hashtags: Vec<String>,
    pub carousel: Vec<CarouselSlide>,
    /// `None` unless a caller explicitly asked for video — generation is
    /// slow, costly, and async (see `AvalAiMediaGen::generate_video`'s doc
    /// comment); never attached by default.
    pub video: Option<GeneratedVideo>,
}

/// What to attach beyond the base text. Image and video generation cost
/// real money and (for video) real time — nothing extra happens unless
/// explicitly asked for.
#[derive(Debug, Clone)]
pub struct SocialBundleOptions {
    pub slide_count: u32,
    pub generate_slide_images: bool,
    pub generate_video: bool,
}

impl Default for SocialBundleOptions {
    fn default() -> Self {
        Self {
            slide_count: 5,
            generate_slide_images: false,
            generate_video: false,
        }
    }
}

/// Drafts the text side of a [`SocialBundle`] — caption, hashtags, and
/// carousel slide copy — in one structured call, so the pieces stay
/// coherent with each other (the hashtags actually match the caption, the
/// slides actually walk through the concept in order).
pub struct SocialContentGenerator {
    agent: Arc<dyn AgentPort>,
}

impl SocialContentGenerator {
    pub fn new(agent: Arc<dyn AgentPort>) -> Self {
        Self { agent }
    }

    /// Draft the text fields of a bundle. `voice_rules` is the tenant's
    /// brand voice (see [`crate::ContentEngine::draft_for_tenant`] for the
    /// same pattern) — `None` for the untenanted / default caller.
    pub async fn draft_bundle_text(
        &self,
        session_id: &SessionId,
        concept: &str,
        voice_rules: Option<&str>,
        slide_count: u32,
    ) -> Result<(String, Vec<String>, Vec<(String, String)>)> {
        let system = build_system_prompt(voice_rules, slide_count);
        let user_prompt = format!("Concept:\n{concept}");

        let config = AgentConfig {
            profile_id: None,
            session_id: *session_id,
            model: None,
            tools: vec![],
            instructions: Some(system),
            budget_limit: None,
            max_iterations: None,
            permission_mode: Default::default(),
            metadata: serde_json::json!({}),
        };
        let run_id = self.agent.create(config).await?;
        let output = self.agent.run(&run_id, Content::text(user_prompt)).await?;

        let text = output
            .content
            .parts
            .iter()
            .find_map(|p| match p {
                rusvel_core::domain::Part::Text(t) => Some(t.clone()),
                _ => None,
            })
            .unwrap_or_default();

        parse_bundle_text(&text)
    }
}

fn build_system_prompt(voice_rules: Option<&str>, slide_count: u32) -> String {
    let voice = match voice_rules {
        Some(rules) => format!("\n\n--- Voice & Style Rules ---\n{rules}"),
        None => String::new(),
    };
    format!(
        "You are a social media content writer. Given one concept, produce a \
         caption, a set of hashtags, and a {slide_count}-slide carousel that \
         walks through the concept.{voice}\n\n\
         Output ONLY a single valid JSON object, no code fence, no extra text, \
         in exactly this shape:\n\
         {{\"caption\": \"...\", \"hashtags\": [\"#...\", \"#...\"], \
         \"slides\": [{{\"heading\": \"...\", \"text\": \"...\"}}]}}\n\n\
         Rules: hashtags start with '#', no spaces inside a hashtag. \
         Produce exactly {slide_count} slides. The first slide is the hook; \
         the last slide is the payoff, not a generic call to action."
    )
}

/// Pull the JSON object out of a model reply (models sometimes wrap it in
/// prose or a code fence despite instructions) and parse it into the three
/// bundle text fields.
fn parse_bundle_text(text: &str) -> Result<(String, Vec<String>, Vec<(String, String)>)> {
    let start = text
        .find('{')
        .ok_or_else(|| RusvelError::Llm("no JSON object found in social bundle reply".into()))?;
    let end = text
        .rfind('}')
        .ok_or_else(|| RusvelError::Llm("no JSON object found in social bundle reply".into()))?;
    if end <= start {
        return Err(RusvelError::Llm(
            "malformed JSON object in social bundle reply".into(),
        ));
    }

    #[derive(Deserialize)]
    struct SlideJson {
        heading: String,
        text: String,
    }
    #[derive(Deserialize)]
    struct BundleJson {
        caption: String,
        #[serde(default)]
        hashtags: Vec<String>,
        #[serde(default)]
        slides: Vec<SlideJson>,
    }

    let parsed: BundleJson = serde_json::from_str(&text[start..=end])
        .map_err(|e| RusvelError::Llm(format!("social bundle JSON parse failed: {e}")))?;

    if parsed.caption.trim().is_empty() {
        return Err(RusvelError::Llm(
            "social bundle reply had an empty caption".into(),
        ));
    }

    let slides = parsed
        .slides
        .into_iter()
        .map(|s| (s.heading, s.text))
        .collect();

    Ok((parsed.caption, parsed.hashtags, slides))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_clean_json() {
        let reply = r##"{"caption":"A cat","hashtags":["#cat","#cute"],"slides":[{"heading":"Meet the cat","text":"It's orange."}]}"##;
        let (caption, hashtags, slides) = parse_bundle_text(reply).unwrap();
        assert_eq!(caption, "A cat");
        assert_eq!(hashtags, vec!["#cat", "#cute"]);
        assert_eq!(slides.len(), 1);
        assert_eq!(slides[0].0, "Meet the cat");
    }

    #[test]
    fn parses_json_wrapped_in_prose_and_a_code_fence() {
        let reply = "Sure, here's the bundle:\n```json\n{\"caption\":\"Hi\",\"hashtags\":[],\"slides\":[]}\n```\nHope that helps!";
        let (caption, hashtags, slides) = parse_bundle_text(reply).unwrap();
        assert_eq!(caption, "Hi");
        assert!(hashtags.is_empty());
        assert!(slides.is_empty());
    }

    #[test]
    fn errors_on_empty_caption() {
        let reply = r#"{"caption":"","hashtags":[],"slides":[]}"#;
        assert!(parse_bundle_text(reply).is_err());
    }

    #[test]
    fn errors_when_no_json_object_present() {
        assert!(parse_bundle_text("I couldn't come up with anything.").is_err());
    }
}
