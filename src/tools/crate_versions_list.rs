use rmcp::{ErrorData, model::{CallToolResult, Content}};
use serde::{Deserialize, Serialize};
use rmcp::schemars::{self, JsonSchema};
use serde_json::json;
use semver::Version;

use super::AppState;

#[derive(Serialize)]
struct VersionEntry {
    version: String,
    yanked: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    rust_version: Option<String>,
    features: Vec<String>,
    dep_count: usize,
}

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
    /// Results per page (default: 30, max: 100)
    pub per_page: Option<usize>,
    /// Page number, 1-indexed (default: 1)
    pub page: Option<usize>,
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

    let total = versions.len();
    let per_page = params.per_page.unwrap_or(30).min(100).max(1);
    let page = params.page.unwrap_or(1).max(1);
    let start = (page - 1) * per_page;
    let versions = &versions[start.min(total)..];
    let versions = &versions[..per_page.min(versions.len())];

    let items: Vec<VersionEntry> = versions.iter().map(|l| {
        let normal_deps = l.deps.iter().filter(|d| {
            d.kind.as_ref().map(|k| matches!(k, crate::sparse_index::DepKind::Normal)).unwrap_or(true)
        }).count();
        // Emit feature names only (not their dep-enable lists) to keep output compact.
        // The full feature map for large crates (tokio, serde) would be enormous across
        // many versions and is rarely needed by an LLM.
        let all_feats = l.all_features();
        let mut feature_names: Vec<String> = all_feats.keys().cloned().collect();
        feature_names.sort_unstable();
        VersionEntry {
            version: l.vers.clone(),
            yanked: l.yanked,
            rust_version: l.rust_version.clone(),
            features: feature_names,
            dep_count: normal_deps,
        }
    }).collect();

    let output = json!({
        "name": name,
        "total": total,
        "page": page,
        "per_page": per_page,
        "count": items.len(),
        "versions": items,
    });

    let json = serde_json::to_string_pretty(&output)
        .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
    Ok(CallToolResult::success(vec![Content::text(json)]))
}
