use super::args::ParsedArgs;
use crate::diagnostics;
use crate::json::{self, JsonValue};
use crate::json_helpers;
use crate::mcp_autoinstall::{self, McpAutoInstallOptions};
use crate::mcp_sources;
use crate::runtimepaths;
use crate::upstream;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const DEFAULT_APPROVED_CATALOG: &str = "catalog/approved-servers.json";
const DEFAULT_REGISTRY_CACHE: &str = "catalog/registry-cache.json";
const OFFICIAL_REGISTRY_HINT: &str = "https://registry.modelcontextprotocol.io";
const MAX_REGISTRY_PAGES: usize = 100;
const MAX_REGISTRY_SERVERS: usize = 10_000;
const MAX_REGISTRY_RESPONSE_BYTES_TOTAL: usize = 32 * 1024 * 1024;
const MAX_REGISTRY_QUERY_CACHE_FILES: usize = 128;

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
    launcher_args: Vec<String>,
    extra_args: Vec<String>,
    registry_type: String,
    package: String,
    transport: String,
    recommended_mode: String,
    affinity: Vec<String>,
    required_headers: Vec<String>,
    required_env: Vec<String>,
    required_args: Vec<String>,
    required_variables: Vec<String>,
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
        diagnostics::stderr_line(
            stderr,
            format_args!("mcpace root not found; expected mcpace.config.json"),
        );
        return 1;
    };

    let query = parsed.name_filter.as_deref().unwrap_or("");
    let result = match discover_servers(&root_path, query, parsed) {
        Ok(value) => value,
        Err(error) => {
            diagnostics::stderr_line(stderr, format_args!("{}", error));
            return 1;
        }
    };

    let exit_code = if json_helpers::bool_at_path(&result, &["ok"]).unwrap_or(false) {
        0
    } else {
        1
    };
    if parsed.json_output {
        let _ = writeln!(stdout, "{}", result.to_pretty_string());
        return exit_code;
    }

    render_discovery(&result, stdout);
    exit_code
}

fn discover_servers(
    root_path: &Path,
    query: &str,
    parsed: &ParsedArgs,
) -> Result<JsonValue, String> {
    let config = read_optional_json(&root_path.join("mcpace.config.json"))?;
    let auto_mode = parsed.action.as_deref() == Some("auto");
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
    let registry_cache_path =
        registry_query_cache_path(&registry_cache_path(root_path, config.as_ref()), query);
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
        || (auto_refresh_registry && registry_cache_needs_refresh && !query.trim().is_empty());
    if effective_refresh {
        match refresh_registry_cache(root_path, &registry_endpoints, &registry_cache_path, query) {
            Ok(value) => registry_refresh = value,
            Err(error) => warnings.push(format!("registry refresh failed: {}", error)),
        }
    }
    let catalog_paths = catalog_paths(root_path, config.as_ref(), &registry_cache_path);
    let installed = installed_server_names(root_path);
    let terms = search_terms(query);
    let mut candidates = Vec::new();

    match json::parse_str(include_str!("../../catalog/approved-servers.json")) {
        Ok(value) => collect_candidates_from_catalog(
            &value,
            "builtin:approved-servers",
            trust_default,
            &installed,
            &terms,
            &mut candidates,
        ),
        Err(error) => warnings.push(format!("built-in approved catalog is invalid: {}", error)),
    }

    for path in &catalog_paths {
        match json_helpers::read_json_file(path) {
            Ok(value) => {
                let registry_source = path == &registry_cache_path;
                if registry_source
                    && json_helpers::string_at_path(&value, &["metadata", "searchQuery"])
                        .map(str::trim)
                        != Some(query.trim())
                {
                    warnings.push(format!(
                        "ignored Registry cache '{}' because its searchQuery metadata does not match '{}'",
                        path.display(),
                        query.trim()
                    ));
                    continue;
                }
                let source = if registry_source {
                    format!("registry:{}", path.display())
                } else {
                    path.display().to_string()
                };
                collect_candidates_from_catalog(
                    &value,
                    &source,
                    trust_default,
                    &installed,
                    &terms,
                    &mut candidates,
                )
            }
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
                    } else if let Some(reason) = missing_candidate_configuration(candidate, parsed)
                    {
                        install_decision = "missing-required-configuration".to_string();
                        install_block_reason = reason;
                    } else if install_allowed(
                        candidate,
                        auto_install_policy,
                        parsed.allow_review_install,
                    ) {
                        let options = install_options_from_candidate(candidate, parsed);
                        if let Some(error) = install_launcher_error(&options) {
                            install_decision = "launcher-unavailable".to_string();
                            install_block_reason = error;
                        } else {
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

    let install_failed = install_decision == "install-failed"
        || automatic_install_results.iter().any(|item| {
            json_helpers::string_at_path(item, &["decision"]) == Some("install-failed")
        });
    let probe_failed = post_install_probe_results
        .iter()
        .any(|item| !json_helpers::bool_at_path(item, &["ok"]).unwrap_or(false));
    let operation_ok = !install_failed && !probe_failed;

    Ok(JsonValue::object([
        ("ok", JsonValue::bool(operation_ok)),
        ("mode", JsonValue::string("dynamic-server-discovery")),
        (
            "summary",
            JsonValue::string("Auto mode searches the embedded and local approved catalogs immediately; named queries refresh a bounded query-specific official Registry cache when missing or stale. Only trusted or explicitly allowed review candidates are installed, then optionally probed with live MCP tools/list evidence."),
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
                    JsonValue::string("mcpace advanced server auto performs the automatic path: use embedded/local approved candidates without a registry-wide download, refresh query-specific registry metadata for named searches, install up to maxAutoInstallsPerRun trusted/approved candidates, probe live tools/list, and leave unknown/review-only registry entries as plan-only"),
                ),
                (
                    "nextDynamicStep",
                    JsonValue::string("after install, MCPace can probe trusted servers automatically when dynamicDiscovery.probeAfterInstall=true; otherwise run mcpace advanced server test <name> --refresh"),
                ),
            ]),
        ),
    ]))
}

