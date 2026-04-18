use std::fs;

use codemov_indexer::{index, IndexOptions};
use codemov_mcp::tools;
use codemov_storage::Store;
use serde_json::json;

fn setup_indexed_repo() -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    fs::write(
        root.join("lib.rs"),
        "pub fn add(a: i32, b: i32) -> i32 { a + b }\npub struct Point { x: f64, y: f64 }\n",
    )
    .unwrap();
    fs::write(
        root.join("util.ts"),
        "import { add } from './lib';\nexport function greet(name: string) { return add(1,2); }\n",
    )
    .unwrap();

    let codemov_dir = root.join(".codemov");
    fs::create_dir_all(&codemov_dir).unwrap();
    let db_path = codemov_dir.join("index.db");
    let mut store = Store::open(&db_path).unwrap();
    index(root, &mut store, &IndexOptions::default()).unwrap();

    let repo_path = root.to_str().unwrap().to_owned();
    (dir, repo_path)
}

#[test]
fn repo_overview_returns_language_breakdown() {
    let (_dir, repo_path) = setup_indexed_repo();
    let result = tools::call("repo_overview", &json!({ "repo_path": repo_path }));
    assert_eq!(result["isError"], false);
    let text = result["content"][0]["text"].as_str().unwrap();
    let v: serde_json::Value = serde_json::from_str(text).unwrap();
    assert!(v["total_files"].as_u64().unwrap() >= 2);
    assert!(v["total_symbols"].as_u64().unwrap() >= 3);
    assert!(v["files_by_language"].is_object());
}

#[test]
fn find_symbol_returns_matches() {
    let (_dir, repo_path) = setup_indexed_repo();
    let result = tools::call(
        "find_symbol",
        &json!({ "repo_path": repo_path, "query": "add" }),
    );
    assert_eq!(result["isError"], false);
    let text = result["content"][0]["text"].as_str().unwrap();
    let v: serde_json::Value = serde_json::from_str(text).unwrap();
    let matches = v["matches"].as_array().unwrap();
    assert!(!matches.is_empty());
    assert!(matches
        .iter()
        .any(|m| m["name"].as_str().unwrap().contains("add")));
}

#[test]
fn find_symbol_respects_limit() {
    let (_dir, repo_path) = setup_indexed_repo();
    let result = tools::call(
        "find_symbol",
        &json!({ "repo_path": repo_path, "query": "a", "limit": 1 }),
    );
    assert_eq!(result["isError"], false);
    let text = result["content"][0]["text"].as_str().unwrap();
    let v: serde_json::Value = serde_json::from_str(text).unwrap();
    assert!(v["matches"].as_array().unwrap().len() <= 1);
}

#[test]
fn trace_impact_returns_deps_and_dependents() {
    let (_dir, repo_path) = setup_indexed_repo();
    let result = tools::call(
        "trace_impact",
        &json!({ "repo_path": &repo_path, "file": "util.ts" }),
    );
    assert_eq!(result["isError"], false);
    let text = result["content"][0]["text"].as_str().unwrap();
    let v: serde_json::Value = serde_json::from_str(text).unwrap();
    assert!(v["dependencies"].is_array());
    assert!(v["dependents"].is_array());
}

#[test]
fn build_context_pack_returns_structured_pack() {
    let (_dir, repo_path) = setup_indexed_repo();
    let result = tools::call(
        "build_context_pack",
        &json!({
            "repo_path": repo_path,
            "task": "explain",
            "target": "add",
            "max_tokens": 2000
        }),
    );
    assert_eq!(result["isError"], false);
    let text = result["content"][0]["text"].as_str().unwrap();
    let v: serde_json::Value = serde_json::from_str(text).unwrap();
    assert_eq!(v["task"], "Explain");
    assert_eq!(v["target"], "add");
    assert!(v["selected_files"].is_array());
    assert!(v["selected_symbols"].is_array());
}

#[test]
fn output_is_deterministic() {
    let (_dir, repo_path) = setup_indexed_repo();
    let args = json!({ "repo_path": &repo_path, "query": "add" });
    let r1 = tools::call("find_symbol", &args);
    let r2 = tools::call("find_symbol", &args);
    assert_eq!(r1.to_string(), r2.to_string());
}
