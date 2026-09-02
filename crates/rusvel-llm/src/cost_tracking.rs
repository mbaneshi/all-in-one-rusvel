//! [`LlmPort`] wrapper: tier resolution + [`MetricStore`] spend recording.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::mpsc;
use uuid::Uuid;

use rusvel_core::domain::*;
use rusvel_core::error::Result;
use rusvel_core::id::SessionId;
use rusvel_core::ports::{LlmPort, MetricStore};

use crate::cost::LLM_COST_METRIC_NAME;
use crate::tier_routing::apply_model_tier;

/// Wraps an inner [`LlmPort`], applies [`apply_model_tier`], and records estimated USD per call to [`MetricStore`].
#[derive(Clone)]
pub struct CostTrackingLlm {
    inner: Arc<dyn LlmPort>,
    metrics: Option<Arc<dyn MetricStore>>,
}

impl CostTrackingLlm {
    pub fn new(inner: Arc<dyn LlmPort>) -> Self {
        Self {
            inner,
            metrics: None,
        }
    }

    pub fn with_metrics(inner: Arc<dyn LlmPort>, metrics: Arc<dyn MetricStore>) -> Self {
        Self {
            inner,
            metrics: Some(metrics),
        }
    }

    async fn record_cost(&self, req: &LlmRequest, resp: &LlmResponse) {
        let Some(store) = &self.metrics else {
            return;
        };
        let req_for_cost = effective_request_for_cost(req, resp);
        let usd = response_cost_usd(req, resp);
        let tier = ModelTier::from_request_metadata(&req_for_cost.metadata);
        let mut tags = vec![
            format!("provider:{:?}", req_for_cost.model.provider),
            format!("model:{}", req_for_cost.model.model),
        ];
        if let Some(t) = tier {
            tags.push(format!("tier:{t}"));
        }
        if resp
            .metadata
            .get(RUSVEL_META_BATCH)
            .and_then(|v| v.as_bool())
            == Some(true)
        {
            tags.push("batch:true".into());
        }
        if let Some(sid) = req_for_cost
            .metadata
            .get(RUSVEL_META_SESSION_ID)
            .and_then(|v| v.as_str())
        {
            tags.push(format!("session:{sid}"));
        }
        if let Some(d) = req_for_cost
            .metadata
            .get(RUSVEL_META_DEPARTMENT_ID)
            .and_then(|v| v.as_str())
        {
            tags.push(format!("dept:{d}"));
        }
        let point = MetricPoint {
            name: LLM_COST_METRIC_NAME.into(),
            value: usd,
            tags,
            recorded_at: Utc::now(),
            metadata: serde_json::json!({
                "input_tokens": resp.usage.input_tokens,
                "output_tokens": resp.usage.output_tokens,
            }),
        };
        if let Err(e) = store.record(&point).await {
            tracing::warn!(error = %e, "metric store record failed for {}", LLM_COST_METRIC_NAME);
        }

        let cost_event = CostEvent {
            id: Uuid::now_v7().to_string(),
            session_id: req_for_cost
                .metadata
                .get(RUSVEL_META_SESSION_ID)
                .and_then(|v| v.as_str())
                .and_then(|s| Uuid::parse_str(s).ok())
                .map(SessionId::from),
            department_id: req_for_cost
                .metadata
                .get(RUSVEL_META_DEPARTMENT_ID)
                .and_then(|v| v.as_str())
                .map(str::to_string),
            model: req_for_cost.model.model.clone(),
            provider: format!("{:?}", req_for_cost.model.provider),
            input_tokens: resp.usage.input_tokens,
            output_tokens: resp.usage.output_tokens,
            cost_usd: usd,
            operation: "llm_generate".into(),
            created_at: Utc::now(),
            metadata: serde_json::json!({
                "input_tokens": resp.usage.input_tokens,
                "output_tokens": resp.usage.output_tokens,
            }),
        };
        if let Err(e) = store.record_cost(cost_event).await {
            tracing::warn!(error = %e, "metric store record_cost failed");
        }
    }
}

/// Per-MTok USD rates for one model, from the catalog file or built-in tables.
#[derive(Debug, Clone, PartialEq)]
struct ModelRates {
    input: f64,
    output: f64,
    /// Cache-read (cached input) rate; defaults to 0.1x input when absent.
    cached_input: Option<f64>,
    /// Cache-write rate; defaults to 1.25x input when absent.
    cache_creation_input: Option<f64>,
}

