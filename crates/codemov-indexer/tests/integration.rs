use std::fs;
use std::path::Path;
use tempfile::TempDir;

use codemov_core::{ImportKind, Language, SymbolKind, TaskType};
use codemov_indexer::{index, IndexOptions};
use codemov_storage::{build_context_pack, ContextRequest, Store};

fn make_fixture() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("lib.rs"),
        "pub fn add(a: i32, b: i32) -> i32 { a + b }\npub struct Point { x: f64, y: f64 }\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("util.ts"),
        "export function greet(name: string) { return `Hello ${name}`; }\nexport interface Config { debug: boolean; }\n",
    )
    .unwrap();
    dir
}

fn fixture_path(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("fixtures")
        .join(name)
}

#[test]
fn index_fixture_counts() {
    let dir = make_fixture();
    let db_path = dir.path().join("index.db");
    let mut store = Store::open(&db_path).unwrap();
    let stats = index(dir.path(), &mut store, &IndexOptions::default()).unwrap();

    assert_eq!(stats.files_indexed, 2, "expected 2 files");
    assert!(stats.symbols_extracted >= 4, "expected at least 4 symbols");
    assert_eq!(stats.errors, 0);
}

#[test]
fn incremental_skip_on_second_run() {
    let dir = make_fixture();
    let db_path = dir.path().join("index.db");
    let mut store = Store::open(&db_path).unwrap();

    let first = index(dir.path(), &mut store, &IndexOptions::default()).unwrap();
    assert_eq!(first.files_indexed, 2);

    let second = index(dir.path(), &mut store, &IndexOptions::default()).unwrap();
    assert_eq!(second.files_indexed, 0, "should skip unchanged files");
    assert_eq!(second.files_skipped, 2);
}

#[test]
fn golden_rust_basic_symbols() {
    let fixture = fixture_path("rust-basic");
    let db = tempfile::NamedTempFile::new().unwrap();
    let mut store = Store::open(db.path()).unwrap();
    let stats = index(&fixture, &mut store, &IndexOptions::default()).unwrap();

    assert_eq!(stats.errors, 0, "no errors expected");
    assert_eq!(stats.files_indexed, 1, "one .rs file");

    let lib_path = fixture.join("src/lib.rs");
    let symbols = store.get_symbols_for_file(&lib_path).unwrap();

    let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"add"), "missing fn add");
    assert!(names.contains(&"subtract"), "missing fn subtract");
    assert!(names.contains(&"Point"), "missing struct Point");
    assert!(names.contains(&"Rectangle"), "missing struct Rectangle");
    assert!(names.contains(&"Direction"), "missing enum Direction");
    assert!(names.contains(&"Shape"), "missing trait Shape");

    // verify impl blocks are captured
    let impls: Vec<_> = symbols
        .iter()
        .filter(|s| s.kind == codemov_core::SymbolKind::Impl)
        .collect();
    assert!(impls.len() >= 2, "expected at least 2 impl blocks");

    // line numbers are deterministic
    let add = symbols.iter().find(|s| s.name == "add").unwrap();
    assert_eq!(add.start_line, 3, "fn add should start on line 3");
    assert_eq!(add.end_line, 5, "fn add should end on line 5");
}

