use rmcp::{ErrorData, model::{CallToolResult, Content}};
use serde::Deserialize;
use rmcp::schemars::{self, JsonSchema};
use serde_json::json;

use super::AppState;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CrateReadmeGetParams {
    /// Crate name
    pub name: String,
    /// Version string. Defaults to latest stable.
    pub version: Option<String>,
}

pub async fn execute(state: &AppState, params: CrateReadmeGetParams) -> Result<CallToolResult, ErrorData> {
    let name = &params.name;
    let version = state.resolve_version(name, params.version.as_deref()).await
        .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

    let client = crate::cratesio::CratesIoClient::new(&state.client, &state.cache);
    let readme_html = client.get_readme(name, &version).await
        .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

    let readme_text = html_to_text(&readme_html);

    let output = json!({
        "name": name,
        "version": version,
        "readme_text": readme_text,
        "readme_html_url": format!("https://crates.io/crates/{name}/{version}/readme"),
    });

    let json = serde_json::to_string_pretty(&output)
        .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

    Ok(CallToolResult::success(vec![Content::text(json)]))
}

/// Convert HTML to plain text, preserving structure as best as possible.
///
/// Key behaviours:
/// - `<pre>`/`<code>` blocks → fenced ``` markdown
/// - `<img alt="...">` → `[alt text]` so badges/shields show their label
/// - `<td>`/`<th>` → cell separator so table rows aren't mashed together
/// - `<script>`/`<style>` content is skipped entirely
/// - HTML entities are decoded
fn html_to_text(html: &str) -> String {
    let mut output = String::new();
    let mut in_pre = false;
    let mut in_code = false; // inline code (not inside pre)
    let mut skip_content = false; // inside <script> or <style>
    let mut tag_buf = String::new();
    let mut in_tag = false;

    for ch in html.chars() {
        if ch == '<' {
            in_tag = true;
            tag_buf.clear();
        } else if ch == '>' && in_tag {
            in_tag = false;
            let tag_lower = tag_buf.trim().to_lowercase();
            let tag_name = tag_lower.split_whitespace().next().unwrap_or("");
            match tag_name {
                "script" | "style" => { skip_content = true; }
                "/script" | "/style" => { skip_content = false; }
                "pre" => {
                    if !in_pre {
                        in_pre = true;
                        in_code = false;
                        output.push_str("\n```\n");
                    }
                }
                "/pre" => {
                    if in_pre {
                        in_pre = false;
                        output.push_str("\n```\n");
                    }
                }
                "code" => {
                    if !in_pre {
                        in_code = true;
                        output.push('`');
                    }
                }
                "/code" => {
                    if !in_pre && in_code {
                        in_code = false;
                        output.push('`');
                    }
                }
                "img" => {
                    // Emit alt text for badges and images so content isn't lost
                    if let Some(alt) = extract_attr(&tag_lower, "alt") {
                        if !alt.is_empty() {
                            output.push('[');
                            output.push_str(&alt);
                            output.push(']');
                        }
                    }
                }
                "p" | "/p" | "br" | "br/" => { output.push('\n'); }
                "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => { output.push('\n'); }
                "/h1" | "/h2" | "/h3" | "/h4" | "/h5" | "/h6" => { output.push_str("\n\n"); }
                "li" => { output.push_str("\n- "); }
                "td" | "th" => { output.push_str("  "); }
                "/tr" => { output.push('\n'); }
                _ => {}
            }
        } else if in_tag {
            tag_buf.push(ch);
        } else if !skip_content {
            output.push(ch);
        }
    }

    // Decode HTML entities
    let output = decode_html_entities(&output);

    // Collapse excessive blank lines
    let mut result = String::new();
    let mut blank_count = 0;
    for line in output.lines() {
        if line.trim().is_empty() {
            blank_count += 1;
            if blank_count <= 2 {
                result.push('\n');
            }
        } else {
            blank_count = 0;
            result.push_str(line);
            result.push('\n');
        }
    }
    result
}

