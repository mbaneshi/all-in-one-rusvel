//! [`LlmPort`] wrapper: tier resolution + [`MetricStore`] spend recording.

use std::sync::Arc;

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
        let mut usd = estimate_cost_usd(
            &req_for_cost.model.provider,
            &req_for_cost.model.model,
            &resp.usage,
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

/// Per-MTok list prices (input, output) for current Claude models.
///
/// The shared [`estimate_llm_cost_usd`] table in `rusvel-core` predates the
/// Claude 5 generation, so current model ids are priced here; anything not
/// matched (historical cost records, other providers) falls through to the
/// shared estimator. Cache reads (~0.1x input) and writes (~1.25x input) are
/// not modeled because [`LlmUsage`] does not carry cache token counts.
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

/// USD estimate for a call: current Claude pricing, else the shared estimator.
fn estimate_cost_usd(provider: &ModelProvider, model: &str, usage: &LlmUsage) -> f64 {
    if *provider == ModelProvider::Claude {
        if let Some((inp, outp)) = claude_5_gen_rates(model) {
            let in_m = f64::from(usage.input_tokens) / 1_000_000.0;
            let out_m = f64::from(usage.output_tokens) / 1_000_000.0;
            return inp * in_m + outp * out_m;
        }
    }
    estimate_llm_cost_usd(provider, model, usage)
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
        let resp = self.inner.generate(req.clone()).await?;
        self.record_cost(&req, &resp).await;
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
                    LlmStreamEvent::Done(resp) => {
                        this.record_cost(&req_snapshot, &resp).await;
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
        let shared = estimate_llm_cost_usd(&ModelProvider::Claude, "claude-opus-4-20250514", &usage);
        assert!((legacy - shared).abs() < 1e-9);
    }
}
