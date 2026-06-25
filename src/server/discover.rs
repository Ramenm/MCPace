use super::args::ParsedArgs;
use crate::json::{self, JsonValue};
use crate::json_helpers;
use crate::mcp_autoinstall::{self, McpAutoInstallOptions};
use crate::mcp_sources;
use crate::runtimepaths;
use crate::upstream;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

const DEFAULT_APPROVED_CATALOG: &str = "catalog/approved-servers.json";
const DEFAULT_REGISTRY_CACHE: &str = "catalog/registry-cache.json";
const OFFICIAL_REGISTRY_HINT: &str = "https://registry.modelcontextprotocol.io";

#[derive(Clone, Debug, Default)]
struct DiscoveryCandidate {
    name: String,
    normalized_name: String,
    title: String,
    description: String,
    source: String,
    trust_level: String,
    install_spec: String,
    server_type: Option<String>,
    command: Option<String>,
    url: Option<String>,
    paths: Vec<String>,
    extra_args: Vec<String>,
    registry_type: String,
    package: String,
    transport: String,
    recommended_mode: String,
    affinity: Vec<String>,
    installed: bool,
    score: usize,
}

pub(super) fn run(
    parsed: &ParsedArgs,
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let root_path = parsed.root_override.clone().or(default_root);
    let Some(root_path) = root_path else {
        let _ = writeln!(stderr, "mcpace root not found; expected mcpace.config.json");
        return 1;
    };

    let query = parsed.name_filter.as_deref().unwrap_or("");
    let result = match discover_servers(&root_path, query, parsed) {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 1;
        }
    };

    if parsed.json_output {
        let _ = writeln!(stdout, "{}", result.to_pretty_string());
        return 0;
    }

    render_discovery(&result, stdout);
    0
}

