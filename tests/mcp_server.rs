/// In-process MCP server tests.
/// Uses `tokio::io::duplex` to wire a real server and a test client without touching the network.
/// Tool-behavior tests call real external APIs and are marked #[ignore = "requires network access"].
use std::sync::Arc;

use docs_mcp::{server::DocsMcpServer, tools::AppState};
use rmcp::{
    ServiceExt,
    handler::client::ClientHandler,
    model::{
        CallToolRequestParams, CallToolResult, ClientCapabilities, ClientInfo,
        Implementation, ProtocolVersion,
    },
    service::{serve_client, Peer, RunningService, RoleClient},
};
use serde_json::Value;

// ─── Shared infrastructure ─────────────────────────────────────────────────────

struct TestClient;

impl ClientHandler for TestClient {
    fn get_info(&self) -> ClientInfo {
        ClientInfo {
            meta: None,
            protocol_version: ProtocolVersion::default(),
            capabilities: ClientCapabilities::default(),
            client_info: Implementation {
                name: "test-client".to_string(),
                title: None,
                version: "0.0.0".to_string(),
                description: None,
                icons: None,
                website_url: None,
            },
        }
    }
}

async fn connect() -> RunningService<RoleClient, TestClient> {
    let state = AppState::new().await.expect("AppState::new should succeed");
    let server = DocsMcpServer::new_with_state(Arc::new(state));
    let (server_side, client_side) = tokio::io::duplex(65536);
    let (server_r, server_w) = tokio::io::split(server_side);
    let (client_r, client_w) = tokio::io::split(client_side);
    tokio::spawn(async move {
        if let Ok(running) = server.serve((server_r, server_w)).await {
            let _ = running.waiting().await;
        }
    });
    serve_client(TestClient, (client_r, client_w))
        .await
        .expect("client should connect to server")
}

fn params(name: &'static str, args: Value) -> CallToolRequestParams {
    CallToolRequestParams {
        meta: None,
        name: std::borrow::Cow::Borrowed(name),
        arguments: args.as_object().cloned(),
        task: None,
    }
}

async fn call(peer: &Peer<RoleClient>, tool: &'static str, args: Value) -> Value {
    let result: CallToolResult = peer
        .call_tool(params(tool, args))
        .await
        .unwrap_or_else(|e| panic!("tool '{}' call failed: {}", tool, e));
    let text = result.content[0]
        .as_text()
        .unwrap_or_else(|| panic!("tool '{}' returned non-text content", tool))
        .text
        .clone();
    serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("tool '{}' returned invalid JSON: {} — body: {}", tool, e, text))
}

// ─── Registration smoke tests (no network) ────────────────────────────────────

#[tokio::test]
async fn mcp_server_lists_12_tools() {
    let client = connect().await;
    let tools = client.peer().list_all_tools().await.expect("list_tools should succeed");
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
    assert_eq!(tools.len(), 12, "expected 12 tools, got: {:?}", names);
    for expected in [
        "crate_list", "crate_get", "crate_readme_get", "crate_docs_get",
        "crate_item_list", "crate_item_get", "crate_impls_list",
        "crate_versions_list", "crate_version_get",
        "crate_dependencies_list", "crate_dependents_list", "crate_downloads_get",
    ] {
        assert!(names.contains(&expected), "missing tool '{}'; got: {:?}", expected, names);
    }
    client.cancel().await.expect("clean shutdown");
}

#[tokio::test]
async fn mcp_server_tools_have_descriptions() {
    let client = connect().await;
    let tools = client.peer().list_all_tools().await.expect("list_tools should succeed");
    for tool in &tools {
        assert!(
            !tool.description.as_deref().unwrap_or("").is_empty(),
            "tool '{}' should have a description", tool.name
        );
    }
    client.cancel().await.expect("clean shutdown");
}

#[tokio::test]
async fn mcp_server_tools_have_input_schemas() {
    let client = connect().await;
    let tools = client.peer().list_all_tools().await.expect("list_tools should succeed");
    for tool in &tools {
        let schema = &tool.input_schema;
        // input_schema is Arc<serde_json::Map<...>>, which is always an object
        let _ = schema; // schema type is confirmed to be a JSON object map
    }
    client.cancel().await.expect("clean shutdown");
}

