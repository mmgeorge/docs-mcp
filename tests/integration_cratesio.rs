/// Integration tests for crates.io API access.
/// These make real network calls and are disabled by default.
/// Run with: cargo test -- --include-ignored
use docs_mcp::tools::{AppState, crate_list, crate_get, crate_versions_list, crate_downloads_get};

async fn make_state() -> AppState {
    AppState::new().await.expect("AppState::new should succeed")
}

fn extract_text(result: &rmcp::model::CallToolResult) -> String {
    // Content = Annotated<RawContent>; deref to RawContent and call as_text()
    result.content[0].as_text().expect("expected text content").text.clone()
}

#[tokio::test]
#[ignore = "requires network access"]
async fn cratesio_crate_list_serde_returns_results() {
    let state = make_state().await;
    let params = crate_list::CrateListParams {
        query: Some("serde".to_string()),
        category: None,
        keyword: None,
        sort: None,
        page: None,
        per_page: Some(5),
    };
    let result = crate_list::execute(&state, params).await
        .expect("crate_list should succeed");
    let text = extract_text(&result);
    let json: serde_json::Value = serde_json::from_str(&text).expect("should be valid JSON");
    let crates = json["crates"].as_array().expect("crates should be array");
    assert!(!crates.is_empty(), "should return at least one crate");
    let found = crates.iter().any(|c| c["name"].as_str() == Some("serde"));
    assert!(found, "serde should appear in results");
}

#[tokio::test]
#[ignore = "requires network access"]
async fn cratesio_crate_get_tokio_returns_features() {
    let state = make_state().await;
    let params = crate_get::CrateGetParams {
        name: "tokio".to_string(),
    };
    let result = crate_get::execute(&state, params).await
        .expect("crate_get should succeed");
    let text = extract_text(&result);
    let json: serde_json::Value = serde_json::from_str(&text).expect("should be valid JSON");
    assert_eq!(json["name"], "tokio");
    assert!(!json["version"].as_str().unwrap_or("").is_empty(), "version should not be empty");
    assert!(json["features"].is_object(), "features should be an object");
}

#[tokio::test]
#[ignore = "requires network access"]
async fn cratesio_versions_list_serde_returns_stable_versions() {
    let state = make_state().await;
    let params = crate_versions_list::CrateVersionsListParams {
        name: "serde".to_string(),
        include_yanked: Some(false),
        include_prerelease: Some(false),
        search: None,
    };
    let result = crate_versions_list::execute(&state, params).await
        .expect("crate_versions_list should succeed");
    let text = extract_text(&result);
    let json: serde_json::Value = serde_json::from_str(&text).expect("should be valid JSON");
    let versions = json["versions"].as_array().expect("versions should be array");
    assert!(!versions.is_empty(), "should have at least one stable version");
    for v in versions {
        let ver = v["num"].as_str().unwrap_or("");
        assert!(!ver.contains('-'), "stable version should not contain '-': {}", ver);
    }
}

#[tokio::test]
#[ignore = "requires network access"]
async fn cratesio_downloads_get_anyhow_returns_nonzero() {
    let state = make_state().await;
    let params = crate_downloads_get::CrateDownloadsGetParams {
        name: "anyhow".to_string(),
        before_date: None,
    };
    let result = crate_downloads_get::execute(&state, params).await
        .expect("crate_downloads_get should succeed");
    let text = extract_text(&result);
    let json: serde_json::Value = serde_json::from_str(&text).expect("should be valid JSON");
    let total = json["total_downloads"].as_u64().unwrap_or(0);
    assert!(total > 0, "anyhow should have non-zero total downloads");
}
