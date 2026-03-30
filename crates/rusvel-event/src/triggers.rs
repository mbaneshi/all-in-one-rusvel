//! Event triggers — pattern-match [`Event`](rusvel_core::domain::Event) kinds and run agents or flows.
//!
//! Pattern rules (aligned with hook matching in `rusvel-api::hook_dispatch`):
//! - `*` matches any kind
//! - exact equality on `event.kind`
//! - suffix: `event.kind.ends_with(pattern)` when the pattern has no `*` (e.g. `chat.completed` vs `code.chat.completed`)
//! - prefix glob: `foo.*` matches `foo` or `foo.bar` or `foo.bar.baz`
//! - suffix glob: `*.bar` matches `x.bar` and `bar`

use std::sync::{Arc, RwLock};

use flow_engine::FlowEngine;
use rusvel_core::domain::{Content, Event, EventTrigger, TriggerAction};
use rusvel_core::id::FlowId;
use rusvel_core::ports::{AgentPort, EventPort, StoragePort};
use serde_json::json;
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::RecvError;

/// Holds registered triggers and runs them when matching events arrive on a broadcast receiver.
pub struct TriggerManager {
    triggers: Arc<RwLock<Vec<EventTrigger>>>,
    agent: Arc<dyn AgentPort>,
    engine: Arc<FlowEngine>,
}

impl TriggerManager {
    pub fn new(
        agent: Arc<dyn AgentPort>,
        storage: Arc<dyn StoragePort>,
        events: Arc<dyn EventPort>,
    ) -> Self {
        let engine = Arc::new(FlowEngine::new(
            storage.clone(),
            events.clone(),
            agent.clone(),
            None,
            None,
        ));
        Self {
            triggers: Arc::new(RwLock::new(Vec::new())),
            agent,
            engine,
        }
    }

    /// Append a trigger (replacing an existing entry with the same `id` if present).
    pub fn register_trigger(&self, trigger: EventTrigger) {
        let mut list = self.triggers.write().expect("trigger list poisoned");
        if let Some(i) = list.iter().position(|t| t.id == trigger.id) {
            list[i] = trigger;
        } else {
            list.push(trigger);
        }
    }

    /// Spawn a task that listens for events and runs matching trigger actions.
    pub fn start(&self, mut rx: broadcast::Receiver<Event>) {
        let triggers = Arc::clone(&self.triggers);
        let agent = Arc::clone(&self.agent);
        let engine = Arc::clone(&self.engine);

        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        let list: Vec<EventTrigger> = {
                            let g = triggers.read().expect("trigger list poisoned");
                            g.clone()
                        };
                        for t in list {
                            if !t.enabled {
                                continue;
                            }
                            if let Some(ref dept) = t.department_id {
                                if event.source != *dept {
                                    continue;
                                }
                            }
                            if !matches_event_pattern(&t.event_pattern, &event.kind) {
                                continue;
                            }

                            let agent = Arc::clone(&agent);
                            let engine = Arc::clone(&engine);
                            let event = event.clone();
                            tokio::spawn(async move {
                                match t.action {
                                    TriggerAction::RunAgent {
                                        persona,
                                        prompt_template,
                                        tools,
                                    } => {
                                        if let Err(e) = run_trigger_agent(
                                            &agent,
                                            &event,
                                            persona,
                                            prompt_template,
                                            tools,
                                        )
                                        .await
                                        {
                                            tracing::warn!(
                                                trigger_id = %t.id,
                                                "event trigger RunAgent failed: {e}"
                                            );
                                        }
                                    }
                                    TriggerAction::RunFlow { flow_id } => {
                                        if let Err(e) =
                                            run_trigger_flow(&engine, &event, &flow_id).await
                                        {
                                            tracing::warn!(
                                                trigger_id = %t.id,
                                                "event trigger RunFlow failed: {e}"
                                            );
                                        }
                                    }
                                }
                            });
                        }
                    }
                    Err(RecvError::Lagged(_)) => {}
                    Err(RecvError::Closed) => break,
                }
            }
        });
    }
}

