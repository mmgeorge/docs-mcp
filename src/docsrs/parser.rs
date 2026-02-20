use std::collections::{HashMap, HashSet};

use regex::Regex;
use serde_json::Value;

use super::types::{Item, RustdocJson};

// ─── Type-to-string ───────────────────────────────────────────────────────────

/// Recursively convert a rustdoc JSON `Type` value to a human-readable string.
///
/// Handles format v57 type representations.
pub fn type_to_string(ty: &Value) -> String {
    if ty.is_null() {
        return "()".to_string();
    }

    let obj = match ty.as_object() {
        Some(o) => o,
        None => return ty.to_string(),
    };

    // Primitive
    if let Some(p) = obj.get("primitive").and_then(|v| v.as_str()) {
        return p.to_string();
    }

    // Generic parameter (e.g. "T")
    if let Some(g) = obj.get("generic").and_then(|v| v.as_str()) {
        return g.to_string();
    }

    // Resolved path (e.g. Option<T>, Vec<T>, custom types)
    if let Some(rp) = obj.get("resolved_path") {
        let name = rp.get("path")
            .or_else(|| rp.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("_");
        let args = rp.get("args")
            .and_then(|a| a.get("angle_bracketed"))
            .and_then(|ab| ab.get("args"))
            .and_then(|a| a.as_array());
        if let Some(args) = args {
            let type_args: Vec<String> = args.iter()
                .filter_map(|a| a.get("type").map(type_to_string))
                .collect();
            if !type_args.is_empty() {
                return format!("{name}<{}>", type_args.join(", "));
            }
        }
        return name.to_string();
    }

    // Borrowed reference (&T or &'a T or &'a mut T)
    if let Some(br) = obj.get("borrowed_ref") {
        let lifetime = br.get("lifetime").and_then(|v| v.as_str());
        let mutable = br.get("mutable").and_then(|v| v.as_bool()).unwrap_or(false);
        let inner = br.get("type").map(type_to_string).unwrap_or_else(|| "_".to_string());
        let mut_str = if mutable { "mut " } else { "" };
        return match lifetime {
            Some(lt) if !lt.is_empty() => {
                // JSON lifetime may already include the apostrophe (e.g. "'a", "'static")
                // or may be bare (e.g. "a"). Normalize to avoid "''a".
                if lt.starts_with('\'') {
                    format!("&{lt} {mut_str}{inner}")
                } else {
                    format!("&'{lt} {mut_str}{inner}")
                }
            },
            _ => format!("&{mut_str}{inner}"),
        };
    }

    // Tuple
    if let Some(tup) = obj.get("tuple").and_then(|v| v.as_array()) {
        let parts: Vec<String> = tup.iter().map(type_to_string).collect();
        return format!("({})", parts.join(", "));
    }

    // Slice [T]
    if let Some(sl) = obj.get("slice") {
        return format!("[{}]", type_to_string(sl));
    }

    // Array [T; N]
    if let Some(arr) = obj.get("array") {
        let elem = arr.get("type").map(type_to_string).unwrap_or_else(|| "_".to_string());
        let len = arr.get("len").and_then(|v| v.as_str()).unwrap_or("_");
        return format!("[{elem}; {len}]");
    }

    // Raw pointer (*const T or *mut T)
    if let Some(rp) = obj.get("raw_pointer") {
        let mutable = rp.get("mutable").and_then(|v| v.as_bool()).unwrap_or(false);
        let inner = rp.get("type").map(type_to_string).unwrap_or_else(|| "_".to_string());
        let mut_str = if mutable { "mut" } else { "const" };
        return format!("*{mut_str} {inner}");
    }

    // ImplTrait (impl Trait1 + Trait2)
    if let Some(bounds) = obj.get("impl_trait").and_then(|v| v.as_array()) {
        let parts: Vec<String> = bounds.iter()
            .filter_map(|b| b.get("trait_bound"))
            .filter_map(|tb| tb.get("trait"))
            .map(type_to_string)
            .collect();
        return format!("impl {}", parts.join(" + "));
    }

    // DynTrait
    if let Some(dt) = obj.get("dyn_trait") {
        let traits = dt.get("traits")
            .and_then(|v| v.as_array())
            .map(|ts| {
                ts.iter()
                    .filter_map(|t| t.get("trait"))
                    .map(type_to_string)
                    .collect::<Vec<_>>()
                    .join(" + ")
            })
            .unwrap_or_default();
        let lifetime = dt.get("lifetime").and_then(|v| v.as_str());
        return match lifetime {
            Some(lt) if !lt.is_empty() => format!("dyn {traits} + {lt}"),
            _ => format!("dyn {traits}"),
        };
    }

    // FunctionPointer
    if let Some(fp) = obj.get("function_pointer") {
        let decl = fp.get("sig")
            .or_else(|| fp.get("decl"));
        if let Some(decl) = decl {
            let inputs = decl.get("inputs")
                .and_then(|v| v.as_array())
                .map(|inputs| {
                    inputs.iter()
                        .filter_map(|i| i.as_array())
                        .map(|pair| {
                            let name = pair.first().and_then(|v| v.as_str()).unwrap_or("_");
                            let ty = pair.get(1).map(type_to_string).unwrap_or_else(|| "_".to_string());
                            format!("{name}: {ty}")
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            let output = decl.get("output").map(type_to_string).unwrap_or_default();
            if output.is_empty() || output == "()" {
                return format!("fn({inputs})");
            } else {
                return format!("fn({inputs}) -> {output}");
            }
        }
    }

    // QualifiedPath (e.g. <T as Trait>::Assoc)
    if let Some(qp) = obj.get("qualified_path") {
        let self_type = qp.get("self_type").map(type_to_string).unwrap_or_else(|| "_".to_string());
        let name = qp.get("name").and_then(|v| v.as_str()).unwrap_or("_");
        let trait_val = qp.get("trait");
        let trait_is_absent = trait_val.map(|v| v.is_null()).unwrap_or(true);
        if trait_is_absent {
            // No explicit trait disambiguation — emit `T::Name` (shorthand the compiler resolves).
            return format!("{self_type}::{name}");
        }
        let trait_name = trait_val.map(type_to_string).unwrap_or_default();
        return format!("<{self_type} as {trait_name}>::{name}");
    }

    // Direct type path (v57 trait bounds / impl for_ / qualified path traits):
    // {"id": N, "path": "Foo", "args": ...} — no "resolved_path" wrapper
    if obj.contains_key("id") {
        if let Some(path_str) = obj.get("path").and_then(|v| v.as_str()) {
            let name = if path_str.is_empty() { "_" } else { path_str };
            let args = obj.get("args")
                .and_then(|a| a.get("angle_bracketed"))
                .and_then(|ab| ab.get("args"))
                .and_then(|a| a.as_array());
            if let Some(args) = args {
                let type_args: Vec<String> = args.iter()
                    .filter_map(|a| a.get("type").map(type_to_string))
                    .collect();
                if !type_args.is_empty() {
                    return format!("{name}<{}>", type_args.join(", "));
                }
            }
            return name.to_string();
        }
    }

    // Fallback
    ty.to_string()
}

// ─── Signature reconstruction ─────────────────────────────────────────────────

/// Reconstruct a function signature from rustdoc JSON format v57.
pub fn function_signature(item: &Item) -> String {
    let inner = match item.inner_for("function") {
        Some(f) => f,
        None => return String::new(),
    };

    let header = inner.get("header");
    let is_async = header.and_then(|h| h.get("is_async")).and_then(|v| v.as_bool()).unwrap_or(false);
    let is_const = header.and_then(|h| h.get("is_const")).and_then(|v| v.as_bool()).unwrap_or(false);
    let is_unsafe = header.and_then(|h| h.get("is_unsafe")).and_then(|v| v.as_bool()).unwrap_or(false);

    let sig = match inner.get("sig") {
        Some(s) => s,
        None => return String::new(),
    };

    let name = item.name.as_deref().unwrap_or("_");

    // Build generic params
    let generics = inner.get("generics");
    let generic_str = format_generics(generics);

    // Build params
    let inputs = sig.get("inputs")
        .and_then(|v| v.as_array())
        .map(|inputs| {
            inputs.iter()
                .filter_map(|i| i.as_array())
                .map(|pair| {
                    let param_name = pair.first().and_then(|v| v.as_str()).unwrap_or("_");
                    let ty = pair.get(1).map(type_to_string).unwrap_or_else(|| "_".to_string());
                    // Normalize self receiver to idiomatic form
                    if param_name == "self" {
                        match ty.as_str() {
                            "Self" => "self".to_string(),
                            "&Self" => "&self".to_string(),
                            "&mut Self" => "&mut self".to_string(),
                            _ => format!("self: {ty}"),
                        }
                    } else {
                        format!("{param_name}: {ty}")
                    }
                })
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();

    let output = sig.get("output")
        .filter(|v| !v.is_null())
        .map(type_to_string);

    let where_str = format_where(generics);

    let mut prefix = String::new();
    if is_const { prefix.push_str("const "); }
    if is_async { prefix.push_str("async "); }
    if is_unsafe { prefix.push_str("unsafe "); }

    let output_str = match &output {
        Some(s) if s != "()" => format!(" -> {s}"),
        _ => String::new(),
    };

    format!("{prefix}fn {name}{generic_str}({inputs}){output_str}{where_str}")
}

/// Reconstruct a struct's signature fields.
pub fn struct_fields(item: &Item) -> Vec<String> {
    let inner = match item.inner_for("struct") {
        Some(s) => s,
        None => return vec![],
    };

    let kind = inner.get("kind");
    if let Some(plain) = kind.and_then(|k| k.get("plain")) {
        let fields = plain.get("fields")
            .and_then(|f| f.as_array())
            .map(|v| v.as_slice()).unwrap_or(&[]);
        fields.iter()
            .filter_map(|id| id.as_str())
            .map(|_id| "/* field */".to_string()) // IDs need resolution from index
            .collect()
    } else {
        vec![]
    }
}

/// Extract generic params from the inner block of any item kind (struct/enum/trait/type alias).
/// Returns a formatted `<T, 'a, const N: usize>` string, or empty string if none.
pub fn format_generics_for_item(item: &Item, kind: &str) -> String {
    for k in &[kind, "struct", "enum", "union", "trait", "type_alias", "typedef"] {
        if let Some(inner) = item.inner_for(k) {
            if let Some(generics) = inner.get("generics") {
                let s = format_generics(Some(generics));
                if !s.is_empty() {
                    return s;
                }
            }
        }
    }
    String::new()
}

fn format_generics(generics: Option<&Value>) -> String {
    let generics = match generics {
        Some(g) => g,
        None => return String::new(),
    };
    let params = match generics.get("params").and_then(|v| v.as_array()) {
        Some(p) => p,
        None => return String::new(),
    };
    if params.is_empty() {
        return String::new();
    }
    let parts: Vec<String> = params.iter()
        .filter_map(|p| {
            let name = p.get("name")?.as_str()?;
            // Skip synthetic impl Trait params — they appear as `foo: impl Trait`
            // in the function inputs and shouldn't be re-emitted in <...>
            if name.starts_with("impl ") {
                return None;
            }
            let kind = p.get("kind");
            // Const generic param: {"const": {"type": T, "default": ...}} → `const N: type`
            if let Some(const_info) = kind.and_then(|k| k.get("const")) {
                let ty_str = const_info.get("type").map(type_to_string).unwrap_or_else(|| "_".to_string());
                return Some(format!("const {name}: {ty_str}"));
            }
            // Type param: may have bounds
            if let Some(type_bounds) = kind.and_then(|k| k.get("type")).and_then(|t| t.get("bounds")) {
                let bounds = type_bounds.as_array()
                    .map(|bs| {
                        bs.iter()
                            .filter_map(|b| b.get("trait_bound"))
                            .filter_map(|tb| tb.get("trait"))
                            .map(type_to_string)
                            .collect::<Vec<_>>()
                            .join(" + ")
                    })
                    .unwrap_or_default();
                if bounds.is_empty() {
                    Some(name.to_string())
                } else {
                    Some(format!("{name}: {bounds}"))
                }
            } else {
                // Lifetime param (kind = {"lifetime": {...}}) or unbounded type param
                Some(name.to_string())
            }
        })
        .collect();
    if parts.is_empty() {
        String::new()
    } else {
        format!("<{}>", parts.join(", "))
    }
}

fn format_where(generics: Option<&Value>) -> String {
    let generics = match generics {
        Some(g) => g,
        None => return String::new(),
    };
    let clauses = match generics.get("where_predicates").and_then(|v| v.as_array()) {
        Some(c) => c,
        None => return String::new(),
    };
    if clauses.is_empty() {
        return String::new();
    }
    let parts: Vec<String> = clauses.iter()
        .filter_map(|c| {
            if let Some(bp) = c.get("bound_predicate") {
                let ty = bp.get("type").map(type_to_string)?;
                let bounds = bp.get("bounds")?.as_array()?;
                let bound_strs: Vec<String> = bounds.iter()
                    .filter_map(|b| b.get("trait_bound"))
                    .filter_map(|tb| tb.get("trait"))
                    .map(type_to_string)
                    .collect();
                if bound_strs.is_empty() {
                    None
                } else {
                    Some(format!("{ty}: {}", bound_strs.join(" + ")))
                }
            } else {
                None
            }
        })
        .collect();
    if parts.is_empty() {
        String::new()
    } else {
        format!("\nwhere\n    {}", parts.join(",\n    "))
    }
}

// ─── Feature flag extraction ──────────────────────────────────────────────────

/// Extract feature requirements from rustdoc JSON item attributes.
///
/// Uses the correct v57 attr format: `name: "feature", value: Some("auth")`
/// NOT the broken `#[cfg(feature = "...")]` pattern.
///
/// Cross-references against the set of declared features from the sparse index.
pub fn extract_feature_requirements(
    attrs: &[String],
    declared_features: &HashSet<String>,
) -> Vec<String> {
    // Lazy static would be cleaner, but we create the regex once per call
    // (attrs are small, so this is acceptable)
    let Ok(re) = Regex::new(r#"name: "feature", value: Some\("([^"]+)"\)"#) else {
        return vec![];
    };

    let mut features: Vec<String> = attrs
        .iter()
        .flat_map(|attr| {
            re.captures_iter(attr)
                .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
                .collect::<Vec<_>>()
        })
        .collect();

    // Cross-reference against declared features (filter out non-feature cfgs)
    if !declared_features.is_empty() {
        features.retain(|f| declared_features.contains(f));
    }

    features.sort();
    features.dedup();
    features
}

// ─── Module tree building ─────────────────────────────────────────────────────

/// A non-module item directly inside a module (used for include_items output).
#[derive(Debug, Clone)]
pub struct ItemSummary {
    pub kind: String,
    pub name: String,
    pub doc_summary: String,
}

#[derive(Debug, Clone)]
pub struct ModuleNode {
    pub path: String,
    pub doc_summary: String,
    /// Count of each item kind directly inside this module (excludes "use"/"import" noise).
    pub item_counts: HashMap<String, usize>,
    /// Direct non-module items (structs, fns, traits, etc.) — populated for include_items.
    pub items: Vec<ItemSummary>,
    pub children: Vec<ModuleNode>,
}

pub fn build_module_tree(doc: &RustdocJson) -> Vec<ModuleNode> {
    // Find the root module
    let root_id = doc.root_id();
    let root_item = doc.index.get(&root_id);
    if root_item.is_none() {
        return vec![];
    }

    // Build children of root
    if let Some(root) = root_item {
        if let Some(module) = root.inner_for("module") {
            let item_ids = module.get("items")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();

            return build_children(&item_ids, doc, 0);
        }
    }
    vec![]
}

fn id_val_to_string(id_val: &Value) -> Option<String> {
    match id_val {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn build_children(item_ids: &[Value], doc: &RustdocJson, depth: usize) -> Vec<ModuleNode> {
    if depth > 5 {
        return vec![];
    }

    let mut modules = vec![];
    let mut other_counts: HashMap<String, usize> = HashMap::new();

    for id_val in item_ids {
        // v57 IDs are integers in JSON; the index HashMap has string keys
        let id = match id_val_to_string(id_val) {
            Some(s) => s,
            None => continue,
        };

        let item = match doc.index.get(&id) {
            Some(i) => i,
            None => continue,
        };

        let kind = item.kind().unwrap_or("unknown");

        if kind == "module" {
            let path = doc.paths.get(&id)
                .map(|p| p.full_path())
                .or_else(|| item.name.clone())
                .unwrap_or_else(|| id.clone());

            let doc_summary = item.doc_summary();

            let sub_items = item.inner_for("module")
                .and_then(|m| m.get("items"))
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();

            let mut item_counts = HashMap::new();
            let mut direct_items = vec![];
            for sub_id_val in &sub_items {
                if let Some(sub_id) = id_val_to_string(sub_id_val) {
                    if let Some(sub_item) = doc.index.get(&sub_id) {
                        if let Some(k) = sub_item.kind() {
                            // Skip "use"/"import" re-exports from counts — they're noise
                            // (re-exported items already appear under their canonical path).
                            if k == "use" || k == "import" { continue; }
                            *item_counts.entry(k.to_string()).or_insert(0) += 1;
                            // Collect non-module items for include_items
                            if k != "module" {
                                direct_items.push(ItemSummary {
                                    kind: k.to_string(),
                                    name: sub_item.name.clone().unwrap_or_default(),
                                    doc_summary: sub_item.doc_summary(),
                                });
                            }
                        }
                    }
                }
            }

            let children = build_children(&sub_items, doc, depth + 1);

            modules.push(ModuleNode {
                path,
                doc_summary,
                item_counts,
                items: direct_items,
                children,
            });
        } else {
            *other_counts.entry(kind.to_string()).or_insert(0) += 1;
        }
    }

    modules
}

// ─── Method parent map ───────────────────────────────────────────────────────

/// Returns the item ID embedded in a rustdoc JSON type node (`resolved_path` or direct id+path).
fn type_item_id(val: &Value) -> Option<String> {
    if let Some(rp) = val.get("resolved_path") {
        return match rp.get("id") {
            Some(Value::Number(n)) => Some(n.to_string()),
            Some(Value::String(s)) => Some(s.clone()),
            _ => None,
        };
    }
    match (val.get("id"), val.get("path")) {
        (Some(Value::Number(n)), Some(_)) => Some(n.to_string()),
        (Some(Value::String(s)), Some(_)) => Some(s.clone()),
        _ => None,
    }
}

/// Build a map from method/associated item ID → parent type's full qualified path.
///
/// Covers inherent impl blocks. Trait-impl method IDs are intentionally excluded
/// because they are covered by looking up the implementing type directly.
fn build_method_parent_map(doc: &RustdocJson) -> HashMap<String, String> {
    let mut map: HashMap<String, String> = HashMap::new();

    for item in doc.index.values() {
        if item.kind() != Some("impl") { continue; }
        let Some(impl_inner) = item.inner_for("impl") else { continue };

        // Inherent impls only (trait field is null/absent)
        let trait_is_null = impl_inner.get("trait").map(|t| t.is_null()).unwrap_or(true);
        if !trait_is_null { continue; }

        let Some(for_val) = impl_inner.get("for") else { continue };

        // Resolve the parent type path: try doc.paths first (gives full qualified path),
        // fall back to type_to_string (gives just the type name).
        let parent_path = type_item_id(for_val)
            .and_then(|id| doc.paths.get(&id))
            .map(|p| p.full_path())
            .unwrap_or_else(|| type_to_string(for_val));

        if parent_path.is_empty() { continue; }

        let method_ids = impl_inner.get("items")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        for method_id_val in &method_ids {
            if let Some(mid) = id_val_to_string(method_id_val) {
                map.insert(mid, parent_path.clone());
            }
        }
    }

    map
}

// ─── Item search ──────────────────────────────────────────────────────────────

pub struct SearchResult {
    pub path: String,
    pub kind: String,
    pub signature: String,
    pub doc_summary: String,
    pub feature_requirements: Vec<String>,
    pub score: f32,
}

/// Search for items in the rustdoc JSON by name or concept.
pub fn search_items(
    doc: &RustdocJson,
    query: &str,
    kind_filter: Option<&str>,
    module_prefix: Option<&str>,
    limit: usize,
    declared_features: &HashSet<String>,
) -> Vec<SearchResult> {
    let query_lower = query.to_lowercase();
    let mut results: Vec<SearchResult> = vec![];

    for (id, item) in &doc.index {
        let path_entry = match doc.paths.get(id) {
            Some(p) => p,
            None => continue,
        };

        let full_path = path_entry.full_path();
        let name = item.name.as_deref().unwrap_or("");
        let item_kind = path_entry.kind_name();

        // Kind filter — normalize user-friendly aliases to rustdoc kind names
        if let Some(kf) = kind_filter {
            let normalized = match kf {
                "fn" => "function",
                "mod" => "module",
                "type" => "type_alias",
                other => other,
            };
            if item_kind != normalized {
                continue;
            }
        }

        // Module prefix filter
        if let Some(prefix) = module_prefix {
            if !full_path.starts_with(prefix) {
                continue;
            }
        }

        // Skip auto-generated or unnamed items
        if name.is_empty() {
            continue;
        }

        let name_lower = name.to_lowercase();
        let doc_summary = item.doc_summary();
        let doc_lower = doc_summary.to_lowercase();

        // Score calculation
        let score = if name_lower == query_lower {
            1.0f32
        } else if name_lower.starts_with(&query_lower) {
            0.9
        } else if name_lower.contains(&query_lower) {
            0.7
        } else if doc_lower.contains(&query_lower) {
            0.2
        } else {
            continue; // no match
        };

        let signature = match item.kind().unwrap_or("") {
            "function" => function_signature(item),
            _ => format!("{} {}", item_kind, name),
        };

        let feature_requirements = extract_feature_requirements(&item.attr_strings(), declared_features);

        results.push(SearchResult {
            path: full_path,
            kind: item_kind.to_string(),
            signature,
            doc_summary,
            feature_requirements,
            score,
        });
    }

    // Second pass: search methods (function items in doc.index but absent from doc.paths).
    // These are inherent methods on structs/enums, not top-level free functions.
    // kind="fn"/"function" specifically targets free functions; methods have kind="method".
    let want_methods = kind_filter.is_none() || kind_filter == Some("method");

    if want_methods {
        let method_parent_map = build_method_parent_map(doc);

        for (id, item) in &doc.index {
            if doc.paths.contains_key(id) { continue; } // already searched above
            if item.kind() != Some("function") { continue; }

            let Some(parent_path) = method_parent_map.get(id) else { continue };
            let name = item.name.as_deref().unwrap_or("");
            if name.is_empty() { continue; }

            // Module prefix filter: parent type path must start with the prefix
            if let Some(prefix) = module_prefix {
                if !parent_path.starts_with(prefix) { continue; }
            }

            let name_lower = name.to_lowercase();
            let parent_lower = parent_path.to_lowercase();
            let doc_summary = item.doc_summary();
            let doc_lower = doc_summary.to_lowercase();

            let score = if name_lower == query_lower {
                1.0f32
            } else if name_lower.starts_with(&query_lower) {
                0.9
            } else if name_lower.contains(&query_lower) {
                0.7
            } else if parent_lower.contains(&query_lower) {
                0.6 // query matches parent type name, e.g. "TokioChildProcess" → all its methods
            } else if doc_lower.contains(&query_lower) {
                0.4
            } else {
                continue;
            };

            let full_path = format!("{parent_path}::{name}");
            let signature = function_signature(item);
            let feature_requirements = extract_feature_requirements(&item.attr_strings(), declared_features);

            results.push(SearchResult {
                path: full_path,
                kind: "method".to_string(),
                signature,
                doc_summary,
                feature_requirements,
                score,
            });
        }
    }

    // Sort by score descending
    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(limit);
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_to_string_primitive() {
        let ty = serde_json::json!({"primitive": "str"});
        assert_eq!(type_to_string(&ty), "str");
    }

    #[test]
    fn test_type_to_string_generic() {
        let ty = serde_json::json!({"generic": "T"});
        assert_eq!(type_to_string(&ty), "T");
    }

    #[test]
    fn test_type_to_string_ref() {
        let ty = serde_json::json!({
            "borrowed_ref": {
                "lifetime": null,
                "mutable": false,
                "type": {"primitive": "str"}
            }
        });
        assert_eq!(type_to_string(&ty), "&str");
    }

    #[test]
    fn test_type_to_string_mut_ref_with_lifetime() {
        let ty = serde_json::json!({
            "borrowed_ref": {
                "lifetime": "a",
                "mutable": true,
                "type": {"generic": "T"}
            }
        });
        assert_eq!(type_to_string(&ty), "&'a mut T");
    }

    #[test]
    fn test_type_to_string_tuple() {
        let ty = serde_json::json!({
            "tuple": [
                {"primitive": "i32"},
                {"primitive": "bool"}
            ]
        });
        assert_eq!(type_to_string(&ty), "(i32, bool)");
    }

    #[test]
    fn test_type_to_string_slice() {
        let ty = serde_json::json!({"slice": {"primitive": "u8"}});
        assert_eq!(type_to_string(&ty), "[u8]");
    }

    #[test]
    fn test_type_to_string_option() {
        let ty = serde_json::json!({
            "resolved_path": {
                "path": "Option",
                "args": {
                    "angle_bracketed": {
                        "args": [
                            {"type": {"primitive": "i32"}}
                        ]
                    }
                }
            }
        });
        assert_eq!(type_to_string(&ty), "Option<i32>");
    }

    #[test]
    fn test_feature_regex_correct_pattern() {
        let attr = r#"#[attr = CfgTrace([NameValue { name: "feature", value: Some("auth"), span: None }])]"#;
        let features = extract_feature_requirements(
            &[attr.to_string()],
            &HashSet::from(["auth".to_string()]),
        );
        assert_eq!(features, vec!["auth"]);
    }

    #[test]
    fn test_feature_regex_old_pattern_fails() {
        // The old broken pattern #[cfg(feature = "...")] would NOT match this format
        let attr = r#"#[attr = CfgTrace([NameValue { name: "feature", value: Some("auth"), span: None }])]"#;
        // Old pattern wouldn't extract "auth" from this attr format
        let old_re = regex::Regex::new(r#"#\[cfg\(feature\s*=\s*"([^"]+)"\)\]"#).unwrap();
        let matches: Vec<&str> = old_re.captures_iter(attr)
            .filter_map(|c| c.get(1).map(|m| m.as_str()))
            .collect();
        assert!(matches.is_empty(), "Old pattern should NOT match v57 attr format");
    }

    #[test]
    fn test_feature_cross_reference() {
        let attr = r#"#[attr = CfgTrace([NameValue { name: "feature", value: Some("undeclared"), span: None }])]"#;
        let declared = HashSet::from(["auth".to_string(), "tls".to_string()]);
        let features = extract_feature_requirements(&[attr.to_string()], &declared);
        // "undeclared" should be filtered out
        assert!(features.is_empty());
    }
}
