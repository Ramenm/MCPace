use crate::json::JsonValue;
use crate::mcp_sources::{self, McpServerWriteOptions, McpServerWriteResult};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default)]
pub struct McpAutoInstallOptions {
    pub spec: String,
    pub name_override: Option<String>,
    pub server_type: Option<String>,
    pub command: Option<String>,
    pub url: Option<String>,
    pub paths: Vec<String>,
    pub extra_args: Vec<String>,
    pub env: Vec<String>,
    pub headers: Vec<String>,
    pub settings_path: Option<PathBuf>,
    pub dry_run: bool,
    pub force: bool,
    pub disabled: bool,
}

#[derive(Clone, Debug)]
pub struct McpAutoInstallPlan {
    pub original_spec: String,
    pub name: String,
    pub method: String,
    pub launcher: String,
    pub server_type: String,
    pub command: Option<String>,
    pub url: Option<String>,
    pub package: Option<String>,
    pub args: Vec<String>,
    pub assumptions: Vec<String>,
    pub next_checks: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct McpAutoInstallResult {
    pub plan: McpAutoInstallPlan,
    pub write: McpServerWriteResult,
}

pub fn install_auto(
    root_path: &Path,
    options: McpAutoInstallOptions,
) -> Result<McpAutoInstallResult, String> {
    let plan = build_auto_install_plan(&options)?;
    let write = mcp_sources::write_mcp_server_entry(
        root_path,
        McpServerWriteOptions {
            name: plan.name.clone(),
            server_type: Some(plan.server_type.clone()),
            command: plan.command.clone(),
            args: plan.args.clone(),
            url: plan.url.clone(),
            env: options.env,
            headers: options.headers,
            settings_path: options.settings_path,
            enabled: !options.disabled,
            dry_run: options.dry_run,
            force: options.force,
        },
    )?;
    Ok(McpAutoInstallResult { plan, write })
}

fn build_auto_install_plan(options: &McpAutoInstallOptions) -> Result<McpAutoInstallPlan, String> {
    let spec = options.spec.trim();
    let explicit_type = options
        .server_type
        .as_deref()
        .map(normalize_install_type)
        .transpose()?;
    let explicit_url = options
        .url
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let explicit_command = options
        .command
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    if spec.is_empty() && explicit_url.is_none() && explicit_command.is_none() {
        return Err(
            "server install requires a package spec, --url <endpoint>, or --command <cmd>"
                .to_string(),
        );
    }

    if explicit_url.is_some()
        || looks_like_url(spec)
        || explicit_type.as_deref() == Some("streamable-http")
    {
        let url = explicit_url.unwrap_or_else(|| spec.to_string());
        let name = resolve_name(options, || name_from_url(&url));
        return Ok(McpAutoInstallPlan {
            original_spec: spec.to_string(),
            name,
            method: "remote-url".to_string(),
            launcher: "remote-url".to_string(),
            server_type: "streamable-http".to_string(),
            command: None,
            url: Some(url),
            package: None,
            args: Vec::new(),
            assumptions: vec![
                "remote Streamable HTTP is treated as session-bound until MCP-Session-Id behavior is observed"
                    .to_string(),
            ],
            next_checks: vec![
                "mcpace server sources --json".to_string(),
                "run remote initialize/session smoke before enabling broad fan-out".to_string(),
            ],
        });
    }

    if let Some(command) = explicit_command {
        let name = resolve_name(options, || name_from_spec(spec, "local-command"));
        let mut args = options.extra_args.clone();
        args.extend(options.paths.iter().cloned());
        return Ok(McpAutoInstallPlan {
            original_spec: spec.to_string(),
            name,
            method: "local-command".to_string(),
            launcher: launcher_from_command(&command),
            server_type: "stdio".to_string(),
            command: Some(command),
            url: None,
            package: if spec.is_empty() { None } else { Some(spec.to_string()) },
            args,
            assumptions: vec![
                "custom stdio command starts conservative until initialize/tools-list evidence is collected"
                    .to_string(),
            ],
            next_checks: vec!["mcpace server test <name> --refresh --json".to_string()],
        });
    }

    let method = choose_package_method(spec, explicit_type.as_deref());
    match method.as_str() {
        "npm" => {
            let package = strip_known_prefix(spec, "npm:").to_string();
            let name = resolve_name(options, || name_from_spec(&package, "npm"));
            let mut args = vec!["-y".to_string(), package.clone()];
            args.extend(options.extra_args.iter().cloned());
            args.extend(options.paths.iter().cloned());
            Ok(McpAutoInstallPlan {
                original_spec: spec.to_string(),
                name,
                method,
                launcher: "npx".to_string(),
                server_type: "stdio".to_string(),
                command: Some("npx".to_string()),
                url: None,
                package: Some(package),
                args,
                assumptions: vec![
                    "npm package execution is explicit through npx -y; install scripts are not run by MCPace itself"
                        .to_string(),
                    "statefulness is inferred later from source hints and live MCP probes, not from a packaged upstream catalog"
                        .to_string(),
                ],
                next_checks: vec!["mcpace server test <name> --refresh --json".to_string()],
            })
        }
        "pypi" => {
            let package = strip_known_prefix(strip_known_prefix(spec, "pypi:"), "uvx:").to_string();
            let name = resolve_name(options, || name_from_spec(&package, "pypi"));
            let mut args = vec![package.clone()];
            args.extend(options.extra_args.iter().cloned());
            args.extend(options.paths.iter().cloned());
            Ok(McpAutoInstallPlan {
                original_spec: spec.to_string(),
                name,
                method,
                launcher: "uvx".to_string(),
                server_type: "stdio".to_string(),
                command: Some("uvx".to_string()),
                url: None,
                package: Some(package),
                args,
                assumptions: vec![
                    "PyPI package execution is explicit through uvx and should be tested in an isolated environment"
                        .to_string(),
                    "statefulness is inferred later from source hints and live MCP probes, not from a packaged upstream catalog"
                        .to_string(),
                ],
                next_checks: vec!["mcpace server test <name> --refresh --json".to_string()],
            })
        }
        "oci" => {
            let image = strip_known_prefix(strip_known_prefix(spec, "oci:"), "docker:").to_string();
            let name = resolve_name(options, || name_from_spec(&image, "oci"));
            let mut args = vec!["run".to_string(), "--rm".to_string(), image.clone()];
            args.extend(options.extra_args.iter().cloned());
            args.extend(options.paths.iter().cloned());
            Ok(McpAutoInstallPlan {
                original_spec: spec.to_string(),
                name,
                method,
                launcher: "oci".to_string(),
                server_type: "stdio".to_string(),
                command: Some("docker".to_string()),
                url: None,
                package: Some(image),
                args,
                assumptions: vec![
                    "containerized MCP servers remain unknown stdio until image provenance and probe results are reviewed"
                        .to_string(),
                ],
                next_checks: vec!["mcpace server test <name> --refresh --json".to_string()],
            })
        }
        other => Err(format!(
            "unsupported server install type '{}'; use npm:, pypi:, oci:, --url, or --command",
            other
        )),
    }
}

fn normalize_install_type(value: &str) -> Result<String, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "" => Ok(String::new()),
        "npm" | "npx" | "node" => Ok("npm".to_string()),
        "pypi" | "python" | "uvx" | "uv" => Ok("pypi".to_string()),
        "oci" | "docker" | "container" => Ok("oci".to_string()),
        "stdio" | "command" | "local" | "local-command" => Ok("stdio".to_string()),
        "http" | "streamable-http" | "streamable_http" | "remote" | "url" => {
            Ok("streamable-http".to_string())
        }
        other => Err(format!(
            "unsupported server install type '{}'; use npm, pypi, oci, stdio, or streamable-http",
            other
        )),
    }
}

