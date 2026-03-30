//! Harvest Engine — opportunity discovery, scoring, and pipeline management.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use rusvel_core::domain::BrowserEvent;
use rusvel_core::domain::VectorSearchResult;
use rusvel_core::engine::Engine;
use rusvel_core::ports::{
    AgentPort, BrowserPort, EmbeddingPort, EventPort, StoragePort, VectorStorePort,
};
use rusvel_core::{
    Capability, Contact, ContactId, Event, EventId, HealthStatus, ObjectFilter, Opportunity,
    OpportunityId, OpportunitySource, OpportunityStage, Result, RusvelError, SessionId,
};

pub mod cdp_source;
pub mod events;
pub mod normalize;
pub mod outcomes;
pub mod pipeline;
pub mod proposal;
pub mod scan_execute;
pub mod scorer;
pub mod source;

pub use cdp_source::{CdpSource, DEFAULT_CDP_EXTRACT_JS, extract_js_listing_cards};
pub use outcomes::{HarvestDealOutcome, HarvestOutcomeRecord};
pub use scan_execute::{HarvestScanParams, scan_from_params};
pub use scorer::ScoringMethod;

use pipeline::{Pipeline, PipelineStats};
use proposal::{Proposal, ProposalGenerator};

/// Result of re-scoring a stored opportunity (LLM or keyword path).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OpportunityScoreUpdate {
    pub score: f64,
    pub reasoning: String,
}

/// Persisted proposal row in the object store (`kind`: `"proposal"`).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct StoredProposalRecord {
    session_id: SessionId,
    opportunity_id: String,
    proposal: Proposal,
}
use scorer::OpportunityScorer;
use source::HarvestSource;

fn filter_session_outcome_hits(results: Vec<VectorSearchResult>, sid: &str) -> Vec<String> {
    results
        .into_iter()
        .filter(|r| {
            r.entry.metadata.get("kind").and_then(|v| v.as_str()) == Some("harvest_outcome")
                && r.entry.metadata.get("session_id").and_then(|v| v.as_str()) == Some(sid)
        })
        .take(5)
        .map(|r| {
            format!(
                "- (similar outcome, score={:.2}) {}",
                r.score,
                r.entry.content.chars().take(120).collect::<String>()
            )
        })
        .collect()
}

/// Configuration for the Harvest engine.
#[derive(Debug, Clone)]
pub struct HarvestConfig {
    pub skills: Vec<String>,
    pub min_budget: Option<f64>,
}

impl Default for HarvestConfig {
    fn default() -> Self {
        Self {
            skills: vec!["rust".into()],
            min_budget: None,
        }
    }
}

/// The Harvest engine discovers, scores, and manages freelance opportunities.
pub struct HarvestEngine {
    storage: Arc<dyn StoragePort>,
    event_port: Option<Arc<dyn EventPort>>,
    agent: Option<Arc<dyn AgentPort>>,
    /// Injected for host wiring; optional tooling may use [`HarvestEngine::on_data_captured`].
    #[allow(dead_code)]
    browser: Option<Arc<dyn BrowserPort>>,
    config: HarvestConfig,
    /// Optional RAG: embed outcomes and retrieve similar rows for scoring (S-044 extension).
    rag: std::sync::Mutex<Option<(Arc<dyn EmbeddingPort>, Arc<dyn VectorStorePort>)>>,
    /// Compact user identity text for LLM scoring (e.g. from [`rusvel_core::UserProfile::to_system_prompt`]).
    user_profile_prompt: Option<String>,
}

impl HarvestEngine {
    pub fn new(storage: Arc<dyn StoragePort>) -> Self {
        Self {
            storage,
            event_port: None,
            agent: None,
            browser: None,
            config: HarvestConfig::default(),
            rag: std::sync::Mutex::new(None),
            user_profile_prompt: None,
        }
    }

    /// Wire session-scoped embedding + vector store (same KB as knowledge when host configures both).
    pub fn configure_rag(
        &self,
        embedding: Option<Arc<dyn EmbeddingPort>>,
        vector_store: Option<Arc<dyn VectorStorePort>>,
    ) {
        if let (Some(e), Some(v)) = (embedding, vector_store) {
            if let Ok(mut g) = self.rag.lock() {
                *g = Some((e, v));
            }
        }
    }

