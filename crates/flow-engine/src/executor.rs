//! DAG executor — builds a petgraph from a FlowDef and walks it.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use chrono::Utc;
use petgraph::stable_graph::{NodeIndex, StableDiGraph};
use petgraph::visit::{EdgeRef, Topo};

use rusvel_core::domain::*;
use rusvel_core::error::{Result, RusvelError};
use rusvel_core::id::{FlowExecutionId, FlowNodeId, PaneId, WindowId};
use rusvel_core::ports::{StoragePort, TerminalPort};
use rusvel_core::terminal::{PaneSize, PaneSource, WindowSource};

use crate::CHECKPOINT_STORE;
use crate::expressions::{flow_parameter_context, resolve_expressions};
use crate::nodes::{NodeContext, NodeRegistry};

struct FlowTerminalState {
    terminal: Arc<dyn TerminalPort>,
    window_id: WindowId,
}

/// State restored from a [`FlowCheckpoint`] for resume.
#[derive(Debug, Clone)]
pub struct ResumeState {
    pub outputs: HashMap<FlowNodeId, serde_json::Value>,
    pub node_results: HashMap<String, FlowNodeResult>,
    pub skipped: HashSet<FlowNodeId>,
    pub completed_success: Vec<String>,
}

impl ResumeState {
    pub fn from_checkpoint(ck: &FlowCheckpoint) -> Result<Self> {
        let mut outputs = HashMap::new();
        for (k, v) in &ck.node_outputs {
            let uuid = uuid::Uuid::parse_str(k).map_err(|_| {
                RusvelError::Validation(format!("invalid node id in checkpoint: {k}"))
            })?;
            outputs.insert(FlowNodeId::from_uuid(uuid), v.clone());
        }
        let mut skipped = HashSet::new();
        for (nid, res) in &ck.node_results {
            if res.status == FlowNodeStatus::Skipped {
                if let Ok(uuid) = uuid::Uuid::parse_str(nid) {
                    skipped.insert(FlowNodeId::from_uuid(uuid));
                }
            }
        }
        Ok(Self {
            outputs,
            node_results: ck.node_results.clone(),
            skipped,
            completed_success: ck.completed_nodes.clone(),
        })
    }
}

/// Optional checkpoint persistence during execution.
pub struct CheckpointCtx<'a> {
    pub storage: &'a Arc<dyn StoragePort>,
    pub flow: &'a FlowDef,
    pub execution_id: FlowExecutionId,
    pub trigger_data: serde_json::Value,
    pub started_at: chrono::DateTime<chrono::Utc>,
}

pub struct ExecuteFlowConfig<'a> {
    pub execution_id: FlowExecutionId,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub resume: Option<ResumeState>,
    pub checkpoint: Option<CheckpointCtx<'a>>,
    pub terminal: Option<Arc<dyn TerminalPort>>,
}

async fn persist_checkpoint(
    ctx: &CheckpointCtx<'_>,
    completed_nodes: &[String],
    outputs: &HashMap<FlowNodeId, serde_json::Value>,
    node_results: &HashMap<String, FlowNodeResult>,
    failed_node: Option<&str>,
    error: Option<&str>,
) -> Result<()> {
    let mut node_outputs = HashMap::new();
    for (k, v) in outputs {
        node_outputs.insert(k.to_string(), v.clone());
    }
    let ck = FlowCheckpoint {
        flow_id: ctx.flow.id.to_string(),
        execution_id: ctx.execution_id.to_string(),
        completed_nodes: completed_nodes.to_vec(),
        node_outputs,
        node_results: node_results.clone(),
        failed_node: failed_node.map(String::from),
        error: error.map(String::from),
        trigger_data: ctx.trigger_data.clone(),
        started_at: Some(ctx.started_at),
        created_at: Utc::now(),
        metadata: serde_json::Value::default(),
    };
    let val = serde_json::to_value(&ck)?;
    ctx.storage
        .objects()
        .put(CHECKPOINT_STORE, &ctx.execution_id.to_string(), val)
        .await
}

