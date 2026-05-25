use crate::text_utils;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CodexMcpCommand {
    pub(crate) server_name: String,
    pub(crate) command: String,
}

pub(crate) fn missing_mcp_server_commands(
    contents: &str,
    skip_server_name: Option<&str>,
) -> Vec<CodexMcpCommand> {
    mcp_server_commands(contents)
        .into_iter()
        .filter(|entry| {
            skip_server_name
                .map(|skip| entry.server_name != skip)
                .unwrap_or(true)
        })
        .filter(|entry| which::which(&entry.command).is_err())
        .collect()
}

pub(crate) fn mcp_server_commands(contents: &str) -> Vec<CodexMcpCommand> {
    let mut commands = Vec::new();
    let mut current_server: Option<String> = None;
    let mut current_command: Option<String> = None;

    for line in contents.split_inclusive('\n') {
        let trimmed = trim_toml_line(line);
        if looks_like_toml_table_header(trimmed) {
            push_mcp_server_command(&mut commands, current_server.take(), current_command.take());
            current_server = parse_mcp_servers_table_name(trimmed);
            continue;
        }

        if current_server.is_some() && current_command.is_none() {
            current_command = parse_toml_string_assignment(trimmed, "command");
        }
    }

    push_mcp_server_command(&mut commands, current_server, current_command);
    commands
}

fn push_mcp_server_command(
    commands: &mut Vec<CodexMcpCommand>,
    server_name: Option<String>,
    command: Option<String>,
) {
    let Some(server_name) = server_name else {
        return;
    };
    let Some(command) = command.filter(|value| !value.trim().is_empty()) else {
        return;
    };
    commands.push(CodexMcpCommand {
        server_name,
        command,
    });
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

fn parse_mcp_servers_table_name(trimmed_line: &str) -> Option<String> {
    let inner = trimmed_line.strip_prefix('[')?.strip_suffix(']')?.trim();
    let name = inner.strip_prefix("mcp_servers.")?.trim();
    parse_toml_string_literal(name).or_else(|| {
        if is_bare_toml_key(name) {
            Some(name.to_string())
        } else {
            None
        }
    })
}

fn parse_toml_string_assignment(trimmed_line: &str, key: &str) -> Option<String> {
    let (left, right) = trimmed_line.split_once('=')?;
    if left.trim() != key {
        return None;
    }
    parse_toml_string_literal(right.trim())
}

fn parse_toml_string_literal(value: &str) -> Option<String> {
    if value.starts_with('"') {
        let end = find_toml_quoted_string_end(value, '"')?;
        return serde_json::from_str::<String>(&value[..=end]).ok();
    }
    if let Some(rest) = value.strip_prefix('\'') {
        let end = rest.find('\'')?;
        return Some(rest[..end].to_string());
    }
    None
}

fn find_toml_quoted_string_end(value: &str, quote: char) -> Option<usize> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_server_commands_parse_quoted_names_and_inline_comments() {
        let commands = mcp_server_commands(
            r#"
[mcp_servers."name.with#hash"] # user comment
command = "tool#name" # another user comment
args = ["serve"]

[mcp_servers.other]
command = 'single-quoted-command' # comment
"#,
        );

        assert_eq!(
            commands,
            vec![
                CodexMcpCommand {
                    server_name: "name.with#hash".to_string(),
                    command: "tool#name".to_string(),
                },
                CodexMcpCommand {
                    server_name: "other".to_string(),
                    command: "single-quoted-command".to_string(),
                },
            ]
        );
    }

    #[test]
    fn toml_basic_string_escapes_values_used_by_client_configs() {
        assert_eq!(
            toml_basic_string("http://127.0.0.1:39022/mcp?x=\"y\"\n"),
            r#""http://127.0.0.1:39022/mcp?x=\"y\"\n""#
        );
    }
}
