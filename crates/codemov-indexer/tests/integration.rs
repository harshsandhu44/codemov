use std::fs;
use std::path::Path;
use tempfile::TempDir;

use codemov_core::{ImportKind, Language, SymbolKind};
use codemov_indexer::{index, IndexOptions};
use codemov_storage::Store;

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
    assert!(ts_names.contains(&"AppConfig"), "missing interface AppConfig");
    assert!(ts_names.contains(&"loadConfig"), "missing fn loadConfig");
    assert!(ts_names.contains(&"DEFAULT_PORT"), "missing const DEFAULT_PORT");
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