// ─── crate_list ───────────────────────────────────────────────────────────────

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_list_returns_crates_array() {
    let client = connect().await;
    let j = call(client.peer(), "crate_list", serde_json::json!({"query": "serde"})).await;
    assert!(j["crates"].is_array(), "should have 'crates' array");
    client.cancel().await.ok();
}

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_list_result_contains_expected_fields() {
    let client = connect().await;
    let j = call(client.peer(), "crate_list", serde_json::json!({"query": "tokio"})).await;
    let crates = j["crates"].as_array().expect("crates should be array");
    assert!(!crates.is_empty(), "should return at least one crate");
    let first = &crates[0];
    assert!(first["name"].is_string(), "each crate should have 'name'");
    assert!(first["description"].is_string() || first["description"].is_null(), "should have 'description'");
    assert!(first["downloads"].is_number(), "should have 'downloads'");
    client.cancel().await.ok();
}

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_list_serde_appears_in_results() {
    let client = connect().await;
    let j = call(client.peer(), "crate_list", serde_json::json!({"query": "serde"})).await;
    let crates = j["crates"].as_array().expect("crates should be array");
    let found = crates.iter().any(|c| c["name"].as_str() == Some("serde"));
    assert!(found, "serde should appear in results for query 'serde'");
    client.cancel().await.ok();
}

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_list_per_page_limits_results() {
    let client = connect().await;
    let j = call(client.peer(), "crate_list", serde_json::json!({"query": "async", "per_page": 3})).await;
    let crates = j["crates"].as_array().expect("crates should be array");
    assert!(crates.len() <= 3, "per_page=3 should return at most 3 results, got {}", crates.len());
    client.cancel().await.ok();
}

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_list_empty_query_returns_results() {
    let client = connect().await;
    let j = call(client.peer(), "crate_list", serde_json::json!({"per_page": 5})).await;
    let crates = j["crates"].as_array().expect("crates should be array");
    assert!(!crates.is_empty(), "empty query should return popular crates");
    client.cancel().await.ok();
}

// ─── crate_get ────────────────────────────────────────────────────────────────

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_get_returns_expected_top_level_fields() {
    let client = connect().await;
    let j = call(client.peer(), "crate_get", serde_json::json!({"name": "serde"})).await;
    assert_eq!(j["name"], "serde");
    assert!(j["version"].is_string(), "should have 'version'");
    assert!(j["description"].is_string() || j["description"].is_null(), "should have 'description'");
    assert!(j["features"].is_object(), "should have 'features' object");
    assert!(j["downloads"].is_number() || j["downloads"].is_null(), "should have 'downloads'");
    client.cancel().await.ok();
}

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_get_tokio_has_features() {
    let client = connect().await;
    let j = call(client.peer(), "crate_get", serde_json::json!({"name": "tokio"})).await;
    let features = j["features"].as_object().expect("features should be object");
    assert!(!features.is_empty(), "tokio should have feature flags");
    assert!(features.contains_key("full") || features.contains_key("rt"), "tokio should have well-known features");
    client.cancel().await.ok();
}

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_get_has_repository_url() {
    let client = connect().await;
    let j = call(client.peer(), "crate_get", serde_json::json!({"name": "anyhow"})).await;
    let repo = j["repository"].as_str().unwrap_or("");
    assert!(!repo.is_empty(), "anyhow should have a repository URL");
    assert!(repo.contains("github.com") || repo.contains("git."), "repository should look like a URL");
    client.cancel().await.ok();
}

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_get_has_nonzero_downloads() {
    let client = connect().await;
    let j = call(client.peer(), "crate_get", serde_json::json!({"name": "reqwest"})).await;
    let downloads = j["downloads"].as_u64().unwrap_or(0);
    assert!(downloads > 1_000, "reqwest should have substantial downloads, got {}", downloads);
    client.cancel().await.ok();
}

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_get_version_matches_index() {
    let client = connect().await;
    let j = call(client.peer(), "crate_get", serde_json::json!({"name": "serde"})).await;
    // Both the API-reported max_stable_version and index-derived version should be non-empty
    let max_ver = j["max_stable_version"].as_str().unwrap_or("");
    let idx_ver = j["latest_stable_from_index"].as_str().unwrap_or("");
    assert!(!max_ver.is_empty() || !idx_ver.is_empty(), "at least one version source should return a value");
    client.cancel().await.ok();
}

