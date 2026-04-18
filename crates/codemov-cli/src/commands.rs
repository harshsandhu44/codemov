use std::path::{Path, PathBuf};

use codemov_indexer::{index as run_index, IndexOptions};
use codemov_storage::Store;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CliError {
    #[error("index error: {0}")]
    Index(#[from] codemov_indexer::indexer::IndexError),
    #[error("storage error: {0}")]
    Store(#[from] codemov_storage::StoreError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Other(String),
}

fn data_dir(root: &Path) -> PathBuf {
    root.join(".codemov")
}

fn db_path(root: &Path) -> PathBuf {
    data_dir(root).join("index.db")
}

fn open_store(root: &Path) -> Result<Store, CliError> {
    let db = db_path(root);
    if !db.exists() {
        return Err(CliError::Other(format!(
            "no index found at {}. Run `codemov init` first.",
            db.display()
        )));
    }
    Ok(Store::open(&db)?)
}

pub fn init(root: &Path) -> Result<(), CliError> {
    let dir = data_dir(root);
    std::fs::create_dir_all(&dir)?;
    let db = db_path(root);
    Store::open(&db)?;
    println!("initialized: {}", db.display());
    Ok(())
}

pub fn index(root: &Path, full: bool, json: bool) -> Result<(), CliError> {
    let db = db_path(root);
    if !db.exists() {
        std::fs::create_dir_all(data_dir(root))?;
    }
    let mut store = Store::open(&db)?;
    let opts = IndexOptions { incremental: !full };
    let stats = run_index(root, &mut store, &opts)?;

    if json {
        println!("{}", serde_json::to_string(&stats)?);
    } else {
        println!(
            "indexed {} files ({} skipped, {} symbols, {} errors) in {}ms",
            stats.files_indexed,
            stats.files_skipped,
            stats.symbols_extracted,
            stats.errors,
            stats.duration_ms
        );
    }
    Ok(())
}

pub fn stats(root: &Path, json: bool) -> Result<(), CliError> {
    let store = open_store(root)?;
    let (files, symbols) = store.get_stats()?;
    let file_stats = store.get_file_stats()?;

    if json {
        let out = serde_json::json!({
            "total_files": files,
            "total_symbols": symbols,
            "files": file_stats.iter().map(|f| serde_json::json!({
                "path": f.path,
                "language": f.language,
                "byte_size": f.byte_size,
                "symbol_count": f.symbol_count,
            })).collect::<Vec<_>>()
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
    } else {
        println!("files:   {files}");
        println!("symbols: {symbols}");
        println!();
        for f in &file_stats {
            println!(
                "  {:6} sym  {:>8}B  [{:<12}]  {}",
                f.symbol_count,
                f.byte_size,
                f.language.as_str(),
                f.path.display()
            );
        }
    }
    Ok(())
}

pub fn find_symbol(root: &Path, query: &str, json: bool) -> Result<(), CliError> {
    let store = open_store(root)?;
    let matches = store.find_symbols(query)?;

    if json {
        let out: Vec<_> = matches
            .iter()
            .map(|m| {
                serde_json::json!({
                    "name": m.name,
                    "kind": m.kind.as_str(),
                    "language": m.language.as_str(),
                    "file": m.file_path,
                    "start_line": m.start_line,
                    "end_line": m.end_line,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&out)?);
    } else if matches.is_empty() {
        println!("no symbols found matching {:?}", query);
    } else {
        for m in &matches {
            println!(
                "{:<20} {:<12} {:<12} {}:{}–{}",
                m.name,
                m.kind.as_str(),
                m.language.as_str(),
                m.file_path.display(),
                m.start_line,
                m.end_line,
            );
        }
    }
    Ok(())
}

pub fn trace_impact(root: &Path, file: &Path, json: bool) -> Result<(), CliError> {
    let store = open_store(root)?;

    // Resolve file to absolute path.
    let abs_file = if file.is_absolute() {
        file.to_path_buf()
    } else {
        root.canonicalize()?.join(file)
    };
    let abs_file = abs_file
        .canonicalize()
        .map_err(|_| CliError::Other(format!("file not found: {}", abs_file.display())))?;

    let deps = store.get_dependencies(&abs_file)?;
    let dependents = store.get_dependents(&abs_file)?;

    if json {
        let out = serde_json::json!({
            "file": abs_file,
            "dependencies": deps,
            "dependents": dependents,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
    } else {
        println!("file: {}", abs_file.display());
        println!();
        if deps.is_empty() {
            println!("dependencies: (none resolved)");
        } else {
            println!("dependencies:");
            for p in &deps {
                println!("  {}", p.display());
            }
        }
        println!();
        if dependents.is_empty() {
            println!("dependents: (none)");
        } else {
            println!("dependents:");
            for p in &dependents {
                println!("  {}", p.display());
            }
        }
    }
    Ok(())
}

pub fn overview(root: &Path, json: bool) -> Result<(), CliError> {
    let store = open_store(root)?;
    let overview = store.get_overview()?;

    if json {
        println!("{}", serde_json::to_string_pretty(&overview)?);
    } else {
        println!("files:   {}", overview.total_files);
        println!("symbols: {}", overview.total_symbols);
        println!();
        println!("by language:");

        let mut langs: Vec<_> = overview.files_by_language.iter().collect();
        langs.sort_by(|(a, _), (b, _)| a.cmp(b));
        for (lang, count) in langs {
            let sym_count = overview.symbols_by_language.get(lang).copied().unwrap_or(0);
            println!("  {:<12}  {} files  {} symbols", lang, count, sym_count);
        }
    }
    Ok(())
}
