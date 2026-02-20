use rmcp::{ErrorData, model::CallToolResult};
use rmcp::model::Content;
use serde::{Deserialize, Serialize};
use rmcp::schemars::{self, JsonSchema};

use crate::cratesio::CrateInfo;
use super::AppState;

#[derive(Serialize)]
struct CrateListEntry<'a> {
    name: &'a str,
    description: Option<&'a str>,
    version: Option<&'a str>,
    newest_version: Option<&'a str>,
    downloads: u64,
    recent_downloads: Option<u64>,
    updated_at: &'a str,
    repository: Option<&'a str>,
}

impl<'a> From<&'a CrateInfo> for CrateListEntry<'a> {
    fn from(c: &'a CrateInfo) -> Self {
        Self {
            name: &c.name,
            description: c.description.as_deref(),
            version: c.max_stable_version.as_deref().or(c.max_version.as_deref()),
            newest_version: c.newest_version.as_deref(),
            downloads: c.downloads,
            recent_downloads: c.recent_downloads,
            updated_at: &c.updated_at,
            repository: c.repository.as_deref(),
        }
    }
}

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

    let entries: Vec<CrateListEntry> = result.crates.iter().map(CrateListEntry::from).collect();
    let output = serde_json::json!({ "crates": entries, "total": result.meta.total });
    let json = serde_json::to_string_pretty(&output)
        .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

    Ok(CallToolResult::success(vec![Content::text(json)]))
}
