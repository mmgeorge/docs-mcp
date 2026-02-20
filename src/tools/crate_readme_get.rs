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

/// Strip HTML tags, preserving code blocks as fenced markdown.
fn html_to_text(html: &str) -> String {
    let mut output = String::new();
    let mut in_pre = false;
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
                "pre" | "code" => {
                    if !in_pre {
                        in_pre = true;
                        output.push_str("\n```\n");
                    }
                }
                "/pre" | "/code" => {
                    if in_pre {
                        in_pre = false;
                        output.push_str("\n```\n");
                    }
                }
                "p" | "/p" | "br" | "br/" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                    output.push('\n');
                }
                "/h1" | "/h2" | "/h3" | "/h4" | "/h5" | "/h6" => {
                    output.push_str("\n\n");
                }
                "li" => {
                    output.push_str("\n- ");
                }
                _ => {}
            }
        } else if in_tag {
            tag_buf.push(ch);
        } else {
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