    pub fn with_browser(mut self, b: Arc<dyn BrowserPort>) -> Self {
        self.browser = Some(b);
        self
    }

    pub fn with_events(mut self, events: Arc<dyn EventPort>) -> Self {
        self.event_port = Some(events);
        self
    }

    pub fn with_agent(mut self, agent: Arc<dyn AgentPort>) -> Self {
        self.agent = Some(agent);
        self
    }

    pub fn with_config(mut self, config: HarvestConfig) -> Self {
        self.config = config;
        self
    }

    pub fn with_user_profile_prompt(mut self, prompt: Option<String>) -> Self {
        self.user_profile_prompt = prompt;
        self
    }

    fn scorer_for_session(
        &self,
        session_id: &SessionId,
        outcome_hints: Vec<String>,
    ) -> OpportunityScorer {
        OpportunityScorer::new(
            self.agent.clone(),
            self.config.skills.clone(),
            self.config.min_budget,
        )
        .with_scoring_session(*session_id)
        .with_outcome_hints(outcome_hints)
        .with_user_profile_prompt(self.user_profile_prompt.clone())
    }

    /// Insert, skip duplicate, or update changed capture for same platform key / URL.
    async fn try_upsert_opportunity(
        &self,
        session_id: &SessionId,
        new: Opportunity,
        pipe: &Pipeline,
    ) -> Result<Option<Opportunity>> {
        let key = new.platform_job_key.clone();
        let url = new.url.clone();
        if key.is_none() && url.is_none() {
            pipe.add(&new).await?;
            return Ok(Some(new));
        }

        let list = pipe.list(session_id, None).await?;
        for mut ex in list {
            let same_key = key
                .as_ref()
                .is_some_and(|k| ex.platform_job_key.as_ref() == Some(k));
            let same_url = url.as_ref().is_some_and(|u| ex.url.as_ref() == Some(u));
            if !same_key && !same_url {
                continue;
            }
            if ex.content_hash == new.content_hash && (same_key || same_url) {
                return Ok(None);
            }
            ex.description = new.description.clone();
            ex.title = new.title.clone();
            ex.score = new.score;
            ex.value_estimate = new.value_estimate;
            ex.content_hash = new.content_hash.clone();
            ex.upstream_score = new.upstream_score;
            ex.budget_min = new.budget_min;
            ex.budget_max = new.budget_max;
            ex.budget_currency = new.budget_currency.clone();
            ex.hourly = new.hourly;
            ex.client_hire_rate = new.client_hire_rate;
            ex.client_total_spent = new.client_total_spent;
            ex.payment_verified = new.payment_verified;
            ex.proposal_count = new.proposal_count;
            ex.metadata = new.metadata.clone();
            pipe.save(&ex).await?;
            self.emit(
                session_id,
                events::OPPORTUNITY_SCORED,
                serde_json::json!({"id": ex.id.to_string(), "updated": true}),
            )
            .await;
            return Ok(Some(ex));
        }

        pipe.add(&new).await?;
        Ok(Some(new))
    }

    /// Skills used for scoring and RSS query expansion (see [`HarvestConfig`]).
    pub fn harvest_skills(&self) -> &[String] {
        &self.config.skills
    }

    /// Emit a domain event if the event port is configured.
    async fn emit(&self, session_id: &SessionId, kind: &str, payload: serde_json::Value) {
        if let Some(ep) = &self.event_port {
            let event = Event {
                id: EventId::new(),
                session_id: Some(*session_id),
                run_id: None,
                source: "harvest".into(),
                kind: kind.into(),
                payload,
                created_at: Utc::now(),
                metadata: serde_json::json!({}),
            };
            let _ = ep.emit(event).await;
        }
    }

