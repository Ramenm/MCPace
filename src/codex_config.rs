use crate::config_edit;

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
        let trimmed = config_edit::trim_toml_line(line);
        if config_edit::looks_like_toml_table_header(trimmed) {
            push_mcp_server_command(&mut commands, current_server.take(), current_command.take());
            current_server = parse_mcp_servers_table_name(trimmed);
            continue;
        }

        if current_server.is_some() && current_command.is_none() {
            current_command = config_edit::parse_toml_string_assignment(trimmed, "command");
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

fn parse_mcp_servers_table_name(trimmed_line: &str) -> Option<String> {
    let inner = trimmed_line.strip_prefix('[')?.strip_suffix(']')?.trim();
    let name = inner.strip_prefix("mcp_servers.")?.trim();
    config_edit::parse_toml_string_literal(name).or_else(|| {
        if config_edit::is_bare_toml_key(name) {
            Some(name.to_string())
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests;
