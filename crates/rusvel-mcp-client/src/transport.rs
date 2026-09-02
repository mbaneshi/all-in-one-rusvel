//! Stdio transport for MCP JSON-RPC communication.
//!
//! Spawns a child process, sends JSON-RPC requests via stdin,
//! reads responses from stdout. Newline-delimited JSON.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, oneshot};
use tracing::{debug, trace, warn};

use rusvel_core::error::{Result, RusvelError};

/// JSON-RPC request.
#[derive(serde::Serialize)]
struct JsonRpcRequest {
    jsonrpc: &'static str,
    id: u64,
    method: String,
    params: serde_json::Value,
}

/// JSON-RPC notification (no id, no response expected).
#[derive(serde::Serialize)]
struct JsonRpcNotification {
    jsonrpc: &'static str,
    method: String,
    params: serde_json::Value,
}

/// JSON-RPC response.
#[derive(serde::Deserialize)]
struct JsonRpcResponse {
    #[allow(dead_code)]
    jsonrpc: String,
    id: Option<u64>,
    result: Option<serde_json::Value>,
    error: Option<JsonRpcError>,
}

#[derive(serde::Deserialize)]
struct JsonRpcError {
    code: i32,
    message: String,
}

/// Pending request waiting for a response.
type PendingMap = HashMap<u64, oneshot::Sender<std::result::Result<serde_json::Value, String>>>;

/// Stdio-based MCP transport.
///
/// Manages a child process, correlates JSON-RPC request/response pairs,
/// and provides async `request()` and `notify()` methods.
pub struct StdioTransport {
    stdin: Mutex<tokio::process::ChildStdin>,
    child: Mutex<Child>,
    next_id: AtomicU64,
    pending: Arc<Mutex<PendingMap>>,
}