fn choose_package_method(spec: &str, explicit_type: Option<&str>) -> String {
    if let Some(kind) = explicit_type {
        if kind == "stdio" {
            return "npm".to_string();
        }
        if !kind.is_empty() {
            return kind.to_string();
        }
    }
    let lower = spec.trim().to_ascii_lowercase();
    if lower.starts_with("pypi:") || lower.starts_with("uvx:") {
        return "pypi".to_string();
    }
    if lower.starts_with("oci:") || lower.starts_with("docker:") {
        return "oci".to_string();
    }
    "npm".to_string()
}

fn resolve_name(options: &McpAutoInstallOptions, fallback: impl FnOnce() -> String) -> String {
    options
        .name_override
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(fallback)
}

fn looks_like_url(value: &str) -> bool {
    let lower = value.trim().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

fn strip_known_prefix<'a>(value: &'a str, prefix: &str) -> &'a str {
    let trimmed = value.trim();
    if trimmed.to_ascii_lowercase().starts_with(prefix) {
        &trimmed[prefix.len()..]
    } else {
        trimmed
    }
}

fn name_from_url(url: &str) -> String {
    let mut value = url
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_matches('/');
    if let Some((host, path)) = value.split_once('/') {
        value = if path.trim().is_empty() { host } else { path };
    }
    let normalized = mcp_sources::normalize_server_name(value);
    if normalized.is_empty() {
        "remote-mcp".to_string()
    } else {
        normalized
    }
}

