use std::path::{Path, PathBuf};

use codemov_core::TaskType;
use codemov_storage::{build_context_pack, ContextRequest, Store};
use serde_json::{json, Map, Value};

pub fn list() -> Value {
    json!({
        "tools": [
            {
                "name": "repo_overview",
                "description": "High-level summary of an indexed repository: file counts, symbol counts, language breakdown.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "repo_path": { "type": "string", "description": "Absolute path to the repository root" }
                    },
                    "required": ["repo_path"]
                }
            },
            {
                "name": "find_symbol",
                "description": "Search for symbols (functions, structs, classes, etc.) by name across the indexed repository.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "repo_path": { "type": "string", "description": "Absolute path to the repository root" },
                        "query":     { "type": "string",  "description": "Symbol name or partial name to search" },
                        "limit":     { "type": "integer", "description": "Max results (default 20)" }
                    },
                    "required": ["repo_path", "query"]
                }
            },
            {
                "name": "trace_impact",
                "description": "Return direct dependencies and dependents of a file in the repository.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "repo_path": { "type": "string", "description": "Absolute path to the repository root" },
                        "file":      { "type": "string", "description": "File path (relative to repo_path or absolute)" }
                    },
                    "required": ["repo_path", "file"]
                }
            },
            {
                "name": "build_context_pack",
                "description": "Build a ranked context pack (files + symbols) for a coding task within a token budget.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "repo_path":   { "type": "string",  "description": "Absolute path to the repository root" },
                        "task":        { "type": "string",  "description": "Task type: explain | bugfix | feature | review" },
                        "target":      { "type": "string",  "description": "File path or symbol name the task centers on" },
                        "max_tokens":  { "type": "integer", "description": "Token budget (default 4000)" }
                    },
                    "required": ["repo_path", "task", "target"]
                }
            }
        ]
    })
}

pub fn call(name: &str, args: &Value) -> Value {
    match name {
        "repo_overview" => repo_overview(args),
        "find_symbol" => find_symbol(args),
        "trace_impact" => trace_impact(args),
        "build_context_pack" => build_context_pack_tool(args),
        _ => tool_error(format!("unknown tool: {name}")),
    }
}

// ── helpers ──────────────────────────────────────────────────────────────────

fn tool_ok(value: Value) -> Value {
    json!({ "content": [{ "type": "text", "text": value.to_string() }], "isError": false })
}

fn tool_error(msg: impl Into<String>) -> Value {
    json!({ "content": [{ "type": "text", "text": msg.into() }], "isError": true })
}

fn get_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("missing required argument: {key}"))
}

fn db_path(repo_path: &str) -> PathBuf {
    PathBuf::from(repo_path).join(".codemov").join("index.db")
}

fn open_store(repo_path: &str) -> Result<Store, String> {
    let db = db_path(repo_path);
    if !db.exists() {
        return Err(format!(
            "no index at {}. Run `codemov init && codemov index` first.",
            db.display()
        ));
    }
    Store::open(&db).map_err(|e| e.to_string())
}

// ── tool implementations ──────────────────────────────────────────────────────

fn repo_overview(args: &Value) -> Value {
    let repo_path = match get_str(args, "repo_path") {
        Ok(p) => p,
        Err(e) => return tool_error(e),
    };
    let store = match open_store(repo_path) {
        Ok(s) => s,
        Err(e) => return tool_error(e),
    };
    match store.get_overview() {
        Ok(ov) => {
            let files_by_language: Map<String, Value> = {
                let mut pairs: Vec<_> = ov.files_by_language.into_iter().collect();
                pairs.sort_by(|a, b| a.0.cmp(&b.0));
                pairs.into_iter().map(|(k, v)| (k, json!(v))).collect()
            };
            let symbols_by_language: Map<String, Value> = {
                let mut pairs: Vec<_> = ov.symbols_by_language.into_iter().collect();
                pairs.sort_by(|a, b| a.0.cmp(&b.0));
                pairs.into_iter().map(|(k, v)| (k, json!(v))).collect()
            };
            tool_ok(json!({
                "total_files": ov.total_files,
                "total_symbols": ov.total_symbols,
                "files_by_language": files_by_language,
                "symbols_by_language": symbols_by_language,
            }))
        }
        Err(e) => tool_error(e.to_string()),
    }
}

fn find_symbol(args: &Value) -> Value {
    let repo_path = match get_str(args, "repo_path") {
        Ok(p) => p,
        Err(e) => return tool_error(e),
    };
    let query = match get_str(args, "query") {
        Ok(q) => q,
        Err(e) => return tool_error(e),
    };
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize;

    let store = match open_store(repo_path) {
        Ok(s) => s,
        Err(e) => return tool_error(e),
    };
    match store.find_symbols(query) {
        Ok(matches) => {
            let total = matches.len().min(limit);
            let results: Vec<Value> = matches
                .into_iter()
                .take(limit)
                .map(|m| {
                    json!({
                        "name": m.name,
                        "kind": format!("{:?}", m.kind),
                        "language": format!("{:?}", m.language),
                        "file_path": m.file_path.display().to_string(),
                        "start_line": m.start_line,
                        "end_line": m.end_line,
                    })
                })
                .collect();
            tool_ok(json!({ "matches": results, "total": total }))
        }
        Err(e) => tool_error(e.to_string()),
    }
}

