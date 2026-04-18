use std::fs;
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
