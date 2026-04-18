use std::path::Path;
use std::time::Instant;

use codemov_core::IndexStats;
use codemov_parser::{extract_imports, extract_symbols};
use codemov_storage::{Store, StoreError};
use thiserror::Error;

use crate::walker::walk_repo;

#[derive(Debug, Error)]
pub enum IndexError {
    #[error("storage: {0}")]
    Store(#[from] StoreError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

pub struct IndexOptions {
    /// Skip files whose stored hash matches current content.
    pub incremental: bool,
}

impl Default for IndexOptions {
    fn default() -> Self {
        Self { incremental: true }
    }
}

pub fn index(
    root: &Path,
    store: &mut Store,
    opts: &IndexOptions,
) -> Result<IndexStats, IndexError> {
    let started = Instant::now();
    let entries = walk_repo(root);

    let mut files_indexed = 0usize;
    let mut files_skipped = 0usize;
    let mut symbols_extracted = 0usize;
    let mut errors = 0usize;

    for entry in &entries {
        let source = match std::fs::read(&entry.path) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("warn: could not read {}: {e}", entry.path.display());
                errors += 1;
                continue;
            }
        };

        let hash = hex_hash(&source);

        if opts.incremental {
            match store.file_hash(&entry.path) {
                Ok(Some(stored)) if stored == hash => {
                    files_skipped += 1;
                    continue;
                }
                Err(e) => {
                    eprintln!(
                        "warn: store lookup failed for {}: {e}",
                        entry.path.display()
                    );
                }
                _ => {}
            }
        }

        let symbols = match extract_symbols(&source, entry.language) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("warn: parse error for {}: {e}", entry.path.display());
                errors += 1;
                vec![]
            }
        };

        let sym_count = symbols.len();

        let file_id = match store.upsert_file(
            &entry.path,
            entry.language,
            &hash,
            entry.byte_size,
            entry.last_modified,
        ) {
            Ok(id) => id,
            Err(e) => {
                eprintln!("warn: failed to store {}: {e}", entry.path.display());
                errors += 1;
                continue;
            }
        };

        if let Err(e) = store.replace_symbols(file_id, &symbols) {
            eprintln!(
                "warn: failed to store symbols for {}: {e}",
                entry.path.display()
            );
            errors += 1;
            continue;
        }

        let imports = match extract_imports(&source, entry.language) {
            Ok(mut edges) => {
                for e in &mut edges {
                    e.source_path = entry.path.clone();
                    e.resolved_path = resolve_import(&entry.path, &e.target_raw);
                }
                edges
            }
            Err(e) => {
                eprintln!(
                    "warn: import extraction failed for {}: {e}",
                    entry.path.display()
                );
                vec![]
            }
        };

        if let Err(e) = store.replace_import_edges(file_id, &imports) {
            eprintln!(
                "warn: failed to store imports for {}: {e}",
                entry.path.display()
            );
        }

        files_indexed += 1;
        symbols_extracted += sym_count;
    }

    Ok(IndexStats {
        files_indexed,
        files_skipped,
        symbols_extracted,
        errors,
        duration_ms: started.elapsed().as_millis() as u64,
    })
}

fn hex_hash(data: &[u8]) -> String {
    blake3::hash(data).to_hex().to_string()
}

/// Resolve a relative TS/JS import to an absolute file path.
/// Returns None for non-relative imports (packages, Rust use paths, etc.).
fn resolve_import(source_path: &Path, target_raw: &str) -> Option<std::path::PathBuf> {
    if !target_raw.starts_with("./") && !target_raw.starts_with("../") {
        return None;
    }
    let dir = source_path.parent()?;
    let base = dir.join(target_raw);

    // If the import already has a known extension, try it directly.
    if let Some(ext) = base.extension() {
        if matches!(ext.to_str(), Some("ts" | "tsx" | "js" | "jsx" | "mjs")) {
            if base.is_file() {
                return base.canonicalize().ok();
            }
        }
    }

    // Probe extensions in priority order.
    for ext in ["ts", "tsx", "js", "jsx", "mjs"] {
        let candidate = base.with_extension(ext);
        if candidate.is_file() {
            return candidate.canonicalize().ok();
        }
    }

    // Try index file inside a directory.
    for ext in ["ts", "tsx", "js"] {
        let candidate = base.join("index").with_extension(ext);
        if candidate.is_file() {
            return candidate.canonicalize().ok();
        }
    }

    None
}
