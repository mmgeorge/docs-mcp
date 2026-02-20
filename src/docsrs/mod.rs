pub mod client;
pub mod parser;
pub mod types;

pub use client::{fetch_rustdoc_json, docs_exist};
pub use parser::{
    type_to_string, function_signature, extract_feature_requirements,
    format_generics_for_item,
    build_module_tree, search_items, ModuleNode, ItemSummary, SearchResult,
};
pub use types::{RustdocJson, Item, PathEntry, Deprecation, Span};
