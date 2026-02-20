use thiserror::Error;

#[derive(Debug, Error)]
pub enum DocsError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("HTTP middleware error: {0}")]
    Middleware(#[from] reqwest_middleware::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Crate not found: {0}")]
    CrateNotFound(String),

    #[error("Docs.rs build not found for {name} {version}")]
    DocsNotFound { name: String, version: String },

    #[error("No stable version found for {0}")]
    NoStableVersion(String),

    #[error("Semver error: {0}")]
    Semver(#[from] semver::Error),

    #[error("{0}")]
    Other(String),
}

impl From<DocsError> for rmcp::ErrorData {
    fn from(e: DocsError) -> Self {
        rmcp::ErrorData::internal_error(e.to_string(), None)
    }
}

pub type Result<T> = std::result::Result<T, DocsError>;
