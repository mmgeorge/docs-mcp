use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use directories::ProjectDirs;
use hex::encode as hex_encode;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{DocsError, Result};

const CACHE_TTL_SECS: u64 = 24 * 60 * 60; // 1 day

#[derive(Serialize, Deserialize)]
struct CacheEntry {
    cached_at: u64, // Unix timestamp (secs)
    url: String,
    body: String, // JSON body as string
}

pub struct DiskCache {
    cache_dir: PathBuf,
}

impl DiskCache {
    pub fn new() -> Result<Self> {
        let cache_dir = resolve_cache_dir()?;
        std::fs::create_dir_all(&cache_dir)?;
        let cache = Self { cache_dir };
        cache.prune_expired()?;
        Ok(cache)
    }

    fn cache_path(&self, key: &str) -> PathBuf {
        self.cache_dir.join(format!("{key}.json"))
    }

    fn cache_key(url: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(url.as_bytes());
        hex_encode(hasher.finalize())
    }

    pub async fn get_json<T>(&self, client: &reqwest_middleware::ClientWithMiddleware, url: &str) -> Result<T>
    where
        T: serde::de::DeserializeOwned,
    {
        let key = Self::cache_key(url);
        let path = self.cache_path(&key);

        if let Some(body) = self.read_valid_cache(&path)? {
            return serde_json::from_str(&body).map_err(DocsError::Json);
        }

        let resp = client.get(url).send().await?;
        if !resp.status().is_success() {
            return Err(DocsError::Other(format!(
                "HTTP {} for {}",
                resp.status(),
                url
            )));
        }
        let body = resp.text().await?;
        let value = serde_json::from_str(&body).map_err(DocsError::Json)?;
        self.write_cache(&path, url, &body)?;
        Ok(value)
    }

    /// Download a zstd-compressed JSON file and return the deserialized value.
    ///
    /// docs.rs serves rustdoc JSON as `Content-Type: application/zstd` bodies.
    /// The decompressed JSON text is cached so repeat calls skip the download.
    pub async fn get_zstd_json<T>(&self, client: &reqwest_middleware::ClientWithMiddleware, url: &str) -> Result<T>
    where
        T: serde::de::DeserializeOwned,
    {
        let key = Self::cache_key(url);
        let path = self.cache_path(&key);

        if let Some(body) = self.read_valid_cache(&path)? {
            return serde_json::from_str(&body).map_err(DocsError::Json);
        }

        let resp = client.get(url).send().await?;
        if !resp.status().is_success() {
            return Err(DocsError::Other(format!(
                "HTTP {} for {}",
                resp.status(),
                url
            )));
        }
        let bytes = resp.bytes().await?;
        let body = decompress_zstd(&bytes)?;
        let value = serde_json::from_str(&body).map_err(DocsError::Json)?;
        self.write_cache(&path, url, &body)?;
        Ok(value)
    }

    pub async fn get_text(&self, client: &reqwest_middleware::ClientWithMiddleware, url: &str) -> Result<String> {
        let key = Self::cache_key(url);
        let path = self.cache_path(&key);

        if let Some(body) = self.read_valid_cache(&path)? {
            // body was stored as JSON string, decode it
            return serde_json::from_str::<String>(&body).map_err(DocsError::Json);
        }

        let resp = client.get(url).send().await?;
        if !resp.status().is_success() {
            return Err(DocsError::Other(format!(
                "HTTP {} for {}",
                resp.status(),
                url
            )));
        }
        let text = resp.text().await?;
        // Store text as JSON string
        let body = serde_json::to_string(&text)?;
        self.write_cache(&path, url, &body)?;
        Ok(text)
    }

    /// Returns true if URL returns success (200), false for 404, error for other failures.
    pub async fn head_check(&self, client: &reqwest_middleware::ClientWithMiddleware, url: &str) -> Result<bool> {
        let resp = client.head(url).send().await?;
        Ok(resp.status().is_success())
    }

    fn read_valid_cache(&self, path: &Path) -> Result<Option<String>> {
        if !path.exists() {
            return Ok(None);
        }
        let raw = std::fs::read_to_string(path)?;
        let entry: CacheEntry = match serde_json::from_str(&raw) {
            Ok(e) => e,
            Err(_) => {
                let _ = std::fs::remove_file(path);
                return Ok(None);
            }
        };
        let now = unix_now();
        if now.saturating_sub(entry.cached_at) > CACHE_TTL_SECS {
            let _ = std::fs::remove_file(path);
            return Ok(None);
        }
        Ok(Some(entry.body))
    }

    fn write_cache(&self, path: &Path, url: &str, body: &str) -> Result<()> {
        let entry = CacheEntry {
            cached_at: unix_now(),
            url: url.to_string(),
            body: body.to_string(),
        };
        let raw = serde_json::to_string(&entry)?;
        std::fs::write(path, raw)?;
        Ok(())
    }

    fn prune_expired(&self) -> Result<()> {
        let now = unix_now();
        let Ok(entries) = std::fs::read_dir(&self.cache_dir) else {
            return Ok(());
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Ok(raw) = std::fs::read_to_string(&path) {
                if let Ok(entry) = serde_json::from_str::<CacheEntry>(&raw) {
                    if now.saturating_sub(entry.cached_at) > CACHE_TTL_SECS {
                        let _ = std::fs::remove_file(&path);
                    }
                }
            }
        }
        Ok(())
    }
}

/// Decompress a zstd-compressed byte slice and return it as a UTF-8 string.
///
/// docs.rs serves rustdoc JSON as `Content-Type: application/zstd` with a
/// `.json.zst` filename. This decompresses the raw bytes to a JSON string.
pub fn decompress_zstd(bytes: &[u8]) -> Result<String> {
    let decompressed = zstd::decode_all(std::io::Cursor::new(bytes))
        .map_err(|e| DocsError::Other(format!("Zstd decompression failed: {e}")))?;
    String::from_utf8(decompressed)
        .map_err(|e| DocsError::Other(format!("Decompressed content is not valid UTF-8: {e}")))
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs()
}

fn resolve_cache_dir() -> Result<PathBuf> {
    if let Some(dirs) = ProjectDirs::from("", "", "docs-mcp") {
        Ok(dirs.cache_dir().to_path_buf())
    } else {
        // Fallback to current directory
        Ok(PathBuf::from(".cache/docs-mcp"))
    }
}