fn read_optional_json(path: &Path) -> Result<Option<JsonValue>, String> {
    if !path.is_file() {
        return Ok(None);
    }
    json_helpers::read_json_file(path)
        .map(Some)
        .map_err(|error| {
            format!(
                "failed to read discovery config '{}': {}",
                path.display(),
                error
            )
        })
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
                "ignored unsafe MCP registry endpoint '{}'; registry refresh endpoints must be https:// base URLs without credentials, query strings, fragments, backslashes, whitespace, or control characters",
                endpoint
            )),
        }
    }
    if endpoints.is_empty() {
        endpoints.push(OFFICIAL_REGISTRY_HINT.to_string());
    }
    let mut seen = BTreeSet::new();
    endpoints.retain(|endpoint| seen.insert(endpoint.clone()));
    endpoints
}

fn normalize_registry_endpoint(value: &str) -> Option<String> {
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.is_empty()
        || trimmed.contains('#')
        || trimmed.contains('?')
        || trimmed.contains('\\')
    {
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

fn registry_query_cache_path(base_path: &Path, query: &str) -> PathBuf {
    let query = query.trim();
    if query.is_empty() {
        return base_path.to_path_buf();
    }
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in query.to_ascii_lowercase().bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    let stem = base_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("registry-cache");
    let file_name = format!("{}-search-{:016x}.json", stem, hash);
    base_path
        .parent()
        .map(|parent| parent.join(&file_name))
        .unwrap_or_else(|| PathBuf::from(file_name))
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
        return true;
    };
    age > Duration::from_secs(ttl_hours.saturating_mul(60 * 60))
}

fn catalog_paths(
    root_path: &Path,
    config: Option<&JsonValue>,
    registry_cache: &Path,
) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let default_catalog = root_path.join(DEFAULT_APPROVED_CATALOG);
    if registry_cache_is_regular_file(&default_catalog) {
        paths.push(default_catalog);
    }
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
    query: &str,
) -> Result<JsonValue, String> {
    let mut failures = Vec::new();
    for endpoint in endpoints {
        match refresh_registry_cache_from_endpoint(root_path, endpoint, cache_path, query) {
            Ok(value) => return Ok(value),
            Err(error) => failures.push(format!("{}: {}", endpoint, error)),
        }
    }
    Err(format!(
        "all configured MCP Registry endpoints failed: {}",
        failures.join(" | ")
    ))
}

