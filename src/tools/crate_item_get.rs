use std::collections::HashSet;

use rmcp::{ErrorData, model::{CallToolResult, Content}};
use serde::Deserialize;
use rmcp::schemars::{self, JsonSchema};
use serde_json::json;

use super::AppState;
use crate::docsrs::{fetch_rustdoc_json, function_signature, extract_feature_requirements};
use crate::docsrs::parser::{type_to_string, format_generics_for_item};
use crate::sparse_index::find_latest_stable;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CrateItemGetParams {
    /// Crate name
    pub name: String,
    /// Version string. Defaults to latest stable.
    pub version: Option<String>,
    /// Fully-qualified item path (e.g. "tokio::sync::Mutex")
    pub item_path: String,
    /// Include inherent methods from impl blocks (default: true)
    pub include_methods: Option<bool>,
    /// Trait impl filtering mode: "filtered" (default) omits ubiquitous blankets like
    /// Borrow/Into/From<T>/Any; "all" returns everything; "none" omits trait impls entirely.
    pub include_trait_impls: Option<String>,
}

pub async fn execute(state: &AppState, params: CrateItemGetParams) -> Result<CallToolResult, ErrorData> {
    let name = &params.name;
    let version = state.resolve_version(name, params.version.as_deref()).await
        .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

    let include_methods = params.include_methods.unwrap_or(true);
    let trait_impl_mode = params.include_trait_impls.as_deref().unwrap_or("filtered");

    let (docs_result, index_result) = tokio::join!(
        fetch_rustdoc_json(name, &version, &state.client, &state.cache),
        state.fetch_index(name)
    );

    let doc = docs_result.map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
    let index_lines = index_result.unwrap_or_default();
    let latest = find_latest_stable(&index_lines);
    let features = latest.map(|l| l.all_features()).unwrap_or_default();
    let declared_features: HashSet<String> = features.keys().cloned().collect();

    // Find item by path — exact match first, then suffix fallback for re-exports
    let target_path = &params.item_path;
    let target_parts: Vec<&str> = target_path.split("::").collect();

    let item_id = doc.paths.iter()
        .find(|(_, p)| p.full_path() == *target_path)
        .or_else(|| {
            // Re-exports: "tokio::sync::Mutex" stored as "tokio::sync::mutex::Mutex"
            // Match via subsequence on non-crate path components (skip first == crate name).
            // "tokio::sync::Mutex" → rest = ["sync", "Mutex"]
            // stored "tokio::sync::mutex::Mutex" → rest = ["sync", "mutex", "Mutex"]
            // ["sync", "Mutex"] is a subsequence of ["sync", "mutex", "Mutex"] ✓
            doc.paths.iter().find(|(_, p)| {
                let parts = &p.path;
                if parts.is_empty() || target_parts.is_empty() { return false; }
                // Crate names must match
                if parts[0] != target_parts[0] { return false; }
                let stored_rest = &parts[1..];
                let target_rest = &target_parts[1..];
                if target_rest.is_empty() { return false; }
                // Check if target_rest is a subsequence of stored_rest
                let mut ti = 0;
                for s in stored_rest {
                    if ti < target_rest.len() && *s == target_rest[ti] {
                        ti += 1;
                    }
                }
                ti == target_rest.len()
            })
        })
        .map(|(id, _)| id.clone());

    let item_id = item_id.ok_or_else(|| {
        // Item not found in doc.paths — check if it's a re-export "use" item in doc.index
        // that points to an external crate (common with facade crates: serde, futures, clap).
        let last_component = target_path.split("::").last().unwrap_or(target_path.as_str());
        let re_export_sources: Vec<String> = doc.index.iter()
            .filter(|(id, item)| {
                !doc.paths.contains_key(*id)
                    && item.name.as_deref() == Some(last_component)
                    && item.kind() == Some("use")
            })
            .filter_map(|(_, item)| {
                item.inner_for("use")
                    .and_then(|u| u.get("source"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
            .take(3)
            .collect();

        if !re_export_sources.is_empty() {
            let sources = re_export_sources.join(", ");
            ErrorData::invalid_params(
                format!("Item '{target_path}' is re-exported in {name} {version} from an \
                         external crate ({sources}). Its full definition is not in the {name} docs. \
                         Look it up in the crate that defines it using crate_item_get."),
                None,
            )
        } else {
            ErrorData::invalid_params(
                format!("Item '{target_path}' not found in {name} {version}. \
                         Use crate_item_list(name=\"{name}\", query=\"{last_component}\") \
                         to search for available items and discover the correct path."),
                None,
            )
        }
    })?;

    let item = doc.index.get(&item_id).ok_or_else(|| {
        // This happens when the item is re-exported from an external sub-crate
        // (e.g. futures::stream::Stream is defined in futures-core, not futures itself).
        // The path is known but the item body was compiled into a different crate's docs.
        let path_entry = doc.paths.get(&item_id);
        let hint = path_entry.map(|p| p.full_path()).unwrap_or_default();
        ErrorData::invalid_params(
            format!("Item '{hint}' is re-exported from an external crate and its full definition \
                     is not available in the {name} docs. Try looking it up directly in the \
                     crate that defines it."),
            None,
        )
    })?;

    let path_entry = &doc.paths[&item_id];
    let kind = path_entry.kind_name();

    // Build signature
    let signature = match kind {
        "function" => function_signature(item),
        _ => {
            let iname = item.name.as_deref().unwrap_or("_");
            let generics = format_generics_for_item(item, kind);
            format!("{kind} {iname}{generics}")
        }
    };

    // Feature requirements
    let feature_requirements = extract_feature_requirements(&item.attr_strings(), &declared_features);

    // Deprecation
    let deprecated = item.deprecation.as_ref().map(|d| json!({
        "since": d.since,
        "note": d.note,
    }));

    // Methods (inherent impls)
    let methods: Vec<serde_json::Value> = if include_methods {
        collect_methods(&doc, item, &declared_features)
    } else {
        vec![]
    };

    // Trait impls
    let trait_impls: Vec<serde_json::Value> = match trait_impl_mode {
        "none" => vec![],
        "all"  => collect_trait_impls(&doc, item, false),
        _      => collect_trait_impls(&doc, item, true),  // "filtered" default
    };

    let output = json!({
        "path": target_path,
        "kind": kind,
        "signature": signature,
        "docs": item.docs,
        "deprecated": deprecated,
        "feature_requirements": feature_requirements,
        "methods": methods,
        "trait_impls": trait_impls,
    });

    let json = serde_json::to_string_pretty(&output)
        .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

    Ok(CallToolResult::success(vec![Content::text(json)]))
}

/// Extract a numeric or string ID value as a String (v57 IDs are integers).
fn id_to_string(v: &serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::String(s) => Some(s.clone()),
        _ => None,
    }
}

/// Get the impl block IDs for a struct/enum/union item.
/// In rustdoc JSON, these are stored in `inner.{kind}.impls` as an integer array.
fn get_impl_ids(item: &crate::docsrs::Item) -> Vec<String> {
    for kind in &["struct", "enum", "union", "primitive"] {
        if let Some(inner) = item.inner_for(kind) {
            if let Some(impls) = inner.get("impls").and_then(|v| v.as_array()) {
                return impls.iter().filter_map(id_to_string).collect();
            }
        }
    }
    vec![]
}

fn collect_methods(
    doc: &crate::docsrs::RustdocJson,
    item: &crate::docsrs::Item,
    declared_features: &HashSet<String>,
) -> Vec<serde_json::Value> {
    let mut methods = vec![];

    // For traits: collect required/provided methods from inner.trait.items directly
    if let Some(trait_inner) = item.inner_for("trait") {
        let trait_items = trait_inner.get("items")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        for method_id_val in &trait_items {
            let Some(method_id) = id_to_string(method_id_val) else { continue };
            let Some(method_item) = doc.index.get(&method_id) else { continue };
            if method_item.kind().unwrap_or("") != "function" { continue; }
            let sig = function_signature(method_item);
            let doc_summary = method_item.doc_summary();
            let feature_reqs = extract_feature_requirements(&method_item.attr_strings(), declared_features);
            methods.push(json!({
                "name": method_item.name,
                "signature": sig,
                "doc_summary": doc_summary,
                "feature_requirements": feature_reqs,
                "deprecated": method_item.deprecation.as_ref().map(|d| &d.note),
            }));
        }
        return methods;
    }

    // For structs/enums/unions: use inherent impl blocks from inner.{kind}.impls
    for impl_id in get_impl_ids(item) {
        let Some(impl_item) = doc.index.get(&impl_id) else { continue };
        let Some(impl_inner) = impl_item.inner_for("impl") else { continue };
        // Inherent impl: "trait" field is null
        if !impl_inner.get("trait").map(|t| t.is_null()).unwrap_or(true) {
            continue;
        }
        let impl_items = impl_inner.get("items")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        for method_id_val in &impl_items {
            let Some(method_id) = id_to_string(method_id_val) else { continue };
            let Some(method_item) = doc.index.get(&method_id) else { continue };
            if method_item.kind().unwrap_or("") != "function" {
                continue;
            }
            let sig = function_signature(method_item);
            let doc_summary = method_item.doc_summary();
            let feature_reqs = extract_feature_requirements(&method_item.attr_strings(), declared_features);
            methods.push(json!({
                "name": method_item.name,
                "signature": sig,
                "doc_summary": doc_summary,
                "feature_requirements": feature_reqs,
                "deprecated": method_item.deprecation.as_ref().map(|d| &d.note),
            }));
        }
    }
    methods
}

/// Trait names that are ubiquitous blanket impls present on virtually every type.
/// These add no useful information and are filtered by default.
const UBIQUITOUS_TRAITS: &[&str] = &[
    "Any",
    "Freeze",
    "Instrument",
    "RefUnwindSafe",
    "UnwindSafe",
    "WithSubscriber",
];

/// Returns true if this trait path is a pure blanket impl that should be hidden.
/// Catches two categories:
/// 1. Names in UBIQUITOUS_TRAITS regardless of generic args.
/// 2. Traits whose only generic arg is a single uppercase letter (e.g. `From<T>`,
///    `Into<U>`, `Borrow<T>`) — these are identity/conversion blankets that apply
///    to every type. Concrete impls like `From<io::Error>` are kept.
fn is_ubiquitous_blanket(trait_path: &str) -> bool {
    // Strip module prefix to get bare name + optional args, e.g. "std::From<T>" → "From<T>"
    let bare = trait_path.rsplit("::").next().unwrap_or(trait_path);
    let name = bare.split('<').next().unwrap_or(bare).trim();

    if UBIQUITOUS_TRAITS.contains(&name) {
        return true;
    }

    // Check for single-letter generic arg: From<T>, Into<U>, Borrow<T>, etc.
    if let Some(args) = bare.strip_prefix(name).and_then(|s| s.strip_prefix('<')).and_then(|s| s.strip_suffix('>')) {
        let arg = args.trim();
        if arg.len() == 1 && arg.chars().next().map_or(false, |c| c.is_uppercase()) {
            return true;
        }
        // Also catch "never" and "!" which are infallible blanket sentinels
        if arg == "never" || arg == "!" {
            return true;
        }
    }

    false
}

fn collect_trait_impls(
    doc: &crate::docsrs::RustdocJson,
    item: &crate::docsrs::Item,
    filter_ubiquitous: bool,
) -> Vec<serde_json::Value> {
    let mut impls = vec![];
    for impl_id in get_impl_ids(item) {
        let Some(impl_item) = doc.index.get(&impl_id) else { continue };
        let Some(impl_inner) = impl_item.inner_for("impl") else { continue };
        // Skip synthetic compiler auto-impls (blanket Send/Sync etc.) — they flood output.
        if impl_inner.get("is_synthetic").and_then(|v| v.as_bool()).unwrap_or(false) {
            continue;
        }
        // Trait impl: "trait" field is non-null
        let Some(trait_) = impl_inner.get("trait") else { continue };
        if trait_.is_null() { continue; }
        // In v57, trait is a direct path object: {"path": "Send", "id": N, "args": ...}
        // Use type_to_string to include generic args (e.g. "From<io::Error>" not just "From")
        let trait_path = type_to_string(trait_);
        if filter_ubiquitous && is_ubiquitous_blanket(&trait_path) {
            continue;
        }
        impls.push(json!({ "trait_path": trait_path }));
    }
    impls
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docsrs::RustdocJson;
    use std::collections::HashSet;

    fn load_rmcp() -> RustdocJson {
        let json_str = std::fs::read_to_string("tests/fixtures/rmcp_0.16.0.json")
            .expect("rmcp fixture must exist");
        serde_json::from_str(&json_str).expect("rmcp fixture must parse")
    }

    // TokioChildProcess: struct id=9410
    // inner.struct.impls = [12027..12047, 9409] (22 total)
    // inherent impl 12027: items=[12015(new),12018(builder),12020(id),12021(graceful_shutdown),12022(into_inner),12024(split)]
    // trait impls include: Send(12028), Sync(12029)

    #[test]
    fn get_impl_ids_returns_all_impls_for_tokiochildprocess() {
        let doc = load_rmcp();
        let item = doc.index.get("9410").expect("TokioChildProcess (id=9410) must exist");
        let impl_ids = get_impl_ids(item);
        assert_eq!(impl_ids.len(), 22, "TokioChildProcess should have 22 impl blocks, got {}", impl_ids.len());
    }

    #[test]
    fn get_impl_ids_empty_for_function_item() {
        let doc = load_rmcp();
        // Functions don't have an impls list
        let fn_item = doc.index.values()
            .find(|i| i.kind() == Some("function"))
            .expect("rmcp must have functions");
        assert!(get_impl_ids(fn_item).is_empty(), "functions should have no impl IDs");
    }

    #[test]
    fn collect_methods_finds_all_inherent_methods() {
        let doc = load_rmcp();
        let item = doc.index.get("9410").expect("TokioChildProcess must exist");
        let features = HashSet::new();
        let methods = collect_methods(&doc, item, &features);
        assert_eq!(methods.len(), 6, "TokioChildProcess should have 6 inherent methods, got {}", methods.len());

        let names: Vec<&str> = methods.iter()
            .filter_map(|m| m.get("name").and_then(|v| v.as_str()))
            .collect();
        assert!(names.contains(&"new"), "should have 'new'");
        assert!(names.contains(&"builder"), "should have 'builder'");
        assert!(names.contains(&"id"), "should have 'id'");
        assert!(names.contains(&"graceful_shutdown"), "should have 'graceful_shutdown'");
        assert!(names.contains(&"into_inner"), "should have 'into_inner'");
        assert!(names.contains(&"split"), "should have 'split'");
    }

    #[test]
    fn collect_methods_entries_have_required_fields() {
        let doc = load_rmcp();
        let item = doc.index.get("9410").expect("TokioChildProcess must exist");
        let features = HashSet::new();
        let methods = collect_methods(&doc, item, &features);
        for method in &methods {
            let name = method.get("name").and_then(|v| v.as_str()).unwrap_or("");
            assert!(!name.is_empty(), "method name should not be empty");
            let sig = method.get("signature").and_then(|v| v.as_str()).unwrap_or("");
            assert!(sig.contains("fn "), "method '{name}' signature should contain 'fn ': {sig}");
        }
    }

    #[test]
    fn collect_trait_impls_excludes_synthetic_and_includes_real_traits() {
        let doc = load_rmcp();
        let item = doc.index.get("9410").expect("TokioChildProcess must exist");
        let impls = collect_trait_impls(&doc, item, true);
        assert!(!impls.is_empty(), "TokioChildProcess should have non-synthetic trait impls");

        let trait_names: Vec<&str> = impls.iter()
            .filter_map(|t| t.get("trait_path").and_then(|v| v.as_str()))
            .collect();

        // Send/Sync are synthetic auto-impls — must be filtered out
        assert!(!trait_names.contains(&"Send"), "synthetic Send should be filtered: {trait_names:?}");
        assert!(!trait_names.contains(&"Sync"), "synthetic Sync should be filtered: {trait_names:?}");

        // Ubiquitous blanket impls should now also be filtered
        assert!(!trait_names.contains(&"From<T>"), "From<T> blanket should be filtered: {trait_names:?}");
        assert!(!trait_names.contains(&"Borrow<T>"), "Borrow<T> blanket should be filtered: {trait_names:?}");
        assert!(!trait_names.contains(&"Into<U>"), "Into<U> blanket should be filtered: {trait_names:?}");
        assert!(!trait_names.iter().any(|t| *t == "Any"), "Any should be filtered: {trait_names:?}");
        // Meaningful crate-specific trait impls should remain
        assert!(!impls.is_empty(), "TokioChildProcess should still have meaningful trait impls after filtering");
    }

    #[test]
    fn collect_trait_impls_entries_have_trait_path_field() {
        let doc = load_rmcp();
        let item = doc.index.get("9410").expect("TokioChildProcess must exist");
        let impls = collect_trait_impls(&doc, item, true);
        for impl_entry in &impls {
            assert!(impl_entry.get("trait_path").is_some(), "each entry must have trait_path field");
            let tp = impl_entry.get("trait_path").and_then(|v| v.as_str()).unwrap_or("");
            assert!(!tp.is_empty(), "trait_path must not be empty");
            assert!(!tp.contains("''"), "trait_path must not have double-apostrophe: {tp}");
        }
    }

    #[test]
    fn collect_trait_impls_excludes_inherent_impls() {
        let doc = load_rmcp();
        let item = doc.index.get("9410").expect("TokioChildProcess must exist");
        // inherent impls have no trait; collect_trait_impls must not include them
        let trait_impls = collect_trait_impls(&doc, item, true);
        let methods = collect_methods(&doc, item, &HashSet::new());
        // The 6 inherent methods should NOT appear in trait_impls
        let trait_names: Vec<&str> = trait_impls.iter()
            .filter_map(|t| t.get("trait_path").and_then(|v| v.as_str()))
            .collect();
        assert!(!trait_names.contains(&"new"), "inherent method 'new' must not appear as trait impl");
        assert_eq!(methods.len(), 6, "inherent methods should still be 6");
    }

    #[test]
    fn id_to_string_handles_integer() {
        let v = serde_json::json!(42);
        assert_eq!(id_to_string(&v), Some("42".to_string()));
    }

    #[test]
    fn id_to_string_handles_string() {
        let v = serde_json::json!("hello");
        assert_eq!(id_to_string(&v), Some("hello".to_string()));
    }

    #[test]
    fn id_to_string_rejects_null() {
        assert_eq!(id_to_string(&serde_json::Value::Null), None);
    }
}
