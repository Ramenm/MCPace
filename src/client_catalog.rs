use crate::json::JsonValue;

#[derive(Clone, Copy, Debug)]
pub struct ClientTarget {
    pub id: &'static str,
    pub family_id: &'static str,
    pub display_name: &'static str,
    pub aliases: &'static [&'static str],
    pub maturity: &'static str,
    pub surface_class: &'static str,
    pub surface_kind: &'static str,
    pub config_format: &'static str,
    pub config_paths: &'static [&'static str],
    pub config_precedence: &'static [&'static str],
    pub native_scopes: &'static [&'static str],
    pub supported_ingresses: &'static [&'static str],
    pub documented_features: &'static [&'static str],
    pub documented_constraints: &'static [&'static str],
    pub notes: &'static [&'static str],
}

pub const CLIENT_TARGETS: &[ClientTarget] = &[
    ClientTarget {
        id: "codex",
        family_id: "codex",
        display_name: "OpenAI Codex",
        aliases: &["codex-cli", "openai-codex"],
        maturity: "documented",
        surface_class: "local",
        surface_kind: "local-cli-ide",
        config_format: "toml",
        config_paths: &["~/.codex/config.toml", ".codex/config.toml"],
        config_precedence: &["project-trusted", "user"],
        native_scopes: &["user", "project"],
        supported_ingresses: &["stdio", "streamable-http"],
        documented_features: &["tools", "oauth"],
        documented_constraints: &["shared-config-cli-ide"],
        notes: &[
            "CLI and IDE extension share the same config layers.",
            "Project config applies only in trusted projects.",
        ],
    },
    ClientTarget {
        id: "claude-code",
        family_id: "claude",
        display_name: "Anthropic Claude Code",
        aliases: &["claude", "claude code", "claudecode"],
        maturity: "documented",
        surface_class: "local",
        surface_kind: "local-cli-ide-browser",
        config_format: "json",
        config_paths: &["~/.claude.json", ".mcp.json"],
        config_precedence: &["project", "user", "local"],
        native_scopes: &["local", "project", "user"],
        supported_ingresses: &["stdio", "streamable-http", "sse"],
        documented_features: &["tools", "oauth"],
        documented_constraints: &["sse-deprecated", "managed-config"],
        notes: &[
            "Project-scoped servers live in .mcp.json.",
            "Local and user-scoped servers live in ~/.claude.json.",
        ],
    },
    ClientTarget {
        id: "claude-api-connector",
        family_id: "claude",
        display_name: "Anthropic Claude API MCP connector",
        aliases: &["claude-api", "anthropic-mcp-connector", "claude-mcp-connector"],
        maturity: "documented",
        surface_class: "cloud",
        surface_kind: "cloud-api-connector",
        config_format: "api-request",
        config_paths: &[],
        config_precedence: &["request"],
        native_scopes: &["request"],
        supported_ingresses: &["streamable-http", "sse"],
        documented_features: &["tools", "oauth"],
        documented_constraints: &["tools-only", "public-http-only", "beta-header-required"],
        notes: &[
            "Only tool calls are supported on this surface.",
            "The server must be publicly reachable over HTTP; local stdio is not supported.",
        ],
    },
    ClientTarget {
        id: "cursor-local",
        family_id: "cursor",
        display_name: "Cursor local editor and CLI",
        aliases: &["cursor", "cursor-cli", "cursor-editor"],
        maturity: "documented",
        surface_class: "local",
        surface_kind: "local-editor-cli",
        config_format: "json",
        config_paths: &["~/.cursor/mcp.json", ".cursor/mcp.json"],
        config_precedence: &["project", "global", "nested"],
        native_scopes: &["project", "global", "nested"],
        supported_ingresses: &["stdio", "streamable-http", "sse"],
        documented_features: &["tools", "oauth"],
        documented_constraints: &["shared-config-cli-editor", "fixed-oauth-callback"],
        notes: &[
            "Cursor CLI uses the same MCP configuration as the editor.",
            "Project config takes precedence over the global file for matching server names.",
        ],
    },
    ClientTarget {
        id: "cursor-cloud-agents",
        family_id: "cursor",
        display_name: "Cursor cloud agents",
        aliases: &["cursor-cloud", "cursor-background-agents"],
        maturity: "documented",
        surface_class: "cloud",
        surface_kind: "cloud-agent",
        config_format: "dashboard-or-project",
        config_paths: &[],
        config_precedence: &["project", "team"],
        native_scopes: &["project", "team"],
        supported_ingresses: &["stdio", "streamable-http"],
        documented_features: &["tools", "oauth"],
        documented_constraints: &["cloud-vm", "no-sse", "http-preferred"],
        notes: &[
            "Cloud agents run away from the local workstation, so local-only assumptions do not hold.",
            "HTTP is the safer transport for secrets because the backend can proxy or terminate auth.",
        ],
    },
    ClientTarget {
        id: "kiro-ide",
        family_id: "kiro",
        display_name: "Kiro IDE",
        aliases: &["kiro", "kiro-editor"],
        maturity: "documented",
        surface_class: "local",
        surface_kind: "local-ide",
        config_format: "json",
        config_paths: &["~/.kiro/settings/mcp.json", ".kiro/settings/mcp.json"],
        config_precedence: &["workspace", "user"],
        native_scopes: &["user", "workspace"],
        supported_ingresses: &["stdio", "streamable-http"],
        documented_features: &["tools"],
        documented_constraints: &["workspace-overrides-user"],
        notes: &[
            "Workspace and user configs merge with workspace precedence.",
            "Local command and remote URL entries share the same JSON file shape.",
        ],
    },
    ClientTarget {
        id: "kiro-cli",
        family_id: "kiro",
        display_name: "Kiro CLI",
        aliases: &["kiro-agent", "kirocli"],
        maturity: "documented",
        surface_class: "local",
        surface_kind: "local-cli",
        config_format: "json",
        config_paths: &["~/.kiro/settings/mcp.json", ".kiro/settings/mcp.json", "~/.kiro/agents/*.json", "<project>/.kiro/agents/*.json"],
        config_precedence: &["agent", "workspace", "user"],
        native_scopes: &["agent", "workspace", "user"],
        supported_ingresses: &["stdio", "streamable-http", "sse"],
        documented_features: &["tools", "prompts", "oauth"],
        documented_constraints: &["agent-overrides-workspace-user", "tool-name-rules"],
        notes: &[
            "Agent-level config can augment or override MCP from shared settings files.",
            "Tool names and descriptions need to stay within Kiro CLI formatting rules.",
        ],
    },
    ClientTarget {
        id: "windsurf",
        family_id: "windsurf",
        display_name: "Windsurf",
        aliases: &["codeium", "windsurf-ide"],
        maturity: "documented",
        surface_class: "local",
        surface_kind: "local-ide",
        config_format: "json",
        config_paths: &["~/.codeium/windsurf/mcp_config.json"],
        config_precedence: &["user"],
        native_scopes: &["user"],
        supported_ingresses: &["stdio", "streamable-http", "sse"],
        documented_features: &["tools", "oauth"],
        documented_constraints: &["tool-budget-100"],
        notes: &[
            "Cascade supports stdio, Streamable HTTP, and SSE MCP servers.",
            "The UI enforces a budget of 100 enabled tools across connected MCP servers.",
        ],
    },
    ClientTarget {
        id: "gemini-cli",
        family_id: "gemini",
        display_name: "Gemini CLI",
        aliases: &["gemini", "google-gemini-cli"],
        maturity: "documented",
        surface_class: "local",
        surface_kind: "local-cli",
        config_format: "json",
        config_paths: &["settings.json"],
        config_precedence: &["workspace", "user", "system"],
        native_scopes: &["workspace", "user", "system"],
        supported_ingresses: &["stdio", "streamable-http", "sse"],
        documented_features: &["tools"],
        documented_constraints: &["settings-json"],
        notes: &[
            "MCP servers are configured through settings.json plus global MCP settings.",
            "Transport support is evolving quickly, so compatibility should be proven with real traces.",
        ],
    },
    ClientTarget {
        id: "github-copilot-cli",
        family_id: "github-copilot",
        display_name: "GitHub Copilot CLI",
        aliases: &["copilot", "copilot-cli", "github-copilot"],
        maturity: "documented",
        surface_class: "local",
        surface_kind: "local-cli",
        config_format: "json",
        config_paths: &["~/.copilot/mcp-config.json"],
        config_precedence: &["session", "user"],
        native_scopes: &["session", "user"],
        supported_ingresses: &["stdio", "streamable-http", "sse"],
        documented_features: &["tools"],
        documented_constraints: &["built-in-github-server", "session-additional-config"],
        notes: &[
            "CLI config can be augmented per session with --additional-mcp-config.",
            "The GitHub MCP server is built in and available without extra configuration.",
        ],
    },
    ClientTarget {
        id: "github-copilot-cloud-agent",
        family_id: "github-copilot",
        display_name: "GitHub Copilot cloud agent",
        aliases: &["copilot-cloud", "github-copilot-cloud"],
        maturity: "documented",
        surface_class: "cloud",
        surface_kind: "cloud-agent",
        config_format: "repo-json",
        config_paths: &["repository-level MCP config"],
        config_precedence: &["repository"],
        native_scopes: &["repository"],
        supported_ingresses: &["stdio", "streamable-http", "sse"],
        documented_features: &["tools"],
        documented_constraints: &["tools-only", "no-remote-oauth", "no-approval-prompts", "repo-level-config"],
        notes: &[
            "Cloud agent supports tools only; resources and prompts are not available on this surface.",
            "Remote MCP servers that depend on OAuth are not currently supported here.",
        ],
    },
    ClientTarget {
        id: "hermes-agent",
        family_id: "hermes",
        display_name: "Hermes Agent",
        aliases: &["hermes", "nous-hermes-agent"],
        maturity: "documented",
        surface_class: "local",
        surface_kind: "local-agent",
        config_format: "yaml",
        config_paths: &["~/.hermes/config.yaml", "~/.hermes/.env"],
        config_precedence: &["cli", "config.yaml", ".env", "defaults"],
        native_scopes: &["user"],
        supported_ingresses: &["stdio", "streamable-http"],
        documented_features: &["tools", "resources", "prompts", "oauth"],
        documented_constraints: &["config-yaml-plus-env", "oauth-pkce", "capability-aware-resource-prompt-wrapper"],
        notes: &[
            "Hermes mixes local stdio and remote HTTP MCP servers in one config.",
            "Resources and prompts become utility wrappers only when the server actually exposes those capabilities.",
        ],
    },
    ClientTarget {
        id: "generic-stdio",
        family_id: "generic",
        display_name: "Generic stdio MCP host",
        aliases: &["generic", "stdio"],
        maturity: "generic",
        surface_class: "generic",
        surface_kind: "generic-stdio-host",
        config_format: "host-defined",
        config_paths: &[],
        config_precedence: &["host-defined"],
        native_scopes: &["host-defined"],
        supported_ingresses: &["stdio"],
        documented_features: &[],
        documented_constraints: &["host-defined"],
        notes: &[
            "Use this when the host launches a command and speaks MCP over stdio.",
        ],
    },
    ClientTarget {
        id: "generic-http",
        family_id: "generic",
        display_name: "Generic Streamable HTTP MCP host",
        aliases: &["http", "streamable-http"],
        maturity: "generic",
        surface_class: "generic",
        surface_kind: "generic-http-host",
        config_format: "host-defined",
        config_paths: &[],
        config_precedence: &["host-defined"],
        native_scopes: &["host-defined"],
        supported_ingresses: &["streamable-http"],
        documented_features: &[],
        documented_constraints: &["host-defined"],
        notes: &[
            "Use this when the host connects to one MCP URL instead of launching a process.",
        ],
    },
    ClientTarget {
        id: "public-http-connector",
        family_id: "generic",
        display_name: "Generic public HTTP MCP connector",
        aliases: &["api-connector", "cloud-http-connector"],
        maturity: "generic",
        surface_class: "cloud",
        surface_kind: "generic-public-http-connector",
        config_format: "request-or-dashboard",
        config_paths: &[],
        config_precedence: &["surface-defined"],
        native_scopes: &["request", "project", "team"],
        supported_ingresses: &["streamable-http", "sse"],
        documented_features: &["tools"],
        documented_constraints: &["tools-only", "public-http-only"],
        notes: &[
            "Use this when the client lives in the cloud and can only reach public HTTP MCP servers.",
        ],
    },
];

