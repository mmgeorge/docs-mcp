# docs-mcp

An [MCP](https://modelcontextprotocol.io/) server for accessing documentation for Rust crates. Does *not* require nightly or downloading crates. Instead uses the rustdoc JSON published on docs.rs. Also expose some additional API calls from crates.io and the crates.io sparse index.

## Quick Start

```bash
cargo build --release
./target/release/docs-mcp
```

Add to your MCP config:

```json
{
  "mcpServers": {
    "docs-mcp": {
      "command": "/path/to/docs-mcp"
    }
  }
}
```

## Tools

| Tool | Description |
|------|-------------|
| `crate_list` | Search crates.io by keyword, category, or free-text |
| `crate_get` | Metadata for a crate: description, downloads, latest version, features, MSRV |
| `crate_readme_get` | Fetch a crate's README as plain text |
| `crate_docs_get` | Structured docs: crate-level `//!` docs, module tree, and item summaries |
| `crate_item_list` | Search for items by name or concept; returns signatures and doc summaries |
| `crate_item_get` | Full docs for a specific item by fully-qualified path |
| `crate_impls_list` | Find trait implementors or all traits a type implements |
| `crate_versions_list` | All published versions with features, MSRV, dep counts, and yank status |
| `crate_version_get` | Per-version metadata: edition, targets, line counts, license, publisher |
| `crate_dependencies_list` | Dependency list for a version with semver requirements and feature flags |
| `crate_dependents_list` | Reverse dependencies — crates that depend on this one |
| `crate_downloads_get` | Per-day download counts by version for the past 90 days |
