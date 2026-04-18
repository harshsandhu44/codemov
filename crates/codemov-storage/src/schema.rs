pub const CREATE_FILES: &str = "
CREATE TABLE IF NOT EXISTS files (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    path         TEXT    NOT NULL UNIQUE,
    language     TEXT    NOT NULL,
    content_hash TEXT    NOT NULL,
    byte_size    INTEGER NOT NULL,
    symbol_count INTEGER NOT NULL DEFAULT 0,
    last_modified INTEGER NOT NULL,
    indexed_at   INTEGER NOT NULL
)";

pub const CREATE_SYMBOLS: &str = "
CREATE TABLE IF NOT EXISTS symbols (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    file_id    INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    name       TEXT    NOT NULL,
    kind       TEXT    NOT NULL,
    start_line INTEGER NOT NULL,
    end_line   INTEGER NOT NULL
)";

pub const CREATE_IDX_SYMBOLS_FILE: &str =
    "CREATE INDEX IF NOT EXISTS idx_symbols_file ON symbols(file_id)";

pub const CREATE_IMPORT_EDGES: &str = "
CREATE TABLE IF NOT EXISTS import_edges (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    source_file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    target_raw     TEXT    NOT NULL,
    resolved_path  TEXT,
    kind           TEXT    NOT NULL,
    line           INTEGER NOT NULL
)";

pub const MIGRATE_IMPORT_EDGES_RESOLVED_PATH: &str =
    "ALTER TABLE import_edges ADD COLUMN resolved_path TEXT";

pub const CREATE_IDX_IMPORT_SOURCE: &str =
    "CREATE INDEX IF NOT EXISTS idx_import_source ON import_edges(source_file_id)";
