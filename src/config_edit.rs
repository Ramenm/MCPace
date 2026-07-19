use crate::codex_config;
use crate::json::{parse_str, JsonValue};
use crate::json_helpers;
use crate::text_utils;
use std::fmt;
use std::path::Path;

mod removal;
pub(crate) use removal::{
    remove_json_mcp_server_entry, remove_toml_mcp_server_block, remove_yaml_mcp_server_entry,
};

pub(crate) type ConfigEditResult<T> = Result<T, ConfigEditError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ConfigEditError {
    JsonParse { path: String, message: String },
    JsonTopLevelObject { path: String },
    JsonServersObject { path: String, key: String },
    ManagedBlockMissingEnd { path: String, adapter: String },
    OverwideManagedBlock { path: String, adapter: String },
}

impl fmt::Display for ConfigEditError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigEditError::JsonParse { path, message } => write!(
                formatter,
                "failed to parse JSON client config '{}': {}",
                path, message
            ),
            ConfigEditError::JsonTopLevelObject { path } => write!(
                formatter,
                "JSON client config '{}' must be a top-level object",
                path
            ),
            ConfigEditError::JsonServersObject { path, key } => write!(
                formatter,
                "JSON client config '{}' has a non-object {} field",
                path, key
            ),
            ConfigEditError::ManagedBlockMissingEnd { path, adapter } => write!(
                formatter,
                "Client config '{}' contains an MCPace begin marker without a matching end marker for '{}'",
                path, adapter
            ),
            ConfigEditError::OverwideManagedBlock { path, adapter } => write!(
                formatter,
                "Client config '{}' contains an over-wide MCPace managed block for '{}' with unrelated TOML tables and no recoverable MCPace table",
                path, adapter
            ),
        }
    }
}

impl std::error::Error for ConfigEditError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClientConfigUpdate {
    pub(crate) contents: String,
    pub(crate) replaced_existing_block: bool,
}

fn config_path_label(path: &Path) -> String {
    path.display().to_string()
}

pub(crate) fn trim_toml_line(line: &str) -> &str {
    let trimmed = line.trim();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (index, ch) in trimmed.char_indices() {
        match quote {
            Some('"') => {
                if escaped {
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == '"' {
                    quote = None;
                }
            }
            Some('\'') => {
                if ch == '\'' {
                    quote = None;
                }
            }
            Some(_) => {}
            None => match ch {
                '"' | '\'' => quote = Some(ch),
                '#' => return trimmed[..index].trim_end(),
                _ => {}
            },
        }
    }
    trimmed
}

pub(crate) fn looks_like_toml_table_header(trimmed_line: &str) -> bool {
    trimmed_line.starts_with('[') && trimmed_line.ends_with(']')
}

pub(crate) fn parse_toml_string_assignment(trimmed_line: &str, key: &str) -> Option<String> {
    let (left, right) = trimmed_line.split_once('=')?;
    if left.trim() != key {
        return None;
    }
    parse_toml_string_literal(right.trim())
}

pub(crate) fn parse_toml_string_literal(value: &str) -> Option<String> {
    if value.starts_with('"') {
        let end = toml_quoted_string_end(value, '"')?;
        return serde_json::from_str::<String>(&value[..=end]).ok();
    }
    if let Some(rest) = value.strip_prefix('\'') {
        let end = rest.find('\'')?;
        return Some(rest[..end].to_string());
    }
    None
}

fn toml_quoted_string_end(value: &str, quote: char) -> Option<usize> {
    let mut escaped = false;
    for (index, ch) in value.char_indices().skip(1) {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == quote {
            return Some(index);
        }
    }
    None
}

pub(crate) fn is_bare_toml_key(value: &str) -> bool {
    text_utils::ascii_alnum_dash_underscore(value)
}

pub(crate) fn toml_basic_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            _ => escaped.push(ch),
        }
    }
    escaped.push('"');
    escaped
}

pub(crate) fn detect_newline(existing: &str) -> &'static str {
    if existing.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