fn trace_impact(args: &Value) -> Value {
    let repo_path = match get_str(args, "repo_path") {
        Ok(p) => p,
        Err(e) => return tool_error(e),
    };
    let file_str = match get_str(args, "file") {
        Ok(f) => f,
        Err(e) => return tool_error(e),
    };

    let store = match open_store(repo_path) {
        Ok(s) => s,
        Err(e) => return tool_error(e),
    };

    let file_path = if Path::new(file_str).is_absolute() {
        PathBuf::from(file_str)
    } else {
        PathBuf::from(repo_path).join(file_str)
    };

    let mut deps: Vec<String> = store
        .get_dependencies(&file_path)
        .unwrap_or_default()
        .into_iter()
        .map(|p| p.display().to_string())
        .collect();
    deps.sort();

    let mut dependents: Vec<String> = store
        .get_dependents(&file_path)
        .unwrap_or_default()
        .into_iter()
        .map(|p| p.display().to_string())
        .collect();
    dependents.sort();

    tool_ok(json!({
        "file": file_path.display().to_string(),
        "dependencies": deps,
        "dependents": dependents,
    }))
}

fn build_context_pack_tool(args: &Value) -> Value {
    let repo_path = match get_str(args, "repo_path") {
        Ok(p) => p,
        Err(e) => return tool_error(e),
    };
    let task_str = match get_str(args, "task") {
        Ok(t) => t,
        Err(e) => return tool_error(e),
    };
    let target = match get_str(args, "target") {
        Ok(t) => t,
        Err(e) => return tool_error(e),
    };
    let max_tokens = args
        .get("max_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(4000) as usize;

    let task = match task_str {
        "explain" => TaskType::Explain,
        "bugfix" => TaskType::Bugfix,
        "feature" => TaskType::Feature,
        "review" => TaskType::Review,
        _ => {
            return tool_error(format!(
                "invalid task '{}'. Use: explain | bugfix | feature | review",
                task_str
            ))
        }
    };

    let store = match open_store(repo_path) {
        Ok(s) => s,
        Err(e) => return tool_error(e),
    };

    let req = ContextRequest {
        task,
        target,
        max_tokens,
        root: Path::new(repo_path),
    };

    match build_context_pack(&store, &req) {
        Ok(pack) => {
            let mut selected_files: Vec<Value> = pack
                .selected_files
                .iter()
                .map(|f| {
                    json!({
                        "path": f.path.display().to_string(),
                        "score": f.score,
                        "why": f.why,
                        "estimated_tokens": f.estimated_tokens,
                    })
                })
                .collect();
            selected_files.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));

            let mut selected_symbols: Vec<Value> = pack
                .selected_symbols
                .iter()
                .map(|s| {
                    json!({
                        "name": s.name,
                        "kind": format!("{:?}", s.kind),
                        "file": s.file.display().to_string(),
                        "start_line": s.start_line,
                        "end_line": s.end_line,
                        "why": s.why,
                    })
                })
                .collect();
            selected_symbols.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));

            tool_ok(json!({
                "task": format!("{:?}", pack.task),
                "target": pack.target,
                "max_tokens": pack.max_tokens,
                "estimated_total_tokens": pack.estimated_total_tokens,
                "selected_files": selected_files,
                "selected_symbols": selected_symbols,
            }))
        }
        Err(e) => tool_error(e.to_string()),
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn list_has_four_tools_with_schemas() {
        let tools = list();
        let arr = tools["tools"].as_array().unwrap();
        assert_eq!(arr.len(), 4);
        for tool in arr {
            assert!(tool.get("name").is_some());
            assert!(tool.get("description").is_some());
            let schema = &tool["inputSchema"];
            assert_eq!(schema["type"], "object");
            assert!(schema.get("properties").is_some());
            assert!(schema.get("required").is_some());
        }
    }

    #[test]
    fn unknown_tool_returns_error() {
        let result = call("no_such_tool", &json!({}));
        assert_eq!(result["isError"], true);
    }

    #[test]
    fn missing_repo_path_returns_error() {
        for tool in [
            "repo_overview",
            "find_symbol",
            "trace_impact",
            "build_context_pack",
        ] {
            let result = call(tool, &json!({}));
            assert_eq!(
                result["isError"], true,
                "tool {tool} should error on missing repo_path"
            );
        }
    }

    #[test]
    fn invalid_task_type_returns_error() {
        let result = call(
            "build_context_pack",
            &json!({ "repo_path": "/tmp/fake", "task": "dance", "target": "foo" }),
        );
        assert_eq!(result["isError"], true);
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("invalid task"));
    }

    #[test]
    fn missing_query_returns_error() {
        let result = call("find_symbol", &json!({ "repo_path": "/tmp/fake" }));
        assert_eq!(result["isError"], true);
    }

    #[test]
    fn missing_file_returns_error() {
        let result = call("trace_impact", &json!({ "repo_path": "/tmp/fake" }));
        assert_eq!(result["isError"], true);
    }

    #[test]
    fn unindexed_repo_returns_error() {
        let result = call(
            "repo_overview",
            &json!({ "repo_path": "/tmp/nonexistent_repo_xyz" }),
        );
        assert_eq!(result["isError"], true);
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("no index"));
    }
}
