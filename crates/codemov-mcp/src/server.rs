use std::io::{self, BufRead, Write};

use serde_json::{json, Value};

use crate::tools;

pub fn run() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let request: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                let err = json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": { "code": -32700, "message": format!("parse error: {e}") }
                });
                writeln!(out, "{err}").ok();
                out.flush().ok();
                continue;
            }
        };

        if let Some(response) = handle_request(&request) {
            writeln!(out, "{response}").ok();
            out.flush().ok();
        }
    }
}

fn handle_request(req: &Value) -> Option<Value> {
    let id = req.get("id").cloned();
    let method = req.get("method")?.as_str()?;

    // Notifications have no id — no response expected
    if id.is_none() {
        return None;
    }

    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "codemov-mcp", "version": env!("CARGO_PKG_VERSION") }
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(tools::list()),
        "tools/call" => {
            let params = req.get("params").cloned().unwrap_or(json!({}));
            let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            Ok(tools::call(name, &args))
        }
        _ => Err(format!("method not found: {method}")),
    };

    Some(match result {
        Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
        Err(e) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32601, "message": e }
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(id: u64, method: &str, params: Value) -> Value {
        json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
    }

    #[test]
    fn initialize_returns_server_info() {
        let r = handle_request(&req(1, "initialize", json!({}))).unwrap();
        assert_eq!(r["result"]["serverInfo"]["name"], "codemov-mcp");
        assert_eq!(r["result"]["protocolVersion"], "2024-11-05");
    }

    #[test]
    fn notification_returns_none() {
        // No id → notification → no response
        let notif = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
        assert!(handle_request(&notif).is_none());
    }

    #[test]
    fn unknown_method_returns_error() {
        let r = handle_request(&req(2, "unknown/method", json!({}))).unwrap();
        assert!(r.get("error").is_some());
    }

    #[test]
    fn tools_list_returns_four_tools() {
        let r = handle_request(&req(3, "tools/list", json!({}))).unwrap();
        let tools = r["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 4);
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"repo_overview"));
        assert!(names.contains(&"find_symbol"));
        assert!(names.contains(&"trace_impact"));
        assert!(names.contains(&"build_context_pack"));
    }
}
