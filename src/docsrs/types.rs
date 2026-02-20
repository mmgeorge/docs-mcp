use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Top-level rustdoc JSON document (format version 57).
#[derive(Debug, Deserialize, Serialize)]
pub struct RustdocJson {
    pub format_version: u32,
    /// The ID of the crate root item (integer in v57 JSON)
    pub root: serde_json::Value,
    /// All items, keyed by their ID (string keys in JSON)
    pub index: HashMap<String, Item>,
    /// Path info for items, keyed by item ID (string keys in JSON)
    pub paths: HashMap<String, PathEntry>,
    /// External crates referenced
    #[serde(default)]
    pub external_crates: HashMap<String, ExternalCrate>,
    /// Crate name
    pub crate_version: Option<String>,
}

impl RustdocJson {
    /// Get the root ID as a string (handles both integer and string JSON representations).
    pub fn root_id(&self) -> String {
        match &self.root {
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        }
    }
}

/// A path entry describing an item's location in the module tree.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PathEntry {
    /// Item kind string, e.g. "module", "struct", "enum", "function", etc.
    pub kind: String,
    /// Components of the fully-qualified path
    pub path: Vec<String>,
    /// Brief summary (first line of docs)
    pub summary: Option<String>,
}

impl PathEntry {
    pub fn kind_name(&self) -> &str {
        &self.kind
    }

    pub fn full_path(&self) -> String {
        self.path.join("::")
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ExternalCrate {
    pub name: String,
    pub html_root_url: Option<String>,
}

/// A single item in the rustdoc JSON index.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Item {
    pub id: serde_json::Value,
    pub name: Option<String>,
    pub docs: Option<String>,
    #[serde(default)]
    pub attrs: Vec<serde_json::Value>,
    pub deprecation: Option<Deprecation>,
    /// Tagged union: {"function": {...}}, {"struct": {...}}, {"module": {...}}, etc.
    pub inner: Value,
    pub span: Option<Span>,
    pub visibility: Option<Value>,
    pub links: Option<HashMap<String, serde_json::Value>>,
}

impl Item {
    /// Returns the kind string from `inner`, e.g. "function", "struct", "module".
    pub fn kind(&self) -> Option<&str> {
        self.inner.as_object()?.keys().next().map(|s| s.as_str())
    }

    /// Returns `inner[kind]` for a given kind string.
    pub fn inner_for(&self, kind: &str) -> Option<&Value> {
        self.inner.get(kind)
    }

    /// Extract attribute strings from the v57 `attrs` array.
    /// Each element is `{"other": "#[...]"}` — returns the inner string values.
    pub fn attr_strings(&self) -> Vec<String> {
        self.attrs.iter().filter_map(|v| {
            v.get("other")?.as_str().map(|s| s.to_string())
        }).collect()
    }

    /// Doc summary: first non-empty line of the doc comment.
    pub fn doc_summary(&self) -> String {
        self.docs
            .as_deref()
            .unwrap_or("")
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("")
            .to_string()
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Deprecation {
    pub since: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Span {
    pub filename: String,
    pub begin: (u32, u32),
    pub end: (u32, u32),
}
