use std::sync::Arc;

use rmcp::{
    ErrorData as McpError,
    ServerHandler,
    handler::server::{
        router::tool::ToolRouter,
        wrapper::Parameters,
    },
    model::*,
    tool, tool_handler, tool_router,
};

use crate::tools::{
    AppState,
    crate_list::{self, CrateListParams},
    crate_get::{self, CrateGetParams},
    crate_readme_get::{self, CrateReadmeGetParams},
    crate_docs_get::{self, CrateDocsGetParams},
    crate_item_list::{self, CrateItemListParams},
    crate_item_get::{self, CrateItemGetParams},
    crate_impls_list::{self, CrateImplsListParams},
    crate_versions_list::{self, CrateVersionsListParams},
    crate_version_get::{self, CrateVersionGetParams},
    crate_dependencies_list::{self, CrateDependenciesListParams},
    crate_dependents_list::{self, CrateDependentsListParams},
    crate_downloads_get::{self, CrateDownloadsGetParams},
};

#[derive(Clone)]
pub struct DocsMcpServer {
    tool_router: ToolRouter<DocsMcpServer>,
    state: Arc<AppState>,
}

#[tool_router]
impl DocsMcpServer {
    pub fn new_with_state(state: Arc<AppState>) -> Self {
        Self {
            tool_router: Self::tool_router(),
            state,
        }
    }

    #[tool(description = "Search crates.io by keyword, category, or free-text query. Returns crate summaries ranked by relevance, download count, or recency. Entry point for crate discovery when you don't have a crate name yet.")]
    async fn crate_list(
        &self,
        Parameters(params): Parameters<CrateListParams>,
    ) -> Result<CallToolResult, McpError> {
        crate_list::execute(&self.state, params).await
    }

    #[tool(description = "Get comprehensive metadata for a single crate: description, homepage, repository, download counts, latest stable version, feature flag definitions, and MSRV. Combines crates.io API with the sparse index for authoritative feature map.")]
    async fn crate_get(
        &self,
        Parameters(params): Parameters<CrateGetParams>,
    ) -> Result<CallToolResult, McpError> {
        crate_get::execute(&self.state, params).await
    }

    #[tool(description = "Fetch the crate's README for a specific version as readable text. Contains the author's intended narrative: why the crate exists, how it compares to alternatives, installation instructions, and quick-start examples.")]
    async fn crate_readme_get(
        &self,
        Parameters(params): Parameters<CrateReadmeGetParams>,
    ) -> Result<CallToolResult, McpError> {
        crate_readme_get::execute(&self.state, params).await
    }

    #[tool(description = "Get high-level documentation structure from rustdoc JSON: the crate-level //! documentation (architecture overview, feature table, usage examples), module tree, and per-module item summaries. Primary entry point for understanding a library you're already using.")]
    async fn crate_docs_get(
        &self,
        Parameters(params): Parameters<CrateDocsGetParams>,
    ) -> Result<CallToolResult, McpError> {
        crate_docs_get::execute(&self.state, params).await
    }

    #[tool(description = "Search for items (types, functions, traits, etc.) within a crate's API by name or concept. Returns ranked results with signatures and doc summaries. Use after crate_docs_get to find specific items without browsing the module tree.")]
    async fn crate_item_list(
        &self,
        Parameters(params): Parameters<CrateItemListParams>,
    ) -> Result<CallToolResult, McpError> {
        crate_item_list::execute(&self.state, params).await
    }

    #[tool(description = "Get complete documentation for a specific item by fully-qualified path. Returns the full doc comment, exact type signature, generic parameters, where clauses, inherent methods, implemented traits, and feature flags. Primary API reference tool.")]
    async fn crate_item_get(
        &self,
        Parameters(params): Parameters<CrateItemGetParams>,
    ) -> Result<CallToolResult, McpError> {
        crate_item_get::execute(&self.state, params).await
    }

    #[tool(description = "Find implementors of a trait, or all traits implemented by a type. Answers: 'what do I need to implement to use this abstraction?' and 'what can I call on this type?' Specify either trait_path or type_path.")]
    async fn crate_impls_list(
        &self,
        Parameters(params): Parameters<CrateImplsListParams>,
    ) -> Result<CallToolResult, McpError> {
        crate_impls_list::execute(&self.state, params).await
    }

    #[tool(description = "List all published versions with feature maps, MSRV, dependency counts, and yank status. Use to understand release history, find when a feature was introduced, audit yanked versions, or compare features across versions.")]
    async fn crate_versions_list(
        &self,
        Parameters(params): Parameters<CrateVersionsListParams>,
    ) -> Result<CallToolResult, McpError> {
        crate_versions_list::execute(&self.state, params).await
    }

    #[tool(description = "Get rich per-version metadata from crates.io: Rust edition, library vs binary targets, binary names, line counts, license, and publisher. Use after crate_versions_list when you need details beyond what the index provides.")]
    async fn crate_version_get(
        &self,
        Parameters(params): Parameters<CrateVersionGetParams>,
    ) -> Result<CallToolResult, McpError> {
        crate_version_get::execute(&self.state, params).await
    }

    #[tool(description = "Get the dependency list for a specific crate version with semver requirements, optional flags, enabled features, and target conditions. Use for due diligence: a large or unusual dependency tree is a risk multiplier.")]
    async fn crate_dependencies_list(
        &self,
        Parameters(params): Parameters<CrateDependenciesListParams>,
    ) -> Result<CallToolResult, McpError> {
        crate_dependencies_list::execute(&self.state, params).await
    }

    #[tool(description = "List crates that depend on a given crate (reverse dependencies). Reveals ecosystem adoption breadth. A crate trusted by 5000 other crates has a different risk profile than one with 20. Use for due diligence.")]
    async fn crate_dependents_list(
        &self,
        Parameters(params): Parameters<CrateDependentsListParams>,
    ) -> Result<CallToolResult, McpError> {
        crate_dependents_list::execute(&self.state, params).await
    }

    #[tool(description = "Get per-day download counts broken out by version for the past 90 days. Use to assess active ecosystem adoption, whether users have migrated to newer versions, and whether a download spike indicates recent adoption by a major project.")]
    async fn crate_downloads_get(
        &self,
        Parameters(params): Parameters<CrateDownloadsGetParams>,
    ) -> Result<CallToolResult, McpError> {
        crate_downloads_get::execute(&self.state, params).await
    }
}

#[tool_handler]
impl ServerHandler for DocsMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            server_info: Implementation {
                name: "docs-mcp".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                title: None,
                description: Some("Rust crate documentation MCP server".to_string()),
                icons: None,
                website_url: None,
            },
            instructions: Some(
                "This server provides accurate, up-to-date access to the Rust crate ecosystem.\n\
                \n\
                DISCOVERY WORKFLOW: crate_list → crate_get → crate_readme_get\n\
                UNDERSTANDING WORKFLOW: crate_docs_get → crate_item_list → crate_item_get → crate_impls_list\n\
                DUE DILIGENCE: crate_versions_list → crate_downloads_get → crate_dependents_list → crate_dependencies_list\n\
                \n\
                All tools default to the latest stable version when version is not specified.".to_string()
            ),
        }
    }
}
