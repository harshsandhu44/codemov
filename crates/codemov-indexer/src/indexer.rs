use std::path::Path;
use std::time::Instant;

use codemov_core::IndexStats;
use codemov_parser::extract_symbols;
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