fn matches_event_pattern(pattern: &str, event_kind: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if event_kind == pattern {
        return true;
    }
    if !pattern.contains('*') && event_kind.ends_with(pattern) {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix(".*") {
        return event_kind == prefix || event_kind.starts_with(&format!("{prefix}."));
    }
    if let Some(suffix) = pattern.strip_prefix("*.") {
        return event_kind == suffix || event_kind.ends_with(&format!(".{suffix}"));
    }
    if pattern.ends_with('*') && !pattern.ends_with(".*") {
        let prefix = pattern.trim_end_matches('*');
        return event_kind.starts_with(prefix);
    }
    false
}

fn expand_prompt_template(template: &str, event: &Event) -> String {
    template
        .replace("{{kind}}", &event.kind)
        .replace("{{source}}", &event.source)
        .replace("{{payload}}", &event.payload.to_string())
}

async fn run_trigger_agent(
    agent: &Arc<dyn AgentPort>,
    event: &Event,
    persona: Option<String>,
    prompt_template: String,
    tools: Vec<String>,
) -> rusvel_core::error::Result<()> {
    let prompt = expand_prompt_template(&prompt_template, event);
    let instructions = if let Some(ref p) = persona {
        format!("You are acting as the '{p}' persona. {prompt}")
    } else {
        prompt.clone()
    };

    let session_id = event.session_id.unwrap_or_else(rusvel_core::SessionId::new);
    let mut metadata = json!({
        "triggered_by": "event_trigger",
        "event_kind": event.kind,
        "event_id": event.id.to_string(),
    });
    if let Some(sid) = event.session_id {
        metadata["trigger_session_id"] = json!(sid.to_string());
    }

    let config = rusvel_core::domain::AgentConfig {
        profile_id: None,
        session_id,
        model: None,
        tools,
        instructions: Some(instructions),
        budget_limit: None,
        max_iterations: None,
        permission_mode: Default::default(),
        metadata,
    };

    let run_id = agent.create(config).await?;
    let input = Content::text(prompt);
    let _ = agent.run(&run_id, input).await?;
    Ok(())
}

async fn run_trigger_flow(
    engine: &Arc<FlowEngine>,
    event: &Event,
    flow_id: &str,
) -> rusvel_core::error::Result<()> {
    let id: FlowId = flow_id
        .parse::<uuid::Uuid>()
        .map(FlowId::from_uuid)
        .map_err(|e| {
            rusvel_core::error::RusvelError::Validation(format!("invalid flow_id: {e}"))
        })?;

    let trigger_data = json!({
        "event_trigger": true,
        "kind": event.kind,
        "source": event.source,
        "payload": event.payload,
        "event_id": event.id.to_string(),
    });

    let _ = engine.run_flow(&id, trigger_data).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pattern_star() {
        assert!(matches_event_pattern("*", "anything"));
    }

    #[test]
    fn pattern_exact_and_suffix() {
        assert!(matches_event_pattern(
            "code.chat.completed",
            "code.chat.completed"
        ));
        assert!(matches_event_pattern(
            "chat.completed",
            "code.chat.completed"
        ));
        assert!(!matches_event_pattern(
            "chat.completed",
            "code.chat.started"
        ));
    }

    #[test]
    fn pattern_prefix_glob() {
        assert!(matches_event_pattern("browser.data.*", "browser.data"));
        assert!(matches_event_pattern("browser.data.*", "browser.data.foo"));
        assert!(!matches_event_pattern("browser.data.*", "browser.other"));
    }

    #[test]
    fn pattern_suffix_glob() {
        assert!(matches_event_pattern("*.completed", "flow.completed"));
        assert!(matches_event_pattern("*.completed", "code.flow.completed"));
    }
}