fn discover_servers(
    root_path: &Path,
    query: &str,
    parsed: &ParsedArgs,
) -> Result<JsonValue, String> {
    let config = read_optional_json(&root_path.join("mcpace.config.json"));
    let auto_mode = parsed.auto_mode || parsed.action.as_deref() == Some("auto");
    let auto_requested = auto_mode || parsed.auto_install;
    let trust_default = config
        .as_ref()
        .and_then(|value| {
            json_helpers::string_at_path(value, &["approvedCatalog", "trustLevelDefault"])
        })
        .unwrap_or("review");
    let auto_install_policy = config
        .as_ref()
        .and_then(|value| json_helpers::string_at_path(value, &["dynamicDiscovery", "autoInstall"]))
        .unwrap_or("trusted-only");
    let max_auto_installs = max_auto_installs_per_run(config.as_ref());
    let probe_after_install = auto_mode
        || config
            .as_ref()
            .and_then(|value| {
                json_helpers::bool_at_path(value, &["dynamicDiscovery", "probeAfterInstall"])
            })
            .unwrap_or(false);
    let mut warnings = Vec::new();
    let registry_endpoints = registry_endpoints(config.as_ref(), &mut warnings);
    let registry_cache_path = registry_cache_path(root_path, config.as_ref());
    let cache_ttl_hours = registry_cache_ttl_hours(config.as_ref());
    let registry_cache_needs_refresh =
        registry_cache_needs_refresh(&registry_cache_path, cache_ttl_hours);
    let refresh_on_discover = config
        .as_ref()
        .and_then(|value| {
            json_helpers::bool_at_path(value, &["dynamicDiscovery", "refreshRegistryOnDiscover"])
        })
        .unwrap_or(false);
    let auto_refresh_registry = config
        .as_ref()
        .and_then(|value| {
            json_helpers::bool_at_path(value, &["dynamicDiscovery", "autoRefreshRegistry"])
        })
        .unwrap_or(true);
    let mut registry_refresh = JsonValue::Null;
    let effective_refresh = parsed.refresh
        || refresh_on_discover
        || (auto_mode && auto_refresh_registry && registry_cache_needs_refresh);
    if effective_refresh {
        match refresh_registry_cache(root_path, &registry_endpoints, &registry_cache_path) {
            Ok(value) => registry_refresh = value,
            Err(error) => warnings.push(format!("registry refresh failed: {}", error)),
        }
    }
    let catalog_paths = catalog_paths(root_path, config.as_ref(), &registry_cache_path);
    let installed = installed_server_names(root_path);
    let terms = search_terms(query);
    let mut candidates = Vec::new();

    for path in &catalog_paths {
        match json_helpers::read_json_file(path) {
            Ok(value) => collect_candidates_from_catalog(
                &value,
                &path.display().to_string(),
                trust_default,
                &installed,
                &terms,
                &mut candidates,
            ),
            Err(error) => warnings.push(format!(
                "failed to read discovery catalog '{}': {}",
                path.display(),
                error
            )),
        }
    }

    let duplicate_candidate_count = deduplicate_discovery_candidates(&mut candidates);
    if duplicate_candidate_count > 0 {
        warnings.push(format!(
            "deduplicated {} duplicate MCP discovery candidates by normalized name using trust/source precedence",
            duplicate_candidate_count
        ));
    }

    candidates.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.normalized_name.cmp(&right.normalized_name))
    });
    candidates.dedup_by(|left, right| left.normalized_name == right.normalized_name);
    if !terms.is_empty() {
        candidates.retain(|candidate| candidate.score > 0);
    }
    let limit = if terms.is_empty() { 50 } else { 20 };
    candidates.truncate(limit);

    let selected_index = selected_install_candidate(&candidates, query);
    let mut install_result = JsonValue::Null;
    let mut install_decision = "plan-only".to_string();
    let mut install_block_reason = String::new();
    let mut automatic_install_results = Vec::new();
    let mut post_install_probe_results = Vec::new();

    if auto_requested {
        if query.trim().is_empty() {
            let sweep = automatic_install_sweep(
                root_path,
                &candidates,
                parsed,
                auto_install_policy,
                max_auto_installs,
                probe_after_install,
            );
            install_decision = sweep.decision;
            install_block_reason = sweep.block_reason;
            automatic_install_results = sweep.install_results;
            post_install_probe_results = sweep.probe_results;
        } else {
            match selected_index {
                Some(index) => {
                    let candidate = &candidates[index];
                    if candidate.installed && !parsed.force {
                        install_decision = "already-installed".to_string();
                        install_block_reason = format!(
                            "server '{}' is already configured; pass --force to overwrite",
                            candidate.name
                        );
                    } else if install_allowed(
                        candidate,
                        auto_install_policy,
                        parsed.allow_review_install,
                    ) {
                        let options = install_options_from_candidate(candidate, parsed);
                        match mcp_autoinstall::install_auto(root_path, options) {
                            Ok(result) => {
                                let normalized_name = result.write.normalized_name.clone();
                                install_decision = if parsed.dry_run {
                                    "dry-run-install-plan".to_string()
                                } else {
                                    "installed".to_string()
                                };
                                install_result = result.to_json_value();
                                if should_probe_after_install(probe_after_install, parsed) {
                                    post_install_probe_results.push(probe_installed_server(
                                        root_path,
                                        &normalized_name,
                                        parsed.timeout_ms,
                                    ));
                                }
                            }
                            Err(error) => {
                                install_decision = "install-failed".to_string();
                                install_block_reason = error;
                            }
                        }
                    } else {
                        install_decision = "blocked-by-trust-policy".to_string();
                        install_block_reason = format!(
                            "candidate '{}' has trustLevel='{}'; auto-install allows {}{}",
                            candidate.name,
                            candidate.trust_level,
                            auto_install_policy,
                            if parsed.allow_review_install {
                                " plus review"
                            } else {
                                ""
                            }
                        );
                    }
                }
                None => {
                    install_decision = "ambiguous-query".to_string();
                    install_block_reason = "auto-install with a query needs one exact or unique best catalog match; omit the query for a safe automatic curated sweep, or make the query more specific".to_string();
                }
            }
        }
    }

    Ok(JsonValue::object([
        ("ok", JsonValue::bool(true)),
        ("mode", JsonValue::string("dynamic-server-discovery")),
        (
            "summary",
            JsonValue::string("One auto mode can refresh registry metadata when cache is missing or stale, search approved/catalog snapshots, install only trusted or explicitly allowed review candidates, and immediately probe live MCP tools/list evidence."),
        ),
        ("query", JsonValue::string(query.trim())),
        ("candidateCount", JsonValue::number(candidates.len())),
        ("installedServerCount", JsonValue::number(installed.len())),
        ("autoMode", JsonValue::bool(auto_mode)),
        ("autoInstallRequested", JsonValue::bool(auto_requested)),
        ("autoInstallPolicy", JsonValue::string(auto_install_policy)),
        ("maxAutoInstallsPerRun", JsonValue::number(max_auto_installs)),
        ("probeAfterInstall", JsonValue::bool(probe_after_install)),
        ("registryCachePath", JsonValue::string(registry_cache_path.display().to_string())),
        ("registryCacheTtlHours", JsonValue::number(cache_ttl_hours as usize)),
        ("registryCacheNeedsRefresh", JsonValue::bool(registry_cache_needs_refresh)),
        ("registryRefresh", registry_refresh),
        ("installDecision", JsonValue::string(install_decision)),
        (
            "installBlockReason",
            if install_block_reason.is_empty() {
                JsonValue::Null
            } else {
                JsonValue::string(install_block_reason)
            },
        ),
        ("installResult", install_result),
        (
            "automaticInstallResults",
            JsonValue::array(automatic_install_results),
        ),
        (
            "postInstallProbeResults",
            JsonValue::array(post_install_probe_results),
        ),
        (
            "registryEndpoints",
            JsonValue::array(registry_endpoints.into_iter().map(JsonValue::string)),
        ),
        (
            "catalogPaths",
            JsonValue::array(
                catalog_paths
                    .iter()
                    .map(|path| JsonValue::string(path.display().to_string())),
            ),
        ),
        (
            "candidates",
            JsonValue::array(candidates.iter().map(candidate_json)),
        ),
        ("warnings", JsonValue::array(warnings.into_iter().map(JsonValue::string))),
        (
            "safety",
            JsonValue::object([
                (
                    "unknownInstallPolicy",
                    JsonValue::string("unknown or blocked candidates are never executed silently; they stay plan-only until catalog trust or an explicit install command is supplied"),
                ),
                (
                    "automaticSweep",
                    JsonValue::string("mcpace server auto performs the user-facing automatic path: refresh cache when needed, install up to maxAutoInstallsPerRun trusted/approved candidates, probe live tools/list, and leave unknown/review-only registry entries as plan-only"),
                ),
                (
                    "nextDynamicStep",
                    JsonValue::string("after install, MCPace can probe trusted servers automatically when dynamicDiscovery.probeAfterInstall=true; otherwise run mcpace server test <name> --refresh"),
                ),
            ]),
        ),
    ]))
}