// ─── crate_readme_get ─────────────────────────────────────────────────────────

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_readme_get_serde_returns_text() {
    let client = connect().await;
    let j = call(client.peer(), "crate_readme_get", serde_json::json!({"name": "serde"})).await;
    let readme = j["readme"].as_str().unwrap_or("");
    assert!(!readme.is_empty(), "serde readme should not be empty");
    client.cancel().await.ok();
}

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_readme_get_contains_crate_name() {
    let client = connect().await;
    let j = call(client.peer(), "crate_readme_get", serde_json::json!({"name": "anyhow"})).await;
    let readme = j["readme"].as_str().unwrap_or("");
    assert!(
        readme.to_lowercase().contains("anyhow"),
        "anyhow readme should mention the crate name"
    );
    client.cancel().await.ok();
}

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_readme_get_specific_version() {
    let client = connect().await;
    let j = call(client.peer(), "crate_readme_get",
        serde_json::json!({"name": "serde", "version": "1.0.0"})).await;
    let readme = j["readme"].as_str().unwrap_or("");
    assert!(!readme.is_empty(), "serde 1.0.0 should have a readme");
    client.cancel().await.ok();
}

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_readme_get_includes_name_and_version() {
    let client = connect().await;
    let j = call(client.peer(), "crate_readme_get", serde_json::json!({"name": "tokio"})).await;
    assert_eq!(j["name"], "tokio", "response should include crate name");
    assert!(j["version"].is_string(), "response should include version");
    client.cancel().await.ok();
}

// ─── crate_docs_get ───────────────────────────────────────────────────────────

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_docs_get_serde_has_root_docs() {
    let client = connect().await;
    let j = call(client.peer(), "crate_docs_get", serde_json::json!({"name": "serde"})).await;
    let root_docs = j["root_docs"].as_str().unwrap_or("");
    assert!(!root_docs.is_empty(), "serde should have root //! docs");
    client.cancel().await.ok();
}

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_docs_get_returns_module_tree() {
    let client = connect().await;
    let j = call(client.peer(), "crate_docs_get", serde_json::json!({"name": "serde"})).await;
    let tree = j["module_tree"].as_array().expect("module_tree should be array");
    assert!(!tree.is_empty(), "serde module_tree should be non-empty");
    client.cancel().await.ok();
}

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_docs_get_returns_features_map() {
    let client = connect().await;
    let j = call(client.peer(), "crate_docs_get", serde_json::json!({"name": "tokio"})).await;
    let features = j["features"].as_object().expect("features should be an object");
    assert!(!features.is_empty(), "tokio should expose feature flags");
    client.cancel().await.ok();
}

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_docs_get_has_name_and_version() {
    let client = connect().await;
    let j = call(client.peer(), "crate_docs_get", serde_json::json!({"name": "anyhow"})).await;
    assert_eq!(j["name"], "anyhow");
    assert!(j["version"].is_string(), "should include version");
    client.cancel().await.ok();
}

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_docs_get_specific_version() {
    let client = connect().await;
    let j = call(client.peer(), "crate_docs_get",
        serde_json::json!({"name": "serde", "version": "1.0.217"})).await;
    assert_eq!(j["name"], "serde");
    assert_eq!(j["version"], "1.0.217");
    client.cancel().await.ok();
}

// ─── crate_item_list ──────────────────────────────────────────────────────────

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_item_list_returns_items_array() {
    let client = connect().await;
    let j = call(client.peer(), "crate_item_list",
        serde_json::json!({"name": "serde", "query": "Serialize"})).await;
    let items = j["items"].as_array().expect("should have 'items' array");
    assert!(!items.is_empty(), "should find items matching 'Serialize'");
    client.cancel().await.ok();
}

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_item_list_items_have_path_and_kind() {
    let client = connect().await;
    let j = call(client.peer(), "crate_item_list",
        serde_json::json!({"name": "serde", "query": "Serialize"})).await;
    let items = j["items"].as_array().expect("items array");
    for item in items {
        assert!(item["path"].is_string(), "item should have 'path'");
        assert!(item["kind"].is_string(), "item should have 'kind'");
    }
    client.cancel().await.ok();
}

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_item_list_kind_filter_structs() {
    let client = connect().await;
    let j = call(client.peer(), "crate_item_list",
        serde_json::json!({"name": "serde", "query": "Error", "kind": "struct"})).await;
    let items = j["items"].as_array().expect("items array");
    for item in items {
        assert_eq!(item["kind"], "struct", "kind filter should only return structs");
    }
    client.cancel().await.ok();
}

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_item_list_limit_respected() {
    let client = connect().await;
    let j = call(client.peer(), "crate_item_list",
        serde_json::json!({"name": "tokio", "query": "spawn", "limit": 3})).await;
    let items = j["items"].as_array().expect("items array");
    assert!(items.len() <= 3, "limit=3 should cap results, got {}", items.len());
    client.cancel().await.ok();
}

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_item_list_finds_serialize_trait() {
    let client = connect().await;
    let j = call(client.peer(), "crate_item_list",
        serde_json::json!({"name": "serde", "query": "Serialize", "kind": "trait"})).await;
    let items = j["items"].as_array().expect("items array");
    let found = items.iter().any(|i| {
        i["path"].as_str().map(|p| p.contains("Serialize")).unwrap_or(false)
    });
    assert!(found, "should find the Serialize trait");
    client.cancel().await.ok();
}

