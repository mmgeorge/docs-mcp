use rmcp::{ErrorData, model::{CallToolResult, Content}};
use serde::Deserialize;
use rmcp::schemars::{self, JsonSchema};
use serde_json::json;
use semver::Version;

use super::AppState;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CrateVersionsListParams {
    /// Crate name
    pub name: String,
    /// Include yanked versions (default: false)
    pub include_yanked: Option<bool>,
    /// Include pre-release versions (default: false)
    pub include_prerelease: Option<bool>,
    /// Filter by semver prefix or substring (e.g. "1.0")
    pub search: Option<String>,
}

pub async fn execute(state: &AppState, params: CrateVersionsListParams) -> Result<CallToolResult, ErrorData> {
    let name = &params.name;
    let include_yanked = params.include_yanked.unwrap_or(false);
    let include_prerelease = params.include_prerelease.unwrap_or(false);

    let lines = state.fetch_index(name).await
        .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

    let mut versions: Vec<_> = lines.into_iter()
        .filter(|l| {
            if !include_yanked && l.yanked { return false; }
            if !include_prerelease && l.vers.contains('-') { return false; }
            if let Some(ref search) = params.search {
                if !l.vers.starts_with(search.as_str()) && !l.vers.contains(search.as_str()) {
                    return false;
                }
            }
            true
        })
        .collect();

    // Sort descending by semver
    versions.sort_by(|a, b| {
        let va = Version::parse(&a.vers).ok();
        let vb = Version::parse(&b.vers).ok();
        vb.cmp(&va)
    });

    let items: Vec<serde_json::Value> = versions.iter().map(|l| {
        let normal_deps = l.deps.iter().filter(|d| {
            d.kind.as_ref().map(|k| matches!(k, crate::sparse_index::DepKind::Normal)).unwrap_or(true)
        }).count();
        // Emit feature names only (not their dep-enable lists) to keep output compact.
        // The full feature map for large crates (tokio, serde) would be enormous across
        // many versions and is rarely needed by an LLM.
        let all_feats = l.all_features();
        let mut feature_names: Vec<&str> = all_feats.keys().map(|k| k.as_str()).collect();
        feature_names.sort_unstable();
        json!({
            "version": l.vers,
            "yanked": l.yanked,
            "rust_version": l.rust_version,
            "features": feature_names,
            "dep_count": normal_deps,
        })
    }).collect();

    let output = json!({
        "name": name,
        "count": items.len(),
        "versions": items,
    });

    let json = serde_json::to_string_pretty(&output)
        .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
    Ok(CallToolResult::success(vec![Content::text(json)]))
}
