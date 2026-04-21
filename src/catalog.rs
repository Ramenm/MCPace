#[derive(Clone, Copy, Debug)]
pub struct CommandSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub aliases: &'static [&'static str],
    pub implemented: bool,
}

pub const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        name: "help",
        description: "Show help for the Rust-only CLI.",
        aliases: &["-h", "--help"],
        implemented: true,
    },
    CommandSpec {
        name: "version",
        description: "Print the MCPace version from mcpace.config.json.",
        aliases: &[],
        implemented: true,
    },
    CommandSpec {
        name: "doctor",
        description: "Inspect host/source readiness without starting a runtime.",
        aliases: &[],
        implemented: true,
    },
    CommandSpec {
        name: "dashboard",
        description: "Serve a local admin dashboard for MCPace.",
        aliases: &["ui"],
        implemented: true,
    },
    CommandSpec {
        name: "serve",
        description: "Serve the local MCPace HTTP surface on one port.",
        aliases: &[],
        implemented: true,
    },
    CommandSpec {
        name: "profile",
        description: "Read-only runtime profile inspection.",
        aliases: &[],
        implemented: true,
    },
    CommandSpec {
        name: "projects",
        description: "Read-only project registry inspection.",
        aliases: &["project"],
        implemented: true,
    },
    CommandSpec {
        name: "candidates",
        description: "Inspect candidate server catalog.",
        aliases: &[],
        implemented: true,
    },
    CommandSpec {
        name: "lab",
        description: "Inspect runtime lab scenarios, coverage, and gap reports.",
        aliases: &[],
        implemented: true,
    },
    CommandSpec {
        name: "server",
        description: "Grouped server inspection command.",
        aliases: &["servers", "capabilities", "server-capabilities"],
        implemented: true,
    },
    CommandSpec {
        name: "verify",
        description: "Grouped verification command.",
        aliases: &[
            "check",
            "status",
            "smoke",
            "readiness",
            "probe",
            "stress-status",
            "stress-startup-status",
        ],
        implemented: true,
    },
    CommandSpec {
        name: "init",
        description: "Bootstrap runtime state layout and readiness.",
        aliases: &["install", "boot"],
        implemented: true,
    },
    CommandSpec {
        name: "hub",
        description: "Manage the local hub lifecycle, status, and logs.",
        aliases: &["start", "autostart"],
        implemented: true,
    },
    CommandSpec {
        name: "stdio-shim",
        description: "Internal bootstrap-only stdio shim proof surface.",
        aliases: &["stdio_shim"],
        implemented: true,
    },
    CommandSpec {
        name: "mcp-server",
        description: "Internal MCP stdio compatibility surface.",
        aliases: &["mcp_server"],
        implemented: true,
    },
    CommandSpec {
        name: "client",
        description: "Grouped client planning/install/export command.",
        aliases: &["setup-clients", "setup-mcp-clients"],
        implemented: true,
    },
    CommandSpec {
        name: "repair",
        description: "Grouped repair/maintenance command.",
        aliases: &["backup", "rotate-logs", "windows-mcp-lease", "auth"],
        implemented: true,
    },
    CommandSpec {
        name: "release",
        description: "Planned grouped release/build command.",
        aliases: &["build-release"],
        implemented: false,
    },
];

pub fn find(name: &str) -> Option<&'static CommandSpec> {
    let normalized = normalize(name);
    COMMANDS.iter().find(|command| {
        command.name == normalized || command.aliases.iter().any(|alias| *alias == normalized)
    })
}

pub fn normalize(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}
