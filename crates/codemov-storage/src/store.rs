use std::path::Path;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use codemov_core::{FileStats, Language, RepoOverview, Symbol, SymbolKind};
use rusqlite::{params, Connection};
use thiserror::Error;

use crate::schema;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("path is not valid utf-8")]
    InvalidPath,
}

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(db_path: &Path) -> Result<Self, StoreError> {
        let conn = Connection::open(db_path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        conn.execute_batch(schema::CREATE_FILES)?;
        conn.execute_batch(schema::CREATE_SYMBOLS)?;
        conn.execute_batch(schema::CREATE_IDX_SYMBOLS_FILE)?;
        Ok(Self { conn })
    }

    pub fn upsert_file(
        &mut self,
        path: &Path,
        language: Language,
        content_hash: &str,
        byte_size: u64,
        last_modified: u64,
    ) -> Result<i64, StoreError> {
        let path_str = path.to_str().ok_or(StoreError::InvalidPath)?;
        let now = now_secs();

        self.conn.execute(
            "INSERT INTO files (path, language, content_hash, byte_size, symbol_count, last_modified, indexed_at)
             VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6)
             ON CONFLICT(path) DO UPDATE SET
                language     = excluded.language,
                content_hash = excluded.content_hash,
                byte_size    = excluded.byte_size,
                last_modified= excluded.last_modified,
                indexed_at   = excluded.indexed_at",
            params![path_str, language.as_str(), content_hash, byte_size, last_modified, now],
        )?;

        let file_id: i64 = self.conn.query_row(
            "SELECT id FROM files WHERE path = ?1",
            params![path_str],
            |r| r.get(0),
        )?;
        Ok(file_id)
    }

    pub fn replace_symbols(&mut self, file_id: i64, symbols: &[Symbol]) -> Result<(), StoreError> {
        self.conn
            .execute("DELETE FROM symbols WHERE file_id = ?1", params![file_id])?;

        {
            let mut stmt = self.conn.prepare_cached(
                "INSERT INTO symbols (file_id, name, kind, start_line, end_line)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for sym in symbols {
                stmt.execute(params![
                    file_id,
                    sym.name,
                    sym.kind.as_str(),
                    sym.start_line,
                    sym.end_line
                ])?;
            }
        }

        self.conn.execute(
            "UPDATE files SET symbol_count = ?1 WHERE id = ?2",
            params![symbols.len(), file_id],
        )?;

        Ok(())
    }

    pub fn file_hash(&self, path: &Path) -> Result<Option<String>, StoreError> {
        let path_str = path.to_str().ok_or(StoreError::InvalidPath)?;
        let result = self.conn.query_row(
            "SELECT content_hash FROM files WHERE path = ?1",
            params![path_str],
            |r| r.get(0),
        );
        match result {
            Ok(h) => Ok(Some(h)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn get_stats(&self) -> Result<(usize, usize), StoreError> {
        let files: usize = self
            .conn
            .query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))?;
        let symbols: usize = self
            .conn
            .query_row("SELECT COUNT(*) FROM symbols", [], |r| r.get(0))?;
        Ok((files, symbols))
    }

    pub fn get_file_stats(&self) -> Result<Vec<FileStats>, StoreError> {
        let mut stmt = self
            .conn
            .prepare("SELECT path, language, byte_size, symbol_count FROM files ORDER BY path")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, u64>(2)?,
                r.get::<_, usize>(3)?,
            ))
        })?;

        let mut out = Vec::new();
        for row in rows {
            let (path, lang_str, byte_size, symbol_count) = row?;
            let language = match lang_str.as_str() {
                "rust" => Language::Rust,
                "typescript" => Language::TypeScript,
                "javascript" => Language::JavaScript,
                _ => Language::Unknown,
            };
            out.push(FileStats {
                path: path.into(),
                language,
                byte_size,
                symbol_count,
            });
        }
        Ok(out)
    }

    pub fn get_overview(&self) -> Result<RepoOverview, StoreError> {
        let mut files_by_language = std::collections::HashMap::new();
        let mut symbols_by_language = std::collections::HashMap::new();

        let mut stmt = self.conn.prepare(
            "SELECT f.language, COUNT(*) as fc, COALESCE(SUM(f.symbol_count), 0) as sc
             FROM files f
             GROUP BY f.language",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, usize>(1)?,
                r.get::<_, usize>(2)?,
            ))
        })?;

        let mut total_files = 0usize;
        let mut total_symbols = 0usize;
        for row in rows {
            let (lang, fc, sc) = row?;
            total_files += fc;
            total_symbols += sc;
            files_by_language.insert(lang.clone(), fc);
            symbols_by_language.insert(lang, sc);
        }

        Ok(RepoOverview {
            total_files,
            total_symbols,
            files_by_language,
            symbols_by_language,
        })
    }

    pub fn get_symbols_for_file(&self, path: &Path) -> Result<Vec<Symbol>, StoreError> {
        let path_str = path.to_str().ok_or(StoreError::InvalidPath)?;
        let file_id: i64 = self.conn.query_row(
            "SELECT id FROM files WHERE path = ?1",
            params![path_str],
            |r| r.get(0),
        )?;

        let mut stmt = self.conn.prepare(
            "SELECT name, kind, start_line, end_line FROM symbols WHERE file_id = ?1 ORDER BY start_line",
        )?;
        let rows = stmt.query_map(params![file_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, u32>(2)?,
                r.get::<_, u32>(3)?,
            ))
        })?;

        let mut out = Vec::new();
        for row in rows {
            let (name, kind_str, start_line, end_line) = row?;
            let kind = SymbolKind::from_str(&kind_str).unwrap_or(SymbolKind::Function);
            out.push(Symbol {
                name,
                kind,
                start_line,
                end_line,
            });
        }
        Ok(out)
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
