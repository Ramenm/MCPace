use crate::text_utils;

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
        aliases: &["--version", "-v"],
        implemented: true,
    },
    CommandSpec {
        name: "doctor",
        description: "Inspect host/source readiness without starting a runtime.",
        aliases: &[],
        implemented: true,
    },
    CommandSpec {
        name: "setup",
        description:
            "Home-first onboarding: create/repair MCPace home, start the endpoint, wire supported local clients, and smoke-test without adding upstream servers.",
        aliases: &["up", "quickstart", "bootstrap", "one-click"],
        implemented: true,
    },
    CommandSpec {
        name: "service",
        description: "Install or inspect user-level autostart for the MCPace endpoint.",
        aliases: &["autostart", "startup"],
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
        description: "Grouped server inspection and automatic MCP package/URL/command install command.",
        aliases: &["servers", "capabilities", "server-capabilities", "mcp", "add-server", "server-install"],
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
        aliases: &["boot"],
        implemented: true,
    },
    CommandSpec {
        name: "hub",
        description: "Manage the local hub lifecycle, status, and logs.",
        aliases: &["start"],
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
        name: "connect",
        description: "Show client-first wiring guidance, endpoint details, and next commands.",
        aliases: &["guide", "next", "onboard"],
        implemented: true,
    },
    CommandSpec {
        name: "cleanup",
        description:
            "Safely inspect or remove disposable cache, logs, and ephemeral runtime markers.",
        aliases: &["clean", "prune"],
        implemented: true,
    },
    CommandSpec {
        name: "repair",
        description: "Grouped repair/maintenance command.",
        aliases: &["backup", "rotate-logs", "windows-mcp-lease", "auth"],
        implemented: true,
    },
    CommandSpec {
        name: "update",
        description: "Check external package-manager update guidance without self-updating.",
        aliases: &["update-check"],
        implemented: true,
    },
    CommandSpec {
        name: "release",
        description: "Build local source release artifacts without publishing.",
        aliases: &["build-release"],
        implemented: true,
    },
];

pub fn find(name: &str) -> Option<&'static CommandSpec> {
    let normalized = normalize(name);
    COMMANDS.iter().find(|command| {
        command.name == normalized || command.aliases.iter().any(|alias| *alias == normalized)
    })
}

pub fn normalize(value: &str) -> String {
    text_utils::normalize_flag(value)
}
