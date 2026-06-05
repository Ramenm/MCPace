use super::sanitize_path_for_display;
use crate::codex_config;
use crate::json::{parse_str, JsonValue};
use crate::json_helpers;
use std::path::Path;

pub(super) fn build_unified_config_diff(path: &Path, before: &str, after: &str) -> String {
    if before == after {
        return String::new();
    }

    let display_path = sanitize_path_for_display(path);
    let mut diff = Vec::new();
    diff.push(format!("--- {} (current)", display_path));
    diff.push(format!("+++ {} (candidate)", display_path));

    let mut before_state = DiffSanitizeState::default();
    if !before.is_empty() {
        for line in before.lines() {
            diff.push(format!(
                "-{}",
                sanitize_config_diff_line(line, &mut before_state)
            ));
        }
    }
    let mut after_state = DiffSanitizeState::default();
    if !after.is_empty() {
        for line in after.lines() {
            diff.push(format!(
                "+{}",
                sanitize_config_diff_line(line, &mut after_state)
            ));
        }
    }

    diff.join("\n")
}

#[derive(Default)]
struct DiffSanitizeState {
    in_sensitive_multiline_value: bool,
    close_marker: Option<&'static str>,
}

fn sanitize_config_diff_line(line: &str, state: &mut DiffSanitizeState) -> String {
    let escaped = escape_diff_control_chars(line);
    if state.in_sensitive_multiline_value {
        if let Some(marker) = state.close_marker {
            if escaped.contains(marker) {
                state.in_sensitive_multiline_value = false;
                state.close_marker = None;
            }
            return "[REDACTED]".to_string();
        }
        if is_top_level_config_boundary(&escaped) {
            state.in_sensitive_multiline_value = false;
        } else {
            return "[REDACTED]".to_string();
        }
    }

    let separator_index = match (escaped.find('='), escaped.find(':')) {
        (Some(equal), Some(colon)) => Some(equal.min(colon)),
        (Some(equal), None) => Some(equal),
        (None, Some(colon)) => Some(colon),
        (None, None) => None,
    };
    let lower_line = escaped.to_ascii_lowercase();
    let key_area = separator_index
        .map(|index| &escaped[..index])
        .unwrap_or(&escaped)
        .to_ascii_lowercase();
    let sensitive_keys = [
        "token",
        "api_key",
        "apikey",
        "api-key",
        "private_key",
        "private-key",
        "secret",
        "password",
        "passwd",
        "auth",
        "authorization",
        "credential",
    ];
    if !sensitive_keys
        .iter()
        .any(|sensitive_key| lower_line.contains(sensitive_key))
    {
        return escaped;
    }

    state.in_sensitive_multiline_value = true;
    state.close_marker = sensitive_multiline_close_marker(&escaped);

    if !sensitive_keys
        .iter()
        .any(|sensitive_key| key_area.contains(sensitive_key))
    {
        return "[REDACTED]".to_string();
    }

    let Some(separator_index) = separator_index else {
        return "[REDACTED]".to_string();
    };
    let prefix = escaped[..=separator_index].trim_end();
    let suffix = if escaped.trim_end().ends_with(',') {
        ","
    } else {
        ""
    };
    format!("{} \"[redacted]\"{}", prefix, suffix)
}

fn sensitive_multiline_close_marker(line: &str) -> Option<&'static str> {
    if line.matches("\"\"\"").count() % 2 == 1 {
        return Some("\"\"\"");
    }
    if line.matches("'''").count() % 2 == 1 {
        return Some("'''");
    }
    None
}

fn is_top_level_config_boundary(line: &str) -> bool {
    let trimmed = line.trim_start();
    if trimmed.is_empty() || line.len() != trimmed.len() {
        return false;
    }
    if trimmed.starts_with('[') {
        return true;
    }
    let Some(separator_index) = trimmed.find('=').or_else(|| trimmed.find(':')) else {
        return false;
    };
    let key = trimmed[..separator_index].trim();
    !key.is_empty()
        && key
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | '"' | '\''))
}