fn read_optional_json(path: &Path) -> Option<JsonValue> {
    json_helpers::read_json_file(path).ok()
}

fn max_auto_installs_per_run(config: Option<&JsonValue>) -> usize {
    config
        .and_then(|value| {
            json_helpers::value_at_path(value, &["dynamicDiscovery", "maxAutoInstallsPerRun"])
        })
        .and_then(JsonValue::as_i64)
        .map(|value| value.max(0) as usize)
        .unwrap_or(4)
}

fn registry_endpoints(config: Option<&JsonValue>, warnings: &mut Vec<String>) -> Vec<String> {
    let raw_endpoints = json_helpers::strings_from_array(config.and_then(|value| {
        json_helpers::array_at_path(value, &["dynamicDiscovery", "registryEndpoints"])
    }));
    let mut endpoints = Vec::new();
    for endpoint in raw_endpoints {
        match normalize_registry_endpoint(&endpoint) {
            Some(normalized) => endpoints.push(normalized),
            None => warnings.push(format!(
                "ignored unsafe MCP registry endpoint '{}'; registry refresh endpoints must be https:// URLs without credentials, fragments, whitespace, or control characters",
                endpoint
            )),
        }
    }
    if endpoints.is_empty() {
        endpoints.push(OFFICIAL_REGISTRY_HINT.to_string());
    }
    endpoints.sort();
    endpoints.dedup();
    endpoints
}

fn normalize_registry_endpoint(value: &str) -> Option<String> {
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.is_empty() || trimmed.contains('#') {
        return None;
    }
    if trimmed
        .chars()
        .any(|ch| ch.is_control() || ch.is_whitespace())
    {
        return None;
    }
    if !trimmed.to_ascii_lowercase().starts_with("https://") {
        return None;
    }
    let rest = &trimmed["https://".len()..];
    let authority = rest.split(['/', '?']).next().unwrap_or("");
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    Some(trimmed.to_string())
}

fn registry_cache_path(root_path: &Path, config: Option<&JsonValue>) -> PathBuf {
    let configured = config
        .and_then(|value| {
            json_helpers::string_at_path(value, &["dynamicDiscovery", "registryCachePath"])
        })
        .unwrap_or(DEFAULT_REGISTRY_CACHE)
        .trim();
    resolve_under_root(root_path, configured)
}

fn registry_cache_ttl_hours(config: Option<&JsonValue>) -> u64 {
    config
        .and_then(|value| {
            json_helpers::value_at_path(value, &["dynamicDiscovery", "registryCacheTtlHours"])
        })
        .and_then(JsonValue::as_i64)
        .filter(|value| *value >= 0)
        .map(|value| value as u64)
        .unwrap_or(24)
}

fn registry_cache_needs_refresh(path: &Path, ttl_hours: u64) -> bool {
    if ttl_hours == 0 || !path.is_file() {
        return true;
    }
    let Ok(metadata) = fs::metadata(path) else {
        return true;
    };
    let Ok(modified) = metadata.modified() else {
        return true;
    };
    let Ok(age) = SystemTime::now().duration_since(modified) else {
        return false;
    };
    age > Duration::from_secs(ttl_hours.saturating_mul(60 * 60))
}

fn catalog_paths(
    root_path: &Path,
    config: Option<&JsonValue>,
    registry_cache: &Path,
) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    paths.push(root_path.join(DEFAULT_APPROVED_CATALOG));
    if registry_cache_is_regular_file(registry_cache) {
        paths.push(registry_cache.to_path_buf());
    }
    for value in json_helpers::strings_from_array(
        config.and_then(|value| json_helpers::array_at_path(value, &["approvedCatalog", "paths"])),
    ) {
        paths.push(resolve_under_root(root_path, &value));
    }
    for value in json_helpers::strings_from_array(config.and_then(|value| {
        json_helpers::array_at_path(value, &["dynamicDiscovery", "catalogPaths"])
    })) {
        paths.push(resolve_under_root(root_path, &value));
    }
    let mut seen = BTreeSet::new();
    paths
        .into_iter()
        .filter(|path| seen.insert(path.display().to_string()))
        .collect()
}

fn registry_cache_is_regular_file(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
        .unwrap_or(false)
}

fn resolve_under_root(root_path: &Path, value: &str) -> PathBuf {
    let path = PathBuf::from(value.trim());
    if path.is_absolute() {
        path
    } else {
        root_path.join(path)
    }
}