#[test]
fn golden_ts_basic_symbols() {
    let fixture = fixture_path("ts-basic");
    let db = tempfile::NamedTempFile::new().unwrap();
    let mut store = Store::open(db.path()).unwrap();
    let stats = index(&fixture, &mut store, &IndexOptions::default()).unwrap();

    assert_eq!(stats.errors, 0, "no errors expected");
    assert_eq!(stats.files_indexed, 2, "two .ts files");

    let index_path = fixture.join("src/index.ts");
    let symbols = store.get_symbols_for_file(&index_path).unwrap();

    let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"Config"), "missing interface Config");
    assert!(names.contains(&"Handler"), "missing type Handler");
    assert!(names.contains(&"EventBus"), "missing class EventBus");
    assert!(names.contains(&"createConfig"), "missing fn createConfig");
    assert!(
        names.contains(&"DEFAULT_TIMEOUT"),
        "missing const DEFAULT_TIMEOUT"
    );
    assert!(names.contains(&"formatPath"), "missing arrow fn formatPath");

    let utils_path = fixture.join("src/utils.ts");
    let utils_syms = store.get_symbols_for_file(&utils_path).unwrap();
    let utils_names: Vec<&str> = utils_syms.iter().map(|s| s.name.as_str()).collect();
    assert!(utils_names.contains(&"validateConfig"));
    assert!(utils_names.contains(&"mergeConfigs"));
    assert!(utils_names.contains(&"ValidationResult"));
}

#[test]
fn golden_incremental_is_deterministic() {
    let fixture = fixture_path("rust-basic");
    let db = tempfile::NamedTempFile::new().unwrap();
    let mut store = Store::open(db.path()).unwrap();

    let first = index(&fixture, &mut store, &IndexOptions::default()).unwrap();
    let lib_path = fixture.join("src/lib.rs");
    let symbols_first = store.get_symbols_for_file(&lib_path).unwrap();

    let second = index(&fixture, &mut store, &IndexOptions::default()).unwrap();
    assert_eq!(second.files_indexed, 0, "incremental: no re-index");
    assert_eq!(second.files_skipped, first.files_indexed);

    let symbols_second = store.get_symbols_for_file(&lib_path).unwrap();
    assert_eq!(
        symbols_first.len(),
        symbols_second.len(),
        "symbol count must be stable"
    );
    for (a, b) in symbols_first.iter().zip(symbols_second.iter()) {
        assert_eq!(a.name, b.name);
        assert_eq!(a.start_line, b.start_line);
        assert_eq!(a.end_line, b.end_line);
    }
}

#[test]
fn find_symbol_exact_match_ranks_first() {
    let fixture = fixture_path("rust-basic");
    let db = tempfile::NamedTempFile::new().unwrap();
    let mut store = Store::open(db.path()).unwrap();
    index(&fixture, &mut store, &IndexOptions::default()).unwrap();

    let matches = store.find_symbols("add").unwrap();
    assert!(!matches.is_empty(), "should find 'add'");
    assert_eq!(matches[0].name, "add", "exact match must rank first");
    assert_eq!(matches[0].kind, SymbolKind::Function);
}

#[test]
fn find_symbol_prefix_match() {
    let fixture = fixture_path("rust-basic");
    let db = tempfile::NamedTempFile::new().unwrap();
    let mut store = Store::open(db.path()).unwrap();
    index(&fixture, &mut store, &IndexOptions::default()).unwrap();

    let matches = store.find_symbols("Rect").unwrap();
    assert!(!matches.is_empty(), "should prefix-match Rectangle");
    assert!(matches.iter().any(|m| m.name == "Rectangle"));
}

#[test]
fn find_symbol_no_match() {
    let fixture = fixture_path("rust-basic");
    let db = tempfile::NamedTempFile::new().unwrap();
    let mut store = Store::open(db.path()).unwrap();
    index(&fixture, &mut store, &IndexOptions::default()).unwrap();

    let matches = store.find_symbols("zzz_nonexistent").unwrap();
    assert!(matches.is_empty());
}

#[test]
fn find_symbol_result_is_stable() {
    let fixture = fixture_path("rust-basic");
    let db = tempfile::NamedTempFile::new().unwrap();
    let mut store = Store::open(db.path()).unwrap();
    index(&fixture, &mut store, &IndexOptions::default()).unwrap();

    let first = store.find_symbols("a").unwrap();
    let second = store.find_symbols("a").unwrap();
    let names_first: Vec<&str> = first.iter().map(|m| m.name.as_str()).collect();
    let names_second: Vec<&str> = second.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names_first, names_second, "results must be deterministic");
}