fn escape_diff_control_chars(line: &str) -> String {
    let mut escaped = String::new();
    for ch in line.chars() {
        if ch == '\x1b' {
            escaped.push_str("\\x1b");
        } else if ch.is_control() && ch != '\t' {
            escaped.push_str(&format!("\\u{{{:x}}}", ch as u32));
        } else {
            escaped.push(ch);
        }
    }
    escaped
}

pub(super) fn detect_newline(existing: &str) -> &'static str {
    if existing.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

fn empty_json_object() -> JsonValue {
    json_helpers::empty_object()
}

pub(super) fn build_toml_managed_block(
    adapter_key_name: &str,
    server_url: &str,
    newline: &str,
) -> String {
    let lines = [
        format!("# BEGIN MCPACE MANAGED BLOCK: {}", adapter_key_name),
        "# This block is managed by `mcpace client install`.".to_string(),
        format!("[mcp_servers.{}]", format_toml_table_key(adapter_key_name)),
        format!("url = {}", codex_config::toml_basic_string(server_url)),
        "enabled = true".to_string(),
        "startup_timeout_sec = 20".to_string(),
        format!("# END MCPACE MANAGED BLOCK: {}", adapter_key_name),
        String::new(),
    ];
    lines.join(newline)
}

fn build_yaml_mcp_servers_entry_block(
    adapter_key_name: &str,
    server_url: &str,
    newline: &str,
) -> String {
    let lines = [
        format!("  # BEGIN MCPACE MANAGED BLOCK: {}", adapter_key_name),
        format!("  {}:", adapter_key_name),
        format!("    url: {}", yaml_double_quoted_string(server_url)),
        "    enabled: true".to_string(),
        "    timeout: 120".to_string(),
        "    connect_timeout: 60".to_string(),
        format!("  # END MCPACE MANAGED BLOCK: {}", adapter_key_name),
        String::new(),
    ];
    lines.join(newline)
}

pub(super) struct ClientConfigUpdate {
    pub(super) contents: String,
    pub(super) replaced_existing_block: bool,
}

pub(super) fn upsert_json_mcp_server(
    existing: &str,
    adapter_key_name: &str,
    servers_object_key: &str,
    server_config: JsonValue,
    config_path: &Path,
) -> Result<ClientConfigUpdate, String> {
    let mut root = if existing.trim().is_empty() {
        json_helpers::empty_object()
    } else {
        parse_str(existing).map_err(|error| {
            format!(
                "failed to parse JSON client config '{}': {}",
                config_path.display(),
                error
            )
        })?
    };

    let root_object = match &mut root {
        JsonValue::Object(map) => map,
        _ => {
            return Err(format!(
                "JSON client config '{}' must be a top-level object",
                config_path.display()
            ))
        }
    };

    let servers_key = if servers_object_key.trim().is_empty() {
        "mcpServers"
    } else {
        servers_object_key.trim()
    };
    let servers_value = root_object
        .entry(servers_key.to_string())
        .or_insert_with(empty_json_object);
    let servers_object = match servers_value {
        JsonValue::Object(map) => map,
        _ => {
            return Err(format!(
                "JSON client config '{}' has a non-object {} field",
                config_path.display(),
                servers_key
            ))
        }
    };

    let replaced_existing_block = servers_object.contains_key(adapter_key_name);
    servers_object.insert(adapter_key_name.to_string(), server_config);

    Ok(ClientConfigUpdate {
        contents: root.to_pretty_string(),
        replaced_existing_block,
    })
}

