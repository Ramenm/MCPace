use crate::json::JsonValue;
use crate::mcp_sources::{self, McpServerWriteOptions, McpServerWriteResult};
use crate::runtimepaths;
use crate::text_utils;
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
    pub launcher_args: Vec<String>,
    pub extra_args: Vec<String>,
    pub env: Vec<String>,
    pub headers: Vec<String>,
    pub settings_path: Option<PathBuf>,
    pub dry_run: bool,
    pub force: bool,
    pub disabled: bool,
    pub profile_hints: Vec<String>,
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
    let plan = build_auto_install_plan(&options, Some(root_path))?;
    let mut profile_hints = options.profile_hints.clone();
    profile_hints.extend(profile_hints_for_plan(&plan));
    profile_hints.sort();
    profile_hints.dedup();

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
            profile_hints,
        },
    )?;
    Ok(McpAutoInstallResult { plan, write })
}

pub fn plan_auto_install(options: &McpAutoInstallOptions) -> Result<McpAutoInstallPlan, String> {
    build_auto_install_plan(options, None)
}

fn build_auto_install_plan(
    options: &McpAutoInstallOptions,
    install_root: Option<&Path>,
) -> Result<McpAutoInstallPlan, String> {
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
        return Ok(filesystem_path_install_plan(options, spec, install_root));
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
                "mcpace advanced server sources --json".to_string(),
                format!("mcpace advanced server test {} --refresh --json", name),
            ],
        });
    }

    if let Some(command) = explicit_command {
        if let Some(command_like) = command_like_spec(&command)? {
            return command_like_install_plan(options, spec, command_like, install_root);
        }
        let name = resolve_name(options, || name_from_spec(spec, "local-command"));
        let mut args = options.launcher_args.clone();
        args.extend(options.extra_args.iter().cloned());
        extend_install_path_args(&mut args, &options.paths, install_root);
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
            next_checks: vec![format!(
                "mcpace advanced server test {} --refresh --json",
                name
            )],
        });
    }

    if let Some(command_like) = command_like_spec(spec)? {
        return command_like_install_plan(options, spec, command_like, install_root);
    }

    let method = choose_package_method(spec, explicit_type.as_deref());
    match method.as_str() {
        "npm" => {
            let package = validate_install_identifier("npm", strip_known_prefix(spec, "npm:"))?;
            let name = resolve_name(options, || name_from_spec(&package, "npm"));
            let mut args = vec!["-y".to_string()];
            args.extend(
                options
                    .launcher_args
                    .iter()
                    .filter(|value| !matches!(value.as_str(), "-y" | "--yes"))
                    .cloned(),
            );
            args.push(package.clone());
            args.extend(options.extra_args.iter().cloned());
            extend_install_path_args(&mut args, &options.paths, install_root);
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
                    "npm package execution uses npx -y; the first live probe/call may download the package and execute its npm lifecycle scripts, so review the package identity and use --dry-run before allowing execution"
                        .to_string(),
                    "statefulness is inferred later from source hints and live MCP probes, not from a packaged upstream catalog"
                        .to_string(),
                ],
                next_checks: vec![format!(
                    "mcpace advanced server test {} --refresh --json",
                    name
                )],
            })
        }
        "pypi" => {
            let package = validate_install_identifier(
                "pypi",
                strip_known_prefix(strip_known_prefix(spec, "pypi:"), "uvx:"),
            )?;
            let name = resolve_name(options, || name_from_spec(&package, "pypi"));
            let mut args = options.launcher_args.clone();
            args.push(package.clone());
            args.extend(options.extra_args.iter().cloned());
            extend_install_path_args(&mut args, &options.paths, install_root);
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
                next_checks: vec![format!(
                    "mcpace advanced server test {} --refresh --json",
                    name
                )],
            })
        }
        "oci" => {
            let image = validate_install_identifier(
                "oci",
                strip_known_prefix(strip_known_prefix(spec, "oci:"), "docker:"),
            )?;
            let name = resolve_name(options, || name_from_spec(&image, "oci"));
            let mut args = vec!["run".to_string(), "--rm".to_string()];
            args.extend(options.launcher_args.iter().cloned());
            args.push(image.clone());
            args.extend(options.extra_args.iter().cloned());
            extend_install_path_args(&mut args, &options.paths, install_root);
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
                next_checks: vec![format!(
                    "mcpace advanced server test {} --refresh --json",
                    name
                )],
            })
        }
        "nuget" | "mcpb" => Err(format!(
            "server install type '{}' is recognized from the MCP Registry but not executable by MCPace yet; it remains plan-only until a safe launcher is implemented",
            method
        )),
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
    install_root: Option<&Path>,
) -> Result<McpAutoInstallPlan, String> {
    if let Some(package) = command_like.package.as_deref() {
        let _ = validate_install_identifier(&command_like.method, package)?;
    }
    let name = resolve_name(options, || name_from_command_like(&command_like, spec));
    let mut args = options.launcher_args.clone();
    args.extend(command_like.args.iter().cloned());
    args.extend(options.extra_args.iter().cloned());
    extend_install_path_args(&mut args, &options.paths, install_root);
    Ok(McpAutoInstallPlan {
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
        next_checks: vec![format!(
            "mcpace advanced server test {} --refresh --json",
            name
        )],
    })
}

