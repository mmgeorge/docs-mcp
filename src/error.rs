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
    fn from(error: DocsError) -> Self {
        match error {
            DocsError::DocsNotFound { .. } => {
                rmcp::ErrorData::invalid_params(error.to_string(), None)
            }
            _ => rmcp::ErrorData::internal_error(error.to_string(), None),
        }
    }
}

pub type Result<T> = std::result::Result<T, DocsError>;

#[cfg(test)]
mod tests {
    use super::DocsError;
    use rmcp::{ErrorData, model::ErrorCode};

    #[test]
    fn missing_docs_build_is_invalid_params() {
        let error = DocsError::DocsNotFound {
            name: "example".to_string(),
            version: "1.0.0".to_string(),
        };

        let response = ErrorData::from(error);
        assert_eq!(response.code, ErrorCode::INVALID_PARAMS);
        assert!(response.message.contains("example 1.0.0"));
    }
}