    async fn scoring_outcome_hints_for_raw(
        &self,
        session_id: &SessionId,
        raw: &source::RawOpportunity,
    ) -> Vec<String> {
        let mut lines = outcomes::recent_outcome_prompt_lines(&self.storage, session_id, 12)
            .await
            .unwrap_or_default();
        lines.extend(self.vector_outcome_hints(session_id, raw).await);
        lines
    }

    async fn vector_outcome_hints(
        &self,
        session_id: &SessionId,
        raw: &source::RawOpportunity,
    ) -> Vec<String> {
        let (emb, vs) = match self.rag.lock().ok().and_then(|g| g.clone()) {
            Some(pair) => pair,
            None => return vec![],
        };
        let query_text = format!("{} {}", raw.title, raw.description);
        let Ok(qv) = emb.embed_one(&query_text).await else {
            return vec![];
        };
        let Ok(results) = vs.search(&qv, 12).await else {
            return vec![];
        };
        let sid = session_id.to_string();
        filter_session_outcome_hits(results, &sid)
    }

    async fn index_outcome_vector(&self, record: &HarvestOutcomeRecord) -> Result<()> {
        let (emb, vs) = match self.rag.lock().ok().and_then(|g| g.clone()) {
            Some(p) => p,
            None => return Ok(()),
        };
        let title = record
            .opportunity_snapshot
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let text = format!("{title} {:?} {}", record.result, record.notes);
        let vec = emb.embed_one(&text).await?;
        let id = format!("harvest_outcome_{}", record.id);
        vs.upsert(
            &id,
            &text,
            vec,
            serde_json::json!({
                "session_id": record.session_id.to_string(),
                "outcome_id": record.id,
                "kind": "harvest_outcome",
            }),
        )
        .await?;
        Ok(())
    }

    /// Scan a source, score results, store them, and return opportunities.
    pub async fn scan(
        &self,
        session_id: &SessionId,
        source: &dyn HarvestSource,
    ) -> Result<Vec<Opportunity>> {
        self.emit(
            session_id,
            events::SCAN_STARTED,
            serde_json::json!({"source": source.name()}),
        )
        .await;

        let raw_items = source.scan().await?;
        let pipe = Pipeline::new(self.storage.clone());
        let mut results = Vec::new();
        let source_kind = source.source_kind();

        for raw_in in &raw_items {
            let mut raw = raw_in.clone();
            normalize::prepare_raw(&mut raw, source_kind.clone());

            let hints = self.scoring_outcome_hints_for_raw(session_id, &raw).await;
            let scored = self
                .scorer_for_session(session_id, hints)
                .score(&raw)
                .await?;

            let opportunity = Opportunity {
                id: OpportunityId::new(),
                session_id: *session_id,
                source: source_kind.clone(),
                title: scored.raw.title.clone(),
                url: scored.raw.url.clone(),
                description: scored.raw.description.clone(),
                score: scored.score,
                stage: OpportunityStage::Cold,
                value_estimate: parse_budget(&scored.raw.budget),
                platform_job_key: scored.raw.platform_job_key.clone(),
                content_hash: scored.raw.content_hash.clone(),
                upstream_score: scored.raw.upstream_score,
                budget_min: scored.raw.budget_min,
                budget_max: scored.raw.budget_max,
                budget_currency: scored.raw.budget_currency.clone(),
                hourly: scored.raw.hourly,
                client_hire_rate: scored.raw.client_hire_rate,
                client_total_spent: scored.raw.client_total_spent,
                payment_verified: scored.raw.payment_verified,
                proposal_count: scored.raw.proposal_count,
                metadata: serde_json::json!({
                    "reasoning": scored.reasoning,
                    "skills": scored.raw.skills,
                    "posted_at": scored.raw.posted_at,
                    "scoring_method": match scored.scoring_method {
                        ScoringMethod::Llm => "llm",
                        ScoringMethod::Keyword => "keyword",
                    },
                }),
            };

            if let Some(opp) = self
                .try_upsert_opportunity(session_id, opportunity, &pipe)
                .await?
            {
                self.emit(
                    session_id,
                    events::OPPORTUNITY_DISCOVERED,
                    serde_json::json!({"id": opp.id.to_string(), "title": &opp.title}),
                )
                .await;
                results.push(opp);
            }
        }

        self.emit(
            session_id,
            events::SCAN_COMPLETED,
            serde_json::json!({"count": results.len()}),
        )
        .await;

        Ok(results)
    }

