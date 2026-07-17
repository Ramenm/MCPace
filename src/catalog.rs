use crate::text_utils;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandVisibility {
    Public,
    HiddenCompatibility,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandRoute {
    Help,
    Version,
    Up,
    Start,
    Stop,
    Restart,
    Status,
    Install,
    Uninstall,
    Advanced,
    Stdio,
    StdioShim,
    Agent,
    Serve,
    Hub,
    McpServer,
}

#[derive(Clone, Copy, Debug)]
pub struct CommandSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub aliases: &'static [&'static str],
    pub visibility: CommandVisibility,
    pub route: CommandRoute,
}

pub const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        name: "help",
        description: "Show the public MCPace command surface.",
        aliases: &["-h", "--help"],
        visibility: CommandVisibility::Public,
        route: CommandRoute::Help,
    },
    CommandSpec {
        name: "version",
        description: "Print the compiled MCPace binary version.",
        aliases: &["-v", "--version"],
        visibility: CommandVisibility::Public,
        route: CommandRoute::Version,
    },
    CommandSpec {
        name: "up",
        description:
            "Create or repair MCPace, start it, wire clients, and configure login startup.",
        aliases: &[],
        visibility: CommandVisibility::Public,
        route: CommandRoute::Up,
    },
    CommandSpec {
        name: "start",
        description: "Start the already-configured runtime for this login session.",
        aliases: &[],
        visibility: CommandVisibility::Public,
        route: CommandRoute::Start,
    },
    CommandSpec {
        name: "stop",
        description: "Stop the current runtime without disabling startup at the next login.",
        aliases: &[],
        visibility: CommandVisibility::Public,
        route: CommandRoute::Stop,
    },
    CommandSpec {
        name: "restart",
        description: "Restart the configured runtime without changing clients or login startup.",
        aliases: &[],
        visibility: CommandVisibility::Public,
        route: CommandRoute::Restart,
    },
    CommandSpec {
        name: "status",
        description: "Show aggregate runtime and login-startup status without changing anything.",
        aliases: &[],
        visibility: CommandVisibility::Public,
        route: CommandRoute::Status,
    },
    CommandSpec {
        name: "install",
        description: "Add or update an upstream MCP server.",
        aliases: &[],
        visibility: CommandVisibility::Public,
        route: CommandRoute::Install,
    },
    CommandSpec {
        name: "uninstall",
        description: "Remove MCPace local integration while preserving configuration and backups.",
        aliases: &[],
        visibility: CommandVisibility::Public,
        route: CommandRoute::Uninstall,
    },
    CommandSpec {
        name: "advanced",
        description:
            "Open diagnostics, server, client, startup, runtime, lease, and maintainer commands.",
        aliases: &[],
        visibility: CommandVisibility::Public,
        route: CommandRoute::Advanced,
    },
    // These entrypoints are intentionally callable but absent from public help and completion.
    // Existing MCP client configurations and installed login entries depend on their exact names.
    CommandSpec {
        name: "stdio",
        description: "Internal MCP stdio transport entrypoint.",
        aliases: &[],
        visibility: CommandVisibility::HiddenCompatibility,
        route: CommandRoute::Stdio,
    },
    CommandSpec {
        name: "stdio-shim",
        description: "Legacy MCP stdio transport entrypoint retained through 0.8.x.",
        aliases: &[],
        visibility: CommandVisibility::HiddenCompatibility,
        route: CommandRoute::StdioShim,
    },
    CommandSpec {
        name: "agent",
        description: "Installed user-login agent entrypoint.",
        aliases: &[],
        visibility: CommandVisibility::HiddenCompatibility,
        route: CommandRoute::Agent,
    },
    CommandSpec {
        name: "serve",
        description: "Legacy managed-runtime entrypoint.",
        aliases: &[],
        visibility: CommandVisibility::HiddenCompatibility,
        route: CommandRoute::Serve,
    },
    CommandSpec {
        name: "hub",
        description: "Internal runtime and lease entrypoint.",
        aliases: &[],
        visibility: CommandVisibility::HiddenCompatibility,
        route: CommandRoute::Hub,
    },
    CommandSpec {
        name: "mcp-server",
        description: "Legacy MCP server entrypoint retained through 0.8.x.",
        aliases: &[],
        visibility: CommandVisibility::HiddenCompatibility,
        route: CommandRoute::McpServer,
    },
];

pub fn find(name: &str) -> Option<&'static CommandSpec> {
    let normalized = normalize(name);
    COMMANDS.iter().find(|command| {
        command.name == normalized || command.aliases.iter().any(|alias| *alias == normalized)
    })
}

pub fn public_commands() -> impl Iterator<Item = &'static CommandSpec> {
    COMMANDS
        .iter()
        .filter(|command| command.visibility == CommandVisibility::Public)
}

pub fn normalize(value: &str) -> String {
    text_utils::normalize_flag(value)
}
