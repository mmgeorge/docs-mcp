use rmcp::{ErrorData, model::{CallToolResult, Content}};
use serde::{Deserialize, Serialize};
use rmcp::schemars::{self, JsonSchema};
use serde_json::json;

use super::AppState;

#[derive(Serialize)]
struct DepEntry {
    crate_id: String,
    req: String,
    kind: String,
    optional: bool,
    default_features: bool,
    features: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CrateDependenciesListParams {
    /// Crate name
    pub name: String,
    /// Exact version string (e.g. "1.0.197"). Defaults to latest stable.
    pub version: Option<String>,
    /// Filter by dep kind: "normal", "dev", "build" (default: all)
    pub kind: Option<String>,
    /// Filter results by dep name substring
    pub search: Option<String>,
}

pub async fn execute(state: &AppState, params: CrateDependenciesListParams) -> Result<CallToolResult, ErrorData> {
    let name = &params.name;
    let version = state.resolve_version(name, params.version.as_deref()).await
        .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

    let client = crate::cratesio::CratesIoClient::new(&state.client, &state.cache);
    let resp = client.get_dependencies(name, &version).await
        .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

    let search_lower = params.search.as_deref().map(|s| s.to_lowercase());
    let kind_filter = params.kind.as_deref();

    let deps = resp.dependencies.into_iter()
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
        .map(|d| DepEntry {
            crate_id: d.crate_id,
            req: d.req,
            kind: d.kind.unwrap_or_else(|| "normal".into()),
            optional: d.optional,
            default_features: d.default_features,
            features: d.features,
            target: d.target,
        })
        .collect::<Vec<_>>();

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