fn refresh_registry_cache(
    root_path: &Path,
    endpoints: &[String],
    cache_path: &Path,
) -> Result<JsonValue, String> {
    let endpoint = endpoints
        .first()
        .map(String::as_str)
        .unwrap_or(OFFICIAL_REGISTRY_HINT);
    let cache_path = if cache_path.is_absolute() {
        cache_path.to_path_buf()
    } else {
        root_path.join(cache_path)
    };
    let _cache_lock =
        runtimepaths::acquire_exclusive_file_lock(&cache_path, "MCP registry cache refresh")?;
    let mut servers = Vec::new();
    let mut next_cursor: Option<String> = None;
    let mut pages = 0usize;
    let mut last_url = String::new();

    for _ in 0..5 {
        let url = registry_list_url_with_cursor(endpoint, next_cursor.as_deref());
        last_url = url.clone();
        let body = fetch_url(&url)?;
        let value = json::parse_str(&body)
            .map_err(|error| format!("invalid registry JSON from {}: {}", url, error))?;
        let Some(page_servers) = json_helpers::array_at_path(&value, &["servers"]) else {
            return Err(format!("{} did not return a servers array", url));
        };
        for server in page_servers {
            servers.push(server.clone());
        }
        pages += 1;
        next_cursor = json_helpers::string_at_path(&value, &["metadata", "nextCursor"])
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        if next_cursor.is_none() {
            break;
        }
    }

    let server_count = servers.len();
    if server_count == 0 {
        return Err(format!("{} returned no registry servers", last_url));
    }
    let cache_value = JsonValue::object([
        ("servers", JsonValue::array(servers)),
        (
            "metadata",
            JsonValue::object([
                ("count", JsonValue::number(server_count)),
                ("pages", JsonValue::number(pages)),
                ("sourceEndpoint", JsonValue::string(endpoint.to_string())),
                (
                    "nextCursor",
                    next_cursor
                        .clone()
                        .map(JsonValue::string)
                        .unwrap_or(JsonValue::Null),
                ),
            ]),
        ),
    ]);
    let mut serialized = cache_value.to_pretty_string();
    serialized.push('\n');
    runtimepaths::write_private_text_atomic(&cache_path, &serialized).map_err(|error| {
        format!(
            "failed to write registry cache '{}': {}",
            cache_path.display(),
            error
        )
    })?;
    Ok(JsonValue::object([
        ("ok", JsonValue::bool(true)),
        ("url", JsonValue::string(last_url)),
        (
            "cachePath",
            JsonValue::string(cache_path.display().to_string()),
        ),
        ("serverCount", JsonValue::number(server_count)),
        ("pageCount", JsonValue::number(pages)),
        (
            "nextCursor",
            next_cursor
                .map(JsonValue::string)
                .unwrap_or(JsonValue::Null),
        ),
    ]))
}

fn registry_list_url_with_cursor(endpoint: &str, cursor: Option<&str>) -> String {
    let trimmed = endpoint.trim().trim_end_matches('/');
    let mut url = if trimmed.contains("/v0.1/servers") {
        if trimmed.contains('?') {
            trimmed.to_string()
        } else {
            format!("{}?limit=100", trimmed)
        }
    } else {
        format!("{}/v0.1/servers?limit=100", trimmed)
    };
    if let Some(cursor) = cursor.map(str::trim).filter(|value| !value.is_empty()) {
        url.push(if url.contains('?') { '&' } else { '?' });
        url.push_str("cursor=");
        url.push_str(&url_query_escape(cursor));
    }
    url
}

fn url_query_escape(value: &str) -> String {
    let mut output = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            output.push(byte as char);
        } else {
            output.push_str(&format!("%{:02X}", byte));
        }
    }
    output
}

fn fetch_url(url: &str) -> Result<String, String> {
    let mut errors = Vec::new();
    for curl_path in trusted_fetch_program_candidates("curl") {
        let mut command = Command::new(&curl_path);
        command.args(["-fsSL", "--max-time", "15", "--", url]);
        configure_fetch_env(&mut command);
        match command.output() {
            Ok(output) if output.status.success() => {
                return String::from_utf8(output.stdout).map_err(|error| {
                    format!("{} output was not UTF-8: {}", curl_path.display(), error)
                });
            }
            Ok(output) => errors.push(format!(
                "{} exited with {}: {}",
                curl_path.display(),
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            )),
            Err(error) => errors.push(format!("{} unavailable: {}", curl_path.display(), error)),
        }
    }

    for powershell_path in trusted_fetch_program_candidates("powershell") {
        let script = "param([string]$Uri); (Invoke-WebRequest -UseBasicParsing -TimeoutSec 15 -Uri $Uri).Content";
        let mut command = Command::new(&powershell_path);
        command.args(["-NoProfile", "-NonInteractive", "-Command", script, url]);
        configure_fetch_env(&mut command);
        match command.output() {
            Ok(output) if output.status.success() => {
                return String::from_utf8(output.stdout).map_err(|error| {
                    format!(
                        "{} output was not UTF-8: {}",
                        powershell_path.display(),
                        error
                    )
                });
            }
            Ok(output) => errors.push(format!(
                "{} exited with {}: {}",
                powershell_path.display(),
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            )),
            Err(error) => errors.push(format!(
                "{} unavailable: {}",
                powershell_path.display(),
                error
            )),
        }
    }

    Err(format!("failed to fetch {} ({})", url, errors.join("; ")))
}

fn trusted_fetch_program_candidates(kind: &str) -> Vec<PathBuf> {
    let raw_candidates: &[&str] = match kind {
        "curl" => {
            #[cfg(windows)]
            {
                &[r"C:\Windows\System32\curl.exe"]
            }
            #[cfg(not(windows))]
            {
                &["/usr/bin/curl", "/bin/curl", "/usr/local/bin/curl"]
            }
        }
        "powershell" => {
            #[cfg(windows)]
            {
                &[
                    r"C:\Program Files\PowerShell\7\pwsh.exe",
                    r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe",
                ]
            }
            #[cfg(not(windows))]
            {
                &["/usr/bin/pwsh", "/usr/local/bin/pwsh"]
            }
        }
        _ => &[],
    };
    raw_candidates
        .iter()
        .map(PathBuf::from)
        .filter(|path| trusted_fetch_program_path(path))
        .collect()
}

