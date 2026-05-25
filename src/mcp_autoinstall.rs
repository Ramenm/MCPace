use crate::json::JsonValue;
use crate::mcp_sources::{self, McpServerWriteOptions, McpServerWriteResult};
use crate::runtimepaths;
use std::path::{Path, PathBuf};

const FILESYSTEM_NPM_PACKAGE: &str = "@modelcontextprotocol/server-filesystem";

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
struct CommandLikeSpec {
    method: String,
    launcher: String,
    command: String,
    args: Vec<String>,
    package: Option<String>,
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

    if spec.is_empty()
        && explicit_url.is_none()
        && explicit_command.is_none()
        && options.paths.is_empty()
        && explicit_type.as_deref() != Some("filesystem")
    {
        return Err(
            "server install requires a package spec, URL, local path, or command-like server command"
                .to_string(),
        );
    }

    if explicit_type.as_deref() == Some("filesystem")
        || (explicit_url.is_none()
            && explicit_command.is_none()
            && ((spec.is_empty() && !options.paths.is_empty()) || looks_like_local_path_spec(spec)))
    {
        return Ok(filesystem_path_install_plan(options, spec));
    }

    let url_requested = explicit_url.is_some()
        || looks_like_url(spec)
        || explicit_type.as_deref() == Some("streamable-http");
    if url_requested {
        let url = explicit_url
            .or_else(|| looks_like_url(spec).then(|| spec.to_string()))
            .ok_or_else(|| "server install --type streamable-http requires --url <endpoint> or an https:// URL spec".to_string())?;
        let name = resolve_name(options, || name_from_url(&url));
        return Ok(McpAutoInstallPlan {
            original_spec: spec.to_string(),
            name: name.clone(),
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
                format!("mcpace connect --server {}", name),
            ],
        });
    }

    if let Some(command) = explicit_command {
        if let Some(command_like) = command_like_spec(&command)? {
            return Ok(command_like_install_plan(options, spec, command_like));
        }
        let name = resolve_name(options, || name_from_spec(spec, "local-command"));
        let mut args = options.extra_args.clone();
        args.extend(options.paths.iter().cloned());
        return Ok(McpAutoInstallPlan {
            original_spec: spec.to_string(),
            name: name.clone(),
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
            next_checks: vec![format!("mcpace server test {} --refresh --json", name)],
        });
    }

    if let Some(command_like) = command_like_spec(spec)? {
        return Ok(command_like_install_plan(options, spec, command_like));
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
                name: name.clone(),
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
                next_checks: vec![format!("mcpace server test {} --refresh --json", name)],
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
                name: name.clone(),
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
                next_checks: vec![format!("mcpace server test {} --refresh --json", name)],
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
                name: name.clone(),
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
                next_checks: vec![format!("mcpace server test {} --refresh --json", name)],
            })
        }
        other => Err(format!(
            "unsupported server install type '{}'; use npm:, pypi:, oci:, an https:// URL, or a command such as npx/uvx/docker run",
            other
        )),
    }
}

fn command_like_install_plan(
    options: &McpAutoInstallOptions,
    spec: &str,
    command_like: CommandLikeSpec,
) -> McpAutoInstallPlan {
    let name = resolve_name(options, || name_from_command_like(&command_like, spec));
    let mut args = command_like.args.clone();
    args.extend(options.extra_args.iter().cloned());
    args.extend(options.paths.iter().cloned());
    McpAutoInstallPlan {
        original_spec: spec.to_string(),
        name: name.clone(),
        method: command_like.method.clone(),
        launcher: command_like.launcher.clone(),
        server_type: "stdio".to_string(),
        command: Some(command_like.command.clone()),
        url: None,
        package: command_like.package.clone(),
        args,
        assumptions: assumptions_for_command_like(&command_like),
        next_checks: vec![format!("mcpace server test {} --refresh --json", name)],
    }
}

fn filesystem_path_install_plan(options: &McpAutoInstallOptions, spec: &str) -> McpAutoInstallPlan {
    let name = resolve_name(options, || "filesystem".to_string());
    let mut args = vec!["-y".to_string(), FILESYSTEM_NPM_PACKAGE.to_string()];
    args.extend(options.extra_args.iter().cloned());

    let mut paths = Vec::new();
    if let Some(path) = local_path_argument(spec) {
        paths.push(path);
    }
    paths.extend(options.paths.iter().cloned());
    if paths.is_empty() {
        paths.push(".".to_string());
    }
    args.extend(paths.iter().map(|path| absolutize_local_path_arg(path)));

    McpAutoInstallPlan {
        original_spec: spec.to_string(),
        name: name.clone(),
        method: "filesystem-path".to_string(),
        launcher: "npx".to_string(),
        server_type: "stdio".to_string(),
        command: Some("npx".to_string()),
        url: None,
        package: Some(FILESYSTEM_NPM_PACKAGE.to_string()),
        args,
        assumptions: vec![
            "local path input was auto-detected as the official filesystem MCP server".to_string(),
            "filesystem access is limited to the configured path arguments".to_string(),
            "statefulness is inferred later from live MCP probes and source hints".to_string(),
        ],
        next_checks: vec![format!("mcpace server test {} --refresh --json", name)],
    }
}

