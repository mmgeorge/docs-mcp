use docs_mcp::sparse_index::{compute_path, find_latest_stable, parse_ndjson, IndexLine};

fn make_line(vers: &str, yanked: bool) -> IndexLine {
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

// ─── compute_path ─────────────────────────────────────────────────────────────

#[test]
fn path_1_char() {
    assert_eq!(compute_path("a"), "1/a");
}

#[test]
fn path_2_chars() {
    assert_eq!(compute_path("io"), "2/io");
}

#[test]
fn path_3_chars() {
    assert_eq!(compute_path("url"), "3/u/url");
}

#[test]
fn path_4_plus_chars() {
    assert_eq!(compute_path("serde"), "se/rd/serde");
}

#[test]
fn path_uppercase_normalised() {
    assert_eq!(compute_path("SERDE"), "se/rd/serde");
}

#[test]
fn path_exactly_4_chars() {
    assert_eq!(compute_path("toml"), "to/ml/toml");
}

// ─── NDJSON parsing ──────────────────────────────────────────────────────────

#[test]
fn ndjson_parses_multiple_lines() {
    let ndjson = r#"{"name":"serde","vers":"1.0.0","deps":[],"cksum":"aaa","features":{},"yanked":false}
{"name":"serde","vers":"1.0.1","deps":[],"cksum":"bbb","features":{},"yanked":false}
"#;
    let lines = parse_ndjson(ndjson).unwrap();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].vers, "1.0.0");
    assert_eq!(lines[1].vers, "1.0.1");
}

#[test]
fn ndjson_skips_empty_lines() {
    let ndjson = r#"{"name":"a","vers":"1.0.0","deps":[],"cksum":"x","features":{},"yanked":false}

{"name":"a","vers":"1.0.1","deps":[],"cksum":"y","features":{},"yanked":false}
"#;
    let lines = parse_ndjson(ndjson).unwrap();
    assert_eq!(lines.len(), 2);
}

#[test]
fn ndjson_parses_features() {
    let ndjson = r#"{"name":"tokio","vers":"1.0.0","deps":[],"cksum":"x","features":{"full":["rt","sync"]},"yanked":false}"#;
    let lines = parse_ndjson(ndjson).unwrap();
    assert_eq!(lines.len(), 1);
    assert!(lines[0].features.contains_key("full"));
}

// ─── find_latest_stable ───────────────────────────────────────────────────────

#[test]
fn latest_stable_basic() {
    let lines = vec![
        make_line("0.9.0", false),
        make_line("1.0.0", false),
        make_line("1.1.0", false),
    ];
    let latest = find_latest_stable(&lines).unwrap();
    assert_eq!(latest.vers, "1.1.0");
}

#[test]
fn latest_stable_ignores_yanked() {
    let lines = vec![
        make_line("1.0.0", false),
        make_line("1.1.0", true), // yanked
        make_line("0.9.0", false),
    ];
    let latest = find_latest_stable(&lines).unwrap();
    assert_eq!(latest.vers, "1.0.0");
}

#[test]
fn latest_stable_ignores_prerelease() {
    let lines = vec![
        make_line("1.0.0", false),
        make_line("2.0.0-alpha.1", false),
        make_line("1.5.0-beta", false),
    ];
    let latest = find_latest_stable(&lines).unwrap();
    assert_eq!(latest.vers, "1.0.0");
}

#[test]
fn latest_stable_fallback_to_prerelease_when_no_stable() {
    let lines = vec![
        make_line("0.1.0-alpha", false),
        make_line("0.2.0-beta", false),
    ];
    let latest = find_latest_stable(&lines).unwrap();
    assert_eq!(latest.vers, "0.2.0-beta");
}

#[test]
fn latest_stable_empty_list_returns_none() {
    let lines: Vec<IndexLine> = vec![];
    assert!(find_latest_stable(&lines).is_none());
}

#[test]
fn latest_stable_all_yanked_returns_none() {
    let lines = vec![
        make_line("1.0.0", true),
        make_line("1.1.0", true),
    ];
    assert!(find_latest_stable(&lines).is_none());
}