fn validate_install_identifier(method: &str, value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!(
            "server install {} package/image identifier cannot be empty",
            method
        ));
    }
    if trimmed.starts_with('-') {
        return Err(format!(
            "server install {} package/image identifier '{}' must not start with '-'",
            method, trimmed
        ));
    }
    if trimmed
        .chars()
        .any(|ch| ch.is_control() || ch.is_whitespace())
    {
        return Err(format!(
            "server install {} package/image identifier '{}' must not contain whitespace or control characters",
            method, trimmed
        ));
    }
    if text_utils::uses_shell_composition(trimmed) {
        return Err(format!(
            "server install {} package/image identifier '{}' must be a single registry identifier, not a shell expression",
            method, trimmed
        ));
    }
    if trimmed.contains("://") {
        return Err(format!(
            "server install {} package/image identifier '{}' must not be a URL; use --url for remote MCP endpoints",
            method, trimmed
        ));
    }
    if method != "oci" {
        let lower = trimmed.to_ascii_lowercase();
        if trimmed.contains(':')
            || lower.starts_with("file:")
            || lower.starts_with("path:")
            || trimmed.starts_with('.')
            || trimmed.starts_with('/')
            || trimmed.starts_with('\\')
        {
            return Err(format!(
                "server install {} package identifier '{}' must be a registry package name/version, not a path, URL, or alias expression",
                method, trimmed
            ));
        }
    }
    Ok(trimmed.to_string())
}

fn profile_hints_for_plan(plan: &McpAutoInstallPlan) -> Vec<String> {
    let mut hints = Vec::new();

    // These are install-intent hints, not trust grants. They only tighten the
    // first policy into a conservative project/session/external class; MCPace
    // still requires initialize/tools-list evidence before widening concurrency.
    if plan_is_explicit_filesystem(plan) {
        hints.extend([
            "filesystem".to_string(),
            "project-root".to_string(),
            "isolated-per-project".to_string(),
        ]);
    }

    if plan.server_type == "streamable-http" {
        hints.push("streamable-http".to_string());
    }

    if plan.method == "oci" {
        hints.extend(["container".to_string(), "unknown-side-effects".to_string()]);
    }

    hints
}

fn plan_is_explicit_filesystem(plan: &McpAutoInstallPlan) -> bool {
    if plan.method == "filesystem-path" {
        return true;
    }
    plan.package
        .as_deref()
        .map(is_official_filesystem_package)
        .unwrap_or(false)
        && has_filesystem_path_argument(plan)
}

fn is_official_filesystem_package(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed == FILESYSTEM_NPM_PACKAGE
        || package_name_without_version(trimmed) == "modelcontextprotocol-server-filesystem"
}

fn has_filesystem_path_argument(plan: &McpAutoInstallPlan) -> bool {
    let mut saw_package = false;
    for arg in &plan.args {
        let trimmed = arg.trim();
        if !saw_package && is_official_filesystem_package(trimmed) {
            saw_package = true;
            continue;
        }
        if !saw_package || trimmed.starts_with('-') || trimmed.is_empty() {
            continue;
        }
        if looks_like_local_path_spec(trimmed) {
            return true;
        }
    }
    false
}

fn extend_install_path_args(args: &mut Vec<String>, paths: &[String], install_root: Option<&Path>) {
    args.extend(
        paths
            .iter()
            .map(|path| absolutize_local_path_arg(path, install_root)),
    );
}

fn filesystem_path_install_plan(
    options: &McpAutoInstallOptions,
    spec: &str,
    install_root: Option<&Path>,
) -> McpAutoInstallPlan {
    let name = resolve_name(options, || "filesystem".to_string());
    let mut args = vec!["-y".to_string()];
    args.extend(
        options
            .launcher_args
            .iter()
            .filter(|value| !matches!(value.as_str(), "-y" | "--yes"))
            .cloned(),
    );
    args.push(FILESYSTEM_NPM_PACKAGE.to_string());
    args.extend(options.extra_args.iter().cloned());

    let mut paths = Vec::new();
    if let Some(path) = local_path_argument(spec) {
        paths.push(path);
    }
    paths.extend(options.paths.iter().cloned());
    if paths.is_empty() {
        paths.push(".".to_string());
    }
    args.extend(
        paths
            .iter()
            .map(|path| absolutize_local_path_arg(path, install_root)),
    );

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
        next_checks: vec![format!(
            "mcpace advanced server test {} --refresh --json",
            name
        )],
    }
}

