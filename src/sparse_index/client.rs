use reqwest_middleware::ClientWithMiddleware;

use crate::cache::DiskCache;
use crate::error::{DocsError, Result};
use super::types::{IndexLine, compute_path};

const INDEX_BASE: &str = "https://index.crates.io";

/// Fetch all index lines for a crate from the sparse index.
pub async fn fetch_index(
    name: &str,
    client: &ClientWithMiddleware,
    cache: &DiskCache,
) -> Result<Vec<IndexLine>> {
    let path = compute_path(name);
    let url = format!("{INDEX_BASE}/{path}");

    let text = cache.get_text(client, &url).await?;
    parse_ndjson(&text)
}

/// Parse NDJSON (newline-delimited JSON) into a list of IndexLine entries.
pub fn parse_ndjson(text: &str) -> Result<Vec<IndexLine>> {
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).map_err(DocsError::Json))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ndjson_basic() {
        let ndjson = r#"{"name":"serde","vers":"1.0.0","deps":[],"cksum":"abc","features":{},"yanked":false}
{"name":"serde","vers":"1.0.1","deps":[],"cksum":"def","features":{},"yanked":false}
"#;
        let lines = parse_ndjson(ndjson).unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].vers, "1.0.0");
        assert_eq!(lines[1].vers, "1.0.1");
    }

    #[test]
    fn test_parse_ndjson_with_features() {
        let ndjson = r#"{"name":"tokio","vers":"1.0.0","deps":[],"cksum":"abc","features":{"full":["rt","sync","io"]},"yanked":false}"#;
        let lines = parse_ndjson(ndjson).unwrap();
        assert_eq!(lines.len(), 1);
        assert!(lines[0].features.contains_key("full"));
    }
}