#[test]
fn golden_mixed_basic_symbols() {
    let fixture = fixture_path("mixed-basic");
    let db = tempfile::NamedTempFile::new().unwrap();
    let mut store = Store::open(db.path()).unwrap();
    let stats = index(&fixture, &mut store, &IndexOptions::default()).unwrap();

    assert_eq!(stats.errors, 0);
    assert_eq!(stats.files_indexed, 2, "one .rs + one .ts");

    let rs_path = fixture.join("src/main.rs");
    let rs_syms = store.get_symbols_for_file(&rs_path).unwrap();
    let rs_names: Vec<&str> = rs_syms.iter().map(|s| s.name.as_str()).collect();
    assert!(rs_names.contains(&"Greeter"), "missing struct Greeter");
    assert!(rs_names.contains(&"run"), "missing fn run");

    let ts_path = fixture.join("src/config.ts");
    let ts_syms = store.get_symbols_for_file(&ts_path).unwrap();
    let ts_names: Vec<&str> = ts_syms.iter().map(|s| s.name.as_str()).collect();
    assert!(
        ts_names.contains(&"AppConfig"),
        "missing interface AppConfig"
    );
    assert!(ts_names.contains(&"loadConfig"), "missing fn loadConfig");
    assert!(
        ts_names.contains(&"DEFAULT_PORT"),
        "missing const DEFAULT_PORT"
    );
}

#[test]
fn import_edges_rust_basic() {
    let fixture = fixture_path("rust-basic");
    let db = tempfile::NamedTempFile::new().unwrap();
    let mut store = Store::open(db.path()).unwrap();
    index(&fixture, &mut store, &IndexOptions::default()).unwrap();

    let lib_path = fixture.join("src/lib.rs");
    let edges = store.get_import_edges_for_file(&lib_path).unwrap();

    assert!(!edges.is_empty(), "expected at least one import edge");
    let targets: Vec<&str> = edges.iter().map(|e| e.target_raw.as_str()).collect();
    assert!(
        targets.iter().any(|t| t.contains("HashMap")),
        "expected use of HashMap; got: {:?}",
        targets
    );
    assert!(
        edges.iter().all(|e| e.kind == ImportKind::Use),
        "all Rust edges should be Use kind"
    );
    assert!(
        edges.iter().all(|e| e.line > 0),
        "all edges should have a valid line number"
    );
}

#[test]
fn import_edges_ts_basic() {
    let fixture = fixture_path("ts-basic");
    let db = tempfile::NamedTempFile::new().unwrap();
    let mut store = Store::open(db.path()).unwrap();
    index(&fixture, &mut store, &IndexOptions::default()).unwrap();

    let index_path = fixture.join("src/index.ts");
    let edges = store.get_import_edges_for_file(&index_path).unwrap();
    let targets: Vec<&str> = edges.iter().map(|e| e.target_raw.as_str()).collect();
    assert!(
        targets.contains(&"events"),
        "expected import of 'events'; got: {:?}",
        targets
    );
    assert!(
        targets.contains(&"path"),
        "expected import of 'path'; got: {:?}",
        targets
    );

    let utils_path = fixture.join("src/utils.ts");
    let utils_edges = store.get_import_edges_for_file(&utils_path).unwrap();
    let utils_targets: Vec<&str> = utils_edges.iter().map(|e| e.target_raw.as_str()).collect();
    assert!(
        utils_targets.contains(&"./index"),
        "expected import of './index'; got: {:?}",
        utils_targets
    );
}

