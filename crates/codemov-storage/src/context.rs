use std::collections::HashMap;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use codemov_core::{
    estimate_tokens, ContextPack, ExcludedCandidate, Language, SelectedFile, SelectedSymbol,
    Snippet, Symbol, SymbolKind, SymbolMatch, TaskType,
};

use crate::store::{Store, StoreError};

pub struct ContextRequest<'a> {
    pub task: TaskType,
    pub target: &'a str,
    pub max_tokens: usize,
    /// Repo root as supplied by the caller (may be relative or absolute).
    pub root: &'a Path,
}

pub fn build_context_pack(
    store: &Store,
    req: &ContextRequest<'_>,
) -> Result<ContextPack, StoreError> {
    let abs_root = req
        .root
        .canonicalize()
        .unwrap_or_else(|_| req.root.to_path_buf());

    // Map canonical-absolute-path → db-stored path
    let all_files = store.get_file_stats()?;
    let abs_to_db: HashMap<PathBuf, PathBuf> = all_files
        .iter()
        .filter_map(|f| {
            let abs = canon_db_path(&abs_root, &f.path)?;
            Some((abs, f.path.clone()))
        })
        .collect();

    let is_file = looks_like_file(req.target);
    let mut file_candidates: Vec<(PathBuf, f32, &'static str)> = Vec::new();
    let mut sym_candidates: Vec<(SymbolMatch, f32, String)> = Vec::new();
    let mut seen_abs: HashSet<PathBuf> = HashSet::new();

    if is_file {
        collect_file_candidates(
            store,
            req,
            &abs_root,
            &abs_to_db,
            &mut file_candidates,
            &mut sym_candidates,
            &mut seen_abs,
        )?;
    } else {
        collect_symbol_candidates(
            store,
            req,
            &abs_root,
            &abs_to_db,
            &mut file_candidates,
            &mut sym_candidates,
            &mut seen_abs,
        )?;
    }

    sort_file_candidates(&mut file_candidates);
    sort_sym_candidates(&mut sym_candidates);

    assemble_pack(req, file_candidates, sym_candidates, is_file)
}

fn collect_file_candidates(
    store: &Store,
    req: &ContextRequest<'_>,
    abs_root: &Path,
    abs_to_db: &HashMap<PathBuf, PathBuf>,
    files: &mut Vec<(PathBuf, f32, &'static str)>,
    syms: &mut Vec<(SymbolMatch, f32, String)>,
    seen: &mut HashSet<PathBuf>,
) -> Result<(), StoreError> {
    let abs = match resolve_abs(abs_root, req.target) {
        Some(p) => p,
        None => return Ok(()),
    };

    seen.insert(abs.clone());
    files.push((abs.clone(), 1.0, "exact file target match"));

    // symbols in target file (use db path for store lookups)
    if let Some(db_path) = abs_to_db.get(&abs) {
        if let Ok(raw_syms) = store.get_symbols_for_file(db_path) {
            for s in raw_syms {
                let score = sym_kind_score(s.kind, req.task);
                syms.push((symbol_to_match(s, abs.clone()), score, "symbol in target file".to_string()));
            }
        }
        add_edges(store, db_path, abs_root, abs_to_db, req.task, files, seen);
    }

    Ok(())
}

fn collect_symbol_candidates(
    store: &Store,
    req: &ContextRequest<'_>,
    abs_root: &Path,
    abs_to_db: &HashMap<PathBuf, PathBuf>,
    files: &mut Vec<(PathBuf, f32, &'static str)>,
    syms: &mut Vec<(SymbolMatch, f32, String)>,
    seen: &mut HashSet<PathBuf>,
) -> Result<(), StoreError> {
    let matches = store.find_symbols(req.target)?;

    for m in &matches {
        // find_symbols returns db-format paths; resolve to abs for internal use
        let abs = canon_db_path(abs_root, &m.file_path).unwrap_or_else(|| m.file_path.clone());
        let mut sm = m.clone();
        sm.file_path = abs.clone();

        let score = sym_query_score(req.target, &m.name, m.kind, req.task);
        let why = sym_why(req.target, &m.name);
        syms.push((sm, score, why));

        if seen.insert(abs.clone()) {
            let fscore = (score * 0.9_f32).min(1.0);
            files.push((abs, fscore, "contains matched symbol"));
        }
    }

    let primary: Vec<PathBuf> = seen.iter().cloned().collect();
    for abs in &primary {
        if let Some(db_path) = abs_to_db.get(abs) {
            add_edges(store, db_path, abs_root, abs_to_db, req.task, files, seen);
        }
    }
    Ok(())
}

fn add_edges(
    store: &Store,
    db_path: &Path,
    _abs_root: &Path,
    abs_to_db: &HashMap<PathBuf, PathBuf>,
    task: TaskType,
    files: &mut Vec<(PathBuf, f32, &'static str)>,
    seen: &mut HashSet<PathBuf>,
) {
    // get_dependencies returns resolved_path values (absolute from canonicalize)
    if let Ok(deps) = store.get_dependencies(db_path) {
        let score = dep_score(task);
        for dep_abs in deps {
            // dep_abs is already absolute (from resolved_path in import_edges)
            if abs_to_db.contains_key(&dep_abs) && seen.insert(dep_abs.clone()) {
                files.push((dep_abs, score, "direct dependency"));
            }
        }
    }
    if let Ok(dtds) = store.get_dependents(db_path) {
        let score = dependent_score(task);
        for dtd_abs in dtds {
            if abs_to_db.contains_key(&dtd_abs) && seen.insert(dtd_abs.clone()) {
                files.push((dtd_abs, score, "direct dependent"));
            }
        }
    }
}

fn assemble_pack(
    req: &ContextRequest<'_>,
    file_candidates: Vec<(PathBuf, f32, &'static str)>,
    sym_candidates: Vec<(SymbolMatch, f32, String)>,
    is_file: bool,
) -> Result<ContextPack, StoreError> {
    let mut budget = req.max_tokens;
    let mut total = 0usize;
    let mut selected_files: Vec<SelectedFile> = Vec::new();
    let mut selected_symbols: Vec<SelectedSymbol> = Vec::new();
    let mut snippets: Vec<Snippet> = Vec::new();
    let mut excluded: Vec<ExcludedCandidate> = Vec::new();

    for (path, score, why) in &file_candidates {
        let t = estimate_tokens(&path.to_string_lossy());
        if budget >= t {
            budget -= t;
            total += t;
            selected_files.push(SelectedFile {
                path: path.clone(),
                score: *score,
                why: why.to_string(),
                estimated_tokens: t,
            });
        } else {
            excluded.push(ExcludedCandidate {
                name: path.to_string_lossy().into_owned(),
                reason: "token budget exhausted".to_string(),
            });
        }
    }

    for (sm, _score, why) in &sym_candidates {
        let card = format!(
            "{} {} {}:{}-{}",
            sm.name,
            sm.kind.as_str(),
            sm.file_path.display(),
            sm.start_line,
            sm.end_line
        );
        let t = estimate_tokens(&card);
        if budget >= t {
            budget -= t;
            total += t;
            selected_symbols.push(SelectedSymbol {
                name: sm.name.clone(),
                kind: sm.kind,
                file: sm.file_path.clone(),
                start_line: sm.start_line,
                end_line: sm.end_line,
                why: why.clone(),
            });
        } else {
            excluded.push(ExcludedCandidate {
                name: sm.name.clone(),
                reason: "token budget exhausted".to_string(),
            });
        }
    }

    // Extract snippets for top symbols
    let target_abs = file_candidates.first().map(|(p, _, _)| p.clone());
    let snippet_srcs: Vec<_> = selected_symbols
        .iter()
        .filter(|s| {
            !is_file || target_abs.as_ref().map_or(false, |tf| *tf == s.file)
        })
        .take(8)
        .map(|s| (s.file.clone(), s.start_line, s.end_line, s.why.clone()))
        .collect();

    for (file, start, end, why) in snippet_srcs {
        match read_snippet(&file, start, end, 2) {
            Ok(code) => {
                let t = estimate_tokens(&code);
                if budget >= t {
                    budget -= t;
                    total += t;
                    snippets.push(Snippet { file, start_line: start, end_line: end, code, why });
                } else {
                    excluded.push(ExcludedCandidate {
                        name: format!("{}:{start}-{end}", file.display()),
                        reason: "token budget exhausted".to_string(),
                    });
                }
            }
            Err(_) => {
                excluded.push(ExcludedCandidate {
                    name: format!("{}:{start}-{end}", file.display()),
                    reason: "could not read file".to_string(),
                });
            }
        }
    }

    Ok(ContextPack {
        task: req.task,
        target: req.target.to_string(),
        max_tokens: req.max_tokens,
        estimated_total_tokens: total,
        selected_files,
        selected_symbols,
        snippets,
        excluded,
    })
}

// ── helpers ──────────────────────────────────────────────────────────────────

fn sort_file_candidates(v: &mut Vec<(PathBuf, f32, &'static str)>) {
    v.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.0.cmp(&b.0))
    });
}

fn sort_sym_candidates(v: &mut Vec<(SymbolMatch, f32, String)>) {
    v.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.0.name.cmp(&b.0.name))
            .then(a.0.file_path.cmp(&b.0.file_path))
            .then(a.0.start_line.cmp(&b.0.start_line))
    });
}