fn name_from_spec(spec: &str, fallback: &str) -> String {
    let stripped = spec
        .trim()
        .trim_start_matches("npm:")
        .trim_start_matches("pypi:")
        .trim_start_matches("uvx:")
        .trim_start_matches("oci:")
        .trim_start_matches("docker:");
    let package_name = package_name_without_version(stripped);
    let candidate = package_name
        .trim_start_matches("@")
        .replace('/', "-")
        .trim_start_matches("modelcontextprotocol-server-")
        .trim_start_matches("mcp-server-")
        .trim_start_matches("server-")
        .trim_start_matches("mcp-")
        .to_string();
    let normalized = mcp_sources::normalize_server_name(&candidate);
    if normalized.is_empty() {
        fallback.to_string()
    } else {
        normalized
    }
}

fn package_name_without_version(spec: &str) -> String {
    let value = spec.trim();
    if let Some(without_at) = value.strip_prefix('@') {
        if let Some((scope, rest)) = without_at.split_once('/') {
            let name = rest.rsplit_once('@').map(|(left, _)| left).unwrap_or(rest);
            return format!("{}-{}", scope, name);
        }
    }
    value
        .split("==")
        .next()
        .unwrap_or(value)
        .rsplit_once('@')
        .map(|(left, _)| left)
        .unwrap_or(value)
        .to_string()
}

fn launcher_from_command(command: &str) -> String {
    let lower = command.trim().to_ascii_lowercase();
    if lower.contains("npx") {
        "npx".to_string()
    } else if lower.contains("uvx") {
        "uvx".to_string()
    } else if lower.contains("docker") {
        "oci".to_string()
    } else if lower.is_empty() {
        "unspecified".to_string()
    } else {
        "local-command".to_string()
    }
}

impl McpAutoInstallPlan {
    pub fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            (
                "originalSpec",
                JsonValue::string(self.original_spec.clone()),
            ),
            ("name", JsonValue::string(self.name.clone())),
            ("method", JsonValue::string(self.method.clone())),
            ("launcher", JsonValue::string(self.launcher.clone())),
            ("type", JsonValue::string(self.server_type.clone())),
            (
                "command",
                self.command
                    .clone()
                    .map(JsonValue::string)
                    .unwrap_or(JsonValue::Null),
            ),
            (
                "url",
                self.url
                    .clone()
                    .map(JsonValue::string)
                    .unwrap_or(JsonValue::Null),
            ),
            (
                "package",
                self.package
                    .clone()
                    .map(JsonValue::string)
                    .unwrap_or(JsonValue::Null),
            ),
            (
                "args",
                JsonValue::array(self.args.iter().cloned().map(JsonValue::string)),
            ),
            (
                "assumptions",
                JsonValue::array(self.assumptions.iter().cloned().map(JsonValue::string)),
            ),
            (
                "nextChecks",
                JsonValue::array(self.next_checks.iter().cloned().map(JsonValue::string)),
            ),
        ])
    }
}

impl McpAutoInstallResult {
    pub fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("plan", self.plan.to_json_value()),
            ("write", self.write.to_json_value()),
        ])
    }
}
