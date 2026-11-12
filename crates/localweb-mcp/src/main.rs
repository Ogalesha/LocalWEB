//! `localweb-mcp` serves the `LocalWeb` daemon to Model Context Protocol clients.
//!
//! The server speaks newline-delimited JSON-RPC 2.0 over stdin/stdout, the
//! standard MCP stdio transport, and implements the `initialize`, `ping`,
//! `tools/list`, and `tools/call` methods. Every tool maps either to one
//! daemon request or to a privileged helper run, so an AI assistant can
//! create projects, start them, and execute commands inside the managed
//! runtime exactly like the desktop application does.

mod tools;

use anyhow::Result;
use serde_json::{Value, json};
use std::io::{BufRead, Read, Write};

/// Protocol revisions this server implements, newest first.
const PROTOCOL_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];
/// Revision used when the client asks for something unknown.
const PROTOCOL_FALLBACK: &str = "2025-06-18";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Phase {
    #[default]
    AwaitingInitialize,
    AwaitingInitialized,
    Ready,
}

#[derive(Default)]
struct Session {
    phase: Phase,
}

#[tokio::main]
async fn main() -> Result<()> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let mut reader = stdin.lock();
    let mut session = Session::default();
    let mut line = Vec::new();
    loop {
        line.clear();
        if reader
            .by_ref()
            .take((localweb_core::MAX_MESSAGE_BYTES + 1) as u64)
            .read_until(b'\n', &mut line)?
            == 0
        {
            break;
        }
        if line.len() > localweb_core::MAX_MESSAGE_BYTES {
            writeln!(
                stdout,
                "{}",
                error_response(&Value::Null, -32600, "request too large")
            )?;
            stdout.flush()?;
            break;
        }
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        let reply = match serde_json::from_slice::<Value>(&line) {
            Ok(message) => session.handle(message).await,
            Err(error) => Some(error_response(
                &Value::Null,
                -32700,
                &format!("parse error: {error}"),
            )),
        };
        if let Some(reply) = reply {
            writeln!(stdout, "{reply}")?;
            stdout.flush()?;
        }
    }
    Ok(())
}

/// Turns one incoming message into an optional JSON-RPC response.
///
/// Returns `None` for notifications and stray responses, which must not be
/// answered on the wire.
impl Session {
    async fn handle(&mut self, message: Value) -> Option<Value> {
        if message.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
            return Some(error_response(
                &Value::Null,
                -32600,
                "invalid JSON-RPC request",
            ));
        }
        let Some(method) = message.get("method").and_then(Value::as_str) else {
            // A response (result/error) has no method and must stay silent, but
            // an id-bearing frame that is neither a request nor a response is
            // malformed and cannot be dropped without leaving the client waiting.
            let is_response = message.get("result").is_some() || message.get("error").is_some();
            return (message.get("id").is_some() && !is_response).then(|| {
                error_response(
                    &Value::Null,
                    -32600,
                    "invalid JSON-RPC request: missing method",
                )
            });
        };
        let Some(id) = message.get("id") else {
            if method == "notifications/initialized" && self.phase == Phase::AwaitingInitialized {
                self.phase = Phase::Ready;
            }
            return None;
        };
        if !(id.is_string() || id.as_i64().is_some() || id.as_u64().is_some()) {
            return Some(error_response(&Value::Null, -32600, "invalid request id"));
        }
        let id = id.clone();
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        let reply = match method {
            "initialize" if self.phase != Phase::AwaitingInitialize => {
                Err((-32600, "session is already initialized".into()))
            }
            "initialize"
                if params
                    .get("protocolVersion")
                    .and_then(Value::as_str)
                    .is_none() =>
            {
                Err((-32602, "missing protocol version".into()))
            }
            "initialize" => {
                self.phase = Phase::AwaitingInitialized;
                Ok(initialize(&params))
            }
            "ping" => Ok(json!({})),
            _ if self.phase != Phase::Ready => Err((
                -32002,
                "complete initialize and notifications/initialized before using tools".into(),
            )),
            "tools/list" => Ok(json!({ "tools": tools::definitions() })),
            "tools/call" => tools_call(&params).await,
            other => Err((-32601, format!("method not found: {other}"))),
        };
        Some(match reply {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err((code, message)) => error_response(&id, code, &message),
        })
    }
}

/// Builds the `initialize` result with version negotiation and guidance.
fn initialize(params: &Value) -> Value {
    let version = params
        .get("protocolVersion")
        .and_then(Value::as_str)
        .filter(|requested| PROTOCOL_VERSIONS.contains(requested))
        .unwrap_or(PROTOCOL_FALLBACK);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": {} },
        "serverInfo": {
            "name": "localweb",
            "title": "LocalWeb",
            "version": env!("CARGO_PKG_VERSION"),
        },
        "instructions": "Check localweb_status first. If needed, start LocalWeb with localweb_daemon_start. Add a folder with localweb_project_add, start it with localweb_project_start, and run commands with localweb_project_exec. Use the returned project URLs for HTTP and HTTPS.",
    })
}

/// Executes one `tools/call` request and wraps the outcome as MCP content.
async fn tools_call(params: &Value) -> Result<Value, (i64, String)> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or((-32602, "missing tool name".to_owned()))?;
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    Ok(match tools::call(name, &arguments).await {
        Ok(text) => json!({
            "content": [{ "type": "text", "text": text }],
            "isError": false,
        }),
        Err(error) => json!({
            "content": [{ "type": "text", "text": error }],
            "isError": true,
        }),
    })
}