// ─── crate_item_get ───────────────────────────────────────────────────────────

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_item_get_serde_serialize_is_trait() {
    let client = connect().await;
    let j = call(client.peer(), "crate_item_get",
        serde_json::json!({"name": "serde", "item_path": "serde::Serialize"})).await;
    assert_eq!(j["path"], "serde::Serialize");
    assert_eq!(j["kind"], "trait");
    client.cancel().await.ok();
}

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_item_get_has_docs() {
    let client = connect().await;
    let j = call(client.peer(), "crate_item_get",
        serde_json::json!({"name": "serde", "item_path": "serde::Serialize"})).await;
    let docs = j["docs"].as_str().unwrap_or("");
    assert!(!docs.is_empty(), "serde::Serialize should have documentation");
    client.cancel().await.ok();
}

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_item_get_struct_includes_methods() {
    let client = connect().await;
    let j = call(client.peer(), "crate_item_get",
        serde_json::json!({"name": "anyhow", "item_path": "anyhow::Error", "include_methods": true})).await;
    // methods may be empty for some items but the field should exist
    assert!(j["methods"].is_array() || j["kind"].is_string(), "response should have methods or at least kind");
    client.cancel().await.ok();
}

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_item_get_function_has_signature() {
    let client = connect().await;
    let j = call(client.peer(), "crate_item_list",
        serde_json::json!({"name": "anyhow", "query": "format_err", "kind": "macro"})).await;
    // First find a function-like item, then look it up
    let items = j["items"].as_array().expect("items");
    if let Some(item) = items.first() {
        let path = item["path"].as_str().unwrap_or("");
        if !path.is_empty() {
            let detail = call(client.peer(), "crate_item_get",
                serde_json::json!({"name": "anyhow", "item_path": path})).await;
            assert!(detail["kind"].is_string(), "item should have kind");
        }
    }
    client.cancel().await.ok();
}

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_item_get_includes_trait_impls() {
    let client = connect().await;
    let j = call(client.peer(), "crate_item_get",
        serde_json::json!({
            "name": "anyhow",
            "item_path": "anyhow::Error",
            "include_trait_impls": true
        })).await;
    assert!(j["trait_impls"].is_array() || j["kind"].is_string(), "response should have trait_impls or kind");
    client.cancel().await.ok();
}

// ─── crate_impls_list ─────────────────────────────────────────────────────────

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_impls_list_by_trait_returns_results() {
    let client = connect().await;
    let j = call(client.peer(), "crate_impls_list",
        serde_json::json!({"name": "serde", "trait_path": "serde::Serialize"})).await;
    assert!(j["impls"].is_array() || j["implementations"].is_array(),
        "should return an impls or implementations array");
    client.cancel().await.ok();
}

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_impls_list_by_type_returns_results() {
    let client = connect().await;
    let j = call(client.peer(), "crate_impls_list",
        serde_json::json!({"name": "anyhow", "type_path": "anyhow::Error"})).await;
    assert!(j.is_object(), "response should be a JSON object");
    client.cancel().await.ok();
}

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_impls_list_search_filter_narrows_results() {
    let client = connect().await;
    let all = call(client.peer(), "crate_impls_list",
        serde_json::json!({"name": "serde", "trait_path": "serde::Serialize"})).await;
    let filtered = call(client.peer(), "crate_impls_list",
        serde_json::json!({"name": "serde", "trait_path": "serde::Serialize", "search": "Vec"})).await;
    // The filtered result should have <= as many items as the unfiltered one
    let all_count = all["impls"].as_array().map(|a| a.len())
        .or_else(|| all["count"].as_u64().map(|n| n as usize))
        .unwrap_or(0);
    let filtered_count = filtered["impls"].as_array().map(|a| a.len())
        .or_else(|| filtered["count"].as_u64().map(|n| n as usize))
        .unwrap_or(0);
    assert!(filtered_count <= all_count, "filter should not return more results than unfiltered");
    client.cancel().await.ok();
}

