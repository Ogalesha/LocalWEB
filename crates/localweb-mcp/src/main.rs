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
use std::io::{BufRead, Write};

/// Protocol revisions this server implements, newest first.
const PROTOCOL_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];
/// Revision used when the client asks for something unknown.
const PROTOCOL_FALLBACK: &str = "2025-06-18";

#[tokio::main]
async fn main() -> Result<()> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let mut reader = stdin.lock();
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(trimmed) {
            Ok(message) => handle(message).await,
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
async fn handle(message: Value) -> Option<Value> {
    let method = message.get("method").and_then(Value::as_str)?;
    let id = message.get("id")?.clone();
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    let reply = match method {
        "initialize" => Ok(initialize(&params)),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools::definitions() })),
        "tools/call" => tools_call(&params).await,
        other => Err((-32601, format!("method not found: {other}"))),
    };
    Some(match reply {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err((code, message)) => error_response(&id, code, &message),
    })
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
        "instructions": "LocalWeb manages local development projects (PHP, Node.js, nginx, Apache, MariaDB, MySQL, PostgreSQL, Redis, Mailpit). Every project is served directly over HTTP on a fixed loopback port and can also answer on its HTTPS hostname when port 443 is free. Typical flow: call localweb_status; if the daemon is down call localweb_daemon_start; register a folder with localweb_project_add; call localweb_project_start; run framework commands such as composer install or automated tests with localweb_project_exec.",
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

    #[tokio::test]
    async fn initialize_negotiates_a_known_version() {
        let reply = handle(json!({
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
        let reply = handle(json!({
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
        let reply = handle(json!({ "jsonrpc": "2.0", "id": 7, "method": "explode" }))
            .await
            .expect("requests are answered");
        assert_eq!(reply["error"]["code"], -32601);
    }

    #[tokio::test]
    async fn notifications_and_stray_responses_are_ignored() {
        assert!(
            handle(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
                .await
                .is_none()
        );
        assert!(
            handle(json!({ "jsonrpc": "2.0", "id": 3, "result": {} }))
                .await
                .is_none()
        );
    }

    #[tokio::test]
    async fn tools_list_advertises_the_catalogue() {
        let reply = handle(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }))
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
        let reply = handle(json!({
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
        let reply = handle(json!({
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
