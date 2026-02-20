use reqwest_middleware::ClientWithMiddleware;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use crate::cache::DiskCache;
use crate::error::{DocsError, Result};

const CRATESIO_BASE: &str = "https://crates.io/api/v1";

// ─── Response types ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CrateInfo {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub homepage: Option<String>,
    pub documentation: Option<String>,
    pub repository: Option<String>,
    pub downloads: u64,
    pub recent_downloads: Option<u64>,
    pub created_at: String,
    pub updated_at: String,
    pub max_stable_version: Option<String>,
    pub max_version: Option<String>,
    pub newest_version: Option<String>,
    pub links: Option<Value>,
    pub categories: Option<Vec<String>>,
    pub keywords: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CrateResponse {
    #[serde(rename = "crate")]
    pub krate: CrateInfo,
    pub versions: Option<Vec<VersionInfo>>,
    pub keywords: Option<Vec<Keyword>>,
    pub categories: Option<Vec<Category>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct VersionInfo {
    pub id: u64,
    pub num: String,
    pub crate_id: Option<String>,  // sometimes missing
    pub dl_path: Option<String>,
    pub readme_path: Option<String>,
    pub license: Option<String>,
    pub edition: Option<String>,
    pub rust_version: Option<String>,
    pub has_lib: Option<bool>,
    pub bins: Option<Vec<String>>,
    pub crate_size: Option<u64>,
    pub downloads: u64,
    pub yanked: bool,
    pub yank_message: Option<String>,
    pub published_by: Option<Publisher>,
    pub created_at: String,
    pub updated_at: Option<String>,
    pub checksum: Option<String>,
    pub features: Option<HashMap<String, Vec<String>>>,
    pub links: Option<Value>,
    pub lib_links: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Publisher {
    pub id: u64,
    pub login: String,
    pub name: Option<String>,
    pub avatar: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Keyword {
    pub id: String,
    pub keyword: String,
    pub crates_cnt: u64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Category {
    pub id: String,
    pub category: String,
    pub crates_cnt: u64,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SearchResult {
    pub crates: Vec<CrateInfo>,
    pub meta: SearchMeta,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SearchMeta {
    pub total: u64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct VersionsResponse {
    pub versions: Vec<VersionInfo>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DependenciesResponse {
    pub dependencies: Vec<Dependency>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Dependency {
    pub id: Option<u64>,
    pub version_id: Option<u64>,
    pub crate_id: String,
    pub req: String,
    pub optional: bool,
    pub default_features: bool,
    pub features: Vec<String>,
    pub target: Option<String>,
    pub kind: Option<String>,
    pub downloads: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ReverseDepsResponse {
    pub dependencies: Vec<ReverseDep>,
    pub versions: Vec<ReverseDepVersion>,
    pub meta: ReverseDepsMetaSerde,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ReverseDep {
    pub id: u64,
    pub version_id: u64,
    pub crate_id: String,
    pub req: String,
    pub optional: bool,
    pub default_features: bool,
    pub features: Vec<String>,
    pub kind: Option<String>,
    pub downloads: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ReverseDepVersion {
    pub id: u64,
    pub num: String,
    #[serde(rename = "crate")]
    pub crate_name: String,
    pub downloads: u64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ReverseDepsMetaSerde {
    pub total: u64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DownloadsResponse {
    pub version_downloads: Vec<VersionDownload>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct VersionDownload {
    pub version: u64, // version ID
    pub downloads: u64,
    pub date: String,
}

// ─── Client ───────────────────────────────────────────────────────────────────

pub struct CratesIoClient<'a> {
    client: &'a ClientWithMiddleware,
    cache: &'a DiskCache,
}

impl<'a> CratesIoClient<'a> {
    pub fn new(client: &'a ClientWithMiddleware, cache: &'a DiskCache) -> Self {
        Self { client, cache }
    }

    pub async fn search(
        &self,
        query: &str,
        category: Option<&str>,
        keyword: Option<&str>,
        sort: Option<&str>,
        page: u32,
        per_page: u32,
    ) -> Result<SearchResult> {
        let mut url = format!("{CRATESIO_BASE}/crates?q={query}&page={page}&per_page={per_page}");
        if let Some(cat) = category {
            url.push_str(&format!("&category={cat}"));
        }
        if let Some(kw) = keyword {
            url.push_str(&format!("&keyword={kw}"));
        }
        if let Some(s) = sort {
            url.push_str(&format!("&sort={s}"));
        }
        self.cache.get_json(self.client, &url).await
    }

    pub async fn get_crate(&self, name: &str) -> Result<CrateResponse> {
        let url = format!("{CRATESIO_BASE}/crates/{name}");
        self.cache.get_json(self.client, &url).await
    }

    pub async fn get_readme(&self, name: &str, version: &str) -> Result<String> {
        let url = format!("{CRATESIO_BASE}/crates/{name}/{version}/readme");
        // README endpoint returns HTML; we fetch as text
        self.cache.get_text(self.client, &url).await.or_else(|e| {
            Err(DocsError::Other(format!("Failed to fetch README: {e}")))
        })
    }

    pub async fn get_version(&self, name: &str, version: &str) -> Result<VersionInfo> {
        let url = format!("{CRATESIO_BASE}/crates/{name}/{version}");
        #[derive(Deserialize)]
        struct Wrapper {
            version: VersionInfo,
        }
        let w: Wrapper = self.cache.get_json(self.client, &url).await?;
        Ok(w.version)
    }

    pub async fn get_versions(&self, name: &str) -> Result<VersionsResponse> {
        let url = format!("{CRATESIO_BASE}/crates/{name}/versions");
        self.cache.get_json(self.client, &url).await
    }

    pub async fn get_dependencies(&self, name: &str, version: &str) -> Result<DependenciesResponse> {
        let url = format!("{CRATESIO_BASE}/crates/{name}/{version}/dependencies");
        self.cache.get_json(self.client, &url).await
    }

    pub async fn get_reverse_deps(
        &self,
        name: &str,
        page: u32,
        per_page: u32,
    ) -> Result<ReverseDepsResponse> {
        let url = format!("{CRATESIO_BASE}/crates/{name}/reverse_dependencies?page={page}&per_page={per_page}");
        self.cache.get_json(self.client, &url).await
    }

    pub async fn get_downloads(&self, name: &str, before_date: Option<&str>) -> Result<DownloadsResponse> {
        let mut url = format!("{CRATESIO_BASE}/crates/{name}/downloads");
        if let Some(d) = before_date {
            url.push_str(&format!("?before_date={d}"));
        }
        self.cache.get_json(self.client, &url).await
    }
}
