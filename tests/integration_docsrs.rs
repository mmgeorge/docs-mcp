/// Integration tests for docs.rs rustdoc JSON fetching and parsing.
/// These make real network calls and are disabled by default.
/// Run with: cargo test -- --include-ignored
use docs_mcp::cache::decompress_zstd;
use docs_mcp::docsrs::RustdocJson;
use docs_mcp::tools::{AppState, crate_docs_get, crate_item_list, crate_item_get};

async fn make_state() -> AppState {
    AppState::new().await.expect("AppState::new should succeed")
}

fn extract_text(result: &rmcp::model::CallToolResult) -> String {
    result.content[0].as_text().expect("expected text content").text.clone()
}

/// Tests the raw HTTP fetch → zstd decompression → RustdocJson deserialization path.
///
/// This exercises exactly what `fetch_rustdoc_json` does internally:
/// 1. GET https://docs.rs/crate/{name}/{version}/json
/// 2. Response is `Content-Type: application/zstd` (not transparently decompressed by reqwest)
/// 3. Decompress bytes with zstd
/// 4. Deserialize as RustdocJson
#[tokio::test]
#[ignore = "requires network access"]
async fn docsrs_raw_fetch_decompress_and_parse() {
    let state = make_state().await;
    let url = "https://docs.rs/crate/serde/1.0.228/json";

    let resp = state.client.get(url).send().await.expect("HTTP request should succeed");
    assert!(resp.status().is_success(), "expected 200, got {}", resp.status());

    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        content_type.contains("zstd"),
        "expected application/zstd content-type, got: {content_type}"
    );

    let bytes = resp.bytes().await.expect("reading response body should succeed");
    assert!(!bytes.is_empty(), "response body should not be empty");

    let json_str = decompress_zstd(&bytes).expect("zstd decompression should succeed");
    assert!(json_str.starts_with('{'), "decompressed content should be a JSON object");

    let doc: RustdocJson =
        serde_json::from_str(&json_str).expect("should deserialize as RustdocJson");
    assert!(doc.format_version >= 33, "format_version should be >= 33");
    assert!(!doc.index.is_empty(), "index should have items");
    assert!(!doc.paths.is_empty(), "paths should have entries");
}

#[tokio::test]
#[ignore = "requires network access"]
async fn docsrs_fetch_serde_rustdoc_json_succeeds() {
    let state = make_state().await;
    let params = crate_docs_get::CrateDocsGetParams {
        name: "serde".to_string(),
        version: Some("1.0.217".to_string()),
        include_items: Some(false),
    };
    let result = crate_docs_get::execute(&state, params).await
        .expect("crate_docs_get should succeed");
    let text = extract_text(&result);
    let json: serde_json::Value = serde_json::from_str(&text).expect("should be valid JSON");
    assert_eq!(json["name"], "serde");
    assert!(!json["root_docs"].as_str().unwrap_or("").is_empty(), "serde should have root docs");
    assert!(json["module_tree"].is_array(), "module_tree should be array");
}

#[tokio::test]
#[ignore = "requires network access"]
async fn docsrs_crate_item_list_serde_serialize() {
    let state = make_state().await;
    let params = crate_item_list::CrateItemListParams {
        name: "serde".to_string(),
        version: None,
        query: "Serialize".to_string(),
        kind: None,
        module_prefix: None,
        limit: Some(10),
    };
    let result = crate_item_list::execute(&state, params).await
        .expect("crate_item_list should succeed");
    let text = extract_text(&result);
    let json: serde_json::Value = serde_json::from_str(&text).expect("should be valid JSON");
    let items = json["items"].as_array().expect("items should be array");
    assert!(!items.is_empty(), "should find items matching 'Serialize'");
    let found_trait = items.iter().any(|i| {
        i["path"].as_str().map(|p| p.contains("Serialize")).unwrap_or(false)
    });
    assert!(found_trait, "should find the Serialize trait");
}

#[tokio::test]
#[ignore = "requires network access"]
async fn docsrs_crate_item_get_serde_serialize_trait() {
    let state = make_state().await;
    let params = crate_item_get::CrateItemGetParams {
        name: "serde".to_string(),
        version: None,
        item_path: "serde::Serialize".to_string(),
        include_methods: None,
        include_trait_impls: None,
    };
    let result = crate_item_get::execute(&state, params).await
        .expect("crate_item_get should succeed");
    let text = extract_text(&result);
    let json: serde_json::Value = serde_json::from_str(&text).expect("should be valid JSON");
    assert_eq!(json["path"], "serde::Serialize");
    assert_eq!(json["kind"], "trait");
    assert!(!json["docs"].as_str().unwrap_or("").is_empty(), "Serialize should have docs");
}

#[tokio::test]
#[ignore = "requires network access"]
async fn docsrs_second_fetch_uses_cache() {
    let state = make_state().await;
    let result1 = crate_docs_get::execute(&state, crate_docs_get::CrateDocsGetParams {
        name: "anyhow".to_string(),
        version: None,
        include_items: Some(false),
    }).await.expect("first fetch should succeed");
    let result2 = crate_docs_get::execute(&state, crate_docs_get::CrateDocsGetParams {
        name: "anyhow".to_string(),
        version: None,
        include_items: Some(false),
    }).await.expect("second fetch should succeed");
    let j1: serde_json::Value = serde_json::from_str(&extract_text(&result1)).unwrap();
    let j2: serde_json::Value = serde_json::from_str(&extract_text(&result2)).unwrap();
    assert_eq!(j1["name"], j2["name"]);
    assert_eq!(j1["version"], j2["version"]);
}