impl ModelRates {
    fn price(&self, usage: &LlmUsage, cache_creation: u64, cache_read: u64) -> f64 {
        const MTOK: f64 = 1_000_000.0;
        self.input * f64::from(usage.input_tokens) / MTOK
            + self.output * f64::from(usage.output_tokens) / MTOK
            + self.cached_input.unwrap_or(self.input * 0.1) * (cache_read as f64) / MTOK
            + self.cache_creation_input.unwrap_or(self.input * 1.25) * (cache_creation as f64)
                / MTOK
    }
}

/// Env var pointing at an optional LiteLLM-style model catalog JSON file.
///
/// When set, per-model prices from the file override the built-in tables;
/// models not present in the file fall back to the built-in tables.
pub const RUSVEL_MODEL_CATALOG_ENV: &str = "RUSVEL_MODEL_CATALOG";

static MODEL_CATALOG: OnceLock<HashMap<String, ModelRates>> = OnceLock::new();

/// Catalog rates keyed by lowercase model id; loaded once from
/// [`RUSVEL_MODEL_CATALOG_ENV`] on first cost computation (empty when unset
/// or unreadable).
fn model_catalog() -> &'static HashMap<String, ModelRates> {
    MODEL_CATALOG.get_or_init(|| match std::env::var(RUSVEL_MODEL_CATALOG_ENV) {
        Ok(path) if !path.trim().is_empty() => match std::fs::read_to_string(&path) {
            Ok(text) => match parse_model_catalog(&text) {
                Ok(map) => {
                    tracing::info!(models = map.len(), %path, "loaded model pricing catalog");
                    map
                }
                Err(e) => {
                    tracing::warn!(%path, error = %e, "invalid {RUSVEL_MODEL_CATALOG_ENV}; using built-in pricing");
                    HashMap::new()
                }
            },
            Err(e) => {
                tracing::warn!(%path, error = %e, "cannot read {RUSVEL_MODEL_CATALOG_ENV}; using built-in pricing");
                HashMap::new()
            }
        },
        _ => HashMap::new(),
    })
}

/// Parse a LiteLLM-style catalog. Tolerant: accepts either a JSON array of
/// entries (`[{"id": ..., "pricing": {"input": per_mtok, ...}}, ...]`) or a
/// map keyed by model id (`{"model": {"input_cost_per_token": ...}, ...}`);
/// entries without both an input and output price are skipped.
fn parse_model_catalog(text: &str) -> std::result::Result<HashMap<String, ModelRates>, String> {
    let val: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let mut map = HashMap::new();
    match &val {
        serde_json::Value::Array(entries) => {
            for entry in entries {
                let Some(id) = entry
                    .get("id")
                    .or_else(|| entry.get("model"))
                    .and_then(|v| v.as_str())
                else {
                    continue;
                };
                if let Some(rates) = catalog_entry_rates(entry) {
                    map.insert(id.to_ascii_lowercase(), rates);
                }
            }
        }
        serde_json::Value::Object(entries) => {
            for (id, entry) in entries {
                if let Some(rates) = catalog_entry_rates(entry) {
                    map.insert(id.to_ascii_lowercase(), rates);
                }
            }
        }
        _ => return Err("expected a JSON array or object of model entries".into()),
    }
    Ok(map)
}

/// Rates from one catalog entry: `pricing.{input,output,...}` (USD per MTok,
/// AvalAI style) or `*_cost_per_token` fields (USD per token, LiteLLM style).
fn catalog_entry_rates(entry: &serde_json::Value) -> Option<ModelRates> {
    let f = |v: &serde_json::Value, key: &str| v.get(key).and_then(serde_json::Value::as_f64);
    if let Some(pricing) = entry.get("pricing") {
        return Some(ModelRates {
            input: f(pricing, "input")?,
            output: f(pricing, "output")?,
            cached_input: f(pricing, "cached_input"),
            cache_creation_input: f(pricing, "cache_creation_input"),
        });
    }
    const MTOK: f64 = 1_000_000.0;
    Some(ModelRates {
        input: f(entry, "input_cost_per_token")? * MTOK,
        output: f(entry, "output_cost_per_token")? * MTOK,
        cached_input: f(entry, "cache_read_input_token_cost").map(|c| c * MTOK),
        cache_creation_input: f(entry, "cache_creation_input_token_cost").map(|c| c * MTOK),
    })
}

