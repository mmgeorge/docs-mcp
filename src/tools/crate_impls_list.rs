use rmcp::{ErrorData, model::{CallToolResult, Content}};
use serde::Deserialize;
use rmcp::schemars::{self, JsonSchema};
use serde_json::json;

use super::AppState;
use crate::docsrs::{fetch_rustdoc_json, parser::type_to_string};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CrateImplsListParams {
    /// Crate name
    pub name: String,
    /// Version string. Defaults to latest stable.
    pub version: Option<String>,
    /// Fully-qualified trait path to find implementors of (e.g. "serde::Serialize")
    pub trait_path: Option<String>,
    /// Fully-qualified type path to find trait implementations for (e.g. "tokio::sync::Mutex")
    pub type_path: Option<String>,
    /// Filter results by name substring
    pub search: Option<String>,
    /// Max results to return (default: 50)
    pub limit: Option<usize>,
}

pub async fn execute(state: &AppState, params: CrateImplsListParams) -> Result<CallToolResult, ErrorData> {
    if params.trait_path.is_none() && params.type_path.is_none() {
        return Err(ErrorData::invalid_params(
            "Either trait_path or type_path must be specified",
            None,
        ));
    }

    let name = &params.name;
    let version = state.resolve_version(name, params.version.as_deref()).await
        .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

    let doc = fetch_rustdoc_json(name, &version, &state.client, &state.cache).await
        .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

    let search_lower = params.search.as_deref().map(|s| s.to_lowercase());
    let limit = params.limit.unwrap_or(50).min(200);

    if let Some(ref trait_path) = params.trait_path {
        // Find all types within this crate that implement the given trait.
        // Match by last component or full path suffix.
        let trait_last = trait_path.rsplit("::").next().unwrap_or(trait_path.as_str());

        let mut implementors: Vec<serde_json::Value> = vec![];
        for item in doc.index.values() {
            let Some(impl_inner) = item.inner_for("impl") else { continue };
            // Skip synthetic compiler-generated impls (Send, Sync, Freeze, etc.)
            if impl_inner.get("is_synthetic").and_then(|v| v.as_bool()).unwrap_or(false) {
                continue;
            }
            // Must be a trait impl (trait field non-null)
            let Some(trait_val) = impl_inner.get("trait") else { continue };
            if trait_val.is_null() { continue; }

            // Match trait by name (last component) or full path
            let t_name = trait_val.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let t_matches = t_name == trait_last
                || t_name == trait_path.as_str()
                || trait_path.ends_with(&format!("::{t_name}"));
            if !t_matches { continue; }

            // Get the type being implemented for
            let for_val = impl_inner.get("for");
            let for_name: String = for_val
                .and_then(|f| f.get("resolved_path"))
                .and_then(|rp| rp.get("path").and_then(|v| v.as_str()))
                .map(|s| s.to_string())
                .unwrap_or_else(|| for_val.map(type_to_string).unwrap_or_default());

            if for_name.is_empty() { continue; }

            if let Some(ref search) = search_lower {
                if !for_name.to_lowercase().contains(search.as_str()) {
                    continue;
                }
            }

            if implementors.len() >= limit { break; }

            // Generic params on the impl (e.g. impl<T: Send> Serialize for Vec<T>)
            let impl_generics: Vec<&str> = impl_inner
                .get("generics").and_then(|g| g.get("params")).and_then(|p| p.as_array())
                .map(|ps| ps.iter().filter_map(|p| p.get("name").and_then(|v| v.as_str())).collect())
                .unwrap_or_default();

            implementors.push(json!({
                "type_name": for_name,
                "impl_generics": if impl_generics.is_empty() { None } else { Some(impl_generics) },
            }));
        }

        let output = json!({
            "name": name,
            "version": version,
            "trait_path": trait_path,
            "count": implementors.len(),
            "implementors": implementors,
        });
        let json = serde_json::to_string_pretty(&output)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        return Ok(CallToolResult::success(vec![Content::text(json)]));
    }

    // type_path branch: find all traits this type implements.
    // Use the type's `inner.{kind}.impls` list for precision (same approach as crate_item_get).
    let type_path_str = params.type_path.as_deref().unwrap();
    let target_parts: Vec<&str> = type_path_str.split("::").collect();

    // Exact match first, then subsequence fallback for re-exports
    let item_id = doc.paths.iter()
        .find(|(_, p)| p.full_path() == type_path_str)
        .or_else(|| {
            doc.paths.iter().find(|(_, p)| {
                let parts = &p.path;
                if parts.is_empty() || target_parts.is_empty() { return false; }
                if parts[0] != target_parts[0] { return false; }
                let stored_rest = &parts[1..];
                let target_rest = &target_parts[1..];
                if target_rest.is_empty() { return false; }
                let mut ti = 0;
                for s in stored_rest {
                    if ti < target_rest.len() && *s == target_rest[ti] { ti += 1; }
                }
                ti == target_rest.len()
            })
        })
        .map(|(id, _)| id.clone());

    let item_id = item_id.ok_or_else(|| {
        ErrorData::invalid_params(
            format!("Type '{type_path_str}' not found in {name} {version}"),
            None,
        )
    })?;

    let item = doc.index.get(&item_id).ok_or_else(|| {
        ErrorData::internal_error(format!("Item ID {item_id} not in index"), None)
    })?;

    // Get impl IDs from the item's inner.{kind}.impls list
    let impl_ids: Vec<String> = {
        let mut ids = vec![];
        for kind in &["struct", "enum", "union", "primitive"] {
            if let Some(inner) = item.inner_for(kind) {
                if let Some(impls) = inner.get("impls").and_then(|v| v.as_array()) {
                    for v in impls {
                        if let Some(id) = match v {
                            serde_json::Value::Number(n) => Some(n.to_string()),
                            serde_json::Value::String(s) => Some(s.clone()),
                            _ => None,
                        } {
                            ids.push(id);
                        }
                    }
                    break;
                }
            }
        }
        ids
    };

    let mut implementations: Vec<serde_json::Value> = vec![];
    for impl_id in &impl_ids {
        let Some(impl_item) = doc.index.get(impl_id) else { continue };
        let Some(impl_inner) = impl_item.inner_for("impl") else { continue };

        let trait_val = impl_inner.get("trait");
        let is_inherent = trait_val.map(|t| t.is_null()).unwrap_or(true);
        // Skip synthetic compiler auto-impls (e.g. auto-trait blanket impls for Send/Sync
        // that the compiler generates automatically — these flood the output with noise).
        let is_synthetic = impl_inner.get("is_synthetic").and_then(|v| v.as_bool()).unwrap_or(false);
        if is_synthetic { continue; }

        // Use type_to_string for full trait path with generic args (e.g. "From<io::Error>")
        let trait_name: Option<String> = if is_inherent {
            None
        } else {
            trait_val.map(type_to_string)
        };

        if let Some(ref search) = search_lower {
            let name_str = trait_name.as_deref().unwrap_or("inherent");
            if !name_str.to_lowercase().contains(search.as_str()) {
                continue;
            }
        }

        if implementations.len() >= limit { break; }

        implementations.push(json!({
            "trait_path": trait_name,
            "is_inherent": is_inherent,
        }));
    }

    let output = json!({
        "name": name,
        "version": version,
        "type_path": type_path_str,
        "count": implementations.len(),
        "implementations": implementations,
    });
    let json = serde_json::to_string_pretty(&output)
        .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
    Ok(CallToolResult::success(vec![Content::text(json)]))
}