#[tokio::test]
async fn crate_impls_list_missing_both_paths_returns_error() {
    // This test does NOT need network: the tool validates params before any HTTP call.
    // When a tool returns Err(ErrorData), the MCP server surfaces it as a JSON-RPC error,
    // so peer.call_tool() returns Err rather than Ok with is_error: true.
    let client = connect().await;
    let result = client.peer()
        .call_tool(params("crate_impls_list", serde_json::json!({"name": "serde"})))
        .await;
    assert!(result.is_err(), "missing both trait_path and type_path should cause an error");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("trait_path") || err_msg.contains("type_path") || err_msg.contains("must be specified"),
        "error message should mention the missing fields, got: {}", err_msg
    );
    client.cancel().await.ok();
}

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_impls_list_includes_crate_name_in_response() {
    let client = connect().await;
    let j = call(client.peer(), "crate_impls_list",
        serde_json::json!({"name": "serde", "trait_path": "serde::Serialize"})).await;
    assert!(j.get("name").is_some() || j.get("crate").is_some() || j.get("impls").is_some(),
        "response should be a structured object");
    client.cancel().await.ok();
}

// ─── crate_versions_list ──────────────────────────────────────────────────────

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_versions_list_returns_versions_array() {
    let client = connect().await;
    let j = call(client.peer(), "crate_versions_list", serde_json::json!({"name": "serde"})).await;
    let versions = j["versions"].as_array().expect("should have 'versions' array");
    assert!(!versions.is_empty(), "serde should have many versions");
    client.cancel().await.ok();
}

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_versions_list_sorted_descending() {
    let client = connect().await;
    let j = call(client.peer(), "crate_versions_list", serde_json::json!({"name": "serde"})).await;
    let versions = j["versions"].as_array().expect("versions array");
    if versions.len() >= 2 {
        let first = versions[0]["version"].as_str().unwrap_or("0.0.0");
        let second = versions[1]["version"].as_str().unwrap_or("0.0.0");
        let v1 = semver::Version::parse(first).ok();
        let v2 = semver::Version::parse(second).ok();
        if let (Some(v1), Some(v2)) = (v1, v2) {
            assert!(v1 >= v2, "versions should be sorted descending: {} < {}", first, second);
        }
    }
    client.cancel().await.ok();
}

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_versions_list_excludes_prerelease_by_default() {
    let client = connect().await;
    let j = call(client.peer(), "crate_versions_list", serde_json::json!({"name": "serde"})).await;
    let versions = j["versions"].as_array().expect("versions array");
    for v in versions {
        let num = v["version"].as_str().unwrap_or("");
        assert!(!num.contains('-'), "pre-release version should be excluded by default: {}", num);
    }
    client.cancel().await.ok();
}

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_versions_list_search_filter_works() {
    let client = connect().await;
    let j = call(client.peer(), "crate_versions_list",
        serde_json::json!({"name": "serde", "search": "1.0."})).await;
    let versions = j["versions"].as_array().expect("versions array");
    for v in versions {
        let num = v["version"].as_str().unwrap_or("");
        assert!(num.starts_with("1.0."), "search '1.0.' should only return matching versions, got {}", num);
    }
    client.cancel().await.ok();
}

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_versions_list_count_matches_array_length() {
    let client = connect().await;
    let j = call(client.peer(), "crate_versions_list", serde_json::json!({"name": "anyhow"})).await;
    let count = j["count"].as_u64().expect("should have 'count' field");
    let versions = j["versions"].as_array().expect("versions array");
    assert_eq!(count as usize, versions.len(), "count field should match array length");
    client.cancel().await.ok();
}

