use rmcp::{ErrorData, model::{CallToolResult, Content}};
use serde::Deserialize;
use rmcp::schemars::{self, JsonSchema};
use serde_json::json;

use super::AppState;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CrateGetParams {
    /// Exact crate name (e.g. "serde")
    pub name: String,
}

pub async fn execute(state: &AppState, params: CrateGetParams) -> Result<CallToolResult, ErrorData> {
    let name = &params.name;
    let client = crate::cratesio::CratesIoClient::new(&state.client, &state.cache);

    // Parallel: crates.io API + sparse index
    let (api_result, index_result) = tokio::join!(
        client.get_crate(name),
        state.fetch_index(name)
    );

    let api = api_result.map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
    let index_lines = index_result.map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

    // Find latest stable from sparse index
    let latest_stable = crate::sparse_index::find_latest_stable(&index_lines);
    let features = latest_stable.map(|l| l.all_features()).unwrap_or_default();

    let krate = &api.krate;
    let output = json!({
        "name": krate.name,
        "description": krate.description,
        "homepage": krate.homepage,
        "documentation": krate.documentation,
        "repository": krate.repository,
        "downloads": krate.downloads,
        "recent_downloads": krate.recent_downloads,
        "created_at": krate.created_at,
        "updated_at": krate.updated_at,
        "max_stable_version": krate.max_stable_version,
        "latest_stable_from_index": latest_stable.map(|l| &l.vers),
        "features": features,
        "keywords": api.keywords.as_ref().map(|kws| kws.iter().map(|k| &k.keyword).collect::<Vec<_>>()),
        "categories": api.categories.as_ref().map(|cats| cats.iter().map(|c| &c.category).collect::<Vec<_>>()),
    });

    let json = serde_json::to_string_pretty(&output)
        .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

    Ok(CallToolResult::success(vec![Content::text(json)]))
}