impl StdioTransport {
    /// Spawn a child process and start the response reader task.
    #[allow(clippy::unused_async_trait_impl)]
    pub async fn spawn(
        command: &str,
        args: &[String],
        env: HashMap<String, String>,
    ) -> Result<Self> {
        debug!(command, ?args, "Spawning MCP server process");

        let mut cmd = Command::new(command);
        cmd.args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        for (k, v) in &env {
            cmd.env(k, v);
        }

        let mut child = cmd.spawn().map_err(|e| {
            RusvelError::Tool(format!("Failed to spawn MCP server '{command}': {e}"))
        })?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| RusvelError::Tool("No stdin on MCP server process".into()))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| RusvelError::Tool("No stdout on MCP server process".into()))?;

        let pending: Arc<Mutex<PendingMap>> = Arc::new(Mutex::new(HashMap::new()));

        // Spawn a task that reads JSON-RPC responses from stdout
        // and dispatches them to the correct pending request.
        let pending_reader = Arc::clone(&pending);
        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();

            while let Ok(Some(line)) = lines.next_line().await {
                let line = line.trim().to_string();
                if line.is_empty() {
                    continue;
                }

                trace!(raw = %line, "MCP stdout");

                let resp: JsonRpcResponse = match serde_json::from_str(&line) {
                    Ok(r) => r,
                    Err(e) => {
                        warn!(error = %e, line = %line, "Failed to parse MCP response");
                        continue;
                    }
                };

                if let Some(id) = resp.id {
                    let mut pending = pending_reader.lock().await;
                    if let Some(tx) = pending.remove(&id) {
                        let result = if let Some(err) = resp.error {
                            Err(format!("MCP error {}: {}", err.code, err.message))
                        } else {
                            Ok(resp.result.unwrap_or(serde_json::Value::Null))
                        };
                        let _ = tx.send(result);
                    } else {
                        warn!(id, "Received response for unknown request ID");
                    }
                }
                // Notifications from the server (no id) are ignored for now.
            }

            debug!("MCP stdout reader ended");
        });

        Ok(Self {
            stdin: Mutex::new(stdin),
            child: Mutex::new(child),
            next_id: AtomicU64::new(1),
            pending,
        })
    }

    /// Send a JSON-RPC request and wait for the response.
    pub async fn request(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);

        let req = JsonRpcRequest {
            jsonrpc: "2.0",
            id,
            method: method.into(),
            params,
        };

        let (tx, rx) = oneshot::channel();

        // Register the pending request before sending.
        self.pending.lock().await.insert(id, tx);

        // Serialize and send.
        let mut line = serde_json::to_string(&req)
            .map_err(|e| RusvelError::Tool(format!("JSON serialize error: {e}")))?;
        line.push('\n');

        {
            let mut stdin = self.stdin.lock().await;
            stdin.write_all(line.as_bytes()).await.map_err(|e| {
                RusvelError::Tool(format!("Failed to write to MCP server stdin: {e}"))
            })?;
            stdin
                .flush()
                .await
                .map_err(|e| RusvelError::Tool(format!("Failed to flush MCP server stdin: {e}")))?;
        }

        debug!(id, method, "Sent MCP request");

        // Wait for response with a timeout.
        let result = tokio::time::timeout(std::time::Duration::from_secs(30), rx).await;

        match result {
            Ok(Ok(Ok(value))) => Ok(value),
            Ok(Ok(Err(err_msg))) => Err(RusvelError::Tool(format!("MCP server error: {err_msg}"))),
            Ok(Err(_)) => Err(RusvelError::Tool(
                "MCP response channel closed unexpectedly".into(),
            )),
            Err(_) => {
                // Remove the pending request on timeout.
                self.pending.lock().await.remove(&id);
                Err(RusvelError::Tool(format!(
                    "MCP request '{method}' timed out after 30s"
                )))
            }
        }
    }

    /// Send a JSON-RPC notification (no response expected).
    pub async fn notify(&self, method: &str, params: serde_json::Value) -> Result<()> {
        let notification = JsonRpcNotification {
            jsonrpc: "2.0",
            method: method.into(),
            params,
        };

        let mut line = serde_json::to_string(&notification)
            .map_err(|e| RusvelError::Tool(format!("JSON serialize error: {e}")))?;
        line.push('\n');

        let mut stdin = self.stdin.lock().await;
        stdin.write_all(line.as_bytes()).await.map_err(|e| {
            RusvelError::Tool(format!("Failed to write notification to MCP server: {e}"))
        })?;
        stdin
            .flush()
            .await
            .map_err(|e| RusvelError::Tool(format!("Failed to flush MCP server stdin: {e}")))?;

        debug!(method, "Sent MCP notification");
        Ok(())
    }

    /// Kill the child process.
    pub async fn shutdown(&self) -> Result<()> {
        let mut child = self.child.lock().await;
        child
            .kill()
            .await
            .map_err(|e| RusvelError::Tool(format!("Failed to kill MCP server: {e}")))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn spawn_echo_server() {
        // Use a simple shell script that acts as a mock MCP server:
        // reads a line, echoes back a JSON-RPC response.
        let script = r#"
            while IFS= read -r line; do
                id=$(echo "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
                method=$(echo "$line" | sed -n 's/.*"method":"\([^"]*\)".*/\1/p')
                if [ "$method" = "initialize" ]; then
                    echo "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"protocolVersion\":\"2024-11-05\",\"serverInfo\":{\"name\":\"mock\",\"version\":\"0.1.0\"},\"capabilities\":{\"tools\":{}}}}"
                elif [ "$method" = "tools/list" ]; then
                    echo "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"tools\":[{\"name\":\"echo\",\"description\":\"Echo back\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"text\":{\"type\":\"string\"}},\"required\":[\"text\"]}}]}}"
                elif [ "$method" = "tools/call" ]; then
                    echo "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"content\":[{\"type\":\"text\",\"text\":\"echoed\"}]}}"
                else
                    echo "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":null}"
                fi
            done
        "#;

        let transport =
            StdioTransport::spawn("bash", &["-c".into(), script.into()], HashMap::new())
                .await
                .unwrap();

        // Test initialize.
        let result = transport
            .request(
                "initialize",
                serde_json::json!({"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "test", "version": "0.1.0"}}),
            )
            .await
            .unwrap();

        assert_eq!(result["serverInfo"]["name"].as_str(), Some("mock"));

        // Test tools/list.
        let result = transport
            .request("tools/list", serde_json::json!({}))
            .await
            .unwrap();

        let tools = result["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"].as_str(), Some("echo"));

        // Test tools/call.
        let result = transport
            .request(
                "tools/call",
                serde_json::json!({"name": "echo", "arguments": {"text": "hi"}}),
            )
            .await
            .unwrap();

        let text = result["content"][0]["text"].as_str().unwrap();
        assert_eq!(text, "echoed");

        transport.shutdown().await.unwrap();
    }
}