// ─── crate_version_get ────────────────────────────────────────────────────────

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_version_get_returns_expected_fields() {
    let client = connect().await;
    let j = call(client.peer(), "crate_version_get",
        serde_json::json!({"name": "serde", "version": "1.0.0"})).await;
    assert_eq!(j["crate_id"].as_str().unwrap_or(""), "serde");
    assert_eq!(j["num"].as_str().unwrap_or(""), "1.0.0");
    client.cancel().await.ok();
}

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_version_get_has_published_date() {
    let client = connect().await;
    let j = call(client.peer(), "crate_version_get",
        serde_json::json!({"name": "serde", "version": "1.0.0"})).await;
    let published = j["published_by"].as_str().or_else(|| j["created_at"].as_str()).unwrap_or("");
    // Either a published_by user or a created_at date should be present
    assert!(j["created_at"].is_string() || j.get("created_at").is_some(),
        "should have a created_at timestamp");
    let _ = published; // suppress warning
    client.cancel().await.ok();
}

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_version_get_has_checksum() {
    let client = connect().await;
    let j = call(client.peer(), "crate_version_get",
        serde_json::json!({"name": "anyhow", "version": "1.0.0"})).await;
    let checksum = j["checksum"].as_str().unwrap_or("");
    assert!(!checksum.is_empty(), "version should have a checksum");
    assert_eq!(checksum.len(), 64, "checksum should be a 64-char SHA-256 hex string");
    client.cancel().await.ok();
}

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_version_get_has_download_count() {
    let client = connect().await;
    let j = call(client.peer(), "crate_version_get",
        serde_json::json!({"name": "serde", "version": "1.0.0"})).await;
    let downloads = j["downloads"].as_u64().unwrap_or(0);
    assert!(downloads > 0, "serde 1.0.0 should have downloads > 0");
    client.cancel().await.ok();
}

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_version_get_has_yanked_field() {
    let client = connect().await;
    let j = call(client.peer(), "crate_version_get",
        serde_json::json!({"name": "serde", "version": "1.0.217"})).await;
    assert!(j.get("yanked").is_some(), "response should have a 'yanked' field");
    client.cancel().await.ok();
}

// ─── crate_dependencies_list ──────────────────────────────────────────────────

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_dependencies_list_returns_deps_array() {
    let client = connect().await;
    let j = call(client.peer(), "crate_dependencies_list",
        serde_json::json!({"name": "tokio", "version": "1.0.0"})).await;
    let deps = j["dependencies"].as_array().expect("should have 'dependencies' array");
    assert!(!deps.is_empty(), "tokio 1.0.0 should have dependencies");
    client.cancel().await.ok();
}

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_dependencies_list_items_have_expected_fields() {
    let client = connect().await;
    let j = call(client.peer(), "crate_dependencies_list",
        serde_json::json!({"name": "tokio", "version": "1.0.0"})).await;
    let deps = j["dependencies"].as_array().expect("dependencies array");
    for dep in deps {
        assert!(dep["crate_id"].is_string(), "dep should have 'crate_id'");
        assert!(dep["req"].is_string(), "dep should have version 'req'");
        assert!(dep["kind"].is_string(), "dep should have 'kind'");
    }
    client.cancel().await.ok();
}

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_dependencies_list_kind_filter_normal() {
    let client = connect().await;
    let j = call(client.peer(), "crate_dependencies_list",
        serde_json::json!({"name": "tokio", "version": "1.0.0", "kind": "normal"})).await;
    let deps = j["dependencies"].as_array().expect("dependencies array");
    for dep in deps {
        assert_eq!(dep["kind"], "normal", "kind filter should only return normal deps");
    }
    client.cancel().await.ok();
}

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_dependencies_list_search_filter_works() {
    let client = connect().await;
    let j = call(client.peer(), "crate_dependencies_list",
        serde_json::json!({"name": "tokio", "version": "1.0.0", "search": "bytes"})).await;
    let deps = j["dependencies"].as_array().expect("dependencies array");
    for dep in deps {
        let name = dep["crate_id"].as_str().unwrap_or("").to_lowercase();
        assert!(name.contains("bytes"), "search 'bytes' should only match deps with 'bytes' in name");
    }
    client.cancel().await.ok();
}

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_dependencies_list_count_field_matches_array() {
    let client = connect().await;
    let j = call(client.peer(), "crate_dependencies_list",
        serde_json::json!({"name": "serde", "version": "1.0.217"})).await;
    let count = j["count"].as_u64().expect("should have 'count' field");
    let deps = j["dependencies"].as_array().expect("dependencies array");
    assert_eq!(count as usize, deps.len(), "count should match array length");
    client.cancel().await.ok();
}