#[test]
fn overview_json_shape() {
    let fixture = fixture_path("ts-basic");
    let db = tempfile::NamedTempFile::new().unwrap();
    let mut store = Store::open(db.path()).unwrap();
    index(&fixture, &mut store, &IndexOptions::default()).unwrap();

    let overview = store.get_overview().unwrap();
    assert_eq!(overview.total_files, 2);
    assert!(overview.total_symbols >= 6);
    assert!(overview.files_by_language.contains_key("typescript"));
    assert_eq!(overview.files_by_language["typescript"], 2);

    let json = serde_json::to_string(&overview).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(v.get("total_files").is_some());
    assert!(v.get("total_symbols").is_some());
    assert!(v.get("files_by_language").is_some());
    assert!(v.get("symbols_by_language").is_some());
}

#[test]
fn trace_impact_ts_basic() {
    let fixture = fixture_path("ts-basic");
    let db = tempfile::NamedTempFile::new().unwrap();
    let mut store = Store::open(db.path()).unwrap();
    index(&fixture, &mut store, &IndexOptions::default()).unwrap();

    let index_ts = fixture.join("src/index.ts").canonicalize().unwrap();
    let utils_ts = fixture.join("src/utils.ts").canonicalize().unwrap();

    // utils.ts imports ./index → index.ts should appear as its dependency
    let deps = store.get_dependencies(&utils_ts).unwrap();
    assert!(
        deps.contains(&index_ts),
        "utils.ts should depend on index.ts; got: {:?}",
        deps
    );

    // index.ts should appear as a dependent of nothing here (it imports only packages)
    let deps_of_index = store.get_dependencies(&index_ts).unwrap();
    assert!(
        deps_of_index.is_empty(),
        "index.ts has no relative imports; got: {:?}",
        deps_of_index
    );

    // index.ts should show utils.ts as a dependent
    let dependents = store.get_dependents(&index_ts).unwrap();
    assert!(
        dependents.contains(&utils_ts),
        "index.ts should be depended on by utils.ts; got: {:?}",
        dependents
    );
}

#[test]
fn stats_json_shape() {
    let fixture = fixture_path("rust-basic");
    let db = tempfile::NamedTempFile::new().unwrap();
    let mut store = Store::open(db.path()).unwrap();
    index(&fixture, &mut store, &IndexOptions::default()).unwrap();

    let file_stats = store.get_file_stats().unwrap();
    assert_eq!(file_stats.len(), 1);

    let f = &file_stats[0];
    assert_eq!(f.language, Language::Rust);
    assert!(f.symbol_count >= 6);
    assert!(f.byte_size > 0);

    let json = serde_json::to_string(&file_stats).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    let entry = &v[0];
    assert!(entry.get("path").is_some());
    assert!(entry.get("language").is_some());
    assert!(entry.get("byte_size").is_some());
    assert!(entry.get("symbol_count").is_some());
}

// ── context pack tests ────────────────────────────────────────────────────────

#[test]
fn context_symbol_target_finds_file_and_symbols() {
    let fixture = fixture_path("rust-basic");
    let db = tempfile::NamedTempFile::new().unwrap();
    let mut store = Store::open(db.path()).unwrap();
    index(&fixture, &mut store, &IndexOptions::default()).unwrap();

    let req = ContextRequest {
        task: TaskType::Explain,
        target: "Point",
        max_tokens: 4000,
        root: &fixture,
    };
    let pack = build_context_pack(&store, &req).unwrap();

    assert_eq!(pack.task, TaskType::Explain);
    assert_eq!(pack.target, "Point");
    assert!(
        !pack.selected_files.is_empty(),
        "should select at least one file"
    );
    assert!(!pack.selected_symbols.is_empty(), "should select symbols");

    // 'Point' symbol should be present
    assert!(
        pack.selected_symbols.iter().any(|s| s.name == "Point"),
        "expected Point in selected symbols"
    );

    // file containing Point should be selected
    let lib_path = fixture.join("src/lib.rs").canonicalize().unwrap();
    assert!(
        pack.selected_files.iter().any(|f| f.path == lib_path),
        "lib.rs should be selected"
    );
}

