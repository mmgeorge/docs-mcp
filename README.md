# docs-mcp

A [Model Context Protocol](https://modelcontextprotocol.io/) server that gives AI agents accurate, up-to-date access to the Rust crate ecosystem. It combines three data sources — the crates.io sparse index, the crates.io REST API, and rustdoc JSON for docs.rs.

## Quick Start

```bash
# Build
cargo build --release

# Run (stdio transport — connect from your MCP client)
./target/release/docs-mcp
```

### Claude Desktop / Claude Code

Add to your MCP config (e.g. `.mcp.json` or `claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "docs-mcp": {
      "command": "/path/to/docs-mcp",
      "args": []
    }
  }
}
```

## Tools

### Discovery

Use these tools when you don't yet know which crate you need, or want a high-level picture of what a crate is before diving into its API.

| Tool | Description |
|------|-------------|
| `crate_list` | Search crates.io by keyword, category, or free-text. Entry point for crate discovery. |
| `crate_get` | Comprehensive metadata for a single crate: description, downloads, latest version, feature flags, MSRV. |
| `crate_readme_get` | Fetch the crate's README as plain text. Contains the author's narrative: why the crate exists, how it compares to alternatives, and quick-start examples. |

**Workflow:** `crate_list` → `crate_get` → `crate_readme_get`

### Understanding

Use these tools once you know which crate you're working with and need to understand its API — finding the right type, reading a method signature, or knowing what traits to implement.

| Tool | Description |
|------|-------------|
| `crate_docs_get` | High-level documentation structure: crate-level `//!` docs, module tree, and per-module item summaries. Start here — it gives you the lay of the land before searching for specifics. |
| `crate_item_list` | Search for items (types, functions, traits, etc.) by name or concept. Returns ranked results with signatures and doc summaries. Use this to find the right item when you know roughly what you're looking for. |
| `crate_item_get` | Complete documentation for a specific item by fully-qualified path: full doc comment, exact type signature, generics, where clauses, all methods, and trait implementations. Use this once you know exactly what you want. |
| `crate_impls_list` | Find implementors of a trait (what types implement `Iterator`?) or all traits a type implements (what can I call on `HashMap`?). |

**Workflow:** `crate_docs_get` → `crate_item_list` → `crate_item_get` → `crate_impls_list`

### Due Diligence

Use these tools when evaluating whether to adopt a crate, auditing its dependency footprint, or checking version history and ecosystem adoption.

| Tool | Description |
|------|-------------|
| `crate_versions_list` | All published versions with feature lists, MSRV, dep counts, and yank status. Useful for understanding release cadence and finding when a feature was introduced. |
| `crate_version_get` | Rich per-version metadata from crates.io: Rust edition, library vs binary targets, line counts, license, and publisher. |
| `crate_dependencies_list` | Dependency list for a specific version with semver requirements, optional flags, enabled features, and target conditions. A large or unusual dep tree is a risk multiplier. |
| `crate_dependents_list` | Reverse dependencies — crates that depend on this one. A crate trusted by 5000 others has a different risk profile than one with 20. |
| `crate_downloads_get` | Per-day download counts broken out by version for the past 90 days. Reveals whether users have migrated to newer versions and whether recent adoption spikes are real. |

**Workflow:** `crate_versions_list` → `crate_downloads_get` → `crate_dependents_list` → `crate_dependencies_list`


