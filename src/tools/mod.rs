use std::sync::Arc;

use async_trait::async_trait;
use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
use http::Extensions;
use nonzero_ext::nonzero;
use reqwest::Request;
use reqwest_middleware::{Middleware, Next};

use crate::cache::DiskCache;
use crate::error::Result;
use crate::sparse_index::{self, IndexLine};

pub mod crate_list;
pub mod crate_get;
pub mod crate_readme_get;
pub mod crate_docs_get;
pub mod crate_item_list;
pub mod crate_item_get;
pub mod crate_impls_list;
pub mod crate_versions_list;
pub mod crate_version_get;
pub mod crate_dependencies_list;
pub mod crate_dependents_list;
pub mod crate_downloads_get;

/// Shared application state, held behind an Arc in the server.
pub struct AppState {
    pub client: reqwest_middleware::ClientWithMiddleware,
    pub cache: DiskCache,
}

impl AppState {
    pub async fn new() -> Result<Self> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::USER_AGENT,
            reqwest::header::HeaderValue::from_static(
                "docs-mcp/0.1 (https://github.com/user/docs-mcp)",
            ),
        );

        let http = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .map_err(crate::error::DocsError::Http)?;

        let rate_mw = RateLimitMiddleware::new();
        let cache = DiskCache::new()?;

        let client = reqwest_middleware::ClientBuilder::new(http)
            .with(rate_mw)
            .build();

        Ok(Self { client, cache })
    }

    /// Resolve a version string: if None or "latest", look up the latest stable version.
    pub async fn resolve_version(&self, name: &str, version: Option<&str>) -> Result<String> {
        match version {
            Some(v) if !v.is_empty() && v != "latest" => Ok(v.to_string()),
            _ => {
                let lines = sparse_index::fetch_index(name, &self.client, &self.cache).await?;
                let latest = sparse_index::find_latest_stable(&lines)
                    .ok_or_else(|| crate::error::DocsError::NoStableVersion(name.to_string()))?;
                Ok(latest.vers.clone())
            }
        }
    }

    /// Fetch all index lines for a crate.
    pub async fn fetch_index(&self, name: &str) -> Result<Vec<IndexLine>> {
        sparse_index::fetch_index(name, &self.client, &self.cache).await
    }
}

// ─── Rate limit middleware ─────────────────────────────────────────────────────

pub struct RateLimitMiddleware {
    limiter: Arc<DefaultDirectRateLimiter>,
}

impl RateLimitMiddleware {
    pub fn new() -> Self {
        let quota = Quota::per_second(nonzero!(1u32));
        let limiter = Arc::new(RateLimiter::direct(quota));
        Self { limiter }
    }
}

#[async_trait]
impl Middleware for RateLimitMiddleware {
    async fn handle(
        &self,
        req: Request,
        extensions: &mut Extensions,
        next: Next<'_>,
    ) -> reqwest_middleware::Result<reqwest::Response> {
        // Only rate limit crates.io API calls (not sparse index or docs.rs)
        if req.url().host_str() == Some("crates.io") {
            self.limiter.until_ready().await;
        }
        next.run(req, extensions).await
    }
}
