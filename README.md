# codemov

Local codebase indexing and context-compression engine.

Indexes source files, extracts symbols, and stores results in a local SQLite database. Built as a CLI-first Rust tool — MCP integration planned for a later phase.

## Phase 1 — CLI Usage

### Install

```sh
cargo install --path crates/codemov-cli
```

Or build locally:

```sh
cargo build --release
# binary: ./target/release/codemov
```

### Commands

**Initialize** the data directory for a repo:

```sh
codemov init [path]
```

Creates `.codemov/index.db` in the target directory.

**Index** a repository:

```sh
codemov index [path]
codemov index [path] --full   # force full re-index
codemov index [path] --json   # JSON output
```

Walks all Rust and TypeScript/JavaScript files (respects `.gitignore`), extracts symbols, and stores results. Subsequent runs are incremental — unchanged files are skipped.

**Stats** — per-file index details:

```sh
codemov stats [path]
codemov stats [path] --json
```

**Overview** — language-level summary:

```sh
codemov overview [path]
codemov overview [path] --json
```

### Supported languages

| Language | Extensions | Extracted symbols |
|----------|------------|-------------------|
| Rust | `.rs` | `fn`, `struct`, `enum`, `trait`, `impl` |
| TypeScript | `.ts`, `.tsx` | `function`, `class`, `interface`, `type`, exported arrow functions |
| JavaScript | `.js`, `.jsx`, `.mjs`, `.cjs` | same as TypeScript |

### Example output

```
$ codemov index .
indexed 13 files (0 skipped, 54 symbols, 0 errors) in 47ms

$ codemov overview
files:   13
symbols: 54

by language:
  rust          13 files  54 symbols
```

## Architecture

```
crates/
  codemov-core      shared domain types (Language, Symbol, RepoFile, …)
  codemov-parser    tree-sitter adapters for Rust and TypeScript
  codemov-storage   SQLite persistence via rusqlite
  codemov-indexer   file walker, language detection, indexing pipeline
  codemov-cli       clap-based CLI binary
```

## Development

```sh
cargo build --workspace
cargo test --workspace
cargo fmt --all
```