fn command_like_spec(value: &str) -> Result<Option<CommandLikeSpec>, String> {
    let tokens = split_command_words(value)?;
    if tokens.len() < 2 {
        return Ok(None);
    }

    let command = tokens[0].clone();
    let base = command_basename(&command).to_ascii_lowercase();
    let tail = tokens[1..].to_vec();

    if base == "npx" {
        let mut args = tail;
        ensure_npx_yes(&mut args);
        let package = first_non_option_arg(&args, 0);
        return Ok(Some(CommandLikeSpec {
            method: "npm".to_string(),
            launcher: "npx".to_string(),
            command,
            args,
            package,
        }));
    }

    if base == "bunx" {
        let package = first_non_option_arg(&tail, 0);
        return Ok(Some(CommandLikeSpec {
            method: "npm".to_string(),
            launcher: "bunx".to_string(),
            command,
            args: tail,
            package,
        }));
    }

    if (base == "pnpm" || base == "yarn") && tail.first().map(|value| value.as_str()) == Some("dlx")
    {
        let package = first_non_option_arg(&tail, 1);
        return Ok(Some(CommandLikeSpec {
            method: "npm".to_string(),
            launcher: base,
            command,
            args: tail,
            package,
        }));
    }

    if base == "uvx" {
        let package = first_non_option_arg(&tail, 0);
        return Ok(Some(CommandLikeSpec {
            method: "pypi".to_string(),
            launcher: "uvx".to_string(),
            command,
            args: tail,
            package,
        }));
    }

    if base == "docker" {
        let package = docker_image_arg(&tail);
        return Ok(Some(CommandLikeSpec {
            method: "oci".to_string(),
            launcher: "oci".to_string(),
            command,
            args: tail,
            package,
        }));
    }

    Ok(Some(CommandLikeSpec {
        method: "local-command".to_string(),
        launcher: launcher_from_command(&command),
        command,
        args: tail,
        package: None,
    }))
}

fn split_command_words(value: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaping = false;

    for ch in value.trim().chars() {
        if escaping {
            current.push(ch);
            escaping = false;
            continue;
        }
        if ch == '\\' {
            if quote.is_some() {
                escaping = true;
            } else {
                current.push(ch);
            }
            continue;
        }
        if let Some(active_quote) = quote {
            if ch == active_quote {
                quote = None;
            } else {
                current.push(ch);
            }
            continue;
        }
        if ch == '\'' || ch == '"' {
            quote = Some(ch);
            continue;
        }
        if ch.is_whitespace() {
            if !current.is_empty() {
                words.push(current.clone());
                current.clear();
            }
            continue;
        }
        current.push(ch);
    }

    if escaping {
        current.push('\\');
    }
    if let Some(active_quote) = quote {
        return Err(format!(
            "server install command spec has an unterminated {} quote",
            active_quote
        ));
    }
    if !current.is_empty() {
        words.push(current);
    }
    Ok(words)
}

fn command_basename(command: &str) -> &str {
    let slash = command.rfind('/');
    let backslash = command.rfind('\\');
    let start = match (slash, backslash) {
        (Some(left), Some(right)) => left.max(right) + 1,
        (Some(index), None) | (None, Some(index)) => index + 1,
        (None, None) => 0,
    };
    command[start..]
        .strip_suffix(".exe")
        .or_else(|| command[start..].strip_suffix(".EXE"))
        .unwrap_or(&command[start..])
}

fn ensure_npx_yes(args: &mut Vec<String>) {
    if args.iter().any(|arg| arg == "-y" || arg == "--yes") {
        return;
    }
    args.insert(0, "-y".to_string());
}

fn first_non_option_arg(args: &[String], start_index: usize) -> Option<String> {
    let mut index = start_index;
    while index < args.len() {
        let arg = args[index].trim();
        if arg == "--" {
            return args.get(index + 1).cloned();
        }
        if arg == "--package" || arg == "-p" {
            index += 2;
            continue;
        }
        if arg.starts_with('-') {
            index += 1;
            continue;
        }
        return Some(args[index].clone());
    }
    None
}

fn docker_image_arg(args: &[String]) -> Option<String> {
    let mut index = usize::from(args.first().map(|value| value == "run").unwrap_or(false));
    while index < args.len() {
        let arg = args[index].trim();
        if arg == "--" {
            return args.get(index + 1).cloned();
        }
        if docker_option_takes_value(arg) {
            index += 2;
            continue;
        }
        if arg.starts_with("--") && arg.contains('=') {
            index += 1;
            continue;
        }
        if arg.starts_with('-') {
            index += 1;
            continue;
        }
        return Some(args[index].clone());
    }
    None
}