#[test]
fn context_file_target_selects_symbols_from_file() {
    let fixture = fixture_path("rust-basic");
    let db = tempfile::NamedTempFile::new().unwrap();
    let mut store = Store::open(db.path()).unwrap();
    index(&fixture, &mut store, &IndexOptions::default()).unwrap();

    let req = ContextRequest {
        task: TaskType::Bugfix,
        target: "src/lib.rs",
        max_tokens: 4000,
        root: &fixture,
    };
    let pack = build_context_pack(&store, &req).unwrap();

    assert!(
        !pack.selected_files.is_empty(),
        "target file should be selected"
    );

    // target file should rank 1.0
    assert_eq!(
        pack.selected_files[0].score, 1.0,
        "exact file match should score 1.0"
    );

    // symbols from the file should be included
    let sym_names: Vec<&str> = pack
        .selected_symbols
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    assert!(sym_names.contains(&"add"), "expected fn add");
    assert!(sym_names.contains(&"Point"), "expected struct Point");
}

#[test]
fn context_token_budget_excludes_when_tight() {
    let fixture = fixture_path("rust-basic");
    let db = tempfile::NamedTempFile::new().unwrap();
    let mut store = Store::open(db.path()).unwrap();
    index(&fixture, &mut store, &IndexOptions::default()).unwrap();

    let tight = ContextRequest {
        task: TaskType::Review,
        target: "src/lib.rs",
        max_tokens: 10, // very tight
        root: &fixture,
    };
    let tight_pack = build_context_pack(&store, &tight).unwrap();

    let full = ContextRequest {
        task: TaskType::Review,
        target: "src/lib.rs",
        max_tokens: 8000,
        root: &fixture,
    };
    let full_pack = build_context_pack(&store, &full).unwrap();

    assert!(
        tight_pack.selected_symbols.len() <= full_pack.selected_symbols.len(),
        "tight budget should select fewer or equal symbols"
    );
    assert!(
        tight_pack.estimated_total_tokens <= tight_pack.max_tokens,
        "estimated tokens must not exceed budget"
    );
    assert!(
        full_pack.excluded.len() <= tight_pack.excluded.len(),
        "full budget should have fewer or equal exclusions"
    );
}

#[test]
fn context_ordering_is_deterministic() {
    let fixture = fixture_path("rust-basic");
    let db = tempfile::NamedTempFile::new().unwrap();
    let mut store = Store::open(db.path()).unwrap();
    index(&fixture, &mut store, &IndexOptions::default()).unwrap();

    let req = ContextRequest {
        task: TaskType::Feature,
        target: "add",
        max_tokens: 4000,
        root: &fixture,
    };

    let pack_a = build_context_pack(&store, &req).unwrap();
    let pack_b = build_context_pack(&store, &req).unwrap();

    let files_a: Vec<_> = pack_a
        .selected_files
        .iter()
        .map(|f| f.path.clone())
        .collect();
    let files_b: Vec<_> = pack_b
        .selected_files
        .iter()
        .map(|f| f.path.clone())
        .collect();
    assert_eq!(files_a, files_b, "file order must be deterministic");

    let syms_a: Vec<_> = pack_a
        .selected_symbols
        .iter()
        .map(|s| s.name.clone())
        .collect();
    let syms_b: Vec<_> = pack_b
        .selected_symbols
        .iter()
        .map(|s| s.name.clone())
        .collect();
    assert_eq!(syms_a, syms_b, "symbol order must be deterministic");
}

