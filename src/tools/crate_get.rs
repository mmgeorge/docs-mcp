use rmcp::{ErrorData, model::{CallToolResult, Content}};
use serde::{Deserialize, Serialize};
use rmcp::schemars::{self, JsonSchema};

use super::AppState;

#[derive(Serialize)]
struct CrateGetOutput<'a> {
    name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    homepage: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    documentation: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    repository: Option<&'a str>,
    downloads: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    recent_downloads: Option<u64>,
    updated_at: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_stable_version: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_version: Option<&'a str>,
    features: std::collections::HashMap<String, Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    keywords: Option<Vec<&'a str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    categories: Option<Vec<&'a str>>,
}

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
    let output = CrateGetOutput {
        name: &krate.name,
        description: krate.description.as_deref(),
        homepage: krate.homepage.as_deref(),
        documentation: krate.documentation.as_deref(),
        repository: krate.repository.as_deref(),
        downloads: krate.downloads,
        recent_downloads: krate.recent_downloads,
        updated_at: &krate.updated_at,
        max_stable_version: krate.max_stable_version.as_deref(),
        max_version: krate.max_version.as_deref(),
        features,
        keywords: api.keywords.as_ref().map(|kws| kws.iter().map(|k| k.keyword.as_str()).collect()),
        categories: api.categories.as_ref().map(|cats| cats.iter().map(|c| c.category.as_str()).collect()),
    };

    let json = serde_json::to_string_pretty(&output)
        .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

    Ok(CallToolResult::success(vec![Content::text(json)]))
}