pub(super) fn upsert_toml_managed_block(
    existing: &str,
    adapter_key_name: &str,
    managed_block: &str,
    config_path: &Path,
) -> Result<ClientConfigUpdate, String> {
    if let Some(span) = find_toml_managed_block(existing, adapter_key_name, config_path)? {
        let updated = apply_toml_managed_block_update(existing, managed_block, span);
        return Ok(ClientConfigUpdate {
            contents: updated,
            replaced_existing_block: true,
        });
    }

    if let Some((start, end)) = find_toml_mcp_servers_table_block(existing, adapter_key_name) {
        let mut updated = String::new();
        updated.push_str(&existing[..start]);
        updated.push_str(managed_block);
        updated.push_str(&existing[end..]);
        return Ok(ClientConfigUpdate {
            contents: updated,
            replaced_existing_block: true,
        });
    }

    let newline = detect_newline(existing);
    let mut updated = existing.to_string();
    if !updated.is_empty() {
        if !updated.ends_with('\n') {
            updated.push_str(newline);
        }
        if !updated.ends_with(&(newline.to_string() + newline)) {
            updated.push_str(newline);
        }
    }
    updated.push_str(managed_block);
    Ok(ClientConfigUpdate {
        contents: updated,
        replaced_existing_block: false,
    })
}

pub(super) fn upsert_yaml_mcp_server(
    existing: &str,
    adapter_key_name: &str,
    server_url: &str,
    config_path: &Path,
) -> Result<ClientConfigUpdate, String> {
    let newline = detect_newline(existing);
    let entry_block = build_yaml_mcp_servers_entry_block(adapter_key_name, server_url, newline);

    if let Some((start, end)) = find_managed_block(existing, adapter_key_name, config_path)? {
        let mut updated = String::new();
        updated.push_str(&existing[..start]);
        updated.push_str(&entry_block);
        updated.push_str(&existing[end..]);
        return Ok(ClientConfigUpdate {
            contents: updated,
            replaced_existing_block: true,
        });
    }

    if let Some((section_start, section_body_start, section_end)) =
        find_yaml_top_level_section(existing, "mcp_servers")
    {
        if let Some((entry_start, entry_end)) =
            find_yaml_section_entry(existing, section_body_start, section_end, adapter_key_name)
        {
            let mut updated = String::new();
            updated.push_str(&existing[..entry_start]);
            updated.push_str(&entry_block);
            updated.push_str(&existing[entry_end..]);
            return Ok(ClientConfigUpdate {
                contents: updated,
                replaced_existing_block: true,
            });
        }

        let mut updated = String::new();
        updated.push_str(&existing[..section_end]);
        if section_end > section_start && !existing[..section_end].ends_with('\n') {
            updated.push_str(newline);
        }
        updated.push_str(&entry_block);
        updated.push_str(&existing[section_end..]);
        return Ok(ClientConfigUpdate {
            contents: updated,
            replaced_existing_block: false,
        });
    }

    let mut updated = existing.to_string();
    if !updated.is_empty() {
        if !updated.ends_with('\n') {
            updated.push_str(newline);
        }
        if !updated.ends_with(&(newline.to_string() + newline)) {
            updated.push_str(newline);
        }
    }
    updated.push_str("mcp_servers:");
    updated.push_str(newline);
    updated.push_str(&entry_block);
    Ok(ClientConfigUpdate {
        contents: updated,
        replaced_existing_block: false,
    })
}

enum TomlManagedBlockSpan {
    Exact {
        start: usize,
        end: usize,
    },
    RecoverOverwide {
        begin_line_start: usize,
        begin_line_end: usize,
        table_start: usize,
        table_end: usize,
        end_line_start: usize,
        end_line_end: usize,
        preserve_between_begin_and_table: bool,
    },
}

fn apply_toml_managed_block_update(
    existing: &str,
    managed_block: &str,
    span: TomlManagedBlockSpan,
) -> String {
    match span {
        TomlManagedBlockSpan::Exact { start, end } => {
            let mut updated = String::new();
            updated.push_str(&existing[..start]);
            updated.push_str(managed_block);
            updated.push_str(&existing[end..]);
            updated
        }
        TomlManagedBlockSpan::RecoverOverwide {
            begin_line_start,
            begin_line_end,
            table_start,
            table_end,
            end_line_start,
            end_line_end,
            preserve_between_begin_and_table,
        } => {
            let mut updated = String::new();
            updated.push_str(&existing[..begin_line_start]);
            if preserve_between_begin_and_table {
                updated.push_str(&existing[begin_line_end..table_start]);
            }
            updated.push_str(managed_block);
            updated.push_str(&existing[table_end..end_line_start]);
            updated.push_str(&existing[end_line_end..]);
            updated
        }
    }
}