fn refresh_registry_cache_from_endpoint(
    root_path: &Path,
    endpoint: &str,
    cache_path: &Path,
    query: &str,
) -> Result<JsonValue, String> {
    let cache_path = if cache_path.is_absolute() {
        cache_path.to_path_buf()
    } else {
        root_path.join(cache_path)
    };
    let _cache_lock =
        runtimepaths::acquire_exclusive_file_lock(&cache_path, "MCP registry cache refresh")?;
    let mut servers = Vec::new();
    let mut next_cursor: Option<String> = None;
    let mut seen_cursors = BTreeSet::new();
    let mut pages = 0usize;
    let mut last_url = String::new();
    let mut response_bytes_total = 0usize;

    for _ in 0..MAX_REGISTRY_PAGES {
        let url = registry_list_url_with_cursor(endpoint, next_cursor.as_deref(), query);
        last_url = url.clone();
        let body = fetch_url(&url)?;
        response_bytes_total = response_bytes_total.saturating_add(body.len());
        if response_bytes_total > MAX_REGISTRY_RESPONSE_BYTES_TOTAL {
            return Err(format!(
                "{} exceeded the {}-byte cumulative Registry response limit; refusing a partial cache update",
                url, MAX_REGISTRY_RESPONSE_BYTES_TOTAL
            ));
        }
        let value = json::parse_str(&body)
            .map_err(|error| format!("invalid registry JSON from {}: {}", url, error))?;
        let Some(page_servers) = json_helpers::array_at_path(&value, &["servers"]) else {
            return Err(format!("{} did not return a servers array", url));
        };
        if servers.len().saturating_add(page_servers.len()) > MAX_REGISTRY_SERVERS {
            return Err(format!(
                "{} exceeded the {}-server Registry result limit; refusing a partial cache update",
                url, MAX_REGISTRY_SERVERS
            ));
        }
        servers.extend(page_servers.iter().cloned());
        pages += 1;
        next_cursor = json_helpers::string_at_path(&value, &["metadata", "nextCursor"])
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        if next_cursor
            .as_ref()
            .is_some_and(|cursor| cursor.len() > 4_096)
        {
            return Err(format!(
                "{} returned an oversized pagination cursor; refusing a partial cache update",
                url
            ));
        }
        let Some(cursor) = next_cursor.as_ref() else {
            break;
        };
        if !seen_cursors.insert(cursor.clone()) {
            return Err(format!(
                "{} returned a repeated pagination cursor; refusing a partial cache update",
                url
            ));
        }
    }

    if next_cursor.is_some() {
        return Err(format!(
            "{} exceeded the {}-page Registry pagination limit; refusing a partial cache update",
            last_url, MAX_REGISTRY_PAGES
        ));
    }
    let truncated = false;
    let server_count = servers.len();
    let cache_value = JsonValue::object([
        ("servers", JsonValue::array(servers)),
        (
            "metadata",
            JsonValue::object([
                ("count", JsonValue::number(server_count)),
                ("pages", JsonValue::number(pages)),
                ("sourceEndpoint", JsonValue::string(endpoint.to_string())),
                ("searchQuery", JsonValue::string(query.trim())),
                ("truncated", JsonValue::bool(truncated)),
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
    prune_registry_query_caches(&cache_path);
    Ok(JsonValue::object([
        ("ok", JsonValue::bool(true)),
        ("url", JsonValue::string(last_url)),
        (
            "cachePath",
            JsonValue::string(cache_path.display().to_string()),
        ),
        ("serverCount", JsonValue::number(server_count)),
        ("pageCount", JsonValue::number(pages)),
        ("searchQuery", JsonValue::string(query.trim())),
        ("truncated", JsonValue::bool(truncated)),
        (
            "nextCursor",
            next_cursor
                .map(JsonValue::string)
                .unwrap_or(JsonValue::Null),
        ),
    ]))
}

fn prune_registry_query_caches(cache_path: &Path) {
    let Some(stem) = cache_path.file_stem().and_then(|value| value.to_str()) else {
        return;
    };
    let Some((base_stem, _hash)) = stem.rsplit_once("-search-") else {
        return;
    };
    let Some(directory) = cache_path.parent() else {
        return;
    };
    let prefix = format!("{}-search-", base_stem);
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let mut files = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let file_type = entry.file_type().ok()?;
            let path = entry.path();
            let file_name = path.file_name()?.to_str()?;
            if !file_type.is_file()
                || file_type.is_symlink()
                || !file_name.starts_with(&prefix)
                || !file_name.to_ascii_lowercase().ends_with(".json")
            {
                return None;
            }
            Some((entry.metadata().ok()?.modified().ok()?, path))
        })
        .collect::<Vec<_>>();
    files.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    for (_modified, path) in files.into_iter().skip(MAX_REGISTRY_QUERY_CACHE_FILES) {
        let _ = fs::remove_file(path);
    }
}

fn registry_list_url_with_cursor(endpoint: &str, cursor: Option<&str>, query: &str) -> String {
    let trimmed = endpoint.trim().trim_end_matches('/');
    let mut url = if trimmed.contains("/v0.1/servers") {
        if trimmed.contains('?') {
            trimmed.to_string()
        } else {
            format!("{}?limit=100&version=latest", trimmed)
        }
    } else {
        format!("{}/v0.1/servers?limit=100&version=latest", trimmed)
    };
    if let Some(query) = (!query.trim().is_empty()).then(|| query.trim()) {
        url.push(if url.contains('?') { '&' } else { '?' });
        url.push_str("search=");
        url.push_str(&url_query_escape(query));
    }
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
    crate::http_client::bounded_get_text(url, Duration::from_secs(15), 8 * 1024 * 1024)
        .map_err(|error| format!("failed to fetch {}: {}", url, error))
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
    let registry_status = json_helpers::string_at_path(
        raw,
        &[
            "_meta",
            "io.modelcontextprotocol.registry/official",
            "status",
        ],
    )
    .unwrap_or("")
    .trim()
    .to_ascii_lowercase();
    if registry_status == "deleted" {
        return None;
    }
    // Official Registry responses wrap the actual server record in {server, _meta}.
    // Approved/local catalogs use the unwrapped shape, so accept both.
    let raw = json_helpers::value_at_path(raw, &["server"]).unwrap_or(raw);
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
    let trust_level = if candidate_source_is_registry(source) && registry_status == "deprecated" {
        "deprecated".to_string()
    } else if candidate_source_is_registry(source) {
        // Registry publication proves discoverability, not MCPace trust. Never
        // accept a publisher-controlled trustLevel or a local catalog default.
        "review".to_string()
    } else {
        json_helpers::string_at_path(raw, &["trustLevel"])
            .unwrap_or(trust_default)
            .trim()
            .to_ascii_lowercase()
    };

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
        launcher_args: Vec::new(),
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
        required_headers: Vec::new(),
        required_env: Vec::new(),
        required_args: Vec::new(),
        required_variables: Vec::new(),
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
        candidate.required_env = required_registry_field_names(package, "environmentVariables");
        candidate.required_args = required_registry_field_names(package, "packageArguments");
        candidate.launcher_args = registry_static_arguments(package, "runtimeArguments");
        candidate
            .extra_args
            .extend(registry_static_arguments(package, "packageArguments"));
        if candidate.install_spec.is_empty()
            && registry_package_base_is_supported(package, &candidate.registry_type)
        {
            candidate.install_spec = install_spec_for_registry_package(
                &candidate.registry_type,
                &candidate.package,
                json_helpers::string_at_path(package, &["version"]).unwrap_or(""),
                &candidate.transport,
            );
        }
    }

    if candidate.install_spec.is_empty() {
        if let Some(remote) = first_registry_remote(raw) {
            let remote_type = json_helpers::string_at_path(remote, &["type"])
                .unwrap_or("streamable-http")
                .trim()
                .to_ascii_lowercase();
            let remote_url = json_helpers::string_at_path(remote, &["url"])
                .unwrap_or("")
                .trim();
            if !remote_url.is_empty() && matches!(remote_type.as_str(), "streamable-http" | "http")
            {
                candidate.registry_type = "remote".to_string();
                candidate.transport = remote_type;
                candidate.required_headers = registry_remote_header_names(remote);
                candidate.required_variables = registry_remote_variable_names(remote);
                candidate.server_type = Some("streamable-http".to_string());
                candidate.url = Some(remote_url.to_string());
                candidate.install_spec = remote_url.to_string();
            }
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

fn required_registry_field_names(record: &JsonValue, field: &str) -> Vec<String> {
    let mut names = json_helpers::array_at_path(record, &[field])
        .unwrap_or(&[])
        .iter()
        .filter(|item| json_helpers::bool_at_path(item, &["isRequired"]).unwrap_or(false))
        .filter(|item| {
            json_helpers::string_at_path(item, &["value"])
                .or_else(|| json_helpers::string_at_path(item, &["default"]))
                .map(str::trim)
                .map(|value| value.is_empty())
                .unwrap_or(true)
        })
        .filter_map(|item| json_helpers::string_at_path(item, &["name"]))
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    names
}

fn registry_static_arguments(record: &JsonValue, field: &str) -> Vec<String> {
    let mut arguments = Vec::new();
    for item in json_helpers::array_at_path(record, &[field]).unwrap_or(&[]) {
        let Some(value) = json_helpers::string_at_path(item, &["value"])
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let argument_type = json_helpers::string_at_path(item, &["type"])
            .unwrap_or("positional")
            .trim();
        if argument_type.eq_ignore_ascii_case("named") {
            let Some(name) = json_helpers::string_at_path(item, &["name"])
                .map(str::trim)
                .filter(|name| !name.is_empty())
            else {
                continue;
            };
            let name = if name.starts_with('-') {
                name.to_string()
            } else {
                format!("--{}", name)
            };
            arguments.push(name);
        }
        arguments.push(value.to_string());
    }
    arguments
}

fn registry_remote_header_names(remote: &JsonValue) -> Vec<String> {
    let mut names = json_helpers::array_at_path(remote, &["headers"])
        .unwrap_or(&[])
        .iter()
        .filter_map(|header| json_helpers::string_at_path(header, &["name"]))
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    names.sort_by_key(|name| name.to_ascii_lowercase());
    names.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    names
}

fn registry_remote_variable_names(remote: &JsonValue) -> Vec<String> {
    let mut names = json_helpers::object_at_path(remote, &["variables"])
        .map(|variables| variables.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    if let Some(url) = json_helpers::string_at_path(remote, &["url"]) {
        let mut rest = url;
        while let Some(start) = rest.find('{') {
            let after_start = &rest[start + 1..];
            let Some(end) = after_start.find('}') else {
                break;
            };
            let name = after_start[..end].trim();
            if !name.is_empty() && name.len() <= 128 {
                names.push(name.to_string());
            }
            rest = &after_start[end + 1..];
        }
    }
    names.sort();
    names.dedup();
    names
}

fn first_registry_remote(raw: &JsonValue) -> Option<&JsonValue> {
    json_helpers::array_at_path(raw, &["remotes"])?
        .iter()
        .find(|remote| {
            matches!(
                json_helpers::string_at_path(remote, &["type"])
                    .unwrap_or("")
                    .trim()
                    .to_ascii_lowercase()
                    .as_str(),
                "streamable-http" | "http"
            ) && json_helpers::string_at_path(remote, &["url"])
                .map(str::trim)
                .map(|url| !url.is_empty())
                .unwrap_or(false)
        })
}

fn first_registry_package(raw: &JsonValue) -> Option<&JsonValue> {
    json_helpers::array_at_path(raw, &["packages"])?
        .iter()
        .find(|package| {
            json_helpers::string_at_path(package, &["transport", "type"])
                .unwrap_or("")
                .trim()
                .eq_ignore_ascii_case("stdio")
        })
}

fn registry_package_base_is_supported(package: &JsonValue, registry_type: &str) -> bool {
    let base = json_helpers::string_at_path(package, &["registryBaseUrl"])
        .unwrap_or("")
        .trim()
        .trim_end_matches('/')
        .to_ascii_lowercase();
    if base.is_empty() {
        return true;
    }
    match registry_type {
        "npm" => base == "https://registry.npmjs.org",
        "pypi" | "python" => matches!(
            base.as_str(),
            "https://pypi.org" | "https://pypi.org/simple"
        ),
        "nuget" => base == "https://api.nuget.org/v3/index.json",
        "oci" | "docker" | "container" | "mcpb" => true,
        _ => false,
    }
}

fn install_spec_for_registry_package(
    registry_type: &str,
    identifier: &str,
    version: &str,
    transport: &str,
) -> String {
    let identifier = identifier.trim();
    if identifier.is_empty() {
        return String::new();
    }
    let version = version.trim();
    let pinned = !version.is_empty() && !version.eq_ignore_ascii_case("latest");
    match registry_type {
        "npm" => {
            let has_version = identifier
                .rfind('@')
                .map(|index| index > identifier.rfind('/').unwrap_or(0))
                .unwrap_or(false);
            if pinned && !has_version {
                format!("npm:{}@{}", identifier, version)
            } else {
                format!("npm:{}", identifier)
            }
        }
        "pypi" | "python" => {
            if pinned && !identifier.contains("==") {
                format!("pypi:{}=={}", identifier, version)
            } else {
                format!("pypi:{}", identifier)
            }
        }
        "oci" | "docker" | "container" => format!("oci:{}", identifier),
        "nuget" => format!("nuget:{}", identifier),
        "mcpb" => format!("mcpb:{}", identifier),
        _ if transport == "streamable-http" || transport == "http" => identifier.to_string(),
        _ => String::new(),
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
        candidate_source_rank(&candidate.source),
        candidate_trust_rank(&candidate.trust_level),
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

fn candidate_source_is_registry(source: &str) -> bool {
    source
        .trim_start()
        .to_ascii_lowercase()
        .starts_with("registry:")
}

fn candidate_source_rank(source: &str) -> usize {
    let normalized = source.replace('\\', "/").to_ascii_lowercase();
    if candidate_source_is_registry(&normalized) {
        1
    } else if normalized == "builtin:approved-servers" {
        3
    } else {
        // Every non-registry source here is an operator-configured local catalog.
        // It must be able to block or replace a built-in/registry candidate.
        5
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
        if let Some(reason) = missing_candidate_configuration(candidate, parsed) {
            skipped_count += 1;
            sweep.install_results.push(JsonValue::object([
                ("name", JsonValue::string(candidate.name.clone())),
                ("decision", JsonValue::string("skip-missing-configuration")),
                ("reason", JsonValue::string(reason)),
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

        let options = install_options_from_candidate(candidate, parsed);
        if let Some(error) = install_launcher_error(&options) {
            skipped_count += 1;
            sweep.install_results.push(JsonValue::object([
                ("name", JsonValue::string(candidate.name.clone())),
                ("decision", JsonValue::string("skip-launcher-unavailable")),
                ("reason", JsonValue::string(error)),
            ]));
            continue;
        }
        match mcp_autoinstall::install_auto(root_path, options) {
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
        Ok(value) => {
            let ok = json_helpers::bool_at_path(&value, &["ok"]).unwrap_or(false);
            JsonValue::object([
                ("name", JsonValue::string(name.to_string())),
                ("ok", JsonValue::bool(ok)),
                ("result", value),
            ])
        }
        Err(error) => JsonValue::object([
            ("name", JsonValue::string(name.to_string())),
            ("ok", JsonValue::bool(false)),
            ("error", JsonValue::string(error)),
        ]),
    }
}

fn missing_candidate_configuration(
    candidate: &DiscoveryCandidate,
    parsed: &ParsedArgs,
) -> Option<String> {
    if !candidate.required_variables.is_empty() {
        return Some(format!(
            "candidate '{}' requires Registry URL variable(s): {}. Resolve them and use 'mcpace install <resolved-url> --as <name>' explicitly",
            candidate.name,
            candidate.required_variables.join(", ")
        ));
    }
    let missing_env = missing_key_value_names(&candidate.required_env, &parsed.env);
    if !missing_env.is_empty() {
        return Some(format!(
            "candidate '{}' requires environment value(s): {}. Supply each value with --env NAME=VALUE; Registry placeholders are never stored as credentials",
            candidate.name,
            missing_env.join(", ")
        ));
    }
    let missing_args = candidate
        .required_args
        .iter()
        .skip(parsed.args.len())
        .cloned()
        .collect::<Vec<_>>();
    if !missing_args.is_empty() {
        return Some(format!(
            "candidate '{}' requires package argument(s): {}. Supply each launcher argument explicitly with --arg=<value> (for named options, for example --arg=--NAME=VALUE)",
            candidate.name,
            missing_args.join(", ")
        ));
    }
    let missing_headers = missing_key_value_names(&candidate.required_headers, &parsed.headers);
    if !missing_headers.is_empty() {
        return Some(format!(
            "candidate '{}' requires HTTP header(s): {}. Supply each value with --header NAME=VALUE; Registry placeholders are never stored as credentials",
            candidate.name,
            missing_headers.join(", ")
        ));
    }
    None
}

fn missing_key_value_names(required: &[String], supplied: &[String]) -> Vec<String> {
    let supplied_names = supplied
        .iter()
        .filter_map(|value| value.split_once('=').map(|(name, _value)| name.trim()))
        .collect::<Vec<_>>();
    required
        .iter()
        .filter(|required| {
            !supplied_names
                .iter()
                .any(|supplied| supplied.eq_ignore_ascii_case(required))
        })
        .cloned()
        .collect()
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

fn install_launcher_error(options: &McpAutoInstallOptions) -> Option<String> {
    let plan = match mcp_autoinstall::plan_auto_install(options) {
        Ok(plan) => plan,
        Err(error) => return Some(error),
    };
    let command = plan.command.as_deref()?.trim();
    if command.is_empty() || which::which(command).is_ok() {
        return None;
    }
    Some(format!(
        "required launcher '{}' is not available on PATH; install it first or choose another candidate",
        command
    ))
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
        launcher_args: candidate.launcher_args.clone(),
        extra_args: candidate
            .extra_args
            .iter()
            .chain(parsed.args.iter())
            .cloned()
            .collect(),
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
            "requiredHeaders",
            JsonValue::array(
                candidate
                    .required_headers
                    .iter()
                    .cloned()
                    .map(JsonValue::string),
            ),
        ),
        (
            "requiredEnvironment",
            JsonValue::array(
                candidate
                    .required_env
                    .iter()
                    .cloned()
                    .map(JsonValue::string),
            ),
        ),
        (
            "requiredArguments",
            JsonValue::array(
                candidate
                    .required_args
                    .iter()
                    .cloned()
                    .map(JsonValue::string),
            ),
        ),
        (
            "requiredVariables",
            JsonValue::array(
                candidate
                    .required_variables
                    .iter()
                    .cloned()
                    .map(JsonValue::string),
            ),
        ),
        (
            "requiresConfiguration",
            JsonValue::bool(
                !candidate.required_headers.is_empty()
                    || !candidate.required_env.is_empty()
                    || !candidate.required_args.is_empty()
                    || !candidate.required_variables.is_empty(),
            ),
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
            "launcherArgs",
            JsonValue::array(
                candidate
                    .launcher_args
                    .iter()
                    .cloned()
                    .map(JsonValue::string),
            ),
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
        "Next: use `mcpace advanced server auto` for one-command discovery/setup/probe; detailed discovery flags remain for debugging."
    );
}

#[cfg(test)]
#[path = "discover/tests.rs"]
mod tests;