/// Execute a flow definition, producing a FlowExecution record (no checkpointing).
pub async fn execute_flow(
    flow: &FlowDef,
    trigger_data: serde_json::Value,
    registry: &NodeRegistry,
    terminal: Option<Arc<dyn TerminalPort>>,
) -> Result<FlowExecution> {
    execute_flow_with_config(
        flow,
        trigger_data,
        registry,
        ExecuteFlowConfig {
            execution_id: FlowExecutionId::new(),
            started_at: Utc::now(),
            resume: None,
            checkpoint: None,
            terminal,
        },
    )
    .await
}

/// Full execution with optional resume state and checkpoint persistence.
pub async fn execute_flow_with_config(
    flow: &FlowDef,
    trigger_data: serde_json::Value,
    registry: &NodeRegistry,
    config: ExecuteFlowConfig<'_>,
) -> Result<FlowExecution> {
    let exec_id = config.execution_id;
    let started_at = config.started_at;

    let mut graph: StableDiGraph<FlowNodeId, String> = StableDiGraph::new();
    let mut id_to_index: HashMap<FlowNodeId, NodeIndex> = HashMap::new();
    let mut id_to_node: HashMap<FlowNodeId, &FlowNodeDef> = HashMap::new();

    for node in &flow.nodes {
        let idx = graph.add_node(node.id);
        id_to_index.insert(node.id, idx);
        id_to_node.insert(node.id, node);
    }

    for conn in &flow.connections {
        if let (Some(&src_idx), Some(&tgt_idx)) = (
            id_to_index.get(&conn.source_node),
            id_to_index.get(&conn.target_node),
        ) {
            graph.add_edge(src_idx, tgt_idx, conn.source_output.clone());
        }
    }

    if petgraph::algo::is_cyclic_directed(&graph) {
        return Err(RusvelError::Validation("Flow contains a cycle".into()));
    }

    let flow_terminal: Option<FlowTerminalState> = if let Some(ref t) = config.terminal {
        let session_id = rusvel_core::id::SessionId::new();
        match t
            .create_window(
                &session_id,
                &format!("flow-{}", flow.id),
                WindowSource::Manual,
            )
            .await
        {
            Ok(window_id) => Some(FlowTerminalState {
                terminal: t.clone(),
                window_id,
            }),
            Err(e) => {
                tracing::warn!("flow terminal window unavailable: {e}");
                None
            }
        }
    } else {
        None
    };

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
    let pane_size = PaneSize { rows: 24, cols: 80 };

    let resume = config.resume;
    let (mut outputs, mut node_results, skipped, mut completed_success): (
        HashMap<FlowNodeId, serde_json::Value>,
        HashMap<String, FlowNodeResult>,
        HashSet<FlowNodeId>,
        Vec<String>,
    ) = match resume {
        Some(mut r) => (
            std::mem::take(&mut r.outputs),
            std::mem::take(&mut r.node_results),
            r.skipped,
            std::mem::take(&mut r.completed_success),
        ),
        None => (HashMap::new(), HashMap::new(), HashSet::new(), Vec::new()),
    };
    let mut skipped = skipped;
    let trigger_for_exec = trigger_data.clone();

    let mut topo = Topo::new(&graph);
    while let Some(node_idx) = topo.next(&graph) {
        let node_id = graph[node_idx];
        let node_def = match id_to_node.get(&node_id) {
            Some(n) => *n,
            None => continue,
        };

        if skipped.contains(&node_id) {
            node_results
                .entry(node_id.to_string())
                .or_insert(FlowNodeResult {
                    status: FlowNodeStatus::Skipped,
                    output: None,
                    error: None,
                    started_at: None,
                    finished_at: None,
                    metadata: serde_json::Value::default(),
                });
            continue;
        }

        if let Some(r) = node_results.get(&node_id.to_string()) {
            if r.status == FlowNodeStatus::Succeeded {
                continue;
            }
        }

        let mut inputs: HashMap<String, serde_json::Value> = HashMap::new();
        for edge in graph.edges_directed(node_idx, petgraph::Direction::Incoming) {
            let source_id = graph[edge.source()];
            if let Some(data) = outputs.get(&source_id) {
                inputs.insert(source_id.to_string(), data.clone());
            }
        }

        if inputs.is_empty() {
            inputs.insert("trigger".into(), trigger_for_exec.clone());
        }

        let handler = match registry.get(&node_def.node_type) {
            Some(h) => h,
            None => {
                return Err(RusvelError::Internal(format!(
                    "Unknown node type: {}",
                    node_def.node_type
                )));
            }
        };

        let tpl_ctx = flow_parameter_context(&trigger_for_exec, &inputs);
        let mut resolved_node = (*node_def).clone();
        resolved_node.parameters = resolve_expressions(&node_def.parameters, &tpl_ctx);

        let ctx = NodeContext {
            node: resolved_node,
            inputs,
            variables: flow.variables.clone(),
        };

        let mut flow_pane: Option<PaneId> = None;
        if let Some(ref ts) = flow_terminal {
            if let Ok(pid) = ts
                .terminal
                .create_pane(
                    &ts.window_id,
                    "tail -f /dev/null",
                    cwd.as_path(),
                    pane_size,
                    PaneSource::FlowNode {
                        flow_id: flow.id.to_string(),
                        node_id: node_id.to_string(),
                        execution_id: exec_id.to_string(),
                    },
                    None,
                )
                .await
            {
                let header = format!(
                    "\r\n\x1b[1;36m[rusvel:flow]\x1b[0m {} ({}) — start\r\n",
                    node_def.name, node_def.node_type
                );
                let _ = ts
                    .terminal
                    .inject_pane_output(&pid, header.as_bytes())
                    .await;
                flow_pane = Some(pid);
            }
        }

        match handler.execute(&ctx).await {
            Ok(output) => {
                outputs.insert(node_id, output.data.clone());
                node_results.insert(
                    node_id.to_string(),
                    FlowNodeResult {
                        status: FlowNodeStatus::Succeeded,
                        output: Some(output.data.clone()),
                        error: None,
                        started_at: Some(Utc::now()),
                        finished_at: Some(Utc::now()),
                        metadata: serde_json::Value::default(),
                    },
                );
                completed_success.push(node_id.to_string());

                if let (Some(ts), Some(pid)) = (&flow_terminal, &flow_pane) {
                    let body = serde_json::to_string_pretty(&output.data)
                        .unwrap_or_else(|_| output.data.to_string());
                    let _ = ts
                        .terminal
                        .inject_pane_output(pid, format!("{body}\r\n").as_bytes())
                        .await;
                    let _ = ts
                        .terminal
                        .inject_pane_output(
                            pid,
                            format!(
                                "\x1b[1;36m[rusvel:flow]\x1b[0m finished: ok ({:?})\r\n",
                                FlowNodeStatus::Succeeded
                            )
                            .as_bytes(),
                        )
                        .await;
                }

                if node_def.node_type == "condition" {
                    for edge in graph.edges_directed(node_idx, petgraph::Direction::Outgoing) {
                        if *edge.weight() != output.output_name {
                            mark_skipped(&graph, edge.target(), &mut skipped);
                        }
                    }
                }

                if let Some(ref ck) = config.checkpoint {
                    persist_checkpoint(ck, &completed_success, &outputs, &node_results, None, None)
                        .await?;
                }
            }
            Err(e) => {
                node_results.insert(
                    node_id.to_string(),
                    FlowNodeResult {
                        status: FlowNodeStatus::Failed,
                        output: None,
                        error: Some(e.to_string()),
                        started_at: Some(Utc::now()),
                        finished_at: Some(Utc::now()),
                        metadata: serde_json::Value::default(),
                    },
                );

                if let (Some(ts), Some(pid)) = (&flow_terminal, &flow_pane) {
                    let _ = ts
                        .terminal
                        .inject_pane_output(pid, format!("{e}\r\n").as_bytes())
                        .await;
                    let _ = ts
                        .terminal
                        .inject_pane_output(
                            pid,
                            format!(
                                "\x1b[1;36m[rusvel:flow]\x1b[0m finished: {:?}\r\n",
                                FlowNodeStatus::Failed
                            )
                            .as_bytes(),
                        )
                        .await;
                }

                match node_def.on_error {
                    FlowErrorBehavior::StopFlow => {
                        if let Some(ref ck) = config.checkpoint {
                            persist_checkpoint(
                                ck,
                                &completed_success,
                                &outputs,
                                &node_results,
                                Some(&node_id.to_string()),
                                Some(&e.to_string()),
                            )
                            .await?;
                        }
                        return Ok(FlowExecution {
                            id: exec_id,
                            flow_id: flow.id,
                            status: FlowExecutionStatus::Failed,
                            trigger_data,
                            node_results,
                            started_at,
                            finished_at: Some(Utc::now()),
                            error: Some(e.to_string()),
                            metadata: serde_json::json!({}),
                        });
                    }
                    FlowErrorBehavior::ContinueOnFail => {}
                    FlowErrorBehavior::UseErrorOutput => {
                        outputs.insert(node_id, serde_json::json!({"error": e.to_string()}));
                    }
                }
            }
        }
    }

    let all_ok = node_results
        .values()
        .all(|r| r.status == FlowNodeStatus::Succeeded || r.status == FlowNodeStatus::Skipped);

    Ok(FlowExecution {
        id: exec_id,
        flow_id: flow.id,
        status: if all_ok {
            FlowExecutionStatus::Succeeded
        } else {
            FlowExecutionStatus::Failed
        },
        trigger_data,
        node_results,
        started_at,
        finished_at: Some(Utc::now()),
        error: None,
        metadata: serde_json::json!({}),
    })
}

