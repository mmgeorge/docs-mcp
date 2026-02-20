use std::collections::HashSet;

use docs_mcp::docsrs::parser::{
    build_module_tree, extract_feature_requirements, format_generics_for_item, function_signature, search_items, type_to_string,
};
use docs_mcp::docsrs::RustdocJson;

// ─── type_to_string ───────────────────────────────────────────────────────────

#[test]
fn type_primitive_str() {
    let ty = serde_json::json!({"primitive": "str"});
    assert_eq!(type_to_string(&ty), "str");
}

#[test]
fn type_primitive_u64() {
    let ty = serde_json::json!({"primitive": "u64"});
    assert_eq!(type_to_string(&ty), "u64");
}

#[test]
fn type_generic() {
    let ty = serde_json::json!({"generic": "T"});
    assert_eq!(type_to_string(&ty), "T");
}

#[test]
fn type_ref_immutable() {
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
fn type_ref_mutable() {
    let ty = serde_json::json!({
        "borrowed_ref": {
            "lifetime": null,
            "mutable": true,
            "type": {"generic": "T"}
        }
    });
    assert_eq!(type_to_string(&ty), "&mut T");
}

#[test]
fn type_ref_with_lifetime() {
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
fn type_tuple_empty() {
    let ty = serde_json::json!({"tuple": []});
    assert_eq!(type_to_string(&ty), "()");
}

#[test]
fn type_tuple_pair() {
    let ty = serde_json::json!({
        "tuple": [
            {"primitive": "i32"},
            {"primitive": "bool"}
        ]
    });
    assert_eq!(type_to_string(&ty), "(i32, bool)");
}

#[test]
fn type_slice() {
    let ty = serde_json::json!({"slice": {"primitive": "u8"}});
    assert_eq!(type_to_string(&ty), "[u8]");
}

#[test]
fn type_array() {
    let ty = serde_json::json!({
        "array": {
            "type": {"primitive": "u8"},
            "len": "32"
        }
    });
    assert_eq!(type_to_string(&ty), "[u8; 32]");
}

#[test]
fn type_raw_pointer_const() {
    let ty = serde_json::json!({
        "raw_pointer": {
            "mutable": false,
            "type": {"primitive": "u8"}
        }
    });
    assert_eq!(type_to_string(&ty), "*const u8");
}

#[test]
fn type_raw_pointer_mut() {
    let ty = serde_json::json!({
        "raw_pointer": {
            "mutable": true,
            "type": {"primitive": "u8"}
        }
    });
    assert_eq!(type_to_string(&ty), "*mut u8");
}

#[test]
fn type_resolved_path_simple() {
    let ty = serde_json::json!({
        "resolved_path": {
            "path": "String",
            "args": null
        }
    });
    assert_eq!(type_to_string(&ty), "String");
}

#[test]
fn type_resolved_path_with_generic() {
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
fn type_resolved_path_nested_generic() {
    let ty = serde_json::json!({
        "resolved_path": {
            "path": "Vec",
            "args": {
                "angle_bracketed": {
                    "args": [
                        {"type": {"resolved_path": {"path": "String", "args": null}}}
                    ]
                }
            }
        }
    });
    assert_eq!(type_to_string(&ty), "Vec<String>");
}

// ─── Feature flag extraction ──────────────────────────────────────────────────

#[test]
fn feature_correct_v57_pattern_extracts() {
    let attr = r#"#[attr = CfgTrace([NameValue { name: "feature", value: Some("auth"), span: None }])]"#;
    let declared = HashSet::from(["auth".to_string()]);
    let features = extract_feature_requirements(&[attr.to_string()], &declared);
    assert_eq!(features, vec!["auth"]);
}

#[test]
fn feature_old_cfg_pattern_does_not_match_v57() {
    // The old broken pattern #[cfg(feature = "...")] does NOT appear in v57 attrs
    let attr = r#"#[attr = CfgTrace([NameValue { name: "feature", value: Some("auth"), span: None }])]"#;
    let old_re = regex::Regex::new(r#"#\[cfg\(feature\s*=\s*"([^"]+)"\)\]"#).unwrap();
    let matches: Vec<_> = old_re.captures_iter(attr).collect();
    assert!(matches.is_empty(), "Old pattern should NOT match v57 attr format");
}

#[test]
fn feature_cross_reference_filters_undeclared() {
    let attr = r#"#[attr = CfgTrace([NameValue { name: "feature", value: Some("undeclared-feat"), span: None }])]"#;
    let declared = HashSet::from(["auth".to_string(), "tls".to_string()]);
    let features = extract_feature_requirements(&[attr.to_string()], &declared);
    assert!(features.is_empty(), "Undeclared features should be filtered out");
}

#[test]
fn feature_multiple_attrs_deduped() {
    let attrs = vec![
        r#"#[attr = CfgTrace([NameValue { name: "feature", value: Some("tls"), span: None }])]"#.to_string(),
        r#"#[attr = CfgTrace([NameValue { name: "feature", value: Some("tls"), span: None }])]"#.to_string(),
        r#"#[attr = CfgTrace([NameValue { name: "feature", value: Some("http2"), span: None }])]"#.to_string(),
    ];
    let declared = HashSet::from(["tls".to_string(), "http2".to_string()]);
    let features = extract_feature_requirements(&attrs, &declared);
    assert_eq!(features.len(), 2);
    assert!(features.contains(&"tls".to_string()));
    assert!(features.contains(&"http2".to_string()));
}

#[test]
fn feature_empty_declared_set_allows_all() {
    // When no declared features are provided, don't filter
    let attr = r#"#[attr = CfgTrace([NameValue { name: "feature", value: Some("anything"), span: None }])]"#;
    let declared: HashSet<String> = HashSet::new();
    let features = extract_feature_requirements(&[attr.to_string()], &declared);
    assert_eq!(features, vec!["anything"]);
}

// ─── Fixture-based parser tests ───────────────────────────────────────────────

/// Load the clap fixture and verify basic structure parses correctly.
#[test]
fn fixture_clap_parses() {
    let json_str = std::fs::read_to_string("tests/fixtures/clap_4.5.59.json")
        .expect("clap fixture should exist");
    let doc: docs_mcp::docsrs::RustdocJson =
        serde_json::from_str(&json_str).expect("clap fixture should parse");

    assert!(doc.format_version >= 33, "format_version should be >= 33, got {}", doc.format_version);
    assert!(!doc.root_id().is_empty(), "root ID should not be empty");
    assert!(!doc.index.is_empty(), "index should not be empty");
    assert!(!doc.paths.is_empty(), "paths should not be empty");
}

#[test]
fn fixture_clap_root_has_docs() {
    let json_str = std::fs::read_to_string("tests/fixtures/clap_4.5.59.json")
        .expect("clap fixture should exist");
    let doc: docs_mcp::docsrs::RustdocJson =
        serde_json::from_str(&json_str).expect("clap fixture should parse");

    let root_id = doc.root_id();
    let root_item = doc.index.get(&root_id).expect("root item should exist in index");
    let docs = root_item.docs.as_deref().unwrap_or("");
    assert!(!docs.is_empty(), "root docs should not be empty (clap has extensive //! docs)");
}

#[test]
fn fixture_rmcp_parses() {
    let json_str = std::fs::read_to_string("tests/fixtures/rmcp_0.16.0.json")
        .expect("rmcp fixture should exist");
    let doc: docs_mcp::docsrs::RustdocJson =
        serde_json::from_str(&json_str).expect("rmcp fixture should parse");

    assert!(doc.format_version >= 33);
    assert!(!doc.root_id().is_empty());
    assert!(!doc.index.is_empty());
}

#[test]
fn fixture_rmcp_has_function_items() {
    let json_str = std::fs::read_to_string("tests/fixtures/rmcp_0.16.0.json")
        .expect("rmcp fixture should exist");
    let doc: docs_mcp::docsrs::RustdocJson =
        serde_json::from_str(&json_str).expect("rmcp fixture should parse");

    let function_count = doc.index.values()
        .filter(|i| i.kind() == Some("function"))
        .count();
    assert!(function_count > 0, "rmcp should have function items");
}

#[test]
fn fixture_rmcp_signature_reconstruction() {
    let json_str = std::fs::read_to_string("tests/fixtures/rmcp_0.16.0.json")
        .expect("rmcp fixture should exist");
    let doc: docs_mcp::docsrs::RustdocJson =
        serde_json::from_str(&json_str).expect("rmcp fixture should parse");

    // Find any function and try to reconstruct its signature
    let fn_item = doc.index.values()
        .find(|i| i.kind() == Some("function") && i.name.is_some())
        .expect("rmcp should have at least one named function");

    let sig = docs_mcp::docsrs::parser::function_signature(fn_item);
    assert!(!sig.is_empty(), "function signature should not be empty");
    assert!(sig.contains("fn "), "signature should contain 'fn '");
}

// ─── Module tree tests ────────────────────────────────────────────────────────

fn load_rmcp() -> RustdocJson {
    let json_str = std::fs::read_to_string("tests/fixtures/rmcp_0.16.0.json")
        .expect("rmcp fixture must exist");
    serde_json::from_str(&json_str).expect("rmcp fixture must parse")
}

fn load_clap() -> RustdocJson {
    let json_str = std::fs::read_to_string("tests/fixtures/clap_4.5.59.json")
        .expect("clap fixture must exist");
    serde_json::from_str(&json_str).expect("clap fixture must parse")
}

#[test]
fn fixture_rmcp_module_tree_is_nonempty() {
    let doc = load_rmcp();
    let tree = build_module_tree(&doc);
    assert!(!tree.is_empty(), "rmcp module tree should not be empty");
}

#[test]
fn fixture_rmcp_module_tree_nodes_have_paths() {
    let doc = load_rmcp();
    let tree = build_module_tree(&doc);
    for node in &tree {
        assert!(!node.path.is_empty(), "module tree node should have a path");
        assert!(node.path.starts_with("rmcp"), "module path should start with crate name, got: {}", node.path);
    }
}

#[test]
fn fixture_rmcp_module_tree_has_item_counts() {
    let doc = load_rmcp();
    let tree = build_module_tree(&doc);
    // At least one node should have non-empty item counts (has structs, fns, etc.)
    let any_with_counts = tree.iter().any(|n| !n.item_counts.is_empty());
    assert!(any_with_counts, "at least one module node should have item counts");
}

#[test]
fn fixture_clap_module_tree_reflects_format_version() {
    // clap fixture is stripped (only module/use items), so tree may be minimal
    // but must not panic and must return a valid result
    let doc = load_clap();
    let tree = build_module_tree(&doc);
    // Result can be empty for stripped fixtures — just ensure it doesn't panic
    let _ = tree;
}

// ─── search_items tests ────────────────────────────────────────────────────────

#[test]
fn fixture_rmcp_search_finds_tokiochildprocess() {
    let doc = load_rmcp();
    let features = HashSet::new();
    let results = search_items(&doc, "TokioChildProcess", None, None, 10, &features);
    assert!(!results.is_empty(), "search for 'TokioChildProcess' should return results");
    let found = results.iter().any(|r| r.path.contains("TokioChildProcess"));
    assert!(found, "TokioChildProcess should appear in results");
}

#[test]
fn fixture_rmcp_search_kind_fn_returns_only_functions() {
    let doc = load_rmcp();
    let features = HashSet::new();
    let results = search_items(&doc, "", Some("fn"), None, 50, &features);
    assert!(!results.is_empty(), "kind=fn should return results");
    for r in &results {
        assert_eq!(r.kind, "function", "kind=fn filter must only return functions, got: {}", r.kind);
    }
}

#[test]
fn fixture_rmcp_search_kind_function_alias_same_as_fn() {
    // "function" and "fn" should be equivalent
    let doc = load_rmcp();
    let features = HashSet::new();
    let by_fn = search_items(&doc, "", Some("fn"), None, 200, &features);
    let by_function = search_items(&doc, "", Some("function"), None, 200, &features);
    assert_eq!(
        by_fn.len(), by_function.len(),
        "kind='fn' and kind='function' should return same count"
    );
}

#[test]
fn fixture_rmcp_search_kind_struct_returns_only_structs() {
    let doc = load_rmcp();
    let features = HashSet::new();
    let results = search_items(&doc, "", Some("struct"), None, 50, &features);
    assert!(!results.is_empty(), "kind=struct should return results");
    for r in &results {
        assert_eq!(r.kind, "struct", "kind=struct filter must only return structs, got: {}", r.kind);
    }
}

#[test]
fn fixture_rmcp_search_kind_trait_returns_only_traits() {
    let doc = load_rmcp();
    let features = HashSet::new();
    let results = search_items(&doc, "", Some("trait"), None, 50, &features);
    assert!(!results.is_empty(), "kind=trait should return results");
    for r in &results {
        assert_eq!(r.kind, "trait", "kind=trait filter must only return traits, got: {}", r.kind);
    }
}

#[test]
fn fixture_rmcp_search_limit_respected() {
    let doc = load_rmcp();
    let features = HashSet::new();
    let results = search_items(&doc, "", None, None, 5, &features);
    assert!(results.len() <= 5, "limit=5 should return at most 5 results, got {}", results.len());
}

#[test]
fn fixture_rmcp_search_results_have_nonempty_paths() {
    let doc = load_rmcp();
    let features = HashSet::new();
    let results = search_items(&doc, "new", None, None, 20, &features);
    for r in &results {
        assert!(!r.path.is_empty(), "search result path must not be empty");
        assert!(!r.kind.is_empty(), "search result kind must not be empty");
    }
}

#[test]
fn fixture_rmcp_search_module_prefix_filter() {
    let doc = load_rmcp();
    let features = HashSet::new();
    let results = search_items(&doc, "", None, Some("rmcp::transport"), 50, &features);
    for r in &results {
        assert!(
            r.path.starts_with("rmcp::transport"),
            "module_prefix filter must apply, got: {}",
            r.path
        );
    }
}

// ─── Path table tests ─────────────────────────────────────────────────────────

#[test]
fn fixture_rmcp_paths_table_has_tokiochildprocess() {
    let doc = load_rmcp();
    // TokioChildProcess is stored as path=['rmcp','transport','child_process','TokioChildProcess']
    let found = doc.paths.values().any(|p| p.full_path() == "rmcp::transport::child_process::TokioChildProcess");
    assert!(found, "paths table should contain TokioChildProcess at its canonical path");
}

#[test]
fn fixture_rmcp_paths_kind_is_string() {
    let doc = load_rmcp();
    // v57: PathEntry.kind is a string like "struct", not a u32
    for p in doc.paths.values() {
        let k = p.kind_name();
        assert!(!k.is_empty(), "path entry kind must not be empty");
        // Should be a known kind name
        let valid = ["struct", "enum", "union", "function", "trait", "trait_alias",
                     "module", "type_alias", "constant", "macro", "use", "impl",
                     "variant", "struct_field", "assoc_type", "assoc_const", "primitive",
                     "extern_crate", "proc_attribute", "proc_derive"];
        assert!(valid.contains(&k), "path kind '{k}' should be a known kind name");
    }
}

// ─── type_to_string v57 direct path format ────────────────────────────────────

#[test]
fn type_direct_path_object_v57() {
    // In v57, trait bounds use a direct path object: {"id": N, "path": "SomeTrait", "args": null}
    // (not wrapped in "resolved_path")
    let ty = serde_json::json!({
        "id": 42,
        "path": "SomeTrait",
        "args": null
    });
    assert_eq!(type_to_string(&ty), "SomeTrait");
}

#[test]
fn type_direct_path_object_with_generic_args() {
    let ty = serde_json::json!({
        "id": 42,
        "path": "Iterator",
        "args": {
            "angle_bracketed": {
                "args": [{"type": {"primitive": "u8"}}]
            }
        }
    });
    assert_eq!(type_to_string(&ty), "Iterator<u8>");
}

// ─── dyn_trait lifetime formatting ────────────────────────────────────────────

#[test]
fn type_dyn_trait_no_lifetime() {
    // dyn_trait without a lifetime: "dyn Error"
    let ty = serde_json::json!({
        "dyn_trait": {
            "lifetime": null,
            "traits": [
                {
                    "trait": {"id": 1, "path": "Error", "args": null}
                }
            ]
        }
    });
    assert_eq!(type_to_string(&ty), "dyn Error");
}

#[test]
fn type_dyn_trait_with_static_lifetime() {
    // JSON has "lifetime": "'static" (already includes apostrophe).
    // Must produce "dyn Error + 'static", NOT "dyn Error + ''static".
    let ty = serde_json::json!({
        "dyn_trait": {
            "lifetime": "'static",
            "traits": [
                {
                    "trait": {"id": 1, "path": "Error", "args": null}
                }
            ]
        }
    });
    let result = type_to_string(&ty);
    assert_eq!(result, "dyn Error + 'static", "got: {result}");
    assert!(!result.contains("''"), "must not produce double-apostrophe, got: {result}");
}

#[test]
fn type_dyn_trait_with_named_lifetime() {
    // JSON has "lifetime": "'a" — should produce "dyn Trait + 'a"
    let ty = serde_json::json!({
        "dyn_trait": {
            "lifetime": "'a",
            "traits": [
                {
                    "trait": {"id": 1, "path": "Trait", "args": null}
                }
            ]
        }
    });
    let result = type_to_string(&ty);
    assert_eq!(result, "dyn Trait + 'a", "got: {result}");
    assert!(!result.contains("''"), "must not produce double-apostrophe, got: {result}");
}

#[test]
fn type_dyn_trait_multi_bound() {
    // dyn Send + Sync + 'static
    let ty = serde_json::json!({
        "dyn_trait": {
            "lifetime": "'static",
            "traits": [
                {"trait": {"id": 1, "path": "Send", "args": null}},
                {"trait": {"id": 2, "path": "Sync", "args": null}}
            ]
        }
    });
    let result = type_to_string(&ty);
    assert_eq!(result, "dyn Send + Sync + 'static", "got: {result}");
}

// ─── borrowed_ref lifetime normalization ──────────────────────────────────────

#[test]
fn type_ref_lifetime_bare_name() {
    // v57 (rmcp fixture style): lifetime stored as bare "a" (no apostrophe)
    let ty = serde_json::json!({
        "borrowed_ref": {
            "lifetime": "a",
            "mutable": false,
            "type": {"generic": "T"}
        }
    });
    let result = type_to_string(&ty);
    assert_eq!(result, "&'a T", "got: {result}");
    assert!(!result.contains("''"), "must not produce double-apostrophe, got: {result}");
}

#[test]
fn type_ref_lifetime_with_apostrophe() {
    // docs.rs style: lifetime stored as "'a" (apostrophe already present in JSON)
    let ty = serde_json::json!({
        "borrowed_ref": {
            "lifetime": "'a",
            "mutable": false,
            "type": {"generic": "T"}
        }
    });
    let result = type_to_string(&ty);
    assert_eq!(result, "&'a T", "got: {result}");
    assert!(!result.contains("''"), "must not produce double-apostrophe, got: {result}");
}

#[test]
fn type_ref_static_lifetime_with_apostrophe() {
    // Lifetime "'static" stored with apostrophe — must not become "''static"
    let ty = serde_json::json!({
        "borrowed_ref": {
            "lifetime": "'static",
            "mutable": false,
            "type": {"primitive": "str"}
        }
    });
    let result = type_to_string(&ty);
    assert_eq!(result, "&'static str", "got: {result}");
    assert!(!result.contains("''"), "must not produce double-apostrophe, got: {result}");
}

// ─── function_signature: self receiver normalization ──────────────────────────

/// Build a minimal Item for testing function_signature.
fn make_fn_item(name: &str, inputs: serde_json::Value, output: Option<serde_json::Value>, generics: Option<serde_json::Value>) -> docs_mcp::docsrs::Item {
    serde_json::from_value(serde_json::json!({
        "id": 1,
        "name": name,
        "docs": null,
        "attrs": [],
        "deprecation": null,
        "span": null,
        "visibility": null,
        "links": null,
        "inner": {
            "function": {
                "sig": {
                    "inputs": inputs,
                    "output": output
                },
                "generics": generics.unwrap_or(serde_json::json!({"params": [], "where_predicates": []})),
                "header": {
                    "is_async": false,
                    "is_const": false,
                    "is_unsafe": false
                }
            }
        }
    })).unwrap()
}

#[test]
fn function_signature_self_ref_normalized() {
    // &self should NOT be rendered as "self: &Self"
    let item = make_fn_item("foo", serde_json::json!([
        ["self", {"borrowed_ref": {"lifetime": null, "mutable": false, "type": {"generic": "Self"}}}]
    ]), None, None);
    let sig = function_signature(&item);
    assert!(sig.contains("(&self)"), "expected &self in sig, got: {sig}");
    assert!(!sig.contains("self: &Self"), "must not contain 'self: &Self', got: {sig}");
}

#[test]
fn function_signature_self_mut_ref_normalized() {
    // &mut self should NOT be rendered as "self: &mut Self"
    let item = make_fn_item("bar", serde_json::json!([
        ["self", {"borrowed_ref": {"lifetime": null, "mutable": true, "type": {"generic": "Self"}}}]
    ]), None, None);
    let sig = function_signature(&item);
    assert!(sig.contains("(&mut self)"), "expected &mut self in sig, got: {sig}");
    assert!(!sig.contains("self: &mut Self"), "must not contain 'self: &mut Self', got: {sig}");
}

#[test]
fn function_signature_consuming_self_normalized() {
    // Consuming self: Self should render as just "self"
    let item = make_fn_item("into_inner", serde_json::json!([
        ["self", {"generic": "Self"}]
    ]), None, None);
    let sig = function_signature(&item);
    // Should be "fn into_inner(self)" not "fn into_inner(self: Self)"
    assert!(sig.contains("(self)"), "expected (self) in sig, got: {sig}");
    assert!(!sig.contains("self: Self"), "must not contain 'self: Self', got: {sig}");
}

#[test]
fn function_signature_self_and_other_params() {
    // Ensure normalization works alongside regular params
    let item = make_fn_item("get", serde_json::json!([
        ["self", {"borrowed_ref": {"lifetime": null, "mutable": false, "type": {"generic": "Self"}}}],
        ["index", {"primitive": "usize"}]
    ]), Some(serde_json::json!({"primitive": "bool"})), None);
    let sig = function_signature(&item);
    assert!(sig.starts_with("fn get(&self, index: usize)"), "got: {sig}");
}

// ─── function_signature: impl Trait synthetic params ─────────────────────────

#[test]
fn function_signature_impl_trait_not_in_generics() {
    // A function `fn foo(f: impl Fn())` should NOT emit `<impl Fn(): Fn()>` in the generic list.
    // The synthetic generic param name "impl Fn()" should be excluded from <...>.
    let item = make_fn_item("foo", serde_json::json!([
        ["f", {"generic": "impl Fn()"}]
    ]), None, Some(serde_json::json!({
        "params": [
            {
                "name": "impl Fn()",
                "kind": {
                    "type": {
                        "bounds": [
                            {"trait_bound": {"trait": {"id": 1, "path": "Fn", "args": null}}}
                        ]
                    }
                }
            }
        ],
        "where_predicates": []
    })));
    let sig = function_signature(&item);
    // Should not include the synthetic param in the generic list
    assert!(!sig.contains("<impl Fn()"), "sig must not include synthetic impl Trait in generics, got: {sig}");
    // The param type should still appear naturally in the inputs
    assert!(sig.contains("impl Fn()"), "impl Trait type should appear in param list, got: {sig}");
}

// ─── function_signature: empty where clause predicates ───────────────────────

#[test]
fn function_signature_empty_where_bounds_skipped() {
    // A where predicate with no bounds should not produce "T: " dangling text.
    let item = make_fn_item("process", serde_json::json!([
        ["data", {"generic": "T"}]
    ]), None, Some(serde_json::json!({
        "params": [
            {"name": "T", "kind": {"type": {"bounds": []}}}
        ],
        "where_predicates": [
            {
                "bound_predicate": {
                    "type": {"generic": "T"},
                    "bounds": []
                }
            }
        ]
    })));
    let sig = function_signature(&item);
    assert!(!sig.contains("T: "), "empty where predicate must not produce 'T: ', got: {sig}");
    assert!(!sig.contains("where"), "empty where clause must be omitted, got: {sig}");
}

// ─── qualified_path type rendering ───────────────────────────────────────────

#[test]
fn type_qualified_path_with_trait_renders_full_form() {
    // <T as Service<Request>>::Response — trait is present
    let ty = serde_json::json!({
        "qualified_path": {
            "name": "Response",
            "args": {"angle_bracketed": {"args": []}},
            "self_type": {"generic": "T"},
            "trait": {"id": 42, "path": "Service", "args": {"angle_bracketed": {"args": [
                {"type": {"resolved_path": {"path": "Request", "args": null}}}
            ]}}}
        }
    });
    let result = type_to_string(&ty);
    assert_eq!(result, "<T as Service<Request>>::Response", "got: {result}");
    assert!(!result.contains("<T as _>"), "must not use anonymous trait, got: {result}");
}

#[test]
fn type_qualified_path_absent_trait_renders_shorthand() {
    // When trait field is absent, output T::Name (not <T as _>::Name)
    let ty = serde_json::json!({
        "qualified_path": {
            "name": "Output",
            "args": {"angle_bracketed": {"args": []}},
            "self_type": {"generic": "T"}
            // "trait" key intentionally absent
        }
    });
    let result = type_to_string(&ty);
    assert_eq!(result, "T::Output", "got: {result}");
    assert!(!result.contains("<T as _>"), "must not use anonymous trait, got: {result}");
}

#[test]
fn type_qualified_path_null_trait_renders_shorthand() {
    // When trait field is null, output T::Name (not <T as ()>::Name or <T as _>::Name)
    let ty = serde_json::json!({
        "qualified_path": {
            "name": "Item",
            "args": {"angle_bracketed": {"args": []}},
            "self_type": {"generic": "I"},
            "trait": null
        }
    });
    let result = type_to_string(&ty);
    assert_eq!(result, "I::Item", "got: {result}");
    assert!(!result.contains("as _"), "must not use anonymous trait, got: {result}");
    assert!(!result.contains("as ()"), "must not use () as trait, got: {result}");
}

// ─── const generic params ─────────────────────────────────────────────────────

#[test]
fn fixture_rmcp_const_generics_have_const_keyword() {
    // ClientCapabilitiesBuilderState (id=1101) has const bool params like EXPERIMENTAL, ROOTS, etc.
    // format_generics_for_item should produce <const EXPERIMENTAL: bool, ...>
    let doc = load_rmcp();
    let item = doc.index.get("1101").expect("ClientCapabilitiesBuilderState (id=1101) must exist");
    let generics = format_generics_for_item(item, "struct");
    assert!(!generics.is_empty(), "ClientCapabilitiesBuilderState should have generics, got empty");
    assert!(generics.contains("const "), "generics must include 'const ' keyword, got: {generics}");
    assert!(generics.contains("bool"), "generics must include 'bool' type, got: {generics}");
    assert!(generics.contains("EXPERIMENTAL"), "generics must include EXPERIMENTAL param, got: {generics}");
}

#[test]
fn type_const_generic_in_format_generics() {
    // Test format_generics_for_item on an item we build ourselves
    use docs_mcp::docsrs::Item;
    let item: Item = serde_json::from_value(serde_json::json!({
        "id": 999,
        "name": "Array",
        "docs": null,
        "attrs": [],
        "deprecation": null,
        "span": null,
        "visibility": null,
        "links": null,
        "inner": {
            "struct": {
                "generics": {
                    "params": [
                        {
                            "name": "N",
                            "kind": {
                                "const": {
                                    "type": {"primitive": "usize"},
                                    "default": null
                                }
                            }
                        }
                    ],
                    "where_predicates": []
                },
                "kind": {"unit": null},
                "impls": []
            }
        }
    })).unwrap();
    let generics = format_generics_for_item(&item, "struct");
    assert_eq!(generics, "<const N: usize>", "got: {generics}");
}

// ─── struct/enum signature includes generics ──────────────────────────────────

#[test]
fn fixture_rmcp_struct_with_lifetime_includes_lifetime_in_generics() {
    // PromptContext (id=10184) has params ["'a", "S"]
    // format_generics_for_item should return "<'a, S>"
    let doc = load_rmcp();
    let item = doc.index.get("10184").expect("PromptContext (id=10184) must exist");
    let generics = format_generics_for_item(item, "struct");
    assert!(!generics.is_empty(), "PromptContext should have generics");
    assert!(generics.contains("'a"), "PromptContext generics must include lifetime 'a, got: {generics}");
    assert!(generics.contains('S'), "PromptContext generics must include type param S, got: {generics}");
}

// ─── method search tests ─────────────────────────────────────────────────────

#[test]
fn search_methods_finds_inherent_methods_without_kind_filter() {
    // TokioChildProcess has 6 inherent methods (new, builder, id, graceful_shutdown, into_inner, split)
    // These are NOT in doc.paths, so only the method search pass finds them.
    let doc = load_rmcp();
    let features = HashSet::new();
    // Search by type name — the method pass should match methods whose parent path contains the query.
    let results = search_items(&doc, "TokioChildProcess", None, None, 50, &features);
    let method_results: Vec<_> = results.iter().filter(|r| r.kind == "method").collect();
    assert!(!method_results.is_empty(), "search for 'TokioChildProcess' with no kind filter should find methods");
    let paths: Vec<&str> = method_results.iter().map(|r| r.path.as_str()).collect();
    // All method results should be on TokioChildProcess
    assert!(
        paths.iter().all(|p| p.contains("TokioChildProcess")),
        "all method results should be on TokioChildProcess, got: {paths:?}"
    );
    // Its ::new method should be present
    assert!(
        paths.iter().any(|p| p.ends_with("::new")),
        "TokioChildProcess::new should appear as a method result, got: {paths:?}"
    );
}

#[test]
fn search_methods_kind_method_filter_returns_only_methods() {
    let doc = load_rmcp();
    let features = HashSet::new();
    let results = search_items(&doc, "", Some("method"), None, 50, &features);
    assert!(!results.is_empty(), "kind=method should return results");
    for r in &results {
        assert_eq!(r.kind, "method", "kind=method must only return methods, got: {}", r.kind);
    }
}

#[test]
fn search_methods_kind_fn_excludes_methods() {
    // kind="fn" should only return free functions, NOT inherent methods
    let doc = load_rmcp();
    let features = HashSet::new();
    let results = search_items(&doc, "", Some("fn"), None, 200, &features);
    for r in &results {
        assert_ne!(r.kind, "method", "kind=fn must not return methods, got method: {}", r.path);
    }
}

#[test]
fn search_methods_path_includes_parent_type() {
    // Method paths should be "ParentType::method_name"
    let doc = load_rmcp();
    let features = HashSet::new();
    let results = search_items(&doc, "", Some("method"), None, 50, &features);
    for r in &results {
        assert!(
            r.path.contains("::"),
            "method path must contain '::' (ParentType::method), got: {}",
            r.path
        );
    }
}

#[test]
fn search_methods_signature_contains_fn_keyword() {
    let doc = load_rmcp();
    let features = HashSet::new();
    let results = search_items(&doc, "new", Some("method"), None, 20, &features);
    for r in &results {
        assert!(
            r.signature.contains("fn "),
            "method signature must contain 'fn ', got: {}",
            r.signature
        );
    }
}

// ─── html_to_text entity decoding ────────────────────────────────────────────
