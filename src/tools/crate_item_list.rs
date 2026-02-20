use std::collections::HashSet;

use rmcp::{ErrorData, model::{CallToolResult, Content}};
use serde::Deserialize;
use rmcp::schemars::{self, JsonSchema};
use serde_json::json;

use super::AppState;
use crate::docsrs::{fetch_rustdoc_json, search_items};
use crate::sparse_index::find_latest_stable;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CrateItemListParams {
    /// Crate name
    pub name: String,
    /// Version string. Defaults to latest stable.
    pub version: Option<String>,
    /// Search string — item name or concept (required)
    pub query: String,
    /// Filter by kind: "struct", "enum", "trait", "fn", "type", "const", "macro"
    pub kind: Option<String>,
    /// Restrict to items under this module path (e.g. "tokio::sync")
    pub module_prefix: Option<String>,
    /// Max results (default: 10, max: 50)
    pub limit: Option<usize>,
}

pub async fn execute(state: &AppState, params: CrateItemListParams) -> Result<CallToolResult, ErrorData> {
    let name = &params.name;
    let version = state.resolve_version(name, params.version.as_deref()).await
        .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
    let limit = params.limit.unwrap_or(10).min(50);

    let (docs_result, index_result) = tokio::join!(
        fetch_rustdoc_json(name, &version, &state.client, &state.cache),
        state.fetch_index(name)
    );

    let doc = match docs_result {
        Ok(d) => d,
        Err(crate::error::DocsError::DocsNotFound { .. }) => {
            // Suggest the user try an earlier version that may have a build.
            return Err(ErrorData::invalid_params(
                format!("No docs.rs build found for {name} {version}. \
                         The latest version may not have been built yet. \
                         Try specifying an older version with the 'version' parameter, \
                         or use crate_docs_get (which falls back to README)."),
                None,
            ));
        }
        Err(e) => return Err(ErrorData::internal_error(e.to_string(), None)),
    };
    let index_lines = index_result.unwrap_or_default();
    let latest = find_latest_stable(&index_lines);
    let features = latest.map(|l| l.all_features()).unwrap_or_default();
    let declared_features: HashSet<String> = features.keys().cloned().collect();

    let results = search_items(
        &doc,
        &params.query,
        params.kind.as_deref(),
        params.module_prefix.as_deref(),
        limit,
        &declared_features,
    );

    let items: Vec<serde_json::Value> = results.iter().map(|r| {
        json!({
            "path": r.path,
            "kind": r.kind,
            "signature": r.signature,
            "doc_summary": r.doc_summary,
            "feature_requirements": r.feature_requirements,
            "score": r.score,
        })
    }).collect();

    let output = json!({
        "name": name,
        "version": version,
        "query": params.query,
        "count": items.len(),
        "items": items,
    });

    let json = serde_json::to_string_pretty(&output)
        .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

    Ok(CallToolResult::success(vec![Content::text(json)]))
}