fn find_managed_block(
    existing: &str,
    adapter_key_name: &str,
    config_path: &Path,
) -> Result<Option<(usize, usize)>, String> {
    let begin_marker = format!("# BEGIN MCPACE MANAGED BLOCK: {}", adapter_key_name);
    let end_marker = format!("# END MCPACE MANAGED BLOCK: {}", adapter_key_name);
    let Some(marker_start) = existing.find(&begin_marker) else {
        return Ok(None);
    };
    let start = existing[..marker_start]
        .rfind('\n')
        .map(|index| index + 1)
        .unwrap_or(0);
    let Some(relative_end) = existing[marker_start..].find(&end_marker) else {
        return Err(format!(
            "Client config '{}' contains an MCPace begin marker without a matching end marker for '{}'",
            config_path.display(),
            adapter_key_name
        ));
    };

    let mut end = marker_start + relative_end + end_marker.len();
    if existing[end..].starts_with("\r\n") {
        end += 2;
    } else if existing[end..].starts_with('\n') {
        end += 1;
    }
    Ok(Some((start, end)))
}

fn find_toml_managed_block(
    existing: &str,
    adapter_key_name: &str,
    config_path: &Path,
) -> Result<Option<TomlManagedBlockSpan>, String> {
    let begin_marker = format!("# BEGIN MCPACE MANAGED BLOCK: {}", adapter_key_name);
    let end_marker = format!("# END MCPACE MANAGED BLOCK: {}", adapter_key_name);
    let Some(marker_start) = existing.find(&begin_marker) else {
        return Ok(None);
    };
    let begin_line_start = line_start(existing, marker_start);
    let begin_line_end = line_end_after(existing, marker_start + begin_marker.len());
    let Some(relative_end) = existing[marker_start..].find(&end_marker) else {
        return Err(format!(
            "Client config '{}' contains an MCPace begin marker without a matching end marker for '{}'",
            config_path.display(),
            adapter_key_name
        ));
    };
    let end_marker_start = marker_start + relative_end;
    let end_line_start = line_start(existing, end_marker_start);
    let end_line_end = line_end_after(existing, end_marker_start + end_marker.len());

    if !range_has_foreign_toml_table_header(
        existing,
        marker_start + begin_marker.len(),
        end_marker_start,
        adapter_key_name,
    ) {
        return Ok(Some(TomlManagedBlockSpan::Exact {
            start: begin_line_start,
            end: end_line_end,
        }));
    }

    let Some((table_start, table_end)) = find_toml_mcp_servers_table_block_in_range(
        existing,
        adapter_key_name,
        marker_start + begin_marker.len(),
        end_marker_start,
    ) else {
        return Err(format!(
            "Client config '{}' contains an over-wide MCPace managed block for '{}' with unrelated TOML tables and no recoverable MCPace table",
            config_path.display(),
            adapter_key_name
        ));
    };

    let preserve_between_begin_and_table = range_has_foreign_toml_table_header(
        existing,
        begin_line_end,
        table_start,
        adapter_key_name,
    );
    Ok(Some(TomlManagedBlockSpan::RecoverOverwide {
        begin_line_start,
        begin_line_end,
        table_start,
        table_end,
        end_line_start,
        end_line_end,
        preserve_between_begin_and_table,
    }))
}

fn find_toml_mcp_servers_table_block(
    existing: &str,
    adapter_key_name: &str,
) -> Option<(usize, usize)> {
    find_toml_mcp_servers_table_block_in_range(existing, adapter_key_name, 0, existing.len())
}