fn trusted_fetch_program_path(path: &Path) -> bool {
    path.is_absolute()
        && fs::symlink_metadata(path)
            .map(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
            .unwrap_or(false)
}

fn configure_fetch_env(command: &mut Command) {
    command.env_clear();
    for key in [
        "SystemRoot",
        "WINDIR",
        "HOME",
        "HTTPS_PROXY",
        "https_proxy",
        "NO_PROXY",
        "no_proxy",
        "SSL_CERT_FILE",
        "SSL_CERT_DIR",
    ] {
        if let Ok(value) = env::var(key) {
            if !value
                .chars()
                .any(|ch| ch == '\0' || ch == '\r' || ch == '\n')
            {
                command.env(key, value);
            }
        }
    }
}

fn installed_server_names(root_path: &Path) -> BTreeSet<String> {
    mcp_sources::load_mcp_server_registry(root_path)
        .map(|registry| registry.servers.keys().cloned().collect())
        .unwrap_or_default()
}

fn collect_candidates_from_catalog(
    value: &JsonValue,
    source: &str,
    trust_default: &str,
    installed: &BTreeSet<String>,
    terms: &[String],
    candidates: &mut Vec<DiscoveryCandidate>,
) {
    if let Some(servers) = json_helpers::object_at_path(value, &["servers"]) {
        for (name, raw) in servers {
            if let Some(mut candidate) =
                candidate_from_record(Some(name), raw, source, trust_default)
            {
                candidate.installed = installed.contains(&candidate.normalized_name);
                candidate.score = candidate_score(&candidate, terms);
                candidates.push(candidate);
            }
        }
    }
    if let Some(servers) = json_helpers::array_at_path(value, &["servers"]) {
        for raw in servers {
            if let Some(mut candidate) = candidate_from_record(None, raw, source, trust_default) {
                candidate.installed = installed.contains(&candidate.normalized_name);
                candidate.score = candidate_score(&candidate, terms);
                candidates.push(candidate);
            }
        }
    }
}

fn candidate_from_record(
    name_hint: Option<&str>,
    raw: &JsonValue,
    source: &str,
    trust_default: &str,
) -> Option<DiscoveryCandidate> {
    let name = json_helpers::string_at_path(raw, &["name"])
        .or(name_hint)
        .unwrap_or("")
        .trim();
    let title = json_helpers::string_at_path(raw, &["title"])
        .or_else(|| json_helpers::string_at_path(raw, &["displayName"]))
        .unwrap_or(name)
        .trim();
    let description = json_helpers::string_at_path(raw, &["description"])
        .or_else(|| json_helpers::string_at_path(raw, &["notes"]))
        .unwrap_or("")
        .trim();
    let trust_level = json_helpers::string_at_path(raw, &["trustLevel"])
        .unwrap_or(trust_default)
        .trim()
        .to_ascii_lowercase();

    let mut candidate = DiscoveryCandidate {
        name: if name.is_empty() {
            title.to_string()
        } else {
            name.to_string()
        },
        normalized_name: String::new(),
        title: title.to_string(),
        description: description.to_string(),
        source: source.to_string(),
        trust_level: if trust_level.is_empty() {
            "review".to_string()
        } else {
            trust_level
        },
        install_spec: json_helpers::string_at_path(raw, &["installSpec"])
            .or_else(|| json_helpers::string_at_path(raw, &["spec"]))
            .unwrap_or("")
            .trim()
            .to_string(),
        server_type: json_helpers::string_at_path(raw, &["type"])
            .map(|value| value.trim().to_string()),
        command: json_helpers::string_at_path(raw, &["command"])
            .map(|value| value.trim().to_string()),
        url: json_helpers::string_at_path(raw, &["url"]).map(|value| value.trim().to_string()),
        paths: json_helpers::strings_from_array(json_helpers::array_at_path(raw, &["paths"])),
        extra_args: json_helpers::strings_from_array(json_helpers::array_at_path(raw, &["args"])),
        registry_type: String::new(),
        package: json_helpers::string_at_path(raw, &["package"])
            .unwrap_or("")
            .trim()
            .to_string(),
        transport: String::new(),
        recommended_mode: json_helpers::string_at_path(raw, &["recommendedMode"])
            .unwrap_or("")
            .trim()
            .to_string(),
        affinity: json_helpers::strings_from_array(json_helpers::array_at_path(raw, &["affinity"])),
        installed: false,
        score: 0,
    };

    if candidate.install_spec.is_empty() && !candidate.package.is_empty() {
        candidate.install_spec = candidate.package.clone();
    }

    if let Some(package) = first_registry_package(raw) {
        candidate.registry_type = json_helpers::string_at_path(package, &["registryType"])
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        candidate.package = json_helpers::string_at_path(package, &["identifier"])
            .unwrap_or("")
            .trim()
            .to_string();
        candidate.transport = json_helpers::string_at_path(package, &["transport", "type"])
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        if candidate.server_type.is_none() && !candidate.transport.is_empty() {
            // Registry package transport is execution transport, not the package manager.
            // Keep it as a server type hint only after installSpec has been prefixed by
            // the package registry type, so PyPI/OCI/NuGet/MCPB records are not
            // accidentally treated as generic npm stdio packages.
            candidate.server_type = Some(candidate.transport.clone());
        }
        if candidate.install_spec.is_empty() {
            candidate.install_spec = install_spec_for_registry_package(
                &candidate.registry_type,
                &candidate.package,
                &candidate.transport,
            );
        }
    }

    if candidate.name.trim().is_empty() && !candidate.package.is_empty() {
        candidate.name = candidate.package.clone();
    }
    candidate.normalized_name = mcp_sources::normalize_server_name(&candidate.name);
    if candidate.normalized_name.is_empty() {
        return None;
    }

    if candidate.install_spec.is_empty()
        && candidate.command.as_deref().unwrap_or("").trim().is_empty()
        && candidate.url.as_deref().unwrap_or("").trim().is_empty()
    {
        return None;
    }
    Some(candidate)
}

fn first_registry_package(raw: &JsonValue) -> Option<&JsonValue> {
    let packages = json_helpers::array_at_path(raw, &["packages"])?;
    packages
        .iter()
        .find(|package| {
            let transport = json_helpers::string_at_path(package, &["transport", "type"])
                .unwrap_or("")
                .to_ascii_lowercase();
            transport == "stdio" || transport == "streamable-http" || transport == "http"
        })
        .or_else(|| packages.first())
}

fn install_spec_for_registry_package(
    registry_type: &str,
    identifier: &str,
    transport: &str,
) -> String {
    let identifier = identifier.trim();
    if identifier.is_empty() {
        return String::new();
    }
    match registry_type {
        "npm" => format!("npm:{}", identifier),
        "pypi" | "python" => format!("pypi:{}", identifier),
        "oci" | "docker" | "container" => format!("oci:{}", identifier),
        "nuget" => format!("nuget:{}", identifier),
        "mcpb" => format!("mcpb:{}", identifier),
        _ if transport == "streamable-http" || transport == "http" => identifier.to_string(),
        _ => identifier.to_string(),
    }
}

fn search_terms(query: &str) -> Vec<String> {
    query
        .trim()
        .to_ascii_lowercase()
        .split(|ch: char| {
            !ch.is_ascii_alphanumeric() && ch != '-' && ch != '_' && ch != '/' && ch != '@'
        })
        .filter(|item| !item.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn candidate_score(candidate: &DiscoveryCandidate, terms: &[String]) -> usize {
    if terms.is_empty() {
        return 1;
    }
    let name = format!(
        "{} {} {} {}",
        candidate.normalized_name, candidate.name, candidate.title, candidate.package
    )
    .to_ascii_lowercase();
    let description = candidate.description.to_ascii_lowercase();
    let mut score = 0usize;
    for term in terms {
        if name.contains(term) {
            score += 8;
        }
        if description.contains(term) {
            score += 3;
        }
    }
    score
}

fn deduplicate_discovery_candidates(candidates: &mut Vec<DiscoveryCandidate>) -> usize {
    let original_len = candidates.len();
    let mut by_name = BTreeMap::<String, DiscoveryCandidate>::new();
    for candidate in std::mem::take(candidates) {
        match by_name.get_mut(&candidate.normalized_name) {
            Some(existing) if candidate_precedence(&candidate) > candidate_precedence(existing) => {
                *existing = candidate;
            }
            Some(_) => {}
            None => {
                by_name.insert(candidate.normalized_name.clone(), candidate);
            }
        }
    }
    candidates.extend(by_name.into_values());
    original_len.saturating_sub(candidates.len())
}

fn candidate_precedence(candidate: &DiscoveryCandidate) -> (usize, usize, usize, usize) {
    (
        usize::from(candidate.installed),
        candidate_trust_rank(&candidate.trust_level),
        candidate_source_rank(&candidate.source),
        candidate.score,
    )
}

fn candidate_trust_rank(value: &str) -> usize {
    match value.trim().to_ascii_lowercase().as_str() {
        "trusted" | "approved" | "curated" => 5,
        "review" | "manual-review" => 3,
        "unknown" | "unreviewed" => 1,
        "blocked" | "deny" | "malware" => 0,
        _ => 2,
    }
}

fn candidate_source_rank(source: &str) -> usize {
    let normalized = source.replace('\\', "/").to_ascii_lowercase();
    if normalized.ends_with(DEFAULT_APPROVED_CATALOG) {
        5
    } else if normalized.contains("registry-cache") {
        2
    } else {
        3
    }
}

fn selected_install_candidate(candidates: &[DiscoveryCandidate], query: &str) -> Option<usize> {
    if candidates.is_empty() || query.trim().is_empty() {
        return None;
    }
    let normalized_query = mcp_sources::normalize_server_name(query);
    if let Some((index, _)) = candidates
        .iter()
        .enumerate()
        .find(|(_, candidate)| candidate.normalized_name == normalized_query)
    {
        return Some(index);
    }
    let best = candidates.first()?.score;
    if best == 0 {
        return None;
    }
    let same_score = candidates
        .iter()
        .filter(|candidate| candidate.score == best)
        .count();
    if same_score == 1 {
        Some(0)
    } else {
        None
    }
}

#[derive(Debug, Default)]
struct AutomaticInstallSweep {
    decision: String,
    block_reason: String,
    install_results: Vec<JsonValue>,
    probe_results: Vec<JsonValue>,
}

fn automatic_install_sweep(
    root_path: &Path,
    candidates: &[DiscoveryCandidate],
    parsed: &ParsedArgs,
    auto_install_policy: &str,
    max_installs: usize,
    probe_after_install: bool,
) -> AutomaticInstallSweep {
    let mut sweep = AutomaticInstallSweep {
        decision: if parsed.dry_run {
            "dry-run-curated-sweep".to_string()
        } else {
            "curated-sweep".to_string()
        },
        block_reason: String::new(),
        install_results: Vec::new(),
        probe_results: Vec::new(),
    };

    if max_installs == 0 {
        sweep.decision = "auto-sweep-disabled".to_string();
        sweep.block_reason = "dynamicDiscovery.maxAutoInstallsPerRun is 0".to_string();
        return sweep;
    }

    let mut installed_count = 0usize;
    let mut skipped_count = 0usize;
    for candidate in candidates {
        if installed_count >= max_installs {
            break;
        }
        if candidate.installed && !parsed.force {
            skipped_count += 1;
            sweep.install_results.push(JsonValue::object([
                ("name", JsonValue::string(candidate.name.clone())),
                ("decision", JsonValue::string("skip-already-installed")),
                (
                    "reason",
                    JsonValue::string("server is already configured; pass --force to replace"),
                ),
            ]));
            continue;
        }
        if !install_allowed(candidate, auto_install_policy, parsed.allow_review_install) {
            skipped_count += 1;
            sweep.install_results.push(JsonValue::object([
                ("name", JsonValue::string(candidate.name.clone())),
                ("decision", JsonValue::string("skip-trust-policy")),
                ("trustLevel", JsonValue::string(candidate.trust_level.clone())),
                ("reason", JsonValue::string("automatic no-query sweeps install only trusted/approved candidates unless review is explicitly allowed")),
            ]));
            continue;
        }

        match mcp_autoinstall::install_auto(
            root_path,
            install_options_from_candidate(candidate, parsed),
        ) {
            Ok(result) => {
                let normalized_name = result.write.normalized_name.clone();
                installed_count += 1;
                sweep.install_results.push(JsonValue::object([
                    ("name", JsonValue::string(candidate.name.clone())),
                    (
                        "decision",
                        JsonValue::string(if parsed.dry_run {
                            "dry-run-install"
                        } else {
                            "installed"
                        }),
                    ),
                    (
                        "trustLevel",
                        JsonValue::string(candidate.trust_level.clone()),
                    ),
                    ("result", result.to_json_value()),
                ]));
                if should_probe_after_install(probe_after_install, parsed) {
                    sweep.probe_results.push(probe_installed_server(
                        root_path,
                        &normalized_name,
                        parsed.timeout_ms,
                    ));
                }
            }
            Err(error) => {
                skipped_count += 1;
                sweep.install_results.push(JsonValue::object([
                    ("name", JsonValue::string(candidate.name.clone())),
                    ("decision", JsonValue::string("install-failed")),
                    (
                        "trustLevel",
                        JsonValue::string(candidate.trust_level.clone()),
                    ),
                    ("error", JsonValue::string(error)),
                ]));
            }
        }
    }

    if installed_count == 0 {
        sweep.decision = if skipped_count == 0 {
            "no-candidates".to_string()
        } else {
            "no-installable-candidates".to_string()
        };
        sweep.block_reason = "no trusted/approved candidates were eligible for automatic install; review/unknown registry entries were left as plan-only".to_string();
    } else {
        sweep.block_reason = format!(
            "installed {} trusted/approved candidate(s) automatically; skipped {} candidate(s); limit {}",
            installed_count, skipped_count, max_installs
        );
    }
    sweep
}

fn should_probe_after_install(probe_after_install: bool, parsed: &ParsedArgs) -> bool {
    probe_after_install && !parsed.dry_run && !parsed.disabled
}

fn probe_installed_server(root_path: &Path, name: &str, timeout_ms: Option<u64>) -> JsonValue {
    match upstream::probe_servers(root_path, Some(name), timeout_ms, true) {
        Ok(value) => JsonValue::object([
            ("name", JsonValue::string(name.to_string())),
            ("ok", JsonValue::bool(true)),
            ("result", value),
        ]),
        Err(error) => JsonValue::object([
            ("name", JsonValue::string(name.to_string())),
            ("ok", JsonValue::bool(false)),
            ("error", JsonValue::string(error)),
        ]),
    }
}

fn install_allowed(candidate: &DiscoveryCandidate, policy: &str, allow_review: bool) -> bool {
    let trust = candidate.trust_level.as_str();
    if trust == "blocked" || trust == "deny" || trust == "malware" {
        return false;
    }
    if trust == "trusted" || trust == "approved" {
        return true;
    }
    if trust == "review" || trust == "reviewed" {
        return allow_review || policy == "review";
    }
    false
}

fn install_options_from_candidate(
    candidate: &DiscoveryCandidate,
    parsed: &ParsedArgs,
) -> McpAutoInstallOptions {
    McpAutoInstallOptions {
        spec: candidate.install_spec.clone(),
        name_override: Some(candidate.name.clone()),
        server_type: candidate.server_type.clone(),
        command: candidate.command.clone(),
        url: candidate.url.clone(),
        paths: candidate.paths.clone(),
        extra_args: candidate.extra_args.clone(),
        env: parsed.env.clone(),
        headers: parsed.headers.clone(),
        settings_path: parsed.settings_path.clone(),
        dry_run: parsed.dry_run,
        force: parsed.force,
        disabled: parsed.disabled,
        profile_hints: profile_hints_from_candidate(candidate),
    }
}

fn profile_hints_from_candidate(candidate: &DiscoveryCandidate) -> Vec<String> {
    let mut hints = Vec::new();

    // Keep identity fields out of semantic profile hints. Package/server names are useful
    // for installation and UI, but they are intentionally weak evidence for runtime policy:
    // random npm packages often contain misleading words such as browser, process, db, or
    // context in the name. MCPace stores only indirect evidence here and waits for
    // initialize/tools-list probes before relaxing concurrency for low-confidence servers.
    for value in [
        candidate.description.as_str(),
        candidate.registry_type.as_str(),
        candidate.transport.as_str(),
        candidate.recommended_mode.as_str(),
    ] {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            hints.push(trimmed.to_string());
        }
    }
    hints.extend(
        candidate
            .affinity
            .iter()
            .map(|value| format!("affinity:{}", value.trim())),
    );
    if !candidate.registry_type.trim().is_empty() {
        hints.push(format!("registry-type:{}", candidate.registry_type.trim()));
    }
    if !candidate.transport.trim().is_empty() {
        hints.push(format!("transport:{}", candidate.transport.trim()));
    }
    if !candidate.recommended_mode.trim().is_empty() {
        hints.push(format!(
            "recommended-mode:{}",
            candidate.recommended_mode.trim()
        ));
    }
    hints.sort();
    hints.dedup();
    hints
}

fn candidate_json(candidate: &DiscoveryCandidate) -> JsonValue {
    let plan = mcp_autoinstall::plan_auto_install(&install_options_from_candidate(
        candidate,
        &ParsedArgs {
            dry_run: true,
            ..ParsedArgs::default()
        },
    ))
    .map(|plan| plan.to_json_value())
    .unwrap_or(JsonValue::Null);
    JsonValue::object([
        ("name", JsonValue::string(candidate.name.clone())),
        (
            "normalizedName",
            JsonValue::string(candidate.normalized_name.clone()),
        ),
        ("title", JsonValue::string(candidate.title.clone())),
        (
            "description",
            JsonValue::string(candidate.description.clone()),
        ),
        ("source", JsonValue::string(candidate.source.clone())),
        (
            "trustLevel",
            JsonValue::string(candidate.trust_level.clone()),
        ),
        ("installed", JsonValue::bool(candidate.installed)),
        ("score", JsonValue::number(candidate.score)),
        (
            "installSpec",
            JsonValue::string(candidate.install_spec.clone()),
        ),
        (
            "registryType",
            JsonValue::string(candidate.registry_type.clone()),
        ),
        ("package", JsonValue::string(candidate.package.clone())),
        ("transport", JsonValue::string(candidate.transport.clone())),
        (
            "recommendedMode",
            JsonValue::string(candidate.recommended_mode.clone()),
        ),
        (
            "affinity",
            JsonValue::array(candidate.affinity.iter().cloned().map(JsonValue::string)),
        ),
        (
            "type",
            candidate
                .server_type
                .clone()
                .map(JsonValue::string)
                .unwrap_or(JsonValue::Null),
        ),
        (
            "command",
            candidate
                .command
                .clone()
                .map(JsonValue::string)
                .unwrap_or(JsonValue::Null),
        ),
        (
            "url",
            candidate
                .url
                .clone()
                .map(JsonValue::string)
                .unwrap_or(JsonValue::Null),
        ),
        (
            "paths",
            JsonValue::array(candidate.paths.iter().cloned().map(JsonValue::string)),
        ),
        (
            "args",
            JsonValue::array(candidate.extra_args.iter().cloned().map(JsonValue::string)),
        ),
        (
            "profileHints",
            JsonValue::array(
                profile_hints_from_candidate(candidate)
                    .into_iter()
                    .map(JsonValue::string),
            ),
        ),
        ("installPlan", plan),
    ])
}

fn render_discovery(result: &JsonValue, stdout: &mut dyn Write) {
    let count = json_helpers::value_at_path(result, &["candidateCount"])
        .and_then(JsonValue::as_i64)
        .unwrap_or(0);
    let decision =
        json_helpers::string_at_path(result, &["installDecision"]).unwrap_or("plan-only");
    let _ = writeln!(
        stdout,
        "Dynamic MCP server discovery: {} candidate(s), installDecision={}",
        count, decision
    );
    if let Some(reason) = json_helpers::string_at_path(result, &["installBlockReason"]) {
        let _ = writeln!(stdout, "  note: {}", reason);
    }
    if let Some(items) = json_helpers::array_at_path(result, &["automaticInstallResults"]) {
        if !items.is_empty() {
            let _ = writeln!(
                stdout,
                "  automatic install sweep: {} result(s)",
                items.len()
            );
        }
    }
    for candidate in json_helpers::array_at_path(result, &["candidates"]).unwrap_or(&[]) {
        let name = json_helpers::string_at_path(candidate, &["name"]).unwrap_or("unknown");
        let trust = json_helpers::string_at_path(candidate, &["trustLevel"]).unwrap_or("review");
        let installed = json_helpers::bool_at_path(candidate, &["installed"]).unwrap_or(false);
        let spec = json_helpers::string_at_path(candidate, &["installSpec"]).unwrap_or("");
        let _ = writeln!(
            stdout,
            "- {} trust={} installed={} spec={}",
            name,
            trust,
            if installed { "yes" } else { "no" },
            if spec.is_empty() { "none" } else { spec }
        );
    }
    let _ = writeln!(
        stdout,
        "Next: use `mcpace server auto` or top-level `mcpace auto` for one-command setup/probe; advanced flags remain for debugging."
    );
}
