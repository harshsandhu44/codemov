# codemov

Local codebase indexing and context-compression engine.

Indexes source files, extracts symbols and import relationships, and stores results in a local SQLite database. Includes a local read-only MCP server for use with Claude Code and other MCP clients.

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

**Context** — build a task-aware context pack under a token budget:

```sh
codemov context --task <type> --target <query-or-file> [--max-tokens <n>] [path] [--json]
```

Returns the most relevant files, symbols, and snippets for a coding task, ranked and selected within the given token budget (default: 4000).

Task types:
- `explain` — prioritizes central files, matched symbols, direct dependencies, and overview snippets
- `bugfix` — prioritizes the target file, its dependents (callers), and near-matching symbols
- `feature` — boosts extension points (traits, interfaces), nearby modules, and import/export surfaces
- `review` — prioritizes the target file, exported API surface, and direct dependencies

The `--target` can be a file path (e.g. `src/parser/mod.rs`) or a symbol/text query (e.g. `Store`). File targets are resolved to the index; symbol queries use prefix/substring matching.

```
$ codemov context --task explain --target Store --max-tokens 3000
task:   explain
target: Store
tokens: 1820 / 3000 budget

files (2):
  [0.90] src/store.rs  (4 tokens)  — contains matched symbol
  ...

symbols (3):
  Store                struct       src/store.rs:19-21  — exact symbol name match
  ...

snippets (3):
  src/store.rs:19-21  — exact symbol name match
    pub struct Store {
        conn: Connection,
    }
```

```sh
codemov context --task bugfix --target src/parser/mod.rs --max-tokens 4000 --json
```

If the budget is too low to include all candidates, the highest-signal subset is returned and lower-priority items appear under `excluded`.

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

## MCP server

codemov exposes a local, read-only MCP server over stdio. MCP clients (Claude Code, Gemini CLI, Codex, etc.) can use it to query the index without running CLI commands.

### Install

```sh
cargo install --path crates/codemov-mcp
```

Or build locally:

```sh
cargo build --release -p codemov-mcp
# binary: ./target/release/codemov-mcp
```

### Run

The server reads from stdin and writes to stdout (JSON-RPC 2.0, newline-delimited):

```sh
codemov-mcp
```

The repo must be initialized and indexed before the server can respond to tool calls:

```sh
codemov init /path/to/repo
codemov index /path/to/repo
codemov-mcp
```

### Tools

| Tool | Purpose | Required inputs |
|------|---------|-----------------|
| `repo_overview` | Language-level summary: file/symbol counts | `repo_path` |
| `find_symbol` | Search symbols by name (ranked) | `repo_path`, `query` |
| `trace_impact` | Direct dependencies and dependents of a file | `repo_path`, `file` |
| `build_context_pack` | Ranked context pack within a token budget | `repo_path`, `task`, `target` |

`task` values for `build_context_pack`: `explain` | `bugfix` | `feature` | `review`

### Claude Code config

Add to `.claude/mcp.json` (or your global MCP config):

```json
{
  "mcpServers": {
    "codemov": {
      "command": "codemov-mcp",
      "args": []
    }
  }
}
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
  codemov-core      shared domain types (Language, Symbol, ImportEdge, SymbolMatch, ContextPack, …)
  codemov-parser    tree-sitter adapters for Rust and TypeScript/JavaScript
  codemov-storage   SQLite persistence via rusqlite (files, symbols, import_edges)
  codemov-indexer   file walker, language detection, indexing pipeline
  codemov-cli       clap-based CLI binary
  codemov-mcp       stdio MCP server (thin adapter over codemov-storage)

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

Tests cover symbol extraction, golden line-number assertions against fixture repos, incremental indexing determinism, import edge extraction, `find-symbol` ranking, resolved path graph queries, JSON output shapes, and context pack generation (symbol targets, file targets, token budgeting, deterministic ordering, snippet extraction, task-specific scoring).
