use super::{
    audit_tool, cached_tools_list, load_servers, probe_timeout_for, server_runtime_callable,
};
use crate::json::JsonValue;
use crate::json_helpers;
use std::path::Path;
use std::time::Instant;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpstreamProjectionSafety {
    /// Project only tools that look read-only or otherwise low-risk from MCP annotations/policies.
    Safe,
    /// Project low-risk tools plus tools with unknown semantics; keep policy-guarded tools hidden.
    Review,
    /// Project every callable upstream tool. Runtime policy checks still apply at call time.
    All,
}

impl UpstreamProjectionSafety {
    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().replace('_', "-").as_str() {
            "all" | "unsafe" | "maximum" | "max" => Self::All,
            "review" | "unknown" | "balanced" => Self::Review,
            _ => Self::Safe,
        }
    }
}

pub fn decode_projected_tool_name(name: &str) -> Option<(String, String)> {
    let rest = name.strip_prefix("u_")?;
    let (server, tool) = rest.split_once('_')?;
    let server = decode_projected_component(server)?;
    let tool = decode_projected_component(tool)?;
    if server.trim().is_empty() || tool.trim().is_empty() {
        return None;
    }
    Some((server, tool))
}

pub fn projected_tool_catalog(
    root_path: &Path,
    timeout_ms: Option<u64>,
    refresh: bool,
    safety: UpstreamProjectionSafety,
    max_tools: Option<usize>,
) -> Result<JsonValue, String> {
    let started = Instant::now();
    let servers = load_servers(root_path)?;
    let mut projected = Vec::new();
    let mut server_count = 0usize;
    let mut ok_server_count = 0usize;
    let mut skipped_server_count = 0usize;
    let mut raw_tool_count = 0usize;
    let mut projected_tool_count = 0usize;
    let mut skipped_guarded_count = 0usize;
    let mut skipped_unknown_count = 0usize;
    let mut skipped_name_count = 0usize;
    let mut truncated = false;

    for server in servers.values() {
        server_count = server_count.saturating_add(1);
        let (runtime_callable, _, _) = server_runtime_callable(root_path, server);
        if !runtime_callable {
            skipped_server_count = skipped_server_count.saturating_add(1);
            continue;
        }
        let effective_timeout = probe_timeout_for(server, timeout_ms);
        let tools = match cached_tools_list(root_path, server, effective_timeout, refresh) {
            Ok((tools, _)) => tools,
            Err(_) => {
                skipped_server_count = skipped_server_count.saturating_add(1);
                continue;
            }
        };
        ok_server_count = ok_server_count.saturating_add(1);
        for tool in tools.as_array().unwrap_or(&[]) {
            raw_tool_count = raw_tool_count.saturating_add(1);
            let Some(original_tool_name) = json_helpers::string_at_path(tool, &["name"])
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                skipped_name_count = skipped_name_count.saturating_add(1);
                continue;
            };

            let audit = audit_tool(server, tool);
            let policy_covered =
                json_helpers::bool_at_path(&audit.value, &["policyCovered"]).unwrap_or(false);
            let guard_recommended =
                json_helpers::bool_at_path(&audit.value, &["guardRecommended"]).unwrap_or(false);
            let unknown_semantics =
                json_helpers::bool_at_path(&audit.value, &["unknownSemantics"]).unwrap_or(false);
            let should_skip = match safety {
                UpstreamProjectionSafety::All => false,
                UpstreamProjectionSafety::Review => policy_covered || guard_recommended,
                UpstreamProjectionSafety::Safe => {
                    policy_covered || guard_recommended || unknown_semantics
                }
            };
            if should_skip {
                if unknown_semantics {
                    skipped_unknown_count = skipped_unknown_count.saturating_add(1);
                } else {
                    skipped_guarded_count = skipped_guarded_count.saturating_add(1);
                }
                continue;
            }

            let Some(projected_name) = encode_projected_tool_name(&server.name, original_tool_name)
            else {
                skipped_name_count = skipped_name_count.saturating_add(1);
                continue;
            };
            if let Some(max_tools) = max_tools {
                if projected_tool_count >= max_tools {
                    truncated = true;
                    continue;
                }
            }
            projected.push(project_tool_definition(
                &server.name,
                original_tool_name,
                &projected_name,
                tool,
                &audit.value,
            ));
            projected_tool_count = projected_tool_count.saturating_add(1);
        }
    }

    Ok(JsonValue::object([
        ("ok", JsonValue::bool(true)),
        ("mode", JsonValue::string("projected-upstream-tools")),
        ("serverCount", JsonValue::number(server_count)),
        ("okServerCount", JsonValue::number(ok_server_count)),
        (
            "skippedServerCount",
            JsonValue::number(skipped_server_count),
        ),
        ("rawToolCount", JsonValue::number(raw_tool_count)),
        (
            "projectedToolCount",
            JsonValue::number(projected_tool_count),
        ),
        (
            "skippedGuardedToolCount",
            JsonValue::number(skipped_guarded_count),
        ),
        (
            "skippedUnknownToolCount",
            JsonValue::number(skipped_unknown_count),
        ),
        (
            "skippedNameToolCount",
            JsonValue::number(skipped_name_count),
        ),
        ("truncated", JsonValue::bool(truncated)),
        (
            "safety",
            JsonValue::string(match safety {
                UpstreamProjectionSafety::Safe => "safe",
                UpstreamProjectionSafety::Review => "review",
                UpstreamProjectionSafety::All => "all",
            }),
        ),
        (
            "maxTools",
            max_tools.map(JsonValue::number).unwrap_or(JsonValue::Null),
        ),
        (
            "elapsedMs",
            JsonValue::number(started.elapsed().as_millis()),
        ),
        ("tools", JsonValue::array(projected)),
    ]))
}

