# codemov

Local codebase indexing and context-compression engine.

Indexes source files, extracts symbols and import relationships, and stores results in a local SQLite database. Built as a CLI-first Rust tool — MCP integration planned for a later phase.

## Install

```sh
cargo install --path crates/codemov-cli
```

Or build locally:

```sh
cargo build --release
# binary: ./target/release/codemov
```

## Commands

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

Walks all Rust and TypeScript/JavaScript files (respects `.gitignore`), extracts symbols and import edges, and stores results. Subsequent runs are incremental — unchanged files are skipped.

**Find symbol** — search symbols by name:

```sh
codemov find-symbol <query> [-p path] [--json]
```

Searches the index for symbols matching `query`. Exact matches rank first, then prefix matches, then substring matches. Output includes symbol kind, language, file path, and line range.

```
$ codemov find-symbol add
add                  function     rust         src/lib.rs:3–5
subtract             function     rust         src/lib.rs:7–9
```

```sh
codemov find-symbol Rect --json
```

```json
[
  {
    "name": "Rectangle",
    "kind": "struct",
    "language": "rust",
    "file": "src/lib.rs",
    "start_line": 16,
    "end_line": 20
  }
]
```

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

**Trace impact** — show direct import dependencies and dependents for a file:

```sh
codemov trace-impact <file> [-p path] [--json]
```

Reports which files the given file imports (dependencies) and which files import it (dependents), based on resolved relative import paths. Rust `use` paths and npm package imports are not resolved to files.

```
$ codemov trace-impact src/utils.ts
file: /repo/src/utils.ts

dependencies:
  /repo/src/index.ts

dependents: (none)
```

## Supported languages

| Language | Extensions | Extracted symbols | Extracted imports |
|----------|------------|-------------------|-------------------|
| Rust | `.rs` | `fn`, `struct`, `enum`, `trait`, `impl` | `use` declarations |
| TypeScript | `.ts`, `.tsx` | `function`, `class`, `interface`, `type`, exported arrow fns | `import`, re-export `from` |
| JavaScript | `.js`, `.jsx`, `.mjs`, `.cjs` | same as TypeScript | `import`, `require()`, re-export `from` |

Import edges are stored in the SQLite database. Relative TS/JS imports (`./`, `../`) are resolved to absolute file paths at index time, enabling the `trace-impact` command.

## Architecture

```
crates/
  codemov-core      shared domain types (Language, Symbol, ImportEdge, SymbolMatch, …)
  codemov-parser    tree-sitter adapters for Rust and TypeScript/JavaScript
  codemov-storage   SQLite persistence via rusqlite (files, symbols, import_edges)
  codemov-indexer   file walker, language detection, indexing pipeline
  codemov-cli       clap-based CLI binary

fixtures/
  rust-basic/       stable Rust fixture repo for deterministic tests
  ts-basic/         stable TypeScript fixture repo for deterministic tests
  mixed-basic/      mixed Rust + TypeScript fixture for multi-language tests
```

## Development

```sh
cargo build --workspace
cargo test --workspace
cargo fmt --all
```

Tests cover symbol extraction, golden line-number assertions against fixture repos, incremental indexing determinism, import edge extraction, `find-symbol` ranking, resolved path graph queries, and JSON output shapes.