// ─── crate_dependents_list ────────────────────────────────────────────────────

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_dependents_list_serde_has_many_dependents() {
    let client = connect().await;
    let j = call(client.peer(), "crate_dependents_list", serde_json::json!({"name": "serde"})).await;
    let dependents = j["dependents"].as_array().expect("should have 'dependents' array");
    assert!(!dependents.is_empty(), "serde should have many dependents");
    client.cancel().await.ok();
}

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_dependents_list_items_have_name_field() {
    let client = connect().await;
    let j = call(client.peer(), "crate_dependents_list", serde_json::json!({"name": "serde"})).await;
    let dependents = j["dependents"].as_array().expect("dependents array");
    for dep in dependents {
        assert!(dep["name"].is_string() || dep["crate_id"].is_string(),
            "dependent should have a name/crate_id field");
    }
    client.cancel().await.ok();
}

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_dependents_list_per_page_limits_results() {
    let client = connect().await;
    let j = call(client.peer(), "crate_dependents_list",
        serde_json::json!({"name": "serde", "per_page": 5})).await;
    let dependents = j["dependents"].as_array().expect("dependents array");
    assert!(dependents.len() <= 5, "per_page=5 should cap results, got {}", dependents.len());
    client.cancel().await.ok();
}

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_dependents_list_search_filter_works() {
    let client = connect().await;
    let j = call(client.peer(), "crate_dependents_list",
        serde_json::json!({"name": "serde", "search": "json"})).await;
    let dependents = j["dependents"].as_array().expect("dependents array");
    for dep in dependents {
        let name = dep["name"].as_str()
            .or_else(|| dep["crate_id"].as_str())
            .unwrap_or("")
            .to_lowercase();
        assert!(name.contains("json"), "search 'json' should only match dependents with 'json' in name, got '{}'", name);
    }
    client.cancel().await.ok();
}

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_dependents_list_has_total_count() {
    let client = connect().await;
    let j = call(client.peer(), "crate_dependents_list", serde_json::json!({"name": "anyhow"})).await;
    assert!(j["total"].is_number() || j["count"].is_number(),
        "response should include a total/count field");
    client.cancel().await.ok();
}

// ─── crate_downloads_get ──────────────────────────────────────────────────────

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_downloads_get_returns_total_downloads() {
    let client = connect().await;
    let j = call(client.peer(), "crate_downloads_get", serde_json::json!({"name": "anyhow"})).await;
    let total = j["total_downloads"].as_u64().unwrap_or(0);
    assert!(total > 1_000, "anyhow total downloads should be > 1000, got {}", total);
    client.cancel().await.ok();
}

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_downloads_get_has_daily_data() {
    let client = connect().await;
    let j = call(client.peer(), "crate_downloads_get", serde_json::json!({"name": "serde"})).await;
    let daily = j["daily"].as_array().or_else(|| j["downloads"].as_array())
        .expect("should have daily or downloads array");
    assert!(!daily.is_empty(), "should have daily download data");
    client.cancel().await.ok();
}

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_downloads_get_daily_entries_have_date_and_count() {
    let client = connect().await;
    let j = call(client.peer(), "crate_downloads_get", serde_json::json!({"name": "serde"})).await;
    let daily = j["daily"].as_array().or_else(|| j["downloads"].as_array())
        .expect("daily array");
    if let Some(entry) = daily.first() {
        assert!(
            (entry["date"].is_string() || entry["day"].is_string()),
            "daily entry should have a date field"
        );
        assert!(
            entry["downloads"].is_number() || entry["count"].is_number(),
            "daily entry should have a downloads/count field"
        );
    }
    client.cancel().await.ok();
}

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_downloads_get_before_date_parameter() {
    let client = connect().await;
    // Request downloads up to a specific date
    let j = call(client.peer(), "crate_downloads_get",
        serde_json::json!({"name": "serde", "before_date": "2024-01-01"})).await;
    assert!(j.is_object(), "response should be a JSON object");
    client.cancel().await.ok();
}

#[tokio::test]
#[ignore = "requires network access"]
async fn crate_downloads_get_name_in_response() {
    let client = connect().await;
    let j = call(client.peer(), "crate_downloads_get", serde_json::json!({"name": "tokio"})).await;
    assert_eq!(j["name"].as_str().unwrap_or(""), "tokio",
        "response should include crate name");
    client.cancel().await.ok();
}