fn command_like_spec(value: &str) -> Result<Option<CommandLikeSpec>, String> {
    if text_utils::uses_shell_composition(value) {
        return Err(
            "server install command spec must be one launcher, URL, or path; remove shell chaining, background operators, pipes, redirects, backticks, or command substitutions"
                .to_string(),
        );
    }
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
    let mut iter = args.iter().skip(start_index).peekable();
    while let Some(raw_arg) = iter.next() {
        let arg = raw_arg.trim();
        if arg == "--" {
            return iter.next().cloned();
        }
        if let Some(value) = inline_package_option_value(arg) {
            return Some(value.to_string());
        }
        if launcher_option_selects_package(arg) {
            return iter.next().cloned();
        }
        if launcher_option_takes_value(arg) {
            let _ = iter.next();
            continue;
        }
        if arg.starts_with("--") && arg.contains('=') {
            continue;
        }
        if arg.starts_with('-') {
            continue;
        }
        return Some(raw_arg.clone());
    }
    None
}

fn inline_package_option_value(arg: &str) -> Option<&str> {
    arg.strip_prefix("--package=")
        .or_else(|| arg.strip_prefix("--from="))
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn launcher_option_selects_package(arg: &str) -> bool {
    matches!(arg, "--package" | "-p" | "--from")
}

fn launcher_option_takes_value(arg: &str) -> bool {
    matches!(
        arg,
        "--registry"
            | "--cache"
            | "--userconfig"
            | "--prefix"
            | "--node-options"
            | "--script-shell"
            | "--call"
            | "-c"
            | "--python"
            | "--with"
            | "--with-editable"
            | "--refresh-package"
            | "--index-url"
            | "--extra-index-url"
            | "--find-links"
            | "--project"
            | "--directory"
            | "--config-file"
    )
}

fn docker_image_arg(args: &[String]) -> Option<String> {
    let start_index = usize::from(args.first().map(|value| value == "run").unwrap_or(false));
    let mut iter = args.iter().skip(start_index).peekable();
    while let Some(raw_arg) = iter.next() {
        let arg = raw_arg.trim();
        if arg == "--" {
            return iter.next().cloned();
        }
        if docker_option_takes_value(arg) {
            let _ = iter.next();
            continue;
        }
        if arg.starts_with("--") && arg.contains('=') {
            continue;
        }
        if arg.starts_with('-') {
            continue;
        }
        return Some(raw_arg.clone());
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
        "nuget" => Ok("nuget".to_string()),
        "mcpb" => Ok("mcpb".to_string()),
        "filesystem" | "fs" | "path" | "directory" | "dir" => Ok("filesystem".to_string()),
        "stdio" | "command" | "local" | "local-command" => Ok("stdio".to_string()),
        "http" | "streamable-http" | "streamable_http" | "remote" | "url" => {
            Ok("streamable-http".to_string())
        }
        other => Err(format!(
            "unsupported server install type '{}'; use auto, npm, pypi, oci, filesystem, stdio, streamable-http, or keep nuget/mcpb as plan-only until supported",
            other
        )),
    }
}

fn choose_package_method(spec: &str, explicit_type: Option<&str>) -> String {
    let lower = spec.trim().to_ascii_lowercase();
    let inferred_from_spec = if lower.starts_with("npm:") {
        Some("npm")
    } else if lower.starts_with("pypi:") || lower.starts_with("uvx:") {
        Some("pypi")
    } else if lower.starts_with("oci:") || lower.starts_with("docker:") {
        Some("oci")
    } else if lower.starts_with("nuget:") {
        Some("nuget")
    } else if lower.starts_with("mcpb:") {
        Some("mcpb")
    } else {
        None
    };
    if let Some(kind) = explicit_type {
        if kind == "stdio" {
            return inferred_from_spec.unwrap_or("npm").to_string();
        }
        if !kind.is_empty() {
            return kind.to_string();
        }
    }
    inferred_from_spec.unwrap_or("npm").to_string()
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

fn absolutize_local_path_arg(value: &str, install_root: Option<&Path>) -> String {
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
    if trimmed == "." {
        if let Some(root) = install_root {
            return install_path_string(
                &std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf()),
            );
        }
    }
    let path = PathBuf::from(trimmed);
    let candidate = if path.is_absolute() {
        path
    } else {
        install_root
            .map(Path::to_path_buf)
            .or_else(|| std::env::current_dir().ok())
            .map(|base| base.join(path))
            .unwrap_or_else(|| PathBuf::from(trimmed))
    };
    install_path_string(&std::fs::canonicalize(&candidate).unwrap_or(candidate))
}

fn install_path_string(path: &Path) -> String {
    let value = path.display().to_string();
    if let Some(rest) = value.strip_prefix("\\\\?\\UNC\\") {
        return format!("\\\\{}", rest);
    }
    value.strip_prefix("\\\\?\\").unwrap_or(&value).to_string()
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
    let trimmed = url.trim();
    let lower = trimmed.to_ascii_lowercase();
    let without_scheme = if lower.starts_with("https://") {
        &trimmed["https://".len()..]
    } else if lower.starts_with("http://") {
        &trimmed["http://".len()..]
    } else {
        trimmed
    };
    let mut value = without_scheme.trim_matches('/');
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

#[cfg(test)]
mod tests;