    /// Re-score an existing opportunity and update its stored score and metadata reasoning.
    pub async fn score_opportunity(
        &self,
        session_id: &SessionId,
        opportunity_id: &str,
    ) -> Result<OpportunityScoreUpdate> {
        let value = self
            .storage
            .objects()
            .get("opportunity", opportunity_id)
            .await?
            .ok_or_else(|| RusvelError::NotFound {
                kind: "opportunity".into(),
                id: opportunity_id.into(),
            })?;

        let mut opp: Opportunity = serde_json::from_value(value)?;
        let raw = source::RawOpportunity {
            title: opp.title.clone(),
            description: opp.description.clone(),
            url: opp.url.clone(),
            budget: opp.value_estimate.map(|v| format!("${v}")),
            skills: opp.metadata["skills"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            posted_at: opp.metadata["posted_at"].as_str().map(String::from),
            source_data: serde_json::json!({}),
            platform_job_key: opp.platform_job_key.clone(),
            content_hash: opp.content_hash.clone(),
            upstream_score: opp.upstream_score,
            budget_min: opp.budget_min,
            budget_max: opp.budget_max,
            budget_currency: opp.budget_currency.clone(),
            hourly: opp.hourly,
            client_hire_rate: opp.client_hire_rate,
            client_total_spent: opp.client_total_spent,
            payment_verified: opp.payment_verified,
            proposal_count: opp.proposal_count,
        };

        let hints = self.scoring_outcome_hints_for_raw(session_id, &raw).await;
        let scored = self
            .scorer_for_session(session_id, hints)
            .score(&raw)
            .await?;
        opp.score = scored.score;
        {
            let mut meta = opp.metadata;
            if !meta.is_object() {
                meta = serde_json::json!({});
            }
            if let Some(obj) = meta.as_object_mut() {
                obj.insert("reasoning".into(), serde_json::json!(scored.reasoning));
                obj.insert(
                    "scoring_method".into(),
                    serde_json::json!(match scored.scoring_method {
                        ScoringMethod::Llm => "llm",
                        ScoringMethod::Keyword => "keyword",
                    }),
                );
            }
            opp.metadata = meta;
        }

        let updated = serde_json::to_value(&opp)?;
        self.storage
            .objects()
            .put("opportunity", opportunity_id, updated)
            .await?;

        self.emit(
            session_id,
            events::OPPORTUNITY_SCORED,
            serde_json::json!({
                "id": opportunity_id,
                "score": scored.score,
                "reasoning": &scored.reasoning
            }),
        )
        .await;

        Ok(OpportunityScoreUpdate {
            score: scored.score,
            reasoning: scored.reasoning,
        })
    }

    /// Generate a proposal for a stored opportunity.
    pub async fn generate_proposal(
        &self,
        session_id: &SessionId,
        opportunity_id: &str,
        profile: &str,
    ) -> Result<Proposal> {
        let agent = self
            .agent
            .as_ref()
            .ok_or_else(|| RusvelError::Internal("No agent configured".into()))?;

        let value = self
            .storage
            .objects()
            .get("opportunity", opportunity_id)
            .await?
            .ok_or_else(|| RusvelError::NotFound {
                kind: "opportunity".into(),
                id: opportunity_id.into(),
            })?;

        let opp: Opportunity = serde_json::from_value(value)?;
        let generator = ProposalGenerator::new(agent.clone());
        let proposal = generator.generate(&opp, profile).await?;

        self.emit(
            session_id,
            events::PROPOSAL_GENERATED,
            serde_json::json!({"opportunity_id": opportunity_id}),
        )
        .await;

        let ts = Utc::now().timestamp_millis();
        let key = format!("{opportunity_id}_{ts}");
        let record = StoredProposalRecord {
            session_id: *session_id,
            opportunity_id: opportunity_id.to_string(),
            proposal: proposal.clone(),
        };
        self.storage
            .objects()
            .put("proposal", &key, serde_json::to_value(&record)?)
            .await?;

        self.emit(
            session_id,
            events::PROPOSAL_PERSISTED,
            serde_json::json!({
                "key": key,
                "opportunity_id": opportunity_id,
            }),
        )
        .await;

        Ok(proposal)
    }

    /// List persisted proposals for a session (`kind`: `"proposal"`).
    pub async fn get_proposals(&self, session_id: &SessionId) -> Result<Vec<Proposal>> {
        let filter = ObjectFilter {
            session_id: Some(*session_id),
            tags: vec![],
            limit: None,
            offset: None,
        };
        let rows = self.storage.objects().list("proposal", filter).await?;
        let mut out = Vec::new();
        for v in rows {
            let rec: StoredProposalRecord = serde_json::from_value(v)?;
            out.push(rec.proposal);
        }
        Ok(out)
    }

    /// Get pipeline statistics for a session.
    pub async fn pipeline(&self, session_id: &SessionId) -> Result<PipelineStats> {
        Pipeline::new(self.storage.clone()).stats(session_id).await
    }

    /// List opportunities, optionally filtered by stage.
    pub async fn list_opportunities(
        &self,
        session_id: &SessionId,
        stage: Option<&OpportunityStage>,
    ) -> Result<Vec<Opportunity>> {
        Pipeline::new(self.storage.clone())
            .list(session_id, stage)
            .await
    }

    /// Move an opportunity to a new pipeline stage (Kanban).
    pub async fn advance_opportunity(
        &self,
        opportunity_id: &str,
        new_stage: OpportunityStage,
    ) -> Result<()> {
        Pipeline::new(self.storage.clone())
            .advance(opportunity_id, new_stage)
            .await
    }

    /// Record won/lost/withdrawn for an opportunity (feeds scorer LLM hints; S-044).
    pub async fn record_opportunity_outcome(
        &self,
        session_id: &SessionId,
        opportunity_id: &str,
        result: HarvestDealOutcome,
        notes: String,
    ) -> Result<HarvestOutcomeRecord> {
        let record =
            outcomes::record_outcome(&self.storage, session_id, opportunity_id, result, notes)
                .await?;
        if let Err(e) = self.index_outcome_vector(&record).await {
            tracing::warn!("Outcome vector index skipped: {e}");
        }
        self.emit(
            session_id,
            events::OUTCOME_RECORDED,
            serde_json::json!({
                "outcome_id": record.id,
                "opportunity_id": opportunity_id,
                "result": match result {
                    HarvestDealOutcome::Won => "won",
                    HarvestDealOutcome::Lost => "lost",
                    HarvestDealOutcome::Withdrawn => "withdrawn",
                },
            }),
        )
        .await;
        Ok(record)
    }

    /// List recorded outcomes for a session (newest first).
    pub async fn list_harvest_outcomes(
        &self,
        session_id: &SessionId,
        limit: u32,
    ) -> Result<Vec<HarvestOutcomeRecord>> {
        outcomes::list_outcomes(&self.storage, session_id, limit).await
    }

    /// Normalize CDP-captured browser payloads into opportunities / CRM contacts (Upwork).
    pub async fn on_data_captured(
        &self,
        session_id: &SessionId,
        event: BrowserEvent,
    ) -> Result<()> {
        let BrowserEvent::DataCaptured {
            platform,
            kind,
            data,
            ..
        } = event
        else {
            return Ok(());
        };
        let source = match platform.as_str() {
            "upwork" => OpportunitySource::Upwork,
            "freelancer" => OpportunitySource::Freelancer,
            "linkedin" => OpportunitySource::LinkedIn,
            _ => return Ok(()),
        };
        match kind.as_str() {
            "job_listing" => {
                if let Some(jobs) = data.get("jobs").and_then(|v| v.as_array()) {
                    for j in jobs {
                        self.ingest_json_row(session_id, j, source.clone(), "cdp")
                            .await?;
                    }
                } else {
                    self.ingest_json_row(session_id, &data, source, "cdp")
                        .await?;
                }
            }
            "client_profile" if platform.as_str() == "upwork" => {
                self.ingest_upwork_client(session_id, &data).await?;
            }
            _ => {}
        }
        Ok(())
    }

    /// Batch ingest jobs from extension, webhooks, or fleet workers (always re-scored in RUSVEL).
    pub async fn ingest_jobs(
        &self,
        session_id: &SessionId,
        rows: &[serde_json::Value],
        platform: &str,
        ingest_source_tag: &str,
    ) -> Result<Vec<Opportunity>> {
        let source_kind = normalize::opportunity_source_from_platform(platform)?;
        let mut results = Vec::new();
        for row in rows {
            if let Some(opp) = self
                .ingest_json_row(session_id, row, source_kind.clone(), ingest_source_tag)
                .await?
            {
                results.push(opp);
            }
        }
        self.emit(
            session_id,
            events::BATCH_INGEST_COMPLETED,
            serde_json::json!({
                "count": results.len(),
                "input_rows": rows.len(),
                "platform": platform,
            }),
        )
        .await;
        Ok(results)
    }

    async fn ingest_json_row(
        &self,
        session_id: &SessionId,
        row: &serde_json::Value,
        source_kind: OpportunitySource,
        ingest_source_tag: &str,
    ) -> Result<Option<Opportunity>> {
        let (src, mut raw) = normalize::raw_from_ingest_row(row, source_kind);
        if raw.title.trim().is_empty() {
            return Ok(None);
        }
        normalize::prepare_raw(&mut raw, src.clone());
        let hints = self.scoring_outcome_hints_for_raw(session_id, &raw).await;
        let scored = self
            .scorer_for_session(session_id, hints)
            .score(&raw)
            .await?;
        let opportunity = Opportunity {
            id: OpportunityId::new(),
            session_id: *session_id,
            source: src,
            title: scored.raw.title.clone(),
            url: scored.raw.url.clone(),
            description: scored.raw.description.clone(),
            score: scored.score,
            stage: OpportunityStage::Cold,
            value_estimate: parse_budget(&scored.raw.budget),
            platform_job_key: scored.raw.platform_job_key.clone(),
            content_hash: scored.raw.content_hash.clone(),
            upstream_score: scored.raw.upstream_score,
            budget_min: scored.raw.budget_min,
            budget_max: scored.raw.budget_max,
            budget_currency: scored.raw.budget_currency.clone(),
            hourly: scored.raw.hourly,
            client_hire_rate: scored.raw.client_hire_rate,
            client_total_spent: scored.raw.client_total_spent,
            payment_verified: scored.raw.payment_verified,
            proposal_count: scored.raw.proposal_count,
            metadata: serde_json::json!({
                "reasoning": scored.reasoning,
                "skills": scored.raw.skills,
                "posted_at": scored.raw.posted_at,
                "ingest_source": ingest_source_tag,
                "scoring_method": match scored.scoring_method {
                    ScoringMethod::Llm => "llm",
                    ScoringMethod::Keyword => "keyword",
                },
            }),
        };
        let pipe = Pipeline::new(self.storage.clone());
        if let Some(opp) = self
            .try_upsert_opportunity(session_id, opportunity, &pipe)
            .await?
        {
            self.emit(
                session_id,
                events::OPPORTUNITY_DISCOVERED,
                serde_json::json!({"id": opp.id.to_string(), "title": &opp.title, "source": ingest_source_tag}),
            )
            .await;
            self.emit(
                session_id,
                events::OPPORTUNITY_INGESTED,
                serde_json::json!({"id": opp.id.to_string(), "source": ingest_source_tag}),
            )
            .await;
            Ok(Some(opp))
        } else {
            Ok(None)
        }
    }

    async fn ingest_upwork_client(
        &self,
        session_id: &SessionId,
        data: &serde_json::Value,
    ) -> Result<()> {
        let name = data
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown client");
        let links: Vec<String> = data
            .get("profile_url")
            .and_then(|v| v.as_str())
            .map(|s| vec![s.to_string()])
            .unwrap_or_default();
        let contact = Contact {
            id: ContactId::new(),
            session_id: *session_id,
            name: name.into(),
            emails: vec![],
            links,
            company: data
                .get("company")
                .and_then(|v| v.as_str())
                .map(String::from),
            role: None,
            tags: vec!["upwork".into(), "browser".into()],
            last_contacted_at: None,
            metadata: data.clone(),
        };
        self.storage
            .objects()
            .put(
                "contact",
                &contact.id.to_string(),
                serde_json::to_value(&contact)?,
            )
            .await?;
        self.emit(
            session_id,
            "harvest.contact.captured",
            serde_json::json!({"contact_id": contact.id.to_string(), "name": contact.name}),
        )
        .await;
        Ok(())
    }
}

/// Parse a budget string like "$5000" into an f64.
fn parse_budget(budget: &Option<String>) -> Option<f64> {
    budget.as_ref().and_then(|s| {
        s.chars()
            .filter(|c| c.is_ascii_digit() || *c == '.')
            .collect::<String>()
            .parse()
            .ok()
    })
}

#[async_trait]
impl Engine for HarvestEngine {
    fn kind(&self) -> &str {
        "harvest"
    }