fn looks_like_file(target: &str) -> bool {
    target.contains('/')
        || target.contains('\\')
        || matches!(
            target.rsplit('.').next().unwrap_or(""),
            "rs" | "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs"
        )
}

/// Resolve a potentially-relative target path to a canonical absolute path.
fn resolve_abs(abs_root: &Path, target: &str) -> Option<PathBuf> {
    let p = if Path::new(target).is_absolute() {
        PathBuf::from(target)
    } else {
        abs_root.join(target)
    };
    p.canonicalize().ok()
}

/// Canonicalize a path that may be stored relative in the DB.
fn canon_db_path(abs_root: &Path, db_path: &Path) -> Option<PathBuf> {
    if db_path.is_absolute() {
        db_path.canonicalize().ok()
    } else {
        abs_root.join(db_path).canonicalize().ok()
    }
}

fn symbol_to_match(s: Symbol, file_path: PathBuf) -> SymbolMatch {
    SymbolMatch {
        name: s.name,
        kind: s.kind,
        language: Language::Unknown,
        file_path,
        start_line: s.start_line,
        end_line: s.end_line,
    }
}

fn dep_score(task: TaskType) -> f32 {
    match task {
        TaskType::Explain => 0.6,
        TaskType::Bugfix => 0.55,
        TaskType::Feature => 0.5,
        TaskType::Review => 0.5,
    }
}

