use rmcp::{ErrorData, model::{CallToolResult, Content}};
use serde::Deserialize;
use rmcp::schemars::{self, JsonSchema};
use serde_json::json;

use super::AppState;
use crate::docsrs::{fetch_rustdoc_json, build_module_tree, ModuleNode, ItemSummary};
use crate::sparse_index::find_latest_stable;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CrateDocsGetParams {
    /// Crate name
    pub name: String,
    /// Version string. Defaults to latest stable.
    pub version: Option<String>,
    /// Include item-level summaries per module (default: false)
    pub include_items: Option<bool>,
}

pub async fn execute(state: &AppState, params: CrateDocsGetParams) -> Result<CallToolResult, ErrorData> {
    let name = &params.name;
    let version = state.resolve_version(name, params.version.as_deref()).await
        .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

    // Parallel: fetch docs.rs JSON + sparse index features
    let (docs_result, index_result) = tokio::join!(
        fetch_rustdoc_json(name, &version, &state.client, &state.cache),
        state.fetch_index(name)
    );

    let index_lines = index_result.unwrap_or_default();
    let latest = find_latest_stable(&index_lines);
    let features = latest.map(|l| l.all_features()).unwrap_or_default();

    let doc = match docs_result {
        Ok(d) => d,
        Err(crate::error::DocsError::DocsNotFound { .. }) => {
            // Fall back to README; features are still available from the sparse index.
            let client = crate::cratesio::CratesIoClient::new(&state.client, &state.cache);
            let readme = client.get_readme(name, &version).await
                .unwrap_or_else(|_| "No documentation available".to_string());
            let output = json!({
                "name": name,
                "version": version,
                "root_docs": readme,
                "note": "docs.rs build not available; showing README instead",
                "module_tree": [],
                "features": features,
            });
            let json = serde_json::to_string_pretty(&output)
                .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
            return Ok(CallToolResult::success(vec![Content::text(json)]));
        }
        Err(e) => return Err(ErrorData::internal_error(e.to_string(), None)),
    };

    // Get root docs
    let root_item = doc.index.get(&doc.root_id());
    let root_docs = root_item
        .and_then(|i| i.docs.as_deref())
        .unwrap_or("")
        .to_string();

    // Build module tree
    let module_tree = build_module_tree(&doc);
    let tree_json = serialize_module_nodes(&module_tree, params.include_items.unwrap_or(false));

    let output = json!({
        "name": name,
        "version": version,
        "format_version": doc.format_version,
        "root_docs": root_docs,
        "features": features,
        "module_tree": tree_json,
    });

    let json = serde_json::to_string_pretty(&output)
        .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

    Ok(CallToolResult::success(vec![Content::text(json)]))
}

fn serialize_item_summary(s: &ItemSummary) -> serde_json::Value {
    json!({
        "kind": s.kind,
        "name": s.name,
        "doc_summary": s.doc_summary,
    })
}

fn serialize_module_nodes(nodes: &[ModuleNode], include_items: bool) -> serde_json::Value {
    let arr: Vec<serde_json::Value> = nodes.iter().map(|n| {
        let mut obj = json!({
            "path": n.path,
            "doc_summary": n.doc_summary,
            "item_counts": n.item_counts,
        });
        if include_items && !n.items.is_empty() {
            obj["items"] = serde_json::Value::Array(
                n.items.iter().map(serialize_item_summary).collect()
            );
        }
        if !n.children.is_empty() {
            obj["children"] = serialize_module_nodes(&n.children, include_items);
        }
        obj
    }).collect();
    serde_json::Value::Array(arr)
}
