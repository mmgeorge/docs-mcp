# AGENTS.md — Developer and Agent Guide

This document describes the architecture, conventions, and patterns you need to know when working on docs-mcp.

## Project Overview

docs-mcp is an MCP server (stdio transport) that provides AI agents with structured access to the Rust crate ecosystem. It is built with:

- **rmcp 0.16** — MCP server framework with `#[tool]` / `#[tool_router]` / `#[tool_handler]` macros
- **tokio** — async runtime (multi-thread)
- **reqwest** + **reqwest-middleware** — HTTP with rate limiting
- **serde_json** — all rustdoc parsing works against `serde_json::Value` trees
- **zstd** — docs.rs serves rustdoc JSON as `.json.zst` compressed files

## Data Sources

1. **crates.io sparse index** (`sparse_index/`) — `https://index.crates.io/{prefix}/{name}` — one line of NDJSON per version. Used for feature flags, MSRV, dep counts. Fast and no rate limit.
2. **crates.io REST API** (`cratesio/`) — `https://crates.io/api/v1/crates/{name}` — richer metadata (download counts, full version history, dependency details). Rate-limited to 1 req/s.
3. **docs.rs rustdoc JSON** (`docsrs/`) — `https://docs.rs/crate/{name}/{version}/json` — full API structure. Served as `.json.zst`. This is the most data-intensive source and is aggressively cached.

## Key Types

### Rustdoc JSON (format version 57)

These field types differ from naive assumptions — see `src/docsrs/types.rs`:

| Field | Type | Notes |
|-------|------|-------|
| `RustdocJson.root` | `serde_json::Value` | Integer in JSON (e.g. `98`), not a string |
| `Item.id` | `serde_json::Value` | Integer |
| `Item.attrs` | `Vec<serde_json::Value>` | Each attr is `{"other": "#[cfg(...)]"}` — use `Item::attr_strings()` |
| `Item.links` | `HashMap<String, serde_json::Value>` | Values are integer item IDs |
| `PathEntry.kind` | `String` | `"enum"`, `"function"`, `"struct"`, etc. |

### AppState (`src/tools/mod.rs`)

Shared across all tool handlers. Contains:
- `client: reqwest_middleware::ClientWithMiddleware` — rate-limited HTTP client
- `cache: Arc<Cache>` — disk cache
- `resolve_version(name, version_hint)` — resolves `None` to latest stable via sparse index

## MCP Server Instructions

The `ServerHandler::get_info()` method in `src/server.rs` returns an `instructions` string that MCP clients surface to agents before tool selection. Keep it short — one paragraph plus the three workflow lines. It currently reads:

```
DISCOVERY WORKFLOW: crate_list → crate_get → crate_readme_get
UNDERSTANDING WORKFLOW: crate_docs_get → crate_item_list → crate_item_get → crate_impls_list
DUE DILIGENCE: crate_versions_list → crate_downloads_get → crate_dependents_list → crate_dependencies_list
```

If you add a new tool category, add a corresponding workflow line here. Individual tool descriptions live on each `#[tool(description = "...")]` attribute in `server.rs` and are the primary documentation surface for agents — keep them precise and action-oriented.

## Adding a New Tool

1. Create `src/tools/my_tool.rs` with `MyToolParams` (derive `Deserialize`, `JsonSchema`) and `pub async fn execute(state: &AppState, params: MyToolParams) -> Result<CallToolResult, ErrorData>`.
2. Add to `src/tools/mod.rs`: `pub mod my_tool;`
3. Register in `src/server.rs`:
   - Add `use crate::tools::my_tool::{self, MyToolParams};`
   - Add a `#[tool(description = "...")]` method to `DocsMcpServer` that calls `my_tool::execute(&self.state, params).await`
4. Write tests in `tests/` or inline.

## Parser Conventions (`src/docsrs/parser.rs`)

### `type_to_string(val: &Value) -> String`

Converts a rustdoc JSON type node to a human-readable string. Key cases:
- `"primitive"` → bare name
- `"resolved_path"` → `Name<args>` or `Name`
- `"borrowed_ref"` → `&'a T` or `&T`
- `"raw_pointer"` → `*const T` / `*mut T`
- `"slice"` → `[T]`
- `"array"` → `[T; N]`
- `"tuple"` → `(A, B, C)`
- `"impl_trait"` → `impl Trait + Trait2`
- `"dyn_trait"` → `dyn Trait + Trait2`
- `"qualified_path"` → `T::Name` (shorthand when no trait) or `<T as Trait>::Name`
- `"generic"` → bare generic name