fn docker_option_takes_value(arg: &str) -> bool {
    matches!(
        arg,
        "-e" | "--env"
            | "-v"
            | "--volume"
            | "-p"
            | "--publish"
            | "--name"
            | "-w"
            | "--workdir"
            | "--network"
            | "--entrypoint"
            | "--user"
            | "-u"
    )
}

fn assumptions_for_command_like(command_like: &CommandLikeSpec) -> Vec<String> {
    match command_like.method.as_str() {
        "npm" => vec![
            format!(
                "{} command-like spec was auto-detected as an npm stdio MCP launcher",
                command_like.launcher
            ),
            "statefulness is inferred later from source hints and live MCP probes, not from a packaged upstream catalog"
                .to_string(),
        ],
        "pypi" => vec![
            "uvx command-like spec was auto-detected as a PyPI stdio MCP launcher".to_string(),
            "statefulness is inferred later from source hints and live MCP probes, not from a packaged upstream catalog"
                .to_string(),
        ],
        "oci" => vec![
            "docker command-like spec was auto-detected as a containerized stdio MCP launcher"
                .to_string(),
            "containerized MCP servers remain unknown stdio until image provenance and probe results are reviewed"
                .to_string(),
        ],
        _ => vec![
            "custom command-like stdio server starts conservative until initialize/tools-list evidence is collected"
                .to_string(),
        ],
    }
}

fn name_from_command_like(command_like: &CommandLikeSpec, spec: &str) -> String {
    if let Some(package) = &command_like.package {
        return name_from_spec(package, &command_like.launcher);
    }
    if let Some(first_arg) = command_like.args.first() {
        return name_from_spec(first_arg, "local-command");
    }
    name_from_spec(spec, "local-command")
}

fn normalize_install_type(value: &str) -> Result<String, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "" | "auto" | "infer" | "detect" => Ok(String::new()),
        "npm" | "npx" | "node" => Ok("npm".to_string()),
        "pypi" | "python" | "uvx" | "uv" => Ok("pypi".to_string()),
        "oci" | "docker" | "container" => Ok("oci".to_string()),
        "filesystem" | "fs" | "path" | "directory" | "dir" => Ok("filesystem".to_string()),
        "stdio" | "command" | "local" | "local-command" => Ok("stdio".to_string()),
        "http" | "streamable-http" | "streamable_http" | "remote" | "url" => {
            Ok("streamable-http".to_string())
        }
        other => Err(format!(
            "unsupported server install type '{}'; use auto, npm, pypi, oci, filesystem, stdio, or streamable-http",
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

fn looks_like_local_path_spec(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() || looks_like_url(trimmed) {
        return false;
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("file:") || lower.starts_with("path:") {
        return true;
    }
    if trimmed == "."
        || trimmed == ".."
        || trimmed.starts_with("./")
        || trimmed.starts_with("../")
        || trimmed.starts_with("~/")
        || trimmed.starts_with('/')
        || trimmed.starts_with('\\')
    {
        return true;
    }
    if trimmed.len() > 2 && trimmed.as_bytes()[1] == b':' {
        return true;
    }
    Path::new(trimmed).exists()
}

fn local_path_argument(spec: &str) -> Option<String> {
    let trimmed = spec.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();
    let value = if lower.starts_with("file://") {
        trimmed[7..].to_string()
    } else if lower.starts_with("file:") || lower.starts_with("path:") {
        trimmed[5..].to_string()
    } else {
        trimmed.to_string()
    };
    Some(if value.trim().is_empty() {
        ".".to_string()
    } else {
        value
    })
}

fn absolutize_local_path_arg(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return ".".to_string();
    }
    if let Some(rest) = trimmed
        .strip_prefix("~/")
        .or_else(|| trimmed.strip_prefix("~\\"))
    {
        if let Some(home) = runtimepaths::user_home_dir() {
            let mut path = home;
            for segment in rest.split(['/', '\\']) {
                if !segment.is_empty() {
                    path.push(segment);
                }
            }
            return path.display().to_string();
        }
    }
    let path = PathBuf::from(trimmed);
    let candidate = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| PathBuf::from(trimmed))
    };
    std::fs::canonicalize(&candidate)
        .unwrap_or(candidate)
        .display()
        .to_string()
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
    let stripped = strip_known_prefix(
        strip_known_prefix(
            strip_known_prefix(
                strip_known_prefix(strip_known_prefix(spec, "npm:"), "pypi:"),
                "uvx:",
            ),
            "oci:",
        ),
        "docker:",
    );
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