    fn name(&self) -> &'static str {
        "Harvest Engine"
    }

    fn capabilities(&self) -> Vec<Capability> {
        vec![Capability::OpportunityDiscovery]
    }

    async fn initialize(&self) -> Result<()> {
        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn health(&self) -> Result<HealthStatus> {
        Ok(HealthStatus {
            healthy: true,
            message: None,
            metadata: serde_json::json!({"skills": self.config.skills}),
        })
    }
}

// ════════════════════════════════════════════════════════════════════
//  Tests
// ════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use rusvel_core::domain::ObjectFilter;
    use rusvel_core::ports::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    // ── In-memory ObjectStore ──────────────────────────────────────

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

    // ── Stub stores ───────────────────────────────────────────────

    struct StubEventStore;
    #[async_trait]
    impl EventStore for StubEventStore {
        async fn append(&self, _: &Event) -> Result<()> {
            Ok(())
        }
        async fn get(&self, _: &EventId) -> Result<Option<Event>> {
            Ok(None)
        }
        async fn query(&self, _: rusvel_core::EventFilter) -> Result<Vec<Event>> {
            Ok(vec![])
        }
    }

    struct StubMetricStore;
    #[async_trait]
    impl MetricStore for StubMetricStore {
        async fn record(&self, _: &rusvel_core::MetricPoint) -> Result<()> {
            Ok(())
        }
        async fn query(
            &self,
            _: rusvel_core::MetricFilter,
        ) -> Result<Vec<rusvel_core::MetricPoint>> {
            Ok(vec![])
        }
    }