fn find_toml_mcp_servers_table_block_in_range(
    existing: &str,
    adapter_key_name: &str,
    range_start: usize,
    range_end: usize,
) -> Option<(usize, usize)> {
    let candidates = table_header_candidates(adapter_key_name);
    let mut start = None;
    let mut offset = range_start;

    for line in existing[range_start..range_end].split_inclusive('\n') {
        let trimmed = codex_config::trim_toml_line(line);
        if start.is_none() {
            if candidates.iter().any(|candidate| trimmed == candidate) {
                start = Some(offset);
            }
        } else if codex_config::looks_like_toml_table_header(trimmed) {
            return Some((start.unwrap_or_default(), offset));
        }
        offset += line.len();
    }

    start.map(|value| (value, range_end))
}

fn range_has_foreign_toml_table_header(
    existing: &str,
    range_start: usize,
    range_end: usize,
    adapter_key_name: &str,
) -> bool {
    let candidates = table_header_candidates(adapter_key_name);
    existing[range_start..range_end]
        .split_inclusive('\n')
        .map(codex_config::trim_toml_line)
        .any(|trimmed| {
            codex_config::looks_like_toml_table_header(trimmed)
                && !candidates.iter().any(|candidate| trimmed == candidate)
        })
}

fn line_start(existing: &str, index: usize) -> usize {
    existing[..index]
        .rfind('\n')
        .map(|line_break| line_break + 1)
        .unwrap_or(0)
}

fn line_end_after(existing: &str, index: usize) -> usize {
    if existing[index..].starts_with("\r\n") {
        index + 2
    } else if existing[index..].starts_with('\n') {
        index + 1
    } else {
        existing[index..]
            .find('\n')
            .map(|relative| index + relative + 1)
            .unwrap_or(existing.len())
    }
}

fn table_header_candidates(adapter_key_name: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    if codex_config::is_bare_toml_key(adapter_key_name) {
        candidates.push(format!("[mcp_servers.{}]", adapter_key_name));
    }
    candidates.push(format!(
        "[mcp_servers.{}]",
        codex_config::toml_basic_string(adapter_key_name)
    ));
    candidates
}

pub(super) fn detect_missing_stdio_command_warnings(
    existing: &str,
    adapter_key_name: &str,
    config_path: &Path,
) -> Vec<String> {
    codex_config::missing_mcp_server_commands(existing, Some(adapter_key_name))
        .into_iter()
        .map(|entry| {
            format!(
                "Existing MCP server '{}' in '{}' uses command '{}', but that program was not found on PATH. MCP clients can fail startup before reaching MCPace; fix or remove that separate entry if startup still fails.",
                entry.server_name,
                config_path.display(),
                entry.command
            )
        })
        .collect()
}

fn find_yaml_top_level_section(existing: &str, key: &str) -> Option<(usize, usize, usize)> {
    let mut start = None;
    let mut body_start = 0usize;
    let mut offset = 0usize;

    for line in existing.split_inclusive('\n') {
        if let Some((indent, line_key)) = parse_yaml_mapping_key(line) {
            if start.is_none() {
                if indent == 0 && line_key == key {
                    start = Some(offset);
                    body_start = offset + line.len();
                }
            } else if indent == 0 {
                return Some((start.unwrap_or_default(), body_start, offset));
            }
        }
        offset += line.len();
    }

    start.map(|value| (value, body_start, existing.len()))
}

fn find_yaml_section_entry(
    existing: &str,
    section_body_start: usize,
    section_end: usize,
    adapter_key_name: &str,
) -> Option<(usize, usize)> {
    let section = &existing[section_body_start..section_end];
    let mut start = None;
    let mut entry_indent = 0usize;
    let mut offset = 0usize;

    for line in section.split_inclusive('\n') {
        if let Some((indent, line_key)) = parse_yaml_mapping_key(line) {
            if start.is_none() {
                if indent > 0 && line_key == adapter_key_name {
                    start = Some(section_body_start + offset);
                    entry_indent = indent;
                }
            } else if indent == entry_indent {
                return Some((start.unwrap_or_default(), section_body_start + offset));
            }
        }
        offset += line.len();
    }

    start.map(|value| (value, section_end))
}