/// Per-MTok list prices (input, output) for current Claude models.
///
/// The shared [`estimate_llm_cost_usd`] table in `rusvel-core` predates the
/// Claude 5 generation, so current model ids are priced here; anything not
/// matched (historical cost records, other providers) falls through to the
/// shared estimator.
fn claude_5_gen_rates(model: &str) -> Option<(f64, f64)> {
    let m = model.to_ascii_lowercase();
    if m.starts_with("claude-opus-5") {
        Some((5.0, 25.0))
    } else if m.starts_with("claude-sonnet-5") {
        Some((3.0, 15.0))
    } else if m.starts_with("claude-haiku-4-5") {
        Some((1.0, 5.0))
    } else {
        None
    }
}

/// USD estimate for a call: catalog price if the model is in
/// [`RUSVEL_MODEL_CATALOG_ENV`], else current Claude pricing, else the shared
/// estimator (which does not model cache tokens).
fn estimate_cost_usd_with_cache(
    provider: &ModelProvider,
    model: &str,
    usage: &LlmUsage,
    cache_creation: u64,
    cache_read: u64,
) -> f64 {
    if let Some(rates) = model_catalog().get(&model.to_ascii_lowercase()) {
        return rates.price(usage, cache_creation, cache_read);
    }
    if *provider == ModelProvider::Claude {
        if let Some((input, output)) = claude_5_gen_rates(model) {
            let rates = ModelRates {
                input,
                output,
                cached_input: None,
                cache_creation_input: None,
            };
            return rates.price(usage, cache_creation, cache_read);
        }
    }
    estimate_llm_cost_usd(provider, model, usage)
}

/// [`estimate_cost_usd_with_cache`] without cache token counts.
#[cfg(test)]
fn estimate_cost_usd(provider: &ModelProvider, model: &str, usage: &LlmUsage) -> f64 {
    estimate_cost_usd_with_cache(provider, model, usage, 0, 0)
}

/// Cache token counts `(creation, read)` from response metadata (set by the
/// Claude provider on both sync and streaming responses).
fn cache_tokens(metadata: &serde_json::Value) -> (u64, u64) {
    let get = |key: &str| {
        metadata
            .get(key)
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0)
    };
    (
        get("cache_creation_input_tokens"),
        get("cache_read_input_tokens"),
    )
}

/// USD cost of one response: estimated from usage (and cache tokens) at the
/// effective model's rates, overridden by actual spend when the provider
/// reports it (`metadata.cost_usd`, e.g. Claude CLI), then batch-discounted.
pub fn response_cost_usd(req: &LlmRequest, resp: &LlmResponse) -> f64 {
    let req_for_cost = effective_request_for_cost(req, resp);
    let (cache_creation, cache_read) = cache_tokens(&resp.metadata);
    let mut usd = estimate_cost_usd_with_cache(
        &req_for_cost.model.provider,
        &req_for_cost.model.model,
        &resp.usage,
        cache_creation,
        cache_read,
    );
    // Claude CLI reports actual spend in metadata (usage tokens are often zero).
    if let Some(actual) = resp.metadata.get("cost_usd").and_then(|v| v.as_f64()) {
        usd = actual;
    }
    if let Some(d) = resp
        .metadata
        .get(RUSVEL_META_BATCH_DISCOUNT)
        .and_then(|v| v.as_f64())
    {
        usd *= d;
    }
    usd
}

/// Stamp the computed per-call cost into `metadata.cost_usd` (when not already
/// reported by the provider) so downstream consumers — e.g. the agent loop's
/// `cost_estimate` accumulation — see the same figure that was recorded.
fn stamp_response_cost(req: &LlmRequest, resp: &mut LlmResponse) {
    if resp
        .metadata
        .get("cost_usd")
        .and_then(|v| v.as_f64())
        .is_some()
    {
        return;
    }
    let usd = response_cost_usd(req, resp);
    match resp.metadata.as_object_mut() {
        Some(obj) => {
            obj.insert("cost_usd".into(), serde_json::json!(usd));
        }
        None => {
            resp.metadata = serde_json::json!({ "cost_usd": usd });
        }
    }
}