    struct StubSessionStore;
    #[async_trait]
    impl SessionStore for StubSessionStore {
        async fn put_session(&self, _: &rusvel_core::Session) -> Result<()> {
            Ok(())
        }
        async fn get_session(&self, _: &SessionId) -> Result<Option<rusvel_core::Session>> {
            Ok(None)
        }
        async fn list_sessions(&self) -> Result<Vec<rusvel_core::SessionSummary>> {
            Ok(vec![])
        }
        async fn put_run(&self, _: &rusvel_core::Run) -> Result<()> {
            Ok(())
        }
        async fn get_run(&self, _: &rusvel_core::RunId) -> Result<Option<rusvel_core::Run>> {
            Ok(None)
        }
        async fn list_runs(&self, _: &SessionId) -> Result<Vec<rusvel_core::Run>> {
            Ok(vec![])
        }
        async fn put_thread(&self, _: &rusvel_core::Thread) -> Result<()> {
            Ok(())
        }
        async fn get_thread(
            &self,
            _: &rusvel_core::ThreadId,
        ) -> Result<Option<rusvel_core::Thread>> {
            Ok(None)
        }
        async fn list_threads(&self, _: &rusvel_core::RunId) -> Result<Vec<rusvel_core::Thread>> {
            Ok(vec![])
        }
    }