pub(crate) fn build_toml_mcp_server_block(
    adapter_key_name: &str,
    server_url: &str,
    newline: &str,
) -> String {
    let lines = [
        format!("# BEGIN MCPACE MANAGED BLOCK: {}", adapter_key_name),
        "# This block is managed by `mcpace advanced client install`.".to_string(),
        format!("[mcp_servers.{}]", format_toml_table_key(adapter_key_name)),
        format!("url = {}", toml_basic_string(server_url)),
        "enabled = true".to_string(),
        "startup_timeout_sec = 20".to_string(),
        format!("# END MCPACE MANAGED BLOCK: {}", adapter_key_name),
        String::new(),
    ];
    lines.join(newline)
}

pub(crate) fn apply_json_mcp_server_entry(
    existing: &str,
    adapter_key_name: &str,
    servers_object_key: &str,
    server_config: JsonValue,
    config_path: &Path,
) -> ConfigEditResult<ClientConfigUpdate> {
    let mut root = if existing.trim().is_empty() {
        json_helpers::empty_object()
    } else {
        parse_str(existing).map_err(|error| ConfigEditError::JsonParse {
            path: config_path_label(config_path),
            message: error.to_string(),
        })?
    };

    let root_object = match &mut root {
        JsonValue::Object(map) => map,
        _ => {
            return Err(ConfigEditError::JsonTopLevelObject {
                path: config_path_label(config_path),
            });
        }
    };

    let servers_key = if servers_object_key.trim().is_empty() {
        "mcpServers"
    } else {
        servers_object_key.trim()
    };
    let servers_value = root_object
        .entry(servers_key.to_string())
        .or_insert_with(json_helpers::empty_object);
    let servers_object = match servers_value {
        JsonValue::Object(map) => map,
        _ => {
            return Err(ConfigEditError::JsonServersObject {
                path: config_path_label(config_path),
                key: servers_key.to_string(),
            });
        }
    };

    let replaced_existing_block = servers_object.contains_key(adapter_key_name);
    servers_object.insert(adapter_key_name.to_string(), server_config);

    Ok(ClientConfigUpdate {
        contents: root.to_pretty_string(),
        replaced_existing_block,
    })
}

