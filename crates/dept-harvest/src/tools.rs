//! Namespaced agent tools (`harvest.*`) for the harvest department.
//!
//! Registered on the [`ToolRegistrar`] during [`DepartmentApp::register`]
//! (ADR-014) so department agents can invoke harvest-engine methods.

use std::sync::Arc;

use harvest_engine::{HarvestEngine, HarvestScanParams, scan_from_params};
use rusvel_core::department::{ToolOutput, ToolRegistrar};
use rusvel_core::domain::OpportunityStage;
use rusvel_core::error::RusvelError;
use rusvel_core::id::SessionId;

/// Registered tool names for the harvest department.
pub const HARVEST_TOOL_IDS: &[&str] = &[
    "harvest.scan",
    "harvest.pipeline.list",
    "harvest.pipeline.stats",
    "harvest.pipeline.advance",
    "harvest.opportunity.score",
    "harvest.proposal.generate",
];

fn parse_session_id(args: &serde_json::Value) -> rusvel_core::error::Result<SessionId> {
    args.get("session_id")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .map(SessionId::from_uuid)
        .ok_or_else(|| RusvelError::Validation("session_id required or invalid".into()))
}

fn parse_opportunity_id(args: &serde_json::Value) -> rusvel_core::error::Result<String> {
    args.get("opportunity_id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from)
        .ok_or_else(|| RusvelError::Validation("opportunity_id required".into()))
}

fn parse_stage(s: &str) -> Option<OpportunityStage> {
    match s {
        "Cold" => Some(OpportunityStage::Cold),
        "Contacted" => Some(OpportunityStage::Contacted),
        "Qualified" => Some(OpportunityStage::Qualified),
        "ProposalSent" => Some(OpportunityStage::ProposalSent),
        "Won" => Some(OpportunityStage::Won),
        "Lost" => Some(OpportunityStage::Lost),
        _ => None,
    }
}

/// Register the `harvest.*` agent tools on the department tool registrar.
pub fn register_tools(reg: &mut ToolRegistrar, engine: Arc<HarvestEngine>) {
    // ── harvest.scan ─────────────────────────────────────────────
    let eng = engine.clone();
    reg.add(
        "harvest",
        "harvest.scan",
        "Scan freelance sources for new opportunities. Call this when the user wants to find, refresh, or discover work opportunities. Scanned items are scored, stored, and enter the pipeline at the Cold stage. Sources: mock (default), upwork, freelancer (RSS; require query).",
        serde_json::json!({
            "type": "object",
            "properties": {
                "session_id": { "type": "string", "description": "Session UUID" },
                "sources": {
                    "type": "array",
                    "items": { "type": "string", "enum": ["mock", "upwork", "freelancer"] },
                    "description": "Source ids to scan; omit for mock-only"
                },
                "query": { "type": "string", "description": "RSS search query for upwork/freelancer sources" }
            },
            "required": ["session_id"]
        }),
        Arc::new(move |args| {
            let eng = eng.clone();
            Box::pin(async move {
                let sid = parse_session_id(&args)?;
                let params = HarvestScanParams::from_job_payload(&args);
                let opps = scan_from_params(eng.as_ref(), &sid, &params, None).await?;
                Ok(ToolOutput {
                    content: serde_json::to_string_pretty(&opps).unwrap_or_else(|_| "[]".into()),
                    is_error: false,
                    metadata: serde_json::json!({ "count": opps.len() }),
                })
            })
        }),
    );

    // ── harvest.pipeline.list ────────────────────────────────────
    let eng = engine.clone();
    reg.add(
        "harvest",
        "harvest.pipeline.list",
        "List stored opportunities, optionally filtered by pipeline stage. Call this to review what is currently in the pipeline before scoring, advancing, or proposing.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "session_id": { "type": "string", "description": "Session UUID" },
                "stage": {
                    "type": "string",
                    "enum": ["Cold", "Contacted", "Qualified", "ProposalSent", "Won", "Lost"],
                    "description": "Filter by pipeline stage (omit for all)"
                }
            },
            "required": ["session_id"]
        }),
        Arc::new(move |args| {
            let eng = eng.clone();
            Box::pin(async move {
                let sid = parse_session_id(&args)?;
                let stage = args
                    .get("stage")
                    .and_then(|v| v.as_str())
                    .and_then(parse_stage);
                let opps = eng.list_opportunities(&sid, stage.as_ref()).await?;
                Ok(ToolOutput {
                    content: serde_json::to_string_pretty(&opps).unwrap_or_else(|_| "[]".into()),
                    is_error: false,
                    metadata: serde_json::json!({ "count": opps.len() }),
                })
            })
        }),
    );

    // ── harvest.pipeline.stats ───────────────────────────────────
    let eng = engine.clone();
    reg.add(
        "harvest",
        "harvest.pipeline.stats",
        "Get pipeline statistics (total opportunities and count per stage). Call this when the user asks for a pipeline overview or health check.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "session_id": { "type": "string", "description": "Session UUID" }
            },
            "required": ["session_id"]
        }),
        Arc::new(move |args| {
            let eng = eng.clone();
            Box::pin(async move {
                let sid = parse_session_id(&args)?;
                let stats = eng.pipeline(&sid).await?;
                Ok(ToolOutput {
                    content: serde_json::to_string_pretty(&stats).unwrap_or_else(|_| "{}".into()),
                    is_error: false,
                    metadata: serde_json::json!({ "total": stats.total }),
                })
            })
        }),
    );

    // ── harvest.pipeline.advance ─────────────────────────────────
    let eng = engine.clone();
    reg.add(
        "harvest",
        "harvest.pipeline.advance",
        "Move an opportunity to a new pipeline stage (Kanban). Call this after outreach, qualification, sending a proposal, or when a deal is won or lost.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "session_id": { "type": "string", "description": "Session UUID" },
                "opportunity_id": { "type": "string", "description": "Opportunity ID" },
                "stage": {
                    "type": "string",
                    "enum": ["Cold", "Contacted", "Qualified", "ProposalSent", "Won", "Lost"],
                    "description": "Target pipeline stage"
                }
            },
            "required": ["session_id", "opportunity_id", "stage"]
        }),
        Arc::new(move |args| {
            let eng = eng.clone();
            Box::pin(async move {
                let _sid = parse_session_id(&args)?;
                let opp_id = parse_opportunity_id(&args)?;
                let stage_str = args.get("stage").and_then(|v| v.as_str()).unwrap_or("");
                let stage = parse_stage(stage_str).ok_or_else(|| {
                    RusvelError::Validation(format!("invalid stage: {stage_str}"))
                })?;
                eng.advance_opportunity(&opp_id, stage).await?;
                Ok(ToolOutput {
                    content: format!("Opportunity {opp_id} moved to {stage_str}"),
                    is_error: false,
                    metadata: serde_json::json!({ "opportunity_id": opp_id, "stage": stage_str }),
                })
            })
        }),
    );

    // ── harvest.opportunity.score ────────────────────────────────
    let eng = engine.clone();
    reg.add(
        "harvest",
        "harvest.opportunity.score",
        "Re-score a stored opportunity and persist the updated score with reasoning. Call this when new information arrives or before deciding whether to pursue an opportunity.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "session_id": { "type": "string", "description": "Session UUID" },
                "opportunity_id": { "type": "string", "description": "Opportunity ID" }
            },
            "required": ["session_id", "opportunity_id"]
        }),
        Arc::new(move |args| {
            let eng = eng.clone();
            Box::pin(async move {
                let sid = parse_session_id(&args)?;
                let opp_id = parse_opportunity_id(&args)?;
                let update = eng.score_opportunity(&sid, &opp_id).await?;
                Ok(ToolOutput {
                    content: serde_json::to_string_pretty(&serde_json::json!({
                        "score": update.score,
                        "reasoning": update.reasoning,
                    }))
                    .unwrap_or_else(|_| "{}".into()),
                    is_error: false,
                    metadata: serde_json::json!({ "opportunity_id": opp_id }),
                })
            })
        }),
    );

    // ── harvest.proposal.generate ────────────────────────────────
    let eng = engine.clone();
    reg.add(
        "harvest",
        "harvest.proposal.generate",
        "Generate a tailored proposal for a stored opportunity using the given freelancer profile. Call this when an opportunity is qualified and the user wants to apply or bid.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "session_id": { "type": "string", "description": "Session UUID" },
                "opportunity_id": { "type": "string", "description": "Opportunity ID" },
                "profile": { "type": "string", "description": "Freelancer profile summary to tailor the proposal" }
            },
            "required": ["session_id", "opportunity_id", "profile"]
        }),
        Arc::new(move |args| {
            let eng = eng.clone();
            Box::pin(async move {
                let sid = parse_session_id(&args)?;
                let opp_id = parse_opportunity_id(&args)?;
                let profile = args
                    .get("profile")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let proposal = eng.generate_proposal(&sid, &opp_id, profile).await?;
                Ok(ToolOutput {
                    content: serde_json::to_string_pretty(&proposal)
                        .unwrap_or_else(|_| "proposal".into()),
                    is_error: false,
                    metadata: serde_json::json!({ "opportunity_id": opp_id }),
                })
            })
        }),
    );

    tracing::debug!(
        count = HARVEST_TOOL_IDS.len(),
        "harvest agent tools registered"
    );
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use rusvel_core::domain::*;
    use rusvel_core::error::Result;
    use rusvel_core::id::*;
    use rusvel_core::ports::*;

    use super::*;

    // ── In-memory test storage (mirrors harvest-engine test stubs) ──

    #[derive(Default)]
    struct MemObjectStore {
        data: Mutex<HashMap<String, HashMap<String, serde_json::Value>>>,
    }

    #[async_trait]
    impl ObjectStore for MemObjectStore {
        async fn put(&self, kind: &str, id: &str, object: serde_json::Value) -> Result<()> {
            self.data
                .lock()
                .unwrap()
                .entry(kind.into())
                .or_default()
                .insert(id.into(), object);
            Ok(())
        }

        async fn get(&self, kind: &str, id: &str) -> Result<Option<serde_json::Value>> {
            Ok(self
                .data
                .lock()
                .unwrap()
                .get(kind)
                .and_then(|m| m.get(id).cloned()))
        }

        async fn delete(&self, kind: &str, id: &str) -> Result<()> {
            if let Some(m) = self.data.lock().unwrap().get_mut(kind) {
                m.remove(id);
            }
            Ok(())
        }

        async fn list(&self, kind: &str, filter: ObjectFilter) -> Result<Vec<serde_json::Value>> {
            let data = self.data.lock().unwrap();
            let Some(m) = data.get(kind) else {
                return Ok(vec![]);
            };
            let mut out: Vec<serde_json::Value> = m
                .values()
                .filter(|v| {
                    filter.session_id.map_or(true, |sid| {
                        v.get("session_id")
                            .and_then(|x| x.as_str())
                            .is_some_and(|s| s == sid.to_string())
                    })
                })
                .cloned()
                .collect();
            if let Some(lim) = filter.limit {
                out.truncate(lim as usize);
            }
            Ok(out)
        }
    }

    struct StubEventStore;
    #[async_trait]
    impl EventStore for StubEventStore {
        async fn append(&self, _: &Event) -> Result<()> {
            Ok(())
        }
        async fn get(&self, _: &EventId) -> Result<Option<Event>> {
            Ok(None)
        }
        async fn query(&self, _: EventFilter) -> Result<Vec<Event>> {
            Ok(vec![])
        }
    }

    struct StubMetricStore;
    #[async_trait]
    impl MetricStore for StubMetricStore {
        async fn record(&self, _: &MetricPoint) -> Result<()> {
            Ok(())
        }
        async fn query(&self, _: MetricFilter) -> Result<Vec<MetricPoint>> {
            Ok(vec![])
        }
    }

    struct StubSessionStore;
    #[async_trait]
    impl SessionStore for StubSessionStore {
        async fn put_session(&self, _: &Session) -> Result<()> {
            Ok(())
        }
        async fn get_session(&self, _: &SessionId) -> Result<Option<Session>> {
            Ok(None)
        }
        async fn list_sessions(&self) -> Result<Vec<SessionSummary>> {
            Ok(vec![])
        }
        async fn put_run(&self, _: &Run) -> Result<()> {
            Ok(())
        }
        async fn get_run(&self, _: &RunId) -> Result<Option<Run>> {
            Ok(None)
        }
        async fn list_runs(&self, _: &SessionId) -> Result<Vec<Run>> {
            Ok(vec![])
        }
        async fn put_thread(&self, _: &Thread) -> Result<()> {
            Ok(())
        }
        async fn get_thread(&self, _: &ThreadId) -> Result<Option<Thread>> {
            Ok(None)
        }
        async fn list_threads(&self, _: &RunId) -> Result<Vec<Thread>> {
            Ok(vec![])
        }
    }

    struct StubJobStore;
    #[async_trait]
    impl JobStore for StubJobStore {
        async fn enqueue(&self, _: &Job) -> Result<()> {
            Ok(())
        }
        async fn dequeue(&self, _: &[JobKind]) -> Result<Option<Job>> {
            Ok(None)
        }
        async fn update(&self, _: &Job) -> Result<()> {
            Ok(())
        }
        async fn get(&self, _: &JobId) -> Result<Option<Job>> {
            Ok(None)
        }
        async fn list(&self, _: JobFilter) -> Result<Vec<Job>> {
            Ok(vec![])
        }
    }

    struct TestStorage {
        objects: MemObjectStore,
        events: StubEventStore,
        metrics: StubMetricStore,
        sessions: StubSessionStore,
        jobs: StubJobStore,
    }

    impl TestStorage {
        fn new() -> Self {
            Self {
                objects: MemObjectStore::default(),
                events: StubEventStore,
                metrics: StubMetricStore,
                sessions: StubSessionStore,
                jobs: StubJobStore,
            }
        }
    }

    impl StoragePort for TestStorage {
        fn events(&self) -> &dyn EventStore {
            &self.events
        }
        fn objects(&self) -> &dyn ObjectStore {
            &self.objects
        }
        fn metrics(&self) -> &dyn MetricStore {
            &self.metrics
        }
        fn sessions(&self) -> &dyn SessionStore {
            &self.sessions
        }
        fn jobs(&self) -> &dyn JobStore {
            &self.jobs
        }
    }

    fn test_engine() -> Arc<HarvestEngine> {
        Arc::new(HarvestEngine::new(Arc::new(TestStorage::new())))
    }

    #[test]
    fn tools_register_with_expected_ids() {
        let mut reg = ToolRegistrar::new();
        register_tools(&mut reg, test_engine());
        let tools = reg.into_tools();
        assert_eq!(tools.len(), HARVEST_TOOL_IDS.len());
        for (tool, expected) in tools.iter().zip(HARVEST_TOOL_IDS) {
            assert_eq!(tool.name, *expected);
            assert_eq!(tool.department_id, "harvest");
            assert!(!tool.description.is_empty());
            assert!(tool.parameters_schema.get("properties").is_some());
        }
    }

    #[tokio::test]
    async fn scan_tool_dispatches_to_engine() {
        let mut reg = ToolRegistrar::new();
        register_tools(&mut reg, test_engine());
        let tools = reg.into_tools();
        let scan = tools.iter().find(|t| t.name == "harvest.scan").unwrap();

        let sid = SessionId::new();
        let out = (scan.handler)(serde_json::json!({ "session_id": sid.to_string() }))
            .await
            .unwrap();
        assert!(!out.is_error);
        assert_eq!(out.metadata["count"], 3); // MockSource yields 3 opportunities
    }

    #[tokio::test]
    async fn stats_tool_reflects_scanned_pipeline() {
        let engine = test_engine();
        let mut reg = ToolRegistrar::new();
        register_tools(&mut reg, engine.clone());
        let tools = reg.into_tools();

        let sid = SessionId::new();
        let scan = tools.iter().find(|t| t.name == "harvest.scan").unwrap();
        (scan.handler)(serde_json::json!({ "session_id": sid.to_string() }))
            .await
            .unwrap();

        let stats = tools
            .iter()
            .find(|t| t.name == "harvest.pipeline.stats")
            .unwrap();
        let out = (stats.handler)(serde_json::json!({ "session_id": sid.to_string() }))
            .await
            .unwrap();
        assert!(!out.is_error);
        assert_eq!(out.metadata["total"], 3);
    }

    #[tokio::test]
    async fn tool_rejects_missing_session_id() {
        let mut reg = ToolRegistrar::new();
        register_tools(&mut reg, test_engine());
        let tools = reg.into_tools();
        let list = tools
            .iter()
            .find(|t| t.name == "harvest.pipeline.list")
            .unwrap();
        let err = (list.handler)(serde_json::json!({})).await;
        assert!(err.is_err());
    }
}