/// Extract a named attribute value from a lowercased tag string.
/// Handles both double-quoted (`attr="val"`) and single-quoted (`attr='val'`) forms.
fn extract_attr(tag_lower: &str, attr: &str) -> Option<String> {
    // Try double-quoted first
    let dq_needle = format!("{attr}=\"");
    if let Some(start) = tag_lower.find(&dq_needle) {
        let rest = &tag_lower[start + dq_needle.len()..];
        if let Some(end) = rest.find('"') {
            return Some(rest[..end].to_string());
        }
    }
    // Try single-quoted
    let sq_needle = format!("{attr}='");
    if let Some(start) = tag_lower.find(&sq_needle) {
        let rest = &tag_lower[start + sq_needle.len()..];
        if let Some(end) = rest.find('\'') {
            return Some(rest[..end].to_string());
        }
    }
    None
}

/// Decode common HTML entities to their character equivalents.
fn decode_html_entities(s: &str) -> String {
    s.replace("&amp;", "&")
     .replace("&lt;", "<")
     .replace("&gt;", ">")
     .replace("&quot;", "\"")
     .replace("&#39;", "'")
     .replace("&apos;", "'")
     .replace("&nbsp;", " ")
     .replace("&#x27;", "'")
     .replace("&#x2F;", "/")
     .replace("&#x60;", "`")
     .replace("&#x3D;", "=")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn img_alt_text_is_preserved() {
        let html = r#"<img src="https://shields.io/badge" alt="build: passing">"#;
        let text = html_to_text(html);
        assert!(text.contains("[build: passing]"), "img alt should appear as [text], got: {text}");
    }

    #[test]
    fn img_without_alt_emits_nothing() {
        let html = r#"<img src="logo.png">"#;
        let text = html_to_text(html);
        assert!(!text.contains('['), "img without alt should emit nothing, got: {text}");
    }

    #[test]
    fn script_content_is_skipped() {
        let html = "<p>before</p><script>var x = secret();</script><p>after</p>";
        let text = html_to_text(html);
        assert!(!text.contains("secret"), "script content must be skipped, got: {text}");
        assert!(text.contains("before"), "content before script must appear");
        assert!(text.contains("after"), "content after script must appear");
    }

    #[test]
    fn style_content_is_skipped() {
        let html = "<p>text</p><style>.foo { color: red; }</style><p>more</p>";
        let text = html_to_text(html);
        assert!(!text.contains("color"), "style content must be skipped, got: {text}");
        assert!(text.contains("text"), "content before style must appear");
        assert!(text.contains("more"), "content after style must appear");
    }

    #[test]
    fn table_cells_are_separated() {
        let html = "<table><tr><td>Cell A</td><td>Cell B</td></tr></table>";
        let text = html_to_text(html);
        assert!(text.contains("Cell A"), "first cell must appear");
        assert!(text.contains("Cell B"), "second cell must appear");
        // Cells should be separated by whitespace, not jammed together
        assert!(!text.contains("Cell ACell B"), "cells must not be concatenated without space");
    }

    #[test]
    fn inline_code_gets_backticks() {
        let html = "<p>Use the <code>spawn</code> function.</p>";
        let text = html_to_text(html);
        assert!(text.contains("`spawn`"), "inline code should be wrapped in backticks, got: {text}");
    }

    #[test]
    fn pre_code_block_gets_fences() {
        let html = "<pre><code>fn main() {}</code></pre>";
        let text = html_to_text(html);
        assert!(text.contains("```"), "pre block should produce fenced code block");
        assert!(text.contains("fn main()"), "code content should be preserved");
    }

    #[test]
    fn extract_attr_double_quoted() {
        assert_eq!(extract_attr(r#"img src="x.png" alt="hello""#, "alt"), Some("hello".to_string()));
    }

    #[test]
    fn extract_attr_single_quoted() {
        assert_eq!(extract_attr("img src='x.png' alt='world'", "alt"), Some("world".to_string()));
    }

    #[test]
    fn extract_attr_missing_returns_none() {
        assert_eq!(extract_attr("img src=\"x.png\"", "alt"), None);
    }
}
