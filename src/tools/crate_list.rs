use rmcp::{ErrorData, model::CallToolResult};
use rmcp::model::Content;
use serde::Deserialize;
use rmcp::schemars::{self, JsonSchema};

use super::AppState;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CrateListParams {
    /// Free-text search query (e.g. "async http client")
    pub query: Option<String>,
    /// Filter by crates.io category slug (e.g. "web-programming")
    pub category: Option<String>,
    /// Filter by crates.io keyword tag
    pub keyword: Option<String>,
    /// Sort order: "relevance" (default), "downloads", "recent-downloads", "recent-updates", "alphabetical"
    pub sort: Option<String>,
    /// Page number (1-indexed, default: 1)
    pub page: Option<u32>,
    /// Results per page (max 100, default: 10)
    pub per_page: Option<u32>,
}

pub async fn execute(state: &AppState, params: CrateListParams) -> Result<CallToolResult, ErrorData> {
    let query = params.query.as_deref().unwrap_or("");
    let page = params.page.unwrap_or(1).max(1);
    let per_page = params.per_page.unwrap_or(10).min(100);

    let client = crate::cratesio::CratesIoClient::new(&state.client, &state.cache);
    let result = client
        .search(
            query,
            params.category.as_deref(),
            params.keyword.as_deref(),
            params.sort.as_deref(),
            page,
            per_page,
        )
        .await
        .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

    let json = serde_json::to_string_pretty(&result)
        .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

    Ok(CallToolResult::success(vec![Content::text(json)]))
}