#[test]
fn context_json_shape_is_stable() {
    let fixture = fixture_path("rust-basic");
    let db = tempfile::NamedTempFile::new().unwrap();
    let mut store = Store::open(db.path()).unwrap();
    index(&fixture, &mut store, &IndexOptions::default()).unwrap();

    let req = ContextRequest {
        task: TaskType::Explain,
        target: "Shape",
        max_tokens: 4000,
        root: &fixture,
    };
    let pack = build_context_pack(&store, &req).unwrap();
    let json = serde_json::to_string(&pack).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert!(v.get("task").is_some());
    assert!(v.get("target").is_some());
    assert!(v.get("max_tokens").is_some());
    assert!(v.get("estimated_total_tokens").is_some());
    assert!(v.get("selected_files").is_some());
    assert!(v.get("selected_symbols").is_some());
    assert!(v.get("snippets").is_some());
    assert!(v.get("excluded").is_some());
    assert_eq!(v["task"], "explain");
    assert_eq!(v["target"], "Shape");
}

#[test]
fn context_snippet_extraction_reads_code() {
    let fixture = fixture_path("rust-basic");
    let db = tempfile::NamedTempFile::new().unwrap();
    let mut store = Store::open(db.path()).unwrap();
    index(&fixture, &mut store, &IndexOptions::default()).unwrap();

    let req = ContextRequest {
        task: TaskType::Explain,
        target: "add",
        max_tokens: 8000,
        root: &fixture,
    };
    let pack = build_context_pack(&store, &req).unwrap();

    assert!(
        !pack.snippets.is_empty(),
        "should extract at least one snippet"
    );
    for sn in &pack.snippets {
        assert!(!sn.code.is_empty(), "snippet code should not be empty");
        assert!(sn.end_line >= sn.start_line, "end >= start");
    }
}

#[test]
fn context_feature_task_boosts_trait_score() {
    let fixture = fixture_path("rust-basic");
    let db = tempfile::NamedTempFile::new().unwrap();
    let mut store = Store::open(db.path()).unwrap();
    index(&fixture, &mut store, &IndexOptions::default()).unwrap();

    let feature_req = ContextRequest {
        task: TaskType::Feature,
        target: "src/lib.rs",
        max_tokens: 8000,
        root: &fixture,
    };
    let feature_pack = build_context_pack(&store, &feature_req).unwrap();

    let bugfix_req = ContextRequest {
        task: TaskType::Bugfix,
        target: "src/lib.rs",
        max_tokens: 8000,
        root: &fixture,
    };
    let bugfix_pack = build_context_pack(&store, &bugfix_req).unwrap();

    // 'Shape' is a trait; it should score higher in feature than in bugfix
    let shape_feature = feature_pack
        .selected_symbols
        .iter()
        .find(|s| s.name == "Shape");
    let shape_bugfix = bugfix_pack
        .selected_symbols
        .iter()
        .find(|s| s.name == "Shape");

    if let (Some(sf), Some(sb)) = (shape_feature, shape_bugfix) {
        // feature should rank Shape at least as high as bugfix
        let pos_feature = feature_pack
            .selected_symbols
            .iter()
            .position(|s| s.name == "Shape")
            .unwrap();
        let pos_bugfix = bugfix_pack
            .selected_symbols
            .iter()
            .position(|s| s.name == "Shape")
            .unwrap();
        let _ = (sf, sb); // used above
        assert!(
            pos_feature <= pos_bugfix,
            "Shape (trait) should rank at least as high in feature task (pos {pos_feature}) vs bugfix (pos {pos_bugfix})"
        );
    }
}

#[test]
fn context_empty_target_returns_empty_pack() {
    let fixture = fixture_path("rust-basic");
    let db = tempfile::NamedTempFile::new().unwrap();
    let mut store = Store::open(db.path()).unwrap();
    index(&fixture, &mut store, &IndexOptions::default()).unwrap();

    let req = ContextRequest {
        task: TaskType::Review,
        target: "zzz_nonexistent_symbol_xyz",
        max_tokens: 4000,
        root: &fixture,
    };
    let pack = build_context_pack(&store, &req).unwrap();

    assert!(
        pack.selected_files.is_empty(),
        "no files for unknown target"
    );
    assert!(
        pack.selected_symbols.is_empty(),
        "no symbols for unknown target"
    );
    assert_eq!(pack.estimated_total_tokens, 0);
}
