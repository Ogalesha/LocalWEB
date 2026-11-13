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
    // A detached reader does not hold up cancellation or runtime shutdown while
    // a client keeps stdin open. Bound both the queue and individual frames.
    let (sender, receiver) = tokio::sync::mpsc::channel(8);
    std::thread::spawn(move || {
        let mut reader = std::io::stdin().lock();
        loop {
            let mut line = Vec::new();
            let result = reader
                .by_ref()
                .take((localweb_core::MAX_MESSAGE_BYTES + 1) as u64)
                .read_until(b'\n', &mut line);
            match result {
                Ok(0) => break,
                Ok(_) => {
                    let oversized = line.len() > localweb_core::MAX_MESSAGE_BYTES;
                    if sender.blocking_send(Ok(line)).is_err() || oversized {
                        break;
                    }
                }
                Err(error) => {
                    let _ = sender.blocking_send(Err(error));
                    break;
                }
            }
        }
    });
    serve(receiver, &mut std::io::stdout()).await
}

async fn serve(
    mut input: tokio::sync::mpsc::Receiver<std::io::Result<Vec<u8>>>,
    stdout: &mut impl Write,
) -> Result<()> {
    let mut session = Session::default();
    let mut calls = tokio::task::JoinSet::new();
    let mut active_id = None;
    let result = loop {
        tokio::select! {
            frame = input.recv() => {
                let Some(frame) = frame else { break Ok(()); };
                let line = match frame { Ok(line) => line, Err(error) => break Err(error.into()) };
                if line.len() > localweb_core::MAX_MESSAGE_BYTES {
                    write_reply(stdout, &error_response(&Value::Null, -32600, "request too large"))?;
                    break Ok(());
                }
                if line.iter().all(u8::is_ascii_whitespace) { continue; }
                let message: Value = match serde_json::from_slice(&line) {
                    Ok(message) => message,
                    Err(error) => {
                        write_reply(stdout, &error_response(&Value::Null, -32700, &format!("parse error: {error}")))?;
                        continue;
                    }
                };
                if is_cancellation(&message, active_id.as_ref()) {
                    // No reply is sent for a cancelled MCP request. Aborting and
                    // joining releases its owned commands before accepting more.
                    calls.abort_all();
                    while calls.join_next().await.is_some() {}
                    active_id = None;
                    continue;
                }
                if session.phase == Phase::Ready && is_tool_request(&message) {
                    let id = message["id"].clone();
                    if active_id.is_some() {
                        write_reply(stdout, &error_response(&id, -32000, "Another tool is running. Wait for it or cancel it first."))?;
                        continue;
                    }
                    let params = message.get("params").cloned().unwrap_or(Value::Null);
                    active_id = Some(id.clone());
                    calls.spawn(async move {
                        match tools_call(&params).await {
                            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
                            Err((code, message)) => error_response(&id, code, &message),
                        }
                    });
                } else if let Some(reply) = session.handle(message).await {
                    write_reply(stdout, &reply)?;
                }
            }
            reply = calls.join_next(), if !calls.is_empty() => {
                if let Some(reply) = reply {
                    let id = active_id.take().unwrap_or(Value::Null);
                    let reply = reply.unwrap_or_else(|_| error_response(&id, -32603, "Tool execution stopped unexpectedly. Refresh before retrying."));
                    write_reply(stdout, &reply)?;
                }
            }
        }
    };
    calls.abort_all();
    while calls.join_next().await.is_some() {}
    result
}

fn write_reply(stdout: &mut impl Write, reply: &Value) -> std::io::Result<()> {
    writeln!(stdout, "{reply}")?;
    stdout.flush()
}

fn is_tool_request(message: &Value) -> bool {
    message["jsonrpc"] == "2.0"
        && message["method"] == "tools/call"
        && message
            .get("id")
            .is_some_and(|id| id.is_string() || id.as_i64().is_some() || id.as_u64().is_some())
}

fn is_cancellation(message: &Value, active_id: Option<&Value>) -> bool {
    message["jsonrpc"] == "2.0"
        && message["method"] == "notifications/cancelled"
        && message.get("id").is_none()
        && active_id.is_some_and(|id| message.pointer("/params/requestId") == Some(id))
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

    #[test]
    fn cancellation_only_matches_a_wellformed_inflight_notification() {
        let request = json!({"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": 9}});
        assert!(is_cancellation(&request, Some(&json!(9))));
        assert!(!is_cancellation(&request, Some(&json!("9"))));
        assert!(!is_cancellation(&request, None));
        assert!(!is_cancellation(
            &json!({"id": 4, "jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": 9}}),
            Some(&json!(9))
        ));
        assert!(!is_cancellation(
            &json!({"method": "notifications/cancelled", "params": {"requestId": 9}}),
            Some(&json!(9))
        ));
    }

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
