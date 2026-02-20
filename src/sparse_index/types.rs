use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// A single entry in the crates.io sparse index (one line of NDJSON per version).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct IndexLine {
    /// Crate name (may be mixed-case but normalized)
    pub name: String,
    /// Version string
    pub vers: String,
    /// Dependencies
    #[serde(default)]
    pub deps: Vec<DepEntry>,
    /// SHA-256 checksum of the .crate file
    pub cksum: String,
    /// Feature map: feature_name -> list of dep features enabled
    #[serde(default)]
    pub features: HashMap<String, Vec<String>>,
    /// Whether this version is yanked
    #[serde(default)]
    pub yanked: bool,
    /// Minimum Rust version (MSRV)
    pub rust_version: Option<String>,
    /// v2 features (merged with features)
    pub features2: Option<HashMap<String, Vec<String>>>,
}

impl IndexLine {
    /// Merged features (features + features2)
    pub fn all_features(&self) -> HashMap<String, Vec<String>> {
        let mut merged = self.features.clone();
        if let Some(f2) = &self.features2 {
            merged.extend(f2.clone());
        }
        merged
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DepEntry {
    pub name: String,
    pub req: String,
    /// The renamed package name (if any)
    pub package: Option<String>,
    pub kind: Option<DepKind>,
    #[serde(default)]
    pub optional: bool,
    #[serde(default)]
    pub default_features: bool,
    #[serde(default)]
    pub features: Vec<String>,
    pub target: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DepKind {
    Normal,
    Dev,
    Build,
}

/// Compute the sparse index path for a crate name.
///
/// Rules:
/// - 1 char:  `1/{name}`
/// - 2 chars: `2/{name}`
/// - 3 chars: `3/{first}/{name}`
/// - 4+ chars: `{first2}/{next2}/{name}`
pub fn compute_path(name: &str) -> String {
    let n = name.to_lowercase();
    match n.len() {
        0 => panic!("empty crate name"),
        1 => format!("1/{n}"),
        2 => format!("2/{n}"),
        3 => format!("3/{}/{n}", &n[0..1]),
        _ => format!("{}/{}/{n}", &n[0..2], &n[2..4]),
    }
}

/// Find the latest stable version from a list of index lines.
///
/// - Filters out yanked versions
/// - Filters out pre-release versions (any version string containing `-`)
/// - Returns the highest semver among the remainder
/// - If no stable version exists, falls back to highest non-yanked version of any kind
pub fn find_latest_stable(lines: &[IndexLine]) -> Option<&IndexLine> {
    use semver::Version;

    // Try stable first
    let stable: Vec<&IndexLine> = lines
        .iter()
        .filter(|l| !l.yanked && !l.vers.contains('-'))
        .collect();

    if !stable.is_empty() {
        return stable
            .into_iter()
            .max_by_key(|l| Version::parse(&l.vers).ok());
    }

    // Fall back to any non-yanked version
    lines
        .iter()
        .filter(|l| !l.yanked)
        .max_by_key(|l| Version::parse(&l.vers).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_path_1_char() {
        assert_eq!(compute_path("a"), "1/a");
    }

    #[test]
    fn test_compute_path_2_chars() {
        assert_eq!(compute_path("io"), "2/io");
    }

    #[test]
    fn test_compute_path_3_chars() {
        assert_eq!(compute_path("url"), "3/u/url");
    }

    #[test]
    fn test_compute_path_4_plus_chars() {
        assert_eq!(compute_path("serde"), "se/rd/serde");
    }

    #[test]
    fn test_compute_path_uppercase() {
        assert_eq!(compute_path("SERDE"), "se/rd/serde");
    }

    #[test]
    fn test_find_latest_stable_ignores_yanked() {
        let lines = vec![
            make_line("1.0.0", false, false),
            make_line("1.1.0", true, false), // yanked
            make_line("0.9.0", false, false),
        ];
        let latest = find_latest_stable(&lines).unwrap();
        assert_eq!(latest.vers, "1.0.0");
    }

    #[test]
    fn test_find_latest_stable_ignores_prerelease() {
        let lines = vec![
            make_line("1.0.0", false, false),
            make_line("1.1.0-alpha.1", false, true),
            make_line("0.9.0", false, false),
        ];
        let latest = find_latest_stable(&lines).unwrap();
        assert_eq!(latest.vers, "1.0.0");
    }

    #[test]
    fn test_find_latest_stable_fallback_to_prerelease() {
        let lines = vec![
            make_line("1.0.0-alpha.1", false, true),
            make_line("0.9.0-beta.1", false, true),
        ];
        let latest = find_latest_stable(&lines).unwrap();
        assert_eq!(latest.vers, "1.0.0-alpha.1");
    }

    fn make_line(vers: &str, yanked: bool, _is_pre: bool) -> IndexLine {
        IndexLine {
            name: "test".to_string(),
            vers: vers.to_string(),
            deps: vec![],
            cksum: "abc".to_string(),
            features: Default::default(),
            yanked,
            rust_version: None,
            features2: None,
        }
    }
}