    struct StubJobStore;
    #[async_trait]
    impl JobStore for StubJobStore {
        async fn enqueue(&self, _: &rusvel_core::Job) -> Result<()> {
            Ok(())
        }
        async fn dequeue(&self, _: &[rusvel_core::JobKind]) -> Result<Option<rusvel_core::Job>> {
            Ok(None)
        }
        async fn update(&self, _: &rusvel_core::Job) -> Result<()> {
            Ok(())
        }
        async fn get(&self, _: &rusvel_core::JobId) -> Result<Option<rusvel_core::Job>> {
            Ok(None)
        }
        async fn list(&self, _: rusvel_core::JobFilter) -> Result<Vec<rusvel_core::Job>> {
            Ok(vec![])
        }
    }

    // ── TestStorage ───────────────────────────────────────────────

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

    // ── Tests ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn scan_with_mock_source_returns_opportunities() {
        let storage = Arc::new(TestStorage::new());
        let engine = HarvestEngine::new(storage.clone()).with_config(HarvestConfig {
            skills: vec!["rust".into(), "axum".into()],
            min_budget: Some(1000.0),
        });

        let session_id = SessionId::new();
        let results = engine.scan(&session_id, &source::MockSource).await.unwrap();

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].title, "Build a REST API in Rust");
        assert!(results[0].score > 0.0);
        assert_eq!(results[0].stage, OpportunityStage::Cold);
    }

    #[tokio::test]
    async fn pipeline_stores_and_lists() {
        let storage = Arc::new(TestStorage::new());
        let engine = HarvestEngine::new(storage.clone()).with_config(HarvestConfig {
            skills: vec!["rust".into()],
            min_budget: None,
        });

        let session_id = SessionId::new();
        let results = engine.scan(&session_id, &source::MockSource).await.unwrap();
        assert!(!results.is_empty());

        let listed = engine.list_opportunities(&session_id, None).await.unwrap();
        assert_eq!(listed.len(), results.len());
    }

    #[tokio::test]
    async fn pipeline_stats_correct() {
        let storage = Arc::new(TestStorage::new());
        let engine = HarvestEngine::new(storage.clone()).with_config(HarvestConfig {
            skills: vec!["rust".into()],
            min_budget: None,
        });

        let session_id = SessionId::new();
        engine.scan(&session_id, &source::MockSource).await.unwrap();

        let stats = engine.pipeline(&session_id).await.unwrap();
        assert_eq!(stats.total, 3);
        // All should be in Cold stage
        assert_eq!(*stats.by_stage.get("Cold").unwrap_or(&0), 3);
    }

    #[tokio::test]
    async fn health_returns_healthy() {
        let engine = HarvestEngine::new(Arc::new(TestStorage::new()));
        let status = engine.health().await.unwrap();
        assert!(status.healthy);
    }

    #[tokio::test]
    async fn get_proposals_respects_session_filter() {
        let storage = Arc::new(TestStorage::new());
        let engine = HarvestEngine::new(storage.clone());
        let sid = SessionId::new();
        let other = SessionId::new();
        let proposal = Proposal {
            body: "hello".into(),
            estimated_value: None,
            tone: "pro".into(),
            metadata: serde_json::json!({}),
        };
        let rec = StoredProposalRecord {
            session_id: sid,
            opportunity_id: "opp-a".into(),
            proposal,
        };
        storage
            .objects()
            .put("proposal", "opp-a_1", serde_json::to_value(&rec).unwrap())
            .await
            .unwrap();

        let mut rec_other = rec.clone();
        rec_other.session_id = other;
        rec_other.opportunity_id = "opp-b".into();
        storage
            .objects()
            .put(
                "proposal",
                "opp-b_1",
                serde_json::to_value(&rec_other).unwrap(),
            )
            .await
            .unwrap();

        let mine = engine.get_proposals(&sid).await.unwrap();
        assert_eq!(mine.len(), 1);
        assert_eq!(mine[0].body, "hello");

        let empty = engine.get_proposals(&SessionId::new()).await.unwrap();
        assert!(empty.is_empty());
    }
}