/// Re-run a single node (inputs from upstream checkpoint outputs).
pub async fn execute_single_node(
    flow: &FlowDef,
    node_id: FlowNodeId,
    outputs: &HashMap<FlowNodeId, serde_json::Value>,
    trigger_data: &serde_json::Value,
    registry: &NodeRegistry,
) -> Result<FlowNodeResult> {
    let mut graph: StableDiGraph<FlowNodeId, String> = StableDiGraph::new();
    let mut id_to_index: HashMap<FlowNodeId, NodeIndex> = HashMap::new();
    let mut id_to_node: HashMap<FlowNodeId, &FlowNodeDef> = HashMap::new();

    for node in &flow.nodes {
        let idx = graph.add_node(node.id);
        id_to_index.insert(node.id, idx);
        id_to_node.insert(node.id, node);
    }

    for conn in &flow.connections {
        if let (Some(&src_idx), Some(&tgt_idx)) = (
            id_to_index.get(&conn.source_node),
            id_to_index.get(&conn.target_node),
        ) {
            graph.add_edge(src_idx, tgt_idx, conn.source_output.clone());
        }
    }

    let node_idx = *id_to_index
        .get(&node_id)
        .ok_or_else(|| RusvelError::Validation("node not in flow".into()))?;
    let node_def = id_to_node
        .get(&node_id)
        .copied()
        .ok_or_else(|| RusvelError::Validation("node not in flow".into()))?;

    let mut inputs: HashMap<String, serde_json::Value> = HashMap::new();
    for edge in graph.edges_directed(node_idx, petgraph::Direction::Incoming) {
        let source_id = graph[edge.source()];
        if let Some(data) = outputs.get(&source_id) {
            inputs.insert(source_id.to_string(), data.clone());
        }
    }
    if inputs.is_empty() {
        inputs.insert("trigger".into(), trigger_data.clone());
    }

    let handler = registry.get(&node_def.node_type).ok_or_else(|| {
        RusvelError::Internal(format!("Unknown node type: {}", node_def.node_type))
    })?;

    let ctx = NodeContext {
        node: node_def.clone(),
        inputs,
        variables: flow.variables.clone(),
    };

    let started = Utc::now();
    match handler.execute(&ctx).await {
        Ok(output) => Ok(FlowNodeResult {
            status: FlowNodeStatus::Succeeded,
            output: Some(output.data),
            error: None,
            started_at: Some(started),
            finished_at: Some(Utc::now()),
            metadata: serde_json::Value::default(),
        }),
        Err(e) => Ok(FlowNodeResult {
            status: FlowNodeStatus::Failed,
            output: None,
            error: Some(e.to_string()),
            started_at: Some(started),
            finished_at: Some(Utc::now()),
            metadata: serde_json::Value::default(),
        }),
    }
}

/// Recursively mark a node and all its downstream descendants as skipped.
fn mark_skipped(
    graph: &StableDiGraph<FlowNodeId, String>,
    node_idx: NodeIndex,
    skipped: &mut HashSet<FlowNodeId>,
) {
    let node_id = graph[node_idx];
    if skipped.insert(node_id) {
        for edge in graph.edges_directed(node_idx, petgraph::Direction::Outgoing) {
            mark_skipped(graph, edge.target(), skipped);
        }
    }
}