pub fn find(id: &str) -> Option<&'static ClientTarget> {
    let normalized = normalize(id);
    CLIENT_TARGETS.iter().find(|target| {
        target.id == normalized
            || target
                .aliases
                .iter()
                .any(|alias| normalize(alias) == normalized)
    })
}

pub fn normalize(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

impl ClientTarget {
    pub fn supports_ingress(&self, ingress: &str) -> bool {
        let normalized = normalize_transport(ingress);
        self.supported_ingresses
            .iter()
            .any(|value| normalize_transport(value) == normalized)
    }

    pub fn has_feature(&self, feature: &str) -> bool {
        let normalized = normalize(feature);
        self.documented_features
            .iter()
            .any(|value| normalize(value) == normalized)
    }

    pub fn has_constraint(&self, constraint: &str) -> bool {
        let normalized = normalize(constraint);
        self.documented_constraints
            .iter()
            .any(|value| normalize(value) == normalized)
    }

    pub fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("id", JsonValue::string(self.id)),
            ("familyId", JsonValue::string(self.family_id)),
            ("displayName", JsonValue::string(self.display_name)),
            (
                "aliases",
                JsonValue::array(self.aliases.iter().copied().map(JsonValue::string)),
            ),
            ("maturity", JsonValue::string(self.maturity)),
            ("surfaceClass", JsonValue::string(self.surface_class)),
            ("surfaceKind", JsonValue::string(self.surface_kind)),
            ("configFormat", JsonValue::string(self.config_format)),
            (
                "configPaths",
                JsonValue::array(self.config_paths.iter().copied().map(JsonValue::string)),
            ),
            (
                "configPrecedence",
                JsonValue::array(
                    self.config_precedence
                        .iter()
                        .copied()
                        .map(JsonValue::string),
                ),
            ),
            (
                "nativeScopes",
                JsonValue::array(self.native_scopes.iter().copied().map(JsonValue::string)),
            ),
            (
                "supportedIngresses",
                JsonValue::array(
                    self.supported_ingresses
                        .iter()
                        .copied()
                        .map(JsonValue::string),
                ),
            ),
            (
                "documentedFeatures",
                JsonValue::array(
                    self.documented_features
                        .iter()
                        .copied()
                        .map(JsonValue::string),
                ),
            ),
            (
                "documentedConstraints",
                JsonValue::array(
                    self.documented_constraints
                        .iter()
                        .copied()
                        .map(JsonValue::string),
                ),
            ),
            (
                "notes",
                JsonValue::array(self.notes.iter().copied().map(JsonValue::string)),
            ),
        ])
    }
}

fn normalize_transport(value: &str) -> String {
    match normalize(value).as_str() {
        "http" | "streamable-http" | "streamable_http" => "streamable-http".to_string(),
        "stdio" | "local-stdio" => "stdio".to_string(),
        "sse" => "sse".to_string(),
        other => other.to_string(),
    }
}
