use rmcp::{ErrorData, model::{CallToolResult, Content}};
use serde::Deserialize;
use rmcp::schemars::{self, JsonSchema};
use serde_json::json;

use super::AppState;

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

    let output = json!({
        "num": v.num,
        "license": v.license,
        "edition": v.edition,
        "rust_version": v.rust_version,
        "has_lib": v.has_lib,
        "bin_names": v.bins,
        "crate_size": v.crate_size,
        "downloads": v.downloads,
        "yanked": v.yanked,
        "yank_message": v.yank_message,
        "published_by": v.published_by.as_ref().map(|p| json!({"login": p.login, "name": p.name})),
        "created_at": v.created_at,
        "checksum": v.checksum,
        "lib_links": v.lib_links,
    });

    let json = serde_json::to_string_pretty(&output)
        .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
    Ok(CallToolResult::success(vec![Content::text(json)]))
}
