use reqwest_middleware::ClientWithMiddleware;

use crate::cache::DiskCache;
use crate::error::{DocsError, Result};
use super::types::RustdocJson;

const DOCSRS_BASE: &str = "https://docs.rs";

/// Fetch the rustdoc JSON for a crate from docs.rs.
///
/// Returns `Err(DocsError::DocsNotFound)` if docs.rs has no successful build.
pub async fn fetch_rustdoc_json(
    name: &str,
    version: &str,
    client: &ClientWithMiddleware,
    cache: &DiskCache,
) -> Result<RustdocJson> {
    let url = format!("{DOCSRS_BASE}/crate/{name}/{version}/json");

    // HEAD check first to avoid downloading a large file that 404s
    let exists = cache.head_check(client, &url).await?;
    if !exists {
        return Err(DocsError::DocsNotFound {
            name: name.to_string(),
            version: version.to_string(),
        });
    }

    let doc: RustdocJson = cache.get_zstd_json(client, &url).await?;

    if doc.format_version < 33 {
        return Err(DocsError::Other(format!(
            "Unsupported rustdoc JSON format version: {}. Expected >= 33.",
            doc.format_version
        )));
    }

    Ok(doc)
}

/// Check if a docs.rs build exists for a crate version (HEAD request only).
pub async fn docs_exist(
    name: &str,
    version: &str,
    client: &ClientWithMiddleware,
    cache: &DiskCache,
) -> Result<bool> {
    let url = format!("{DOCSRS_BASE}/crate/{name}/{version}/json");
    cache.head_check(client, &url).await
}