### `function_signature(item: &Item) -> String`

Builds a complete function signature string including generics, params, return type, and where clauses. Handles:
- Async (`async fn`)
- `self` receiver normalization (always `&self` or `&mut self`, not raw pointer form)
- `impl Trait` in parameter position (synthesizes param names)
- Lifetime params (`'a`)
- Const generic params (`const N: usize`)

### `format_generics_for_item(item: &Item, kind: &str) -> String`

Builds `<T: Bound, 'a, const N: usize>` string for struct/enum/trait definitions. Looks up generics from `item.inner.{kind}.generics`. Returns empty string if no generics.

### `build_module_tree(doc: &RustdocJson) -> Vec<ModuleNode>`

Builds a recursive module tree. Each `ModuleNode` has:
- `path: String` — fully qualified module path
- `item_counts: HashMap<String, usize>` — kind → count (excludes `"use"` / `"import"` kinds)
- `items: Vec<ItemSummary>` — populated when `include_items` is requested
- `children: Vec<ModuleNode>`

### `search_items(doc, query, kind, module_prefix, limit, declared_features)`

Fuzzy search over item names. Scoring:
- Exact match: 100
- Starts with query: 80
- Contains query: 60
- Word boundary match: 70
- Substring in path segments: 30

Results include: `path`, `kind`, `signature`, `doc_summary` (first sentence of doc comment), `feature_requirements`, `score`.

## Error Handling

Use `DocsError` from `src/error.rs` internally. At tool boundaries, convert to `ErrorData`:
- `ErrorData::internal_error(msg, None)` — unexpected failures
- `ErrorData::invalid_params(msg, None)` — user error (bad crate name, no docs build found)

The `DocsNotFound` variant should produce `invalid_params` errors (not `internal_error`) since it's expected when docs.rs hasn't built the latest version yet.

## Testing

Tests live in `tests/`:
- `tests/integration.rs` — integration tests that make real network calls (ignored by default, run with `cargo test -- --ignored`)
- `tests/unit_docsrs_parser.rs` — unit tests for parser functions using JSON fixtures
- `tests/lib_tests.rs` — unit tests for tool-level logic

Fixtures:
- `tests/fixtures/clap_4.5.59.json` — 23 items (modules and use-reexports only). Use for `build_module_tree` and root doc tests.
- `tests/fixtures/rmcp_0.16.0.json` — 13771 items including 3003 functions. Use for `function_signature`, `type_to_string`, `format_generics`, and kind-specific tests.

**Convention**: Add a test for every bug you fix. Name tests descriptively after what they assert (not the bug number).

## Caching

`src/cache.rs` — SHA-256 keyed, 1-day TTL, stored in platform cache dir. Cache entries are raw bytes. The cache is checked before every HTTP request.

To bust the cache during development, delete the cache directory or change the URL format slightly.

## Known Limitations

- **Re-exported items**: When a crate re-exports an item from another crate, the item ID appears in `paths` but not `index`. `crate_item_get` returns a helpful error in this case directing users to look up the defining crate.
- **docs.rs build lag**: The latest version of a crate may not have a docs.rs build yet. `crate_docs_get` falls back to README in this case; `crate_item_list` and `crate_item_get` return an `invalid_params` error with guidance.
- **Synthetic trait impls**: `Send`, `Sync`, `Unpin`, etc. are auto-traits that rustdoc marks `is_synthetic: true`. These are filtered from `crate_item_get` trait impl lists and `crate_impls_list` type-path queries to reduce noise.
- **Rustdoc JSON format version**: This server targets format version 57. Future rustdoc releases may change field shapes; the `RustdocJson.format_version` field can be used for version checks.

## Rate Limiting

The crates.io REST API client is rate-limited to 1 request/second using the `governor` crate. The sparse index and docs.rs have no artificial rate limit in this server (but be a good citizen).

## Building and Running

```bash
# Debug build
cargo build

# Release build (required when running as MCP server, since the binary is locked while running)
cargo build --release

# Run tests
cargo test

# Run with logging
RUST_LOG=debug cargo run
```

**Important**: If you're running the MCP server and want to rebuild, you must stop the server first — the running process holds the binary file locked on Windows.