/// Builds a JSON-RPC error response.
fn error_response(id: &Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn ready_session() -> Session {
        let mut session = Session::default();
        session.handle(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}})).await.unwrap();
        assert_eq!(session.phase, Phase::AwaitingInitialized);
        assert!(
            session
                .handle(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
                .await
                .is_none()
        );
        assert_eq!(session.phase, Phase::Ready);
        session
    }

    #[tokio::test]
    async fn tools_require_both_initialization_steps_and_reinitialization_is_rejected() {
        let mut session = Session::default();
        let tools = json!({"jsonrpc":"2.0","id":2,"method":"tools/list"});
        assert_eq!(
            session.handle(tools.clone()).await.unwrap()["error"]["code"],
            -32002
        );
        session
            .handle(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
            .await;
        assert_eq!(session.phase, Phase::AwaitingInitialize);
        session.handle(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}})).await;
        assert_eq!(
            session.handle(tools.clone()).await.unwrap()["error"]["code"],
            -32002
        );
        session
            .handle(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
            .await;
        assert!(session.handle(tools).await.unwrap().get("result").is_some());
        assert_eq!(session.handle(json!({"jsonrpc":"2.0","id":3,"method":"initialize","params":{"protocolVersion":"2025-06-18"}})).await.unwrap()["error"]["code"], -32600);
        assert_eq!(session.phase, Phase::Ready);
    }

    #[tokio::test]
    async fn malformed_initialization_does_not_advance_the_session() {
        let mut session = Session::default();
        assert_eq!(
            session
                .handle(json!({"jsonrpc":"2.0","id":1,"method":"initialize"}))
                .await
                .unwrap()["error"]["code"],
            -32602
        );
        assert_eq!(session.phase, Phase::AwaitingInitialize);
        assert!(
            session
                .handle(json!({"jsonrpc":"2.0","id":2,"method":"ping"}))
                .await
                .unwrap()
                .get("result")
                .is_some()
        );
        assert_eq!(session.phase, Phase::AwaitingInitialize);
    }

    #[tokio::test]
    async fn initialize_negotiates_a_known_version() {
        let reply = Session::default()
            .handle(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": { "protocolVersion": "2024-11-05" }
            }))
            .await
            .expect("requests are answered");
        assert_eq!(reply["result"]["protocolVersion"], "2024-11-05");
        assert_eq!(reply["result"]["serverInfo"]["name"], "localweb");
    }

    #[tokio::test]
    async fn initialize_falls_back_for_unknown_versions() {
        let reply = Session::default()
            .handle(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": { "protocolVersion": "1999-01-01" }
            }))
            .await
            .expect("requests are answered");
        assert_eq!(reply["result"]["protocolVersion"], PROTOCOL_FALLBACK);
    }

    #[tokio::test]
    async fn unknown_method_reports_method_not_found() {
        let reply = ready_session()
            .await
            .handle(json!({ "jsonrpc": "2.0", "id": 7, "method": "explode" }))
            .await
            .expect("requests are answered");
        assert_eq!(reply["error"]["code"], -32601);
    }

    #[tokio::test]
    async fn requests_without_a_method_are_rejected_instead_of_ignored() {
        let reply = Session::default()
            .handle(json!({ "jsonrpc": "2.0", "id": 9 }))
            .await
            .expect("malformed requests are answered");
        assert_eq!(reply["error"]["code"], -32600);
        assert_eq!(reply["id"], Value::Null);
        assert!(
            Session::default()
                .handle(json!({ "jsonrpc": "2.0", "id": 10, "method": 42 }))
                .await
                .is_some()
        );
        assert!(
            Session::default()
                .handle(json!({ "jsonrpc": "2.0", "id": 11, "result": {} }))
                .await
                .is_none()
        );
    }

    #[tokio::test]
    async fn notifications_and_stray_responses_are_ignored() {
        assert!(
            Session::default()
                .handle(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
                .await
                .is_none()
        );
        assert!(
            Session::default()
                .handle(json!({ "jsonrpc": "2.0", "id": 3, "result": {} }))
                .await
                .is_none()
        );
    }

    #[tokio::test]
    async fn tools_list_advertises_the_catalogue() {
        let reply = ready_session()
            .await
            .handle(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }))
            .await
            .expect("requests are answered");
        let tools = reply["result"]["tools"]
            .as_array()
            .expect("tools is an array");
        assert_eq!(tools.len(), 19);
        assert!(
            tools
                .iter()
                .all(|tool| tool["name"].is_string() && tool["inputSchema"].is_object())
        );
    }

    #[tokio::test]
    async fn tools_call_requires_a_name() {
        let reply = ready_session()
            .await
            .handle(json!({
                "jsonrpc": "2.0",
                "id": 4,
                "method": "tools/call",
                "params": {}
            }))
            .await
            .expect("requests are answered");
        assert_eq!(reply["error"]["code"], -32602);
    }

    #[tokio::test]
    async fn tools_call_reports_unknown_tools_as_tool_errors() {
        let reply = ready_session()
            .await
            .handle(json!({
                "jsonrpc": "2.0",
                "id": 5,
                "method": "tools/call",
                "params": { "name": "nope" }
            }))
            .await
            .expect("requests are answered");
        assert_eq!(reply["result"]["isError"], true);
    }
}
