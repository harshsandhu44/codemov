use std::fs;
use std::path::Path;
use tempfile::TempDir;

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
    assert!(names.contains(&"DEFAULT_TIMEOUT"), "missing const DEFAULT_TIMEOUT");
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