fn dependent_score(task: TaskType) -> f32 {
    match task {
        TaskType::Explain => 0.4,
        TaskType::Bugfix => 0.6,
        TaskType::Feature => 0.5,
        TaskType::Review => 0.5,
    }
}

fn sym_kind_score(kind: SymbolKind, task: TaskType) -> f32 {
    let base: f32 = match kind {
        SymbolKind::Trait | SymbolKind::Interface => 0.85,
        SymbolKind::Struct | SymbolKind::Class | SymbolKind::Enum => 0.80,
        SymbolKind::Function => 0.75,
        SymbolKind::Export | SymbolKind::Constant => 0.70,
        _ => 0.65,
    };
    let boost: f32 = match task {
        TaskType::Feature => match kind {
            SymbolKind::Trait | SymbolKind::Interface => 0.1,
            _ => 0.0,
        },
        TaskType::Review => match kind {
            SymbolKind::Function | SymbolKind::Export => 0.05,
            _ => 0.0,
        },
        _ => 0.0,
    };
    (base + boost).min(1.0)
}

fn sym_query_score(query: &str, name: &str, kind: SymbolKind, task: TaskType) -> f32 {
    let base: f32 = if name == query {
        1.0
    } else if name.to_lowercase().starts_with(&query.to_lowercase()) {
        0.8
    } else {
        0.6
    };
    let boost: f32 = match task {
        TaskType::Feature => match kind {
            SymbolKind::Trait | SymbolKind::Interface => 0.1,
            _ => 0.0,
        },
        TaskType::Review => match kind {
            SymbolKind::Function | SymbolKind::Export => 0.05,
            _ => 0.0,
        },
        _ => 0.0,
    };
    (base + boost).min(1.0)
}

fn sym_why(query: &str, name: &str) -> String {
    if name == query {
        "exact symbol name match".to_string()
    } else if name.to_lowercase().starts_with(&query.to_lowercase()) {
        format!("symbol name prefix match for '{query}'")
    } else {
        format!("symbol name substring match for '{query}'")
    }
}

fn read_snippet(file: &Path, start_line: u32, end_line: u32, padding: u32) -> std::io::Result<String> {
    let content = std::fs::read_to_string(file)?;
    let lines: Vec<&str> = content.lines().collect();
    let total = lines.len() as u32;
    let from = start_line.saturating_sub(1 + padding) as usize;
    let to = ((end_line + padding).min(total)) as usize;
    Ok(lines[from..to].join("\n"))
}
