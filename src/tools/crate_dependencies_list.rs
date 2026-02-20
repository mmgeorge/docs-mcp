use rmcp::{ErrorData, model::{CallToolResult, Content}};
use serde::Deserialize;
use rmcp::schemars::{self, JsonSchema};
use serde_json::json;

use super::AppState;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CrateDependenciesListParams {
    /// Crate name
    pub name: String,
    /// Exact version string
    pub version: String,
    /// Filter by dep kind: "normal", "dev", "build" (default: all)
    pub kind: Option<String>,
    /// Filter results by dep name substring
    pub search: Option<String>,
}

pub async fn execute(state: &AppState, params: CrateDependenciesListParams) -> Result<CallToolResult, ErrorData> {
    let name = &params.name;
    let version = &params.version;

    let client = crate::cratesio::CratesIoClient::new(&state.client, &state.cache);
    let resp = client.get_dependencies(name, version).await
        .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

    let search_lower = params.search.as_deref().map(|s| s.to_lowercase());
    let kind_filter = params.kind.as_deref();

    let deps: Vec<serde_json::Value> = resp.dependencies.into_iter()
        .filter(|d| {
            if let Some(kf) = kind_filter {
                let dep_kind = d.kind.as_deref().unwrap_or("normal");
                if dep_kind != kf { return false; }
            }
            if let Some(ref search) = search_lower {
                if !d.crate_id.to_lowercase().contains(search.as_str()) {
                    return false;
                }
            }
            true
        })
        .map(|d| json!({
            "crate_id": d.crate_id,
            "req": d.req,
            "kind": d.kind.as_deref().unwrap_or("normal"),
            "optional": d.optional,
            "default_features": d.default_features,
            "features": d.features,
            "target": d.target,
        }))
        .collect();

    let output = json!({
        "name": name,
        "version": version,
        "count": deps.len(),
        "dependencies": deps,
    });

    let json = serde_json::to_string_pretty(&output)
        .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
    Ok(CallToolResult::success(vec![Content::text(json)]))
}
