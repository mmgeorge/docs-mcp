use docs_mcp::cache::decompress_zstd;

fn zstd_compress(data: &[u8]) -> Vec<u8> {
    zstd::encode_all(std::io::Cursor::new(data), 0).unwrap()
}

#[test]
fn decompress_roundtrip_json() {
    let original = r#"{"format_version":57,"root":1,"index":{},"paths":{}}"#;
    let compressed = zstd_compress(original.as_bytes());
    let result = decompress_zstd(&compressed).unwrap();
    assert_eq!(result, original);
    // Verify it parses as valid JSON
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["format_version"], 57);
}

#[test]
fn decompress_empty_json_object() {
    let compressed = zstd_compress(b"{}");
    let result = decompress_zstd(&compressed).unwrap();
    assert_eq!(result, "{}");
}

#[test]
fn decompress_large_json() {
    // Simulate a larger JSON document to exercise streaming decompression
    let mut json = String::from("{\"items\":[");
    for i in 0..1000 {
        if i > 0 {
            json.push(',');
        }
        json.push_str(&format!("{{\"id\":{i},\"name\":\"item_{i}\"}}"));
    }
    json.push_str("]}");

    let compressed = zstd_compress(json.as_bytes());
    let result = decompress_zstd(&compressed).unwrap();
    assert_eq!(result, json);
}

#[test]
fn decompress_errors_on_invalid_zstd_bytes() {
    let err = decompress_zstd(b"this is not zstd data").unwrap_err();
    assert!(
        err.to_string().contains("Zstd decompression failed"),
        "unexpected error: {err}"
    );
}

#[test]
fn decompress_errors_on_empty_input() {
    let err = decompress_zstd(b"").unwrap_err();
    assert!(
        err.to_string().contains("Zstd decompression failed"),
        "unexpected error: {err}"
    );
}

#[test]
fn decompress_errors_on_non_utf8_output() {
    // Create a valid zstd frame whose payload is invalid UTF-8
    let invalid_utf8: &[u8] = &[0xFF, 0xFE, 0x80];
    let compressed = zstd_compress(invalid_utf8);
    let err = decompress_zstd(&compressed).unwrap_err();
    assert!(
        err.to_string().contains("not valid UTF-8"),
        "unexpected error: {err}"
    );
}

/// Test with the real downloaded .zst fixture from docs.rs (if present).
#[test]
fn decompress_real_docsrs_fixture_if_present() {
    let fixture = "D:\\code\\docs-mcp\\.temp\\serde_test.json.zst";
    if !std::path::Path::new(fixture).exists() {
        eprintln!("Skipping: fixture not found at {fixture}");
        return;
    }
    let bytes = std::fs::read(fixture).unwrap();
    let json = decompress_zstd(&bytes).unwrap();
    assert!(json.starts_with('{'), "decompressed content should be JSON object");
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(parsed["format_version"].as_u64().unwrap_or(0) >= 33);
}