fn effective_request_for_cost(req: &LlmRequest, resp: &LlmResponse) -> LlmRequest {
    let mut out = req.clone();
    if let Some(m) = resp
        .metadata
        .get(RUSVEL_META_COST_MODEL)
        .and_then(|v| v.as_str())
    {
        out.model.model = m.to_string();
    }
    if let Some(p) = resp
        .metadata
        .get(RUSVEL_META_COST_PROVIDER)
        .and_then(|v| v.as_str())
    {
        out.model.provider = match p {
            "Claude" => ModelProvider::Claude,
            "OpenAI" => ModelProvider::OpenAI,
            "Ollama" => ModelProvider::Ollama,
            "Gemini" => ModelProvider::Gemini,
            _ => ModelProvider::Other(p.into()),
        };
    }
    out
}

#[async_trait]
impl LlmPort for CostTrackingLlm {
    async fn generate(&self, request: LlmRequest) -> Result<LlmResponse> {
        let req = apply_model_tier(request);
        let mut resp = self.inner.generate(req.clone()).await?;
        self.record_cost(&req, &resp).await;
        stamp_response_cost(&req, &mut resp);
        Ok(resp)
    }

    async fn stream(&self, request: LlmRequest) -> Result<mpsc::Receiver<LlmStreamEvent>> {
        let req = apply_model_tier(request);
        let req_snapshot = req.clone();
        let mut inner_rx = self.inner.stream(req).await?;
        let this = self.clone();
        let (tx, out_rx) = mpsc::channel(32);
        tokio::spawn(async move {
            while let Some(ev) = inner_rx.recv().await {
                match ev {
                    LlmStreamEvent::Done(mut resp) => {
                        this.record_cost(&req_snapshot, &resp).await;
                        stamp_response_cost(&req_snapshot, &mut resp);
                        let _ = tx.send(LlmStreamEvent::Done(resp)).await;
                    }
                    other => {
                        if tx.send(other).await.is_err() {
                            break;
                        }
                    }
                }
            }
        });
        Ok(out_rx)
    }

    async fn embed(&self, model: &ModelRef, text: &str) -> Result<Vec<f32>> {
        self.inner.embed(model, text).await
    }

    async fn list_models(&self) -> Result<Vec<ModelRef>> {
        self.inner.list_models().await
    }

    async fn submit_batch(&self, batch: LlmBatchRequest) -> Result<LlmBatchSubmitResult> {
        let LlmBatchRequest { items, metadata } = batch;
        let items = items
            .into_iter()
            .map(|mut item| {
                item.request = apply_model_tier(item.request);
                item
            })
            .collect();
        self.inner
            .submit_batch(LlmBatchRequest { items, metadata })
            .await
    }

    async fn poll_batch(&self, handle: &BatchHandle) -> Result<LlmBatchPollResult> {
        let out = self.inner.poll_batch(handle).await?;
        let this = self.clone();
        for item in &out.items {
            if let Some(resp) = &item.response {
                let req = match &item.model {
                    Some(m) => LlmRequest {
                        model: m.clone(),
                        messages: vec![],
                        tools: vec![],
                        temperature: None,
                        max_tokens: None,
                        metadata: serde_json::json!({}),
                    },
                    None => request_stub_for_batch_cost(resp),
                };
                this.record_cost(&req, resp).await;
            }
        }
        Ok(out)
    }
}