pub(crate) fn apply_toml_mcp_server_block(
    existing: &str,
    adapter_key_name: &str,
    managed_block: &str,
    config_path: &Path,
) -> ConfigEditResult<ClientConfigUpdate> {
    if let Some(span) = locate_toml_managed_block(existing, adapter_key_name, config_path)? {
        let updated = apply_toml_managed_block_update(existing, managed_block, span);
        return Ok(ClientConfigUpdate {
            contents: updated,
            replaced_existing_block: true,
        });
    }

    if let Some((start, end)) = locate_toml_mcp_servers_table_block(existing, adapter_key_name) {
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

pub(crate) fn apply_yaml_mcp_server_entry(
    existing: &str,
    adapter_key_name: &str,
    server_url: &str,
    config_path: &Path,
) -> ConfigEditResult<ClientConfigUpdate> {
    let newline = detect_newline(existing);
    let entry_block = build_yaml_mcp_servers_entry_block(adapter_key_name, server_url, newline);

    if let Some((start, end)) = locate_managed_block(existing, adapter_key_name, config_path)? {
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
        locate_yaml_top_level_section(existing, "mcp_servers")
    {
        if let Some((entry_start, entry_end)) =
            locate_yaml_section_entry(existing, section_body_start, section_end, adapter_key_name)
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

pub(crate) fn detect_missing_stdio_command_warnings(
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

fn locate_managed_block(
    existing: &str,
    adapter_key_name: &str,
    config_path: &Path,
) -> ConfigEditResult<Option<(usize, usize)>> {
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
        return Err(ConfigEditError::ManagedBlockMissingEnd {
            path: config_path_label(config_path),
            adapter: adapter_key_name.to_string(),
        });
    };

    let mut end = marker_start + relative_end + end_marker.len();
    if existing[end..].starts_with("\r\n") {
        end += 2;
    } else if existing[end..].starts_with('\n') {
        end += 1;
    }
    Ok(Some((start, end)))
}

fn locate_toml_managed_block(
    existing: &str,
    adapter_key_name: &str,
    config_path: &Path,
) -> ConfigEditResult<Option<TomlManagedBlockSpan>> {
    let begin_marker = format!("# BEGIN MCPACE MANAGED BLOCK: {}", adapter_key_name);
    let end_marker = format!("# END MCPACE MANAGED BLOCK: {}", adapter_key_name);
    let Some(marker_start) = existing.find(&begin_marker) else {
        return Ok(None);
    };
    let begin_line_start = line_start(existing, marker_start);
    let begin_line_end = line_end_after(existing, marker_start + begin_marker.len());
    let Some(relative_end) = existing[marker_start..].find(&end_marker) else {
        return Err(ConfigEditError::ManagedBlockMissingEnd {
            path: config_path_label(config_path),
            adapter: adapter_key_name.to_string(),
        });
    };
    let end_marker_start = marker_start + relative_end;
    let end_line_start = line_start(existing, end_marker_start);
    let end_line_end = line_end_after(existing, end_marker_start + end_marker.len());

    if !range_has_foreign_toml_header(
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

    let Some((table_start, table_end)) = locate_toml_mcp_servers_table_block_in_range(
        existing,
        adapter_key_name,
        marker_start + begin_marker.len(),
        end_marker_start,
    ) else {
        return Err(ConfigEditError::OverwideManagedBlock {
            path: config_path_label(config_path),
            adapter: adapter_key_name.to_string(),
        });
    };

    let preserve_between_begin_and_table =
        range_has_foreign_toml_header(existing, begin_line_end, table_start, adapter_key_name);
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

fn locate_toml_mcp_servers_table_block(
    existing: &str,
    adapter_key_name: &str,
) -> Option<(usize, usize)> {
    locate_toml_mcp_servers_table_block_in_range(existing, adapter_key_name, 0, existing.len())
}

fn locate_toml_mcp_servers_table_block_in_range(
    existing: &str,
    adapter_key_name: &str,
    range_start: usize,
    range_end: usize,
) -> Option<(usize, usize)> {
    let candidates = table_header_candidates(adapter_key_name);
    let mut start = None;
    let mut offset = range_start;

    for line in existing[range_start..range_end].split_inclusive('\n') {
        let trimmed = trim_toml_line(line);
        if start.is_none() {
            if candidates.iter().any(|candidate| trimmed == candidate) {
                start = Some(offset);
            }
        } else if looks_like_toml_table_header(trimmed) {
            return Some((start.unwrap_or_default(), offset));
        }
        offset += line.len();
    }

    start.map(|value| (value, range_end))
}

fn range_has_foreign_toml_header(
    existing: &str,
    range_start: usize,
    range_end: usize,
    adapter_key_name: &str,
) -> bool {
    let candidates = table_header_candidates(adapter_key_name);
    existing[range_start..range_end]
        .split_inclusive('\n')
        .map(trim_toml_line)
        .any(|trimmed| {
            looks_like_toml_table_header(trimmed)
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
    if is_bare_toml_key(adapter_key_name) {
        candidates.push(format!("[mcp_servers.{}]", adapter_key_name));
    }
    candidates.push(format!(
        "[mcp_servers.{}]",
        toml_basic_string(adapter_key_name)
    ));
    candidates
}

fn locate_yaml_top_level_section(existing: &str, key: &str) -> Option<(usize, usize, usize)> {
    let mut start = None;
    let mut body_start = 0usize;
    let mut offset = 0usize;

    for line in existing.split_inclusive('\n') {
        if let Some((indent, line_key)) = read_yaml_mapping_key(line) {
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

fn locate_yaml_section_entry(
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
        if let Some((indent, line_key)) = read_yaml_mapping_key(line) {
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

fn read_yaml_mapping_key(line: &str) -> Option<(usize, String)> {
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
    if is_bare_toml_key(value) {
        value.to_string()
    } else {
        toml_basic_string(value)
    }
}

fn yaml_double_quoted_string(value: &str) -> String {
    toml_basic_string(value)
}

#[cfg(test)]
mod tests;