pub fn encode_projected_tool_name(server: &str, tool: &str) -> Option<String> {
    let server = encode_projected_component(server.trim());
    let tool = encode_projected_component(tool.trim());
    if server.is_empty() || tool.is_empty() {
        return None;
    }
    let name = format!("u_{}_{}", server, tool);
    if projected_tool_name_is_recommended_shape(&name) {
        Some(name)
    } else {
        None
    }
}

fn encode_projected_component(value: &str) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(value.len() * 2);
    for byte in value.as_bytes() {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    out
}

fn decode_projected_component(value: &str) -> Option<String> {
    if value.is_empty() || value.len() % 2 != 0 {
        return None;
    }
    let mut bytes = Vec::new();
    for index in (0..value.len()).step_by(2) {
        let byte = u8::from_str_radix(&value[index..index + 2], 16).ok()?;
        bytes.push(byte);
    }
    String::from_utf8(bytes).ok()
}

fn projected_tool_name_is_recommended_shape(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && !name.contains("__")
        && name.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '_' || character == '-'
        })
}

fn project_tool_definition(
    server_name: &str,
    original_tool_name: &str,
    projected_name: &str,
    tool: &JsonValue,
    audit: &JsonValue,
) -> JsonValue {
    let mut object = tool.as_object().cloned().unwrap_or_default();
    let title = json_helpers::string_at_path(tool, &["title"])
        .or_else(|| json_helpers::string_at_path(tool, &["name"]))
        .unwrap_or(original_tool_name)
        .to_string();
    let description = json_helpers::string_at_path(tool, &["description"])
        .unwrap_or("")
        .trim()
        .to_string();
    object.insert("name".to_string(), JsonValue::string(projected_name));
    object.insert(
        "title".to_string(),
        JsonValue::string(format!("{} · {}", server_name, title)),
    );
    object.insert(
        "description".to_string(),
        JsonValue::string(projected_description(
            server_name,
            original_tool_name,
            &description,
        )),
    );
    object
        .entry("inputSchema".to_string())
        .or_insert_with(default_tool_input_schema);
    let mut meta = json_helpers::object_at_path(tool, &["_meta"])
        .cloned()
        .unwrap_or_default();
    meta.insert(
        "mcpace/upstreamServer".to_string(),
        JsonValue::string(server_name),
    );
    meta.insert(
        "mcpace/upstreamTool".to_string(),
        JsonValue::string(original_tool_name),
    );
    meta.insert("mcpace/projected".to_string(), JsonValue::bool(true));
    meta.insert(
        "mcpace/policyStatus".to_string(),
        json_helpers::string_at_path(audit, &["policyStatus"])
            .map(JsonValue::string)
            .unwrap_or(JsonValue::Null),
    );
    object.insert("_meta".to_string(), JsonValue::Object(meta));
    JsonValue::Object(object)
}

fn projected_description(server_name: &str, original_tool_name: &str, description: &str) -> String {
    let prefix = format!(
        "Upstream MCP tool `{}` on server `{}` routed through MCPace. ",
        original_tool_name, server_name
    );
    if description.is_empty() {
        prefix
    } else {
        format!("{}{}", prefix, description)
    }
}

fn default_tool_input_schema() -> JsonValue {
    JsonValue::object([("type", JsonValue::string("object"))])
}