fn parse_yaml_mapping_key(line: &str) -> Option<(usize, String)> {
    let without_newline = line.trim_end_matches(['\r', '\n']);
    if without_newline.trim().is_empty() {
        return None;
    }
    let indent = without_newline.len() - without_newline.trim_start().len();
    let content = without_newline.trim_start();
    if content.starts_with('#') || content.starts_with('-') {
        return None;
    }
    let colon_index = content.find(':')?;
    let key = content[..colon_index].trim();
    if key.is_empty() {
        return None;
    }
    Some((indent, trim_yaml_key_quotes(key).to_string()))
}

fn trim_yaml_key_quotes(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|trimmed| trimmed.strip_suffix('"'))
        .or_else(|| {
            value
                .strip_prefix('\'')
                .and_then(|trimmed| trimmed.strip_suffix('\''))
        })
        .unwrap_or(value)
}

fn format_toml_table_key(value: &str) -> String {
    if codex_config::is_bare_toml_key(value) {
        value.to_string()
    } else {
        codex_config::toml_basic_string(value)
    }
}

fn yaml_double_quoted_string(value: &str) -> String {
    codex_config::toml_basic_string(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn toml_managed_block_repair_preserves_foreign_tables_when_marker_overreaches() {
        let existing = concat!(
            "model = \"gpt\"\n",
            "\n",
            "# BEGIN MCPACE MANAGED BLOCK: MCPace\n",
            "# This block is managed by `mcpace client install`.\n",
            "[mcp_servers.MCPace]\n",
            "url = \"http://127.0.0.1:1/mcp\"\n",
            "enabled = true\n",
            "[plugins]\n",
            "enabled = true\n",
            "\n",
            "[notice]\n",
            "text = \"keep me\"\n",
            "# END MCPACE MANAGED BLOCK: MCPace\n",
            "approval_policy = \"never\"\n",
        );
        let managed_block = build_toml_managed_block("MCPace", "http://127.0.0.1:39022/mcp", "\n");

        let update =
            upsert_toml_managed_block(existing, "MCPace", &managed_block, Path::new("config.toml"))
                .expect("over-wide managed block should be recoverable");

        assert!(update.replaced_existing_block);
        assert!(update.contents.contains("[plugins]\nenabled = true"));
        assert!(update.contents.contains("[notice]\ntext = \"keep me\""));
        assert!(update
            .contents
            .contains("url = \"http://127.0.0.1:39022/mcp\""));
        assert!(!update.contents.contains("url = \"http://127.0.0.1:1/mcp\""));
        assert_eq!(
            update
                .contents
                .matches("# BEGIN MCPACE MANAGED BLOCK: MCPace")
                .count(),
            1
        );
        assert_eq!(
            update
                .contents
                .matches("# END MCPACE MANAGED BLOCK: MCPace")
                .count(),
            1
        );
        let new_end = update
            .contents
            .find("# END MCPACE MANAGED BLOCK: MCPace")
            .expect("new end marker should exist");
        let plugins = update
            .contents
            .find("[plugins]")
            .expect("foreign table should be preserved");
        assert!(new_end < plugins);
    }

    #[test]
    fn toml_managed_block_rejects_unrecoverable_overwide_marker() {
        let existing = concat!(
            "# BEGIN MCPACE MANAGED BLOCK: MCPace\n",
            "[plugins]\n",
            "enabled = true\n",
            "# END MCPACE MANAGED BLOCK: MCPace\n",
        );
        let managed_block = build_toml_managed_block("MCPace", "http://127.0.0.1:39022/mcp", "\n");

        let error = match upsert_toml_managed_block(
            existing,
            "MCPace",
            &managed_block,
            Path::new("config.toml"),
        ) {
            Ok(_) => panic!("foreign table without MCPace table should not be rewritten"),
            Err(error) => error,
        };

        assert!(error.contains("over-wide MCPace managed block"));
    }
}
