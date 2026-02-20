use rmcp::{ErrorData, model::{CallToolResult, Content}};
use serde::Deserialize;
use rmcp::schemars::{self, JsonSchema};
use serde_json::json;

use super::AppState;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CrateDependentsListParams {
    /// Crate name to find dependents of
    pub name: String,
    /// Page number (default: 1)
    pub page: Option<u32>,
    /// Results per page (max 100, default: 20)
    pub per_page: Option<u32>,
    /// Filter results by dependent crate name substring
    pub search: Option<String>,
}

pub async fn execute(state: &AppState, params: CrateDependentsListParams) -> Result<CallToolResult, ErrorData> {
    let name = &params.name;
    let page = params.page.unwrap_or(1).max(1);
    let per_page = params.per_page.unwrap_or(20).min(100);

    let client = crate::cratesio::CratesIoClient::new(&state.client, &state.cache);
    let resp = client.get_reverse_deps(name, page, per_page).await
        .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

    // Build version ID → crate name lookup
    let version_map: std::collections::HashMap<u64, &str> = resp.versions.iter()
        .map(|v| (v.id, v.crate_name.as_str()))
        .collect();

    let search_lower = params.search.as_deref().map(|s| s.to_lowercase());

    let deps: Vec<serde_json::Value> = resp.dependencies.iter()
        .filter(|d| {
            let crate_name = version_map.get(&d.version_id).unwrap_or(&"?");
            if let Some(ref search) = search_lower {
                if !crate_name.to_lowercase().contains(search.as_str()) {
                    return false;
                }
            }
            true
        })
        .map(|d| {
            let crate_name = version_map.get(&d.version_id).unwrap_or(&"?");
            json!({
                "crate_id": d.crate_id,
                "dependent_crate": crate_name,
                "req": d.req,
                "optional": d.optional,
                "default_features": d.default_features,
                "features": d.features,
                "kind": d.kind,
            })
        })
        .collect();

    let output = json!({
        "name": name,
        "total": resp.meta.total,
        "page": page,
        "per_page": per_page,
        "count": deps.len(),
        "dependents": deps,
    });

    let json = serde_json::to_string_pretty(&output)
        .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
    Ok(CallToolResult::success(vec![Content::text(json)]))
}
