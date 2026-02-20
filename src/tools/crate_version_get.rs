use rmcp::{ErrorData, model::{CallToolResult, Content}};
use serde::{Deserialize, Serialize};
use rmcp::schemars::{self, JsonSchema};

use super::AppState;

#[derive(Serialize)]
struct PublisherOutput {
    login: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

#[derive(Serialize)]
struct VersionGetOutput {
    num: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    license: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    edition: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rust_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    has_lib: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bin_names: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    crate_size: Option<u64>,
    downloads: u64,
    yanked: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    yank_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    published_by: Option<PublisherOutput>,
    created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    checksum: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    lib_links: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CrateVersionGetParams {
    /// Crate name
    pub name: String,
    /// Exact version string (e.g. "1.0.197")
    pub version: String,
}

pub async fn execute(state: &AppState, params: CrateVersionGetParams) -> Result<CallToolResult, ErrorData> {
    let name = &params.name;
    let version = &params.version;

    let client = crate::cratesio::CratesIoClient::new(&state.client, &state.cache);
    let v = client.get_version(name, version).await
        .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

    let output = VersionGetOutput {
        num: v.num,
        license: v.license,
        edition: v.edition,
        rust_version: v.rust_version,
        has_lib: v.has_lib,
        bin_names: v.bins,
        crate_size: v.crate_size,
        downloads: v.downloads,
        yanked: v.yanked,
        yank_message: v.yank_message,
        published_by: v.published_by.map(|p| PublisherOutput { login: p.login, name: p.name }),
        created_at: v.created_at,
        checksum: v.checksum,
        lib_links: v.lib_links,
    };

    let json = serde_json::to_string_pretty(&output)
        .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
    Ok(CallToolResult::success(vec![Content::text(json)]))
}
