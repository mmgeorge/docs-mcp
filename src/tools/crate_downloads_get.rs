use rmcp::{ErrorData, model::{CallToolResult, Content}};
use serde::Deserialize;
use rmcp::schemars::{self, JsonSchema};
use serde_json::json;
use std::collections::HashMap;

use super::AppState;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CrateDownloadsGetParams {
    /// Crate name
    pub name: String,
    /// ISO date (YYYY-MM-DD). Returns 90 days ending on this date. Defaults to today.
    pub before_date: Option<String>,
}

pub async fn execute(state: &AppState, params: CrateDownloadsGetParams) -> Result<CallToolResult, ErrorData> {
    let name = &params.name;
    let client = crate::cratesio::CratesIoClient::new(&state.client, &state.cache);

    // Fetch download stats and version list in parallel
    let (downloads_result, versions_result) = tokio::join!(
        client.get_downloads(name, params.before_date.as_deref()),
        client.get_versions(name)
    );

    let downloads = downloads_result.map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
    let versions = versions_result.map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

    // Build version ID → semver string map
    let version_map: HashMap<u64, &str> = versions.versions.iter()
        .map(|v| (v.id, v.num.as_str()))
        .collect();

    let mut total_30d: u64 = 0;
    let mut total_90d: u64 = 0;
    let mut versions_breakdown: HashMap<&str, u64> = HashMap::new();

    // Determine 30d cutoff (rough: just first 30 lines if sorted, or use dates)
    // We'll compute from dates if available
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let cutoff_30 = params.before_date.as_deref()
        .map(|d| subtract_days(d, 30))
        .unwrap_or_else(|| subtract_days(&today, 30));

    let items: Vec<serde_json::Value> = downloads.version_downloads.iter().map(|vd| {
        let ver = version_map.get(&vd.version).copied().unwrap_or("?");
        total_90d += vd.downloads;
        if vd.date >= cutoff_30 {
            total_30d += vd.downloads;
        }
        *versions_breakdown.entry(ver).or_insert(0) += vd.downloads;
        json!({
            "version": ver,
            "date": vd.date,
            "downloads": vd.downloads,
        })
    }).collect();

    // Sort versions_breakdown by download count
    let mut breakdown_sorted: Vec<(&str, u64)> = versions_breakdown.into_iter().collect();
    breakdown_sorted.sort_by(|a, b| b.1.cmp(&a.1));

    let output = json!({
        "name": name,
        "before_date": params.before_date,
        "total_30d": total_30d,
        "total_90d": total_90d,
        "versions_breakdown": breakdown_sorted.iter()
            .map(|(v, c)| json!({"version": v, "downloads": c}))
            .collect::<Vec<_>>(),
        "version_downloads": items,
    });

    let json = serde_json::to_string_pretty(&output)
        .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
    Ok(CallToolResult::success(vec![Content::text(json)]))
}

/// Subtract N days from an ISO date string (YYYY-MM-DD). Returns the original on error.
fn subtract_days(date: &str, days: i64) -> String {
    use chrono::NaiveDate;
    NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .ok()
        .and_then(|d| d.checked_sub_signed(chrono::Duration::days(days)))
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| date.to_string())
}
