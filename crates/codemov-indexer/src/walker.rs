use std::path::{Path, PathBuf};

use codemov_core::Language;
use ignore::WalkBuilder;

pub struct FileEntry {
    pub path: PathBuf,
    pub language: Language,
    pub byte_size: u64,
    pub last_modified: u64,
}

pub fn walk_repo(root: &Path) -> Vec<FileEntry> {
    let mut entries = Vec::new();

    for result in WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .build()
    {
        let entry = match result {
            Ok(e) => e,
            Err(_) => continue,
        };

        if entry.file_type().map(|t| !t.is_file()).unwrap_or(true) {
            continue;
        }

        let path = entry.path().to_path_buf();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let language = Language::from_extension(ext);

        if language == Language::Unknown {
            continue;
        }

        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };

        let last_modified = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);

        entries.push(FileEntry {
            path,
            language,
            byte_size: meta.len(),
            last_modified,
        });
    }

    entries.sort_by(|a, b| a.path.cmp(&b.path));
    entries
}