fn request_stub_for_batch_cost(resp: &LlmResponse) -> LlmRequest {
    let model = resp
        .metadata
        .get(RUSVEL_META_COST_MODEL)
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let provider = match resp
        .metadata
        .get(RUSVEL_META_COST_PROVIDER)
        .and_then(|v| v.as_str())
    {
        Some("Claude") => ModelProvider::Claude,
        Some("OpenAI") => ModelProvider::OpenAI,
        Some("Ollama") => ModelProvider::Ollama,
        Some("Gemini") => ModelProvider::Gemini,
        Some(p) => ModelProvider::Other(p.into()),
        None => ModelProvider::Claude,
    };
    LlmRequest {
        model: ModelRef { provider, model },
        messages: vec![],
        tools: vec![],
        temperature: None,
        max_tokens: None,
        metadata: serde_json::json!({}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use rusvel_core::domain::MetricFilter;
    use rusvel_core::error::RusvelError;
    use std::sync::Mutex;

    struct RecordingMetrics {
        points: Mutex<Vec<MetricPoint>>,
    }

    impl RecordingMetrics {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                points: Mutex::new(Vec::new()),
            })
        }
    }

    #[async_trait]
    impl MetricStore for RecordingMetrics {
        async fn record(&self, point: &MetricPoint) -> rusvel_core::error::Result<()> {
            self.points.lock().unwrap().push(point.clone());
            Ok(())
        }

        async fn query(
            &self,
            _filter: MetricFilter,
        ) -> rusvel_core::error::Result<Vec<MetricPoint>> {
            Ok(self.points.lock().unwrap().clone())
        }
    }

    struct EchoModelProvider;

    #[async_trait]
    impl LlmPort for EchoModelProvider {
        async fn generate(&self, request: LlmRequest) -> Result<LlmResponse> {
            Ok(LlmResponse {
                content: Content::text(request.model.model.clone()),
                finish_reason: FinishReason::Stop,
                usage: LlmUsage {
                    input_tokens: 1000,
                    output_tokens: 500,
                },
                metadata: serde_json::json!({}),
            })
        }

        async fn embed(&self, _: &ModelRef, _: &str) -> Result<Vec<f32>> {
            Err(RusvelError::Llm("no embed".into()))
        }

        async fn list_models(&self) -> Result<Vec<ModelRef>> {
            Ok(vec![])
        }
    }

    #[tokio::test]
    async fn records_metric_after_generate() {
        let metrics = RecordingMetrics::new();
        let llm = CostTrackingLlm::with_metrics(
            Arc::new(EchoModelProvider),
            metrics.clone() as Arc<dyn MetricStore>,
        );
        let req = LlmRequest {
            model: ModelRef {
                provider: ModelProvider::Claude,
                model: "claude-sonnet-5".into(),
            },
            messages: vec![],
            tools: vec![],
            temperature: None,
            max_tokens: None,
            metadata: serde_json::json!({
                RUSVEL_META_MODEL_TIER: "fast",
                RUSVEL_META_SESSION_ID: "sess-test-1",
            }),
        };
        let resp = llm.generate(req).await.unwrap();
        match &resp.content.parts[0] {
            Part::Text(t) => assert!(t.contains("haiku")),
            _ => panic!("expected text"),
        }
        let pts = metrics.points.lock().unwrap();
        assert_eq!(pts.len(), 1);
        assert_eq!(pts[0].name, LLM_COST_METRIC_NAME);
        assert!(pts[0].value > 0.0);
        assert!(
            pts[0]
                .tags
                .iter()
                .any(|t| t.contains("session:sess-test-1"))
        );
        assert!(pts[0].tags.iter().any(|t| t.starts_with("tier:")));
    }

    #[tokio::test]
    async fn records_dept_tag_when_present() {
        let metrics = RecordingMetrics::new();
        let llm = CostTrackingLlm::with_metrics(
            Arc::new(EchoModelProvider),
            metrics.clone() as Arc<dyn MetricStore>,
        );
        let mut meta = serde_json::Map::new();
        meta.insert(RUSVEL_META_MODEL_TIER.into(), serde_json::json!("fast"));
        meta.insert(
            RUSVEL_META_SESSION_ID.into(),
            serde_json::json!("sess-test-1"),
        );
        meta.insert(
            RUSVEL_META_DEPARTMENT_ID.into(),
            serde_json::json!("harvest"),
        );
        let req = LlmRequest {
            model: ModelRef {
                provider: ModelProvider::Claude,
                model: "claude-sonnet-5".into(),
            },
            messages: vec![],
            tools: vec![],
            temperature: None,
            max_tokens: None,
            metadata: serde_json::Value::Object(meta),
        };
        let _ = llm.generate(req).await.unwrap();
        let pts = metrics.points.lock().unwrap();
        assert_eq!(pts.len(), 1);
        assert!(pts[0].tags.iter().any(|t| t == "dept:harvest"));
    }

    struct BatchPollOnlyProvider;

    #[async_trait]
    impl LlmPort for BatchPollOnlyProvider {
        async fn generate(&self, _: LlmRequest) -> Result<LlmResponse> {
            Err(RusvelError::Llm("no sync".into()))
        }

        async fn embed(&self, _: &ModelRef, _: &str) -> Result<Vec<f32>> {
            Err(RusvelError::Llm("no embed".into()))
        }

        async fn list_models(&self) -> Result<Vec<ModelRef>> {
            Ok(vec![])
        }

        async fn poll_batch(&self, _: &BatchHandle) -> Result<LlmBatchPollResult> {
            Ok(LlmBatchPollResult {
                status: BatchJobStatus::Ended,
                items: vec![LlmBatchItemOutcome::ok_with_model(
                    "row-1",
                    ModelRef {
                        provider: ModelProvider::Claude,
                        model: "claude-sonnet-5".into(),
                    },
                    LlmResponse {
                        content: Content::text("batch ok"),
                        finish_reason: FinishReason::Stop,
                        usage: LlmUsage {
                            input_tokens: 1_000_000,
                            output_tokens: 0,
                        },
                        metadata: serde_json::json!({
                            RUSVEL_META_BATCH: true,
                            RUSVEL_META_BATCH_DISCOUNT: LLM_BATCH_COST_MULTIPLIER,
                        }),
                    },
                )],
                metadata: serde_json::json!({}),
            })
        }
    }

    #[tokio::test]
    async fn batch_poll_records_half_of_sync_list_price() {
        let metrics = RecordingMetrics::new();
        let llm = CostTrackingLlm::with_metrics(
            Arc::new(BatchPollOnlyProvider),
            metrics.clone() as Arc<dyn MetricStore>,
        );
        let handle = BatchHandle {
            provider: ModelProvider::Claude,
            id: "msgbatch_test".into(),
        };
        llm.poll_batch(&handle).await.unwrap();

        let pts = metrics.points.lock().unwrap();
        assert_eq!(pts.len(), 1);
        let sync_usd = estimate_cost_usd(
            &ModelProvider::Claude,
            "claude-sonnet-5",
            &LlmUsage {
                input_tokens: 1_000_000,
                output_tokens: 0,
            },
        );
        let expected = sync_usd * LLM_BATCH_COST_MULTIPLIER;
        assert!((pts[0].value - expected).abs() < 1e-9);
        assert!(pts[0].tags.iter().any(|t| t == "batch:true"));
    }

    #[tokio::test]
    async fn generate_stamps_cost_usd_metadata() {
        // Even without a MetricStore the wrapper must stamp the per-call cost
        // so the agent loop can accumulate it into AgentOutput::cost_estimate.
        let llm = CostTrackingLlm::new(Arc::new(EchoModelProvider));
        let req = LlmRequest {
            model: ModelRef {
                provider: ModelProvider::Claude,
                model: "claude-sonnet-5".into(),
            },
            messages: vec![],
            tools: vec![],
            temperature: None,
            max_tokens: None,
            metadata: serde_json::json!({}),
        };
        let resp = llm.generate(req).await.unwrap();
        let stamped = resp.metadata.get("cost_usd").and_then(|v| v.as_f64());
        // 1000 in @ $3/MTok + 500 out @ $15/MTok
        let expected = 1000.0 * 3.0 / 1e6 + 500.0 * 15.0 / 1e6;
        assert!(stamped.is_some(), "cost_usd not stamped: {}", resp.metadata);
        assert!((stamped.unwrap() - expected).abs() < 1e-12);
    }

    #[tokio::test]
    async fn stream_done_stamps_cost_usd_metadata() {
        let llm = CostTrackingLlm::new(Arc::new(EchoModelProvider));
        let req = LlmRequest {
            model: ModelRef {
                provider: ModelProvider::Claude,
                model: "claude-sonnet-5".into(),
            },
            messages: vec![],
            tools: vec![],
            temperature: None,
            max_tokens: None,
            metadata: serde_json::json!({}),
        };
        let mut rx = llm.stream(req).await.unwrap();
        let mut done: Option<LlmResponse> = None;
        while let Some(ev) = rx.recv().await {
            if let LlmStreamEvent::Done(resp) = ev {
                done = Some(resp);
            }
        }
        let done = done.expect("stream should end with Done");
        assert!(
            done.metadata
                .get("cost_usd")
                .and_then(|v| v.as_f64())
                .is_some_and(|c| c > 0.0),
            "streaming Done must carry a positive stamped cost_usd: {}",
            done.metadata
        );
    }

    #[test]
    fn stamp_preserves_provider_reported_cost() {
        let req = request_stub_for_batch_cost(&LlmResponse {
            content: Content::text(""),
            finish_reason: FinishReason::Stop,
            usage: LlmUsage::default(),
            metadata: serde_json::json!({}),
        });
        let mut resp = LlmResponse {
            content: Content::text("x"),
            finish_reason: FinishReason::Stop,
            usage: LlmUsage::default(),
            metadata: serde_json::json!({ "cost_usd": 0.42 }),
        };
        stamp_response_cost(&req, &mut resp);
        assert_eq!(
            resp.metadata.get("cost_usd").and_then(|v| v.as_f64()),
            Some(0.42)
        );
    }

    #[test]
    fn parses_avalai_style_catalog() {
        let json = r#"[
            {"id": "claude-sonnet-5", "object": "model",
             "pricing": {"input": 2.0, "cached_input": 0.2, "cache_creation_input": 4.0, "output": 10.0},
             "mode": "chat", "extra_field": [1, 2]},
            {"id": "no-pricing-model", "mode": "chat"},
            {"id": "partial", "pricing": {"input": 1.0}}
        ]"#;
        let map = parse_model_catalog(json).unwrap();
        assert_eq!(map.len(), 1);
        let r = map.get("claude-sonnet-5").unwrap();
        assert_eq!(
            *r,
            ModelRates {
                input: 2.0,
                output: 10.0,
                cached_input: Some(0.2),
                cache_creation_input: Some(4.0),
            }
        );
        // $2/MTok in + $10/MTok out + cached read/write at catalog rates.
        let usage = LlmUsage {
            input_tokens: 1_000_000,
            output_tokens: 1_000_000,
        };
        assert!((r.price(&usage, 0, 0) - 12.0).abs() < 1e-9);
        assert!((r.price(&usage, 1_000_000, 1_000_000) - 16.2).abs() < 1e-9);
    }

    #[test]
    fn parses_litellm_per_token_catalog() {
        let json = r#"{
            "claude-sonnet-5": {
                "input_cost_per_token": 2e-6,
                "output_cost_per_token": 1e-5,
                "cache_read_input_token_cost": 2e-7,
                "litellm_provider": "anthropic"
            }
        }"#;
        let map = parse_model_catalog(json).unwrap();
        let r = map.get("claude-sonnet-5").unwrap();
        assert!((r.input - 2.0).abs() < 1e-9);
        assert!((r.output - 10.0).abs() < 1e-9);
        assert!((r.cached_input.unwrap() - 0.2).abs() < 1e-9);
        assert_eq!(r.cache_creation_input, None);
    }

    #[test]
    fn catalog_rejects_non_collection_json() {
        assert!(parse_model_catalog("42").is_err());
        assert!(parse_model_catalog("not json").is_err());
    }

    #[test]
    fn response_cost_includes_cache_tokens_at_default_multipliers() {
        let req = LlmRequest {
            model: ModelRef {
                provider: ModelProvider::Claude,
                model: "claude-sonnet-5".into(),
            },
            messages: vec![],
            tools: vec![],
            temperature: None,
            max_tokens: None,
            metadata: serde_json::json!({}),
        };
        let resp = LlmResponse {
            content: Content::text("x"),
            finish_reason: FinishReason::Stop,
            usage: LlmUsage {
                input_tokens: 1_000_000,
                output_tokens: 0,
            },
            metadata: serde_json::json!({
                "cache_creation_input_tokens": 1_000_000u64,
                "cache_read_input_tokens": 1_000_000u64,
            }),
        };
        // $3 input + $3.75 cache write (1.25x) + $0.30 cache read (0.1x).
        let usd = response_cost_usd(&req, &resp);
        assert!((usd - 7.05).abs() < 1e-9, "got {usd}");
    }

    #[test]
    fn claude_5_gen_pricing_per_mtok() {
        let usage = LlmUsage {
            input_tokens: 1_000_000,
            output_tokens: 1_000_000,
        };
        let opus = estimate_cost_usd(&ModelProvider::Claude, "claude-opus-5", &usage);
        assert!((opus - 30.0).abs() < 1e-9); // $5 in + $25 out
        let sonnet = estimate_cost_usd(&ModelProvider::Claude, "claude-sonnet-5", &usage);
        assert!((sonnet - 18.0).abs() < 1e-9); // $3 in + $15 out
        let haiku = estimate_cost_usd(&ModelProvider::Claude, "claude-haiku-4-5", &usage);
        assert!((haiku - 6.0).abs() < 1e-9); // $1 in + $5 out
        // Historical model ids fall through to the shared estimator.
        let legacy = estimate_cost_usd(&ModelProvider::Claude, "claude-opus-4-20250514", &usage);
        let shared =
            estimate_llm_cost_usd(&ModelProvider::Claude, "claude-opus-4-20250514", &usage);
        assert!((legacy - shared).abs() < 1e-9);
    }
}
