use crate::execution::ExecutionPolicy;
use crate::json::JsonValue;
use crate::json_helpers;
use crate::mcp_protocol as mcp;
use crate::resources;
use std::collections::{hash_map::DefaultHasher, BTreeMap, BTreeSet};
#[cfg(test)]
use std::env;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::{mpsc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

mod diagnostics;
mod http_runtime;
mod inventory;
mod lease_queue;
mod lease_runtime;
mod policy_audit;
mod policy_suggestions;
mod process_config;
mod projection;
mod server_config;
mod session_pool;
mod source_type;
mod stdio_runtime;
mod tool_cache;

pub use self::inventory::{
    audit_tool_policies, catalog_tools, configured_inventory, probe_servers, suggest_tool_policies,
    surface_manifest,
};
pub use self::lease_runtime::{
    call_tool, call_tool_with_context, call_tool_with_pooled_context, call_tools,
    call_tools_with_context, call_tools_with_pooled_context, callable_server_names,
    collect_allow_arguments, collect_allowed_tool_risk_classes, list_tools, request_once,
    tool_policy_info,
};
pub use self::projection::{
    decode_projected_tool_name, encode_projected_tool_name, projected_tool_catalog,
    UpstreamProjectionSafety,
};
pub use self::session_pool::UpstreamSessionPool;

#[cfg(test)]
use self::diagnostics::stderr_suffix;
#[cfg(test)]
use self::diagnostics::DIAGNOSTIC_REDACTION;
#[cfg(test)]
use self::inventory::{catalog_cache_counts, flatten_catalog_tools};
#[cfg(test)]
use self::lease_runtime::{validate_upstream_batch_tool_policy, validate_upstream_tool_policy};
use self::policy_audit::{audit_tool, tool_policy_summaries};
#[cfg(test)]
use self::policy_suggestions::report as policy_suggestion_report;
#[cfg(test)]
use self::process_config::spawn_program_for_command;
use self::process_config::{
    expand_template, redact_command, resolve_command_for_cwd, validate_stdio_cwd,
};
#[cfg(test)]
use self::server_config::env_var_names_from_array;
use self::server_config::{
    context_string, load_servers, optional_json_string, run_server_tasks, select_servers,
};
use self::source_type::infer_source_type;
#[cfg(test)]
use self::stdio_runtime::run_stdio_request;
use self::tool_cache::{cached_tools_list, read_cached_tools, tool_list_cache_key};
#[cfg(test)]
use self::tool_cache::{
    prune_tool_list_cache, write_cached_tools, CachedToolList, ToolListCacheKey, TOOL_LIST_CACHE,
};
#[cfg(test)]
use crate::json::parse_str;
#[cfg(test)]
use crate::platform_utils::current_platform_alias;

const DEFAULT_TIMEOUT_MS: u64 = 120_000;
const DEFAULT_PROBE_TIMEOUT_MS: u64 = 30_000;
const TOOL_LIST_CACHE_TTL: Duration = Duration::from_secs(30);
const TOOL_LIST_CACHE_MAX_ENTRIES: usize = 128;
const UPSTREAM_SESSION_IDLE_TTL: Duration = Duration::from_secs(300);
const INITIALIZE_ID: i64 = 1;
const METHOD_ID: i64 = 2;
const UPSTREAM_TOOL_LIST_MAX_PAGES: usize = 100;
const UPSTREAM_TOOL_LIST_MAX_TOOLS: usize = 10_000;
const UPSTREAM_TOOL_LIST_MAX_BYTES: usize = 32 * 1024 * 1024;

struct ToolListPagination {
    tools: Vec<JsonValue>,
    seen_cursors: BTreeSet<String>,
    pages: usize,
    bytes: usize,
}

impl ToolListPagination {
    fn new() -> Self {
        Self {
            tools: Vec::new(),
            seen_cursors: BTreeSet::new(),
            pages: 0,
            bytes: 0,
        }
    }

    fn add_page(
        &mut self,
        server_name: &str,
        result: &JsonValue,
    ) -> Result<Option<String>, String> {
        let page_tools = json_helpers::array_at_path(result, &["tools"]).ok_or_else(|| {
            format!(
                "upstream server '{}' tools/list result must contain a tools array",
                server_name
            )
        })?;
        if self.tools.len().saturating_add(page_tools.len()) > UPSTREAM_TOOL_LIST_MAX_TOOLS {
            return Err(format!(
                "upstream server '{}' tools/list exceeded the {}-tool safety limit",
                server_name, UPSTREAM_TOOL_LIST_MAX_TOOLS
            ));
        }
        for tool in page_tools {
            self.bytes = self.bytes.saturating_add(tool.to_compact_string().len());
            if self.bytes > UPSTREAM_TOOL_LIST_MAX_BYTES {
                return Err(format!(
                    "upstream server '{}' tools/list exceeded the {}-byte cumulative safety limit",
                    server_name, UPSTREAM_TOOL_LIST_MAX_BYTES
                ));
            }
            self.tools.push(tool.clone());
        }
        self.pages = self.pages.saturating_add(1);
        let cursor = match json_helpers::value_at_path(result, &["nextCursor"]) {
            None | Some(JsonValue::Null) => None,
            Some(JsonValue::String(value)) if value.trim().is_empty() => None,
            Some(JsonValue::String(value)) if value.len() <= 4_096 => Some(value.clone()),
            Some(JsonValue::String(_)) => {
                return Err(format!(
                    "upstream server '{}' tools/list returned an oversized nextCursor",
                    server_name
                ));
            }
            Some(_) => {
                return Err(format!(
                    "upstream server '{}' tools/list returned a non-string nextCursor",
                    server_name
                ));
            }
        };
        let Some(cursor) = cursor else {
            return Ok(None);
        };
        if self.pages >= UPSTREAM_TOOL_LIST_MAX_PAGES {
            return Err(format!(
                "upstream server '{}' tools/list exceeded the {}-page safety limit",
                server_name, UPSTREAM_TOOL_LIST_MAX_PAGES
            ));
        }
        if !self.seen_cursors.insert(cursor.clone()) {
            return Err(format!(
                "upstream server '{}' tools/list repeated pagination cursor '{}'",
                server_name, cursor
            ));
        }
        Ok(Some(cursor))
    }

    fn finish(self) -> JsonValue {
        JsonValue::object([("tools", JsonValue::array(self.tools))])
    }
}

fn max_pooled_upstream_sessions() -> usize {
    resources::default_upstream_session_pool_limit()
}

#[derive(Clone, Debug)]
struct UpstreamServerConfig {
    name: String,
    enabled: bool,
    disabled_reason: Option<String>,
    source_type: String,
    command: Option<String>,
    args: Vec<String>,
    env: BTreeMap<String, String>,
    headers: BTreeMap<String, String>,
    cwd: Option<PathBuf>,
    url: Option<String>,
    timeout_ms: u64,
    execution: ExecutionPolicy,
    tool_policies: Vec<ToolRiskPolicy>,
}

#[derive(Clone, Debug)]
struct ToolRiskPolicy {
    tools: Vec<String>,
    risk_class: Option<String>,
    allow_argument: Option<String>,
    description: Option<String>,
}

#[derive(Clone, Debug)]
struct UpstreamServerPolicy {
    profile_enabled: bool,
    platform_supported: bool,
    runtime_enabled: bool,
    execution: ExecutionPolicy,
    tool_policies: Vec<ToolRiskPolicy>,
}

#[derive(Clone, Debug)]
pub struct UpstreamToolCall {
    pub tool: String,
    pub arguments: JsonValue,
}

#[derive(Clone, Debug, Default)]
pub struct UpstreamLeaseContext {
    pub client_id: Option<String>,
    pub session_id: Option<String>,
    pub project_root: Option<String>,
    pub transport: Option<String>,
    pub metadata: Option<JsonValue>,
    pub ttl_ms: Option<u128>,
    pub allow_arguments: BTreeSet<String>,
    pub allowed_tool_risk_classes: BTreeSet<String>,
}

fn cache_root_path(root_path: &Path) -> String {
    root_path
        .canonicalize()
        .unwrap_or_else(|_| root_path.to_path_buf())
        .display()
        .to_string()
}

fn server_fingerprint(server: &UpstreamServerConfig) -> String {
    let env_values = fingerprint_secret_map(&server.env);
    let header_values = fingerprint_secret_map(&server.headers);
    format!(
        "protocol={}|enabled={}|type={}|command={}|args={}|env={}|headers={}|cwd={}|url={}|timeout={}|execution={}",
        mcp::CURRENT_PROTOCOL_VERSION,
        server.enabled,
        server.source_type,
        server.command.as_deref().unwrap_or_default(),
        server.args.join("\u{1f}"),
        env_values,
        header_values,
        server
            .cwd
            .as_ref()
            .map(|value| value.display().to_string())
            .unwrap_or_default(),
        server.url.as_deref().unwrap_or_default(),
        server.timeout_ms,
        server.execution.fingerprint()
    )
}

fn fingerprint_secret_map(values: &BTreeMap<String, String>) -> String {
    values
        .iter()
        .map(|(key, value)| format!("{}:{}", key, fingerprint_secret_value(value)))
        .collect::<Vec<_>>()
        .join("\u{1f}")
}

fn fingerprint_secret_value(value: &str) -> String {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    format!("len{}-hash{:016x}", value.len(), hasher.finish())
}

fn ensure_callable_stdio(root_path: &Path, server: &UpstreamServerConfig) -> Result<(), String> {
    let (runtime_callable, _resolved_command, command_error) =
        server_runtime_callable(root_path, server);
    if runtime_callable {
        return Ok(());
    }
    if !server.enabled {
        return Err(format!(
            "upstream server '{}' is disabled: {}",
            server.name,
            server
                .disabled_reason
                .as_deref()
                .unwrap_or("server is disabled by source or policy")
        ));
    }
    if let Some(error) = command_error {
        return Err(error);
    }
    if server.source_type != "stdio" && server.source_type != "http" {
        return Err(format!(
            "upstream server '{}' uses '{}' transport. This MCPace bridge forwards stdio and Streamable HTTP/HTTPS upstreams; configure a compatibility adapter or call runtime_diagnostics for exact status.",
            server.name, server.source_type
        ));
    }
    Err(format!(
        "upstream server '{}' is not callable through the stdio bridge",
        server.name
    ))
}

fn timeout_for(server: &UpstreamServerConfig, requested_ms: Option<u64>) -> Duration {
    let millis = requested_ms
        .unwrap_or(server.timeout_ms)
        .clamp(1_000, 300_000);
    Duration::from_millis(millis)
}

fn probe_timeout_for(server: &UpstreamServerConfig, requested_ms: Option<u64>) -> Duration {
    let default_ms = server.timeout_ms.min(DEFAULT_PROBE_TIMEOUT_MS);
    let millis = requested_ms.unwrap_or(default_ms).clamp(1_000, 300_000);
    Duration::from_millis(millis)
}

pub(crate) fn http_upstream_url_is_callable(url: &str) -> bool {
    http_runtime::http_upstream_configuration_error(url, &BTreeMap::new()).is_none()
}

fn server_runtime_callable(
    root_path: &Path,
    server: &UpstreamServerConfig,
) -> (bool, Option<PathBuf>, Option<String>) {
    if !server.enabled {
        return (false, None, None);
    }
    if server.source_type == "http" {
        let Some(url) = server
            .url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return (
                false,
                None,
                Some(format!(
                    "HTTP upstream server '{}' has no url configured",
                    server.name
                )),
            );
        };
        if let Some(error) = http_runtime::http_upstream_configuration_error(url, &server.headers) {
            return (false, None, Some(error));
        }
        return (true, None, None);
    }
    if server.source_type == "legacy-sse" {
        return (
            false,
            None,
            Some(format!(
                "upstream server '{}' declares the deprecated HTTP+SSE transport. Use a stdio compatibility adapter or migrate the endpoint to Streamable HTTP before direct forwarding.",
                server.name
            )),
        );
    }
    if server.source_type != "stdio" {
        return (
            false,
            None,
            Some(format!(
                "upstream server '{}' uses unsupported '{}' transport",
                server.name, server.source_type
            )),
        );
    }
    if server.args.iter().any(|argument| argument.contains('\0')) {
        return (
            false,
            None,
            Some(format!(
                "upstream server '{}' contains a NUL byte in a stdio argument",
                server.name
            )),
        );
    }
    if let Some((name, _value)) = server.env.iter().find(|(name, value)| {
        name.is_empty() || name.contains('=') || name.contains('\0') || value.contains('\0')
    }) {
        return (
            false,
            None,
            Some(format!(
                "upstream server '{}' contains an invalid environment entry '{}'",
                server.name, name
            )),
        );
    }
    let Some(command) = server
        .command
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return (
            false,
            None,
            Some(format!(
                "upstream server '{}' has no stdio command configured",
                server.name
            )),
        );
    };

    let cwd = server.cwd.as_deref().unwrap_or(root_path);
    if let Some(cwd_error) = validate_stdio_cwd(cwd, &server.name) {
        return (false, None, Some(cwd_error));
    }

    match resolve_command_for_cwd(command, cwd) {
        Ok(path) => (true, Some(path), None),
        Err(error) => (
            false,
            None,
            Some(format!(
                "failed to resolve command '{}' for upstream server '{}': {}",
                command, server.name, error
            )),
        ),
    }
}

fn upstream_blocked_status(server: &UpstreamServerConfig) -> &'static str {
    if !server.enabled {
        return "disabled";
    }
    match server.source_type.as_str() {
        "stdio" => "blocked-command-not-found",
        "http" => "blocked-http-upstream",
        "legacy-sse" => "blocked-legacy-sse-upstream",
        _ => "blocked-unsupported-transport",
    }
}

fn catalog_server(
    root_path: &Path,
    server: &UpstreamServerConfig,
    timeout_ms: Option<u64>,
    refresh: bool,
) -> JsonValue {
    let started = Instant::now();
    let (runtime_callable, resolved_command, command_error) =
        server_runtime_callable(root_path, server);
    let effective_timeout = probe_timeout_for(server, timeout_ms);
    if !runtime_callable {
        let status = upstream_blocked_status(server);
        return JsonValue::object([
            ("name", JsonValue::string(&server.name)),
            ("ok", JsonValue::bool(false)),
            ("enabled", JsonValue::bool(server.enabled)),
            ("sourceType", JsonValue::string(&server.source_type)),
            ("runtimeCallable", JsonValue::bool(false)),
            ("status", JsonValue::string(status)),
            (
                "command",
                server
                    .command
                    .as_ref()
                    .map(|value| JsonValue::string(redact_command(value)))
                    .unwrap_or(JsonValue::Null),
            ),
            (
                "resolvedCommand",
                resolved_command
                    .as_ref()
                    .map(|value| JsonValue::string(value.display().to_string()))
                    .unwrap_or(JsonValue::Null),
            ),
            (
                "url",
                server
                    .url
                    .as_ref()
                    .map(JsonValue::string)
                    .unwrap_or(JsonValue::Null),
            ),
            ("toolCount", JsonValue::number(0)),
            ("tools", JsonValue::array([])),
            (
                "elapsedMs",
                JsonValue::number(started.elapsed().as_millis()),
            ),
            ("cacheHit", JsonValue::bool(false)),
            (
                "cacheTtlMs",
                JsonValue::number(TOOL_LIST_CACHE_TTL.as_millis()),
            ),
            (
                "error",
                command_error.map(JsonValue::string).unwrap_or_else(|| {
                    JsonValue::string(
                        server
                            .disabled_reason
                            .as_deref()
                            .unwrap_or(status)
                            .to_string(),
                    )
                }),
            ),
        ]);
    }

    match cached_tools_list(root_path, server, effective_timeout, refresh) {
        Ok((raw_tools, cache_hit)) => {
            let tools = raw_tools
                .as_array()
                .unwrap_or(&[])
                .iter()
                .map(tool_summary)
                .collect::<Vec<_>>();
            JsonValue::object([
                ("name", JsonValue::string(&server.name)),
                ("ok", JsonValue::bool(true)),
                ("enabled", JsonValue::bool(server.enabled)),
                ("sourceType", JsonValue::string(&server.source_type)),
                ("runtimeCallable", JsonValue::bool(true)),
                ("status", JsonValue::string("listed-tools")),
                (
                    "timeoutMs",
                    JsonValue::number(effective_timeout.as_millis()),
                ),
                (
                    "elapsedMs",
                    JsonValue::number(started.elapsed().as_millis()),
                ),
                ("cacheHit", JsonValue::bool(cache_hit)),
                (
                    "cacheTtlMs",
                    JsonValue::number(TOOL_LIST_CACHE_TTL.as_millis()),
                ),
                (
                    "command",
                    server
                        .command
                        .as_ref()
                        .map(|value| JsonValue::string(redact_command(value)))
                        .unwrap_or(JsonValue::Null),
                ),
                (
                    "resolvedCommand",
                    resolved_command
                        .as_ref()
                        .map(|value| JsonValue::string(value.display().to_string()))
                        .unwrap_or(JsonValue::Null),
                ),
                ("toolCount", JsonValue::number(tools.len())),
                ("tools", JsonValue::array(tools)),
            ])
        }
        Err(error) => JsonValue::object([
            ("name", JsonValue::string(&server.name)),
            ("ok", JsonValue::bool(false)),
            ("enabled", JsonValue::bool(server.enabled)),
            ("sourceType", JsonValue::string(&server.source_type)),
            ("runtimeCallable", JsonValue::bool(true)),
            ("status", JsonValue::string("catalog-failed")),
            (
                "timeoutMs",
                JsonValue::number(effective_timeout.as_millis()),
            ),
            (
                "elapsedMs",
                JsonValue::number(started.elapsed().as_millis()),
            ),
            ("cacheHit", JsonValue::bool(false)),
            (
                "cacheTtlMs",
                JsonValue::number(TOOL_LIST_CACHE_TTL.as_millis()),
            ),
            (
                "command",
                server
                    .command
                    .as_ref()
                    .map(|value| JsonValue::string(redact_command(value)))
                    .unwrap_or(JsonValue::Null),
            ),
            (
                "resolvedCommand",
                resolved_command
                    .as_ref()
                    .map(|value| JsonValue::string(value.display().to_string()))
                    .unwrap_or(JsonValue::Null),
            ),
            ("toolCount", JsonValue::number(0)),
            ("tools", JsonValue::array([])),
            ("error", JsonValue::string(error)),
        ]),
    }
}

pub fn callable_tools_raw_catalog(
    root_path: &Path,
    timeout_ms: Option<u64>,
    refresh: bool,
) -> Result<JsonValue, String> {
    let started = Instant::now();
    let servers = load_servers(root_path)?;
    let callable_servers = servers
        .values()
        .filter(|server| server_runtime_callable(root_path, server).0)
        .cloned()
        .collect::<Vec<_>>();
    let results = run_server_tasks(
        root_path,
        callable_servers,
        timeout_ms,
        move |root, server, timeout| catalog_server_raw(root, server, timeout, refresh),
    );
    let ok_count = results
        .iter()
        .filter(|item| json_helpers::bool_at_path(item, &["ok"]).unwrap_or(false))
        .count();
    let failed_count = results.len().saturating_sub(ok_count);
    let tool_count = results
        .iter()
        .filter_map(|item| json_helpers::value_at_path(item, &["toolCount"]))
        .filter_map(JsonValue::as_i64)
        .sum::<i64>();

    Ok(JsonValue::object([
        ("ok", JsonValue::bool(failed_count == 0)),
        ("mode", JsonValue::string("raw-callable-tools-catalog")),
        (
            "summary",
            JsonValue::string(
                "Discovered raw tools/list definitions for runtime-callable upstream MCP servers with bounded parallel workers.",
            ),
        ),
        ("serverCount", JsonValue::number(results.len())),
        ("okCount", JsonValue::number(ok_count)),
        ("failedCount", JsonValue::number(failed_count)),
        ("toolCount", JsonValue::number(tool_count)),
        ("elapsedMs", JsonValue::number(started.elapsed().as_millis())),
        ("servers", JsonValue::array(results)),
    ]))
}

pub fn callable_tools_raw_catalog_for_servers(
    root_path: &Path,
    server_names: &[String],
    timeout_ms: Option<u64>,
    refresh: bool,
) -> Result<JsonValue, String> {
    let started = Instant::now();
    let servers = load_servers(root_path)?;
    let mut seen = BTreeSet::new();
    let mut selected_servers = Vec::new();

    for server_name in server_names {
        let requested = server_name.trim();
        if requested.is_empty() {
            continue;
        }
        let Some(server) = servers
            .values()
            .find(|server| server.name.eq_ignore_ascii_case(requested))
        else {
            continue;
        };
        let key = server.name.to_ascii_lowercase();
        if !seen.insert(key) || !server_runtime_callable(root_path, server).0 {
            continue;
        }
        selected_servers.push(server.clone());
    }

    let results = run_server_tasks(
        root_path,
        selected_servers,
        timeout_ms,
        move |root, server, timeout| catalog_server_raw(root, server, timeout, refresh),
    );
    let ok_count = results
        .iter()
        .filter(|item| json_helpers::bool_at_path(item, &["ok"]).unwrap_or(false))
        .count();
    let failed_count = results.len().saturating_sub(ok_count);
    let tool_count = results
        .iter()
        .filter_map(|item| json_helpers::value_at_path(item, &["toolCount"]))
        .filter_map(JsonValue::as_i64)
        .sum::<i64>();

    Ok(JsonValue::object([
        ("ok", JsonValue::bool(failed_count == 0)),
        (
            "mode",
            JsonValue::string("raw-callable-tools-candidate-catalog"),
        ),
        (
            "summary",
            JsonValue::string(
                "Discovered raw tools/list definitions only for query-ranked candidate upstream MCP servers, avoiding full upstream fan-out for targeted search.",
            ),
        ),
        ("requestedServerCount", JsonValue::number(server_names.len())),
        ("serverCount", JsonValue::number(results.len())),
        ("okCount", JsonValue::number(ok_count)),
        ("failedCount", JsonValue::number(failed_count)),
        ("toolCount", JsonValue::number(tool_count)),
        ("elapsedMs", JsonValue::number(started.elapsed().as_millis())),
        ("servers", JsonValue::array(results)),
    ]))
}

pub fn callable_tools_cached_catalog(root_path: &Path) -> Result<JsonValue, String> {
    let started = Instant::now();
    let servers = load_servers(root_path)?;
    let mut results = Vec::new();
    let mut ok_count = 0usize;
    let mut missing_count = 0usize;
    let mut tool_count = 0usize;

    for server in servers.values() {
        if !server.enabled || (server.source_type != "stdio" && server.source_type != "http") {
            continue;
        }
        let key = tool_list_cache_key(root_path, server);
        if let Some(raw_tools) = read_cached_tools(&key) {
            let count = raw_tools.as_array().map(|items| items.len()).unwrap_or(0);
            tool_count = tool_count.saturating_add(count);
            ok_count = ok_count.saturating_add(1);
            results.push(JsonValue::object([
                ("name", JsonValue::string(&server.name)),
                ("ok", JsonValue::bool(true)),
                ("enabled", JsonValue::bool(server.enabled)),
                ("sourceType", JsonValue::string(&server.source_type)),
                ("runtimeCallable", JsonValue::bool(true)),
                ("status", JsonValue::string("cached-tools")),
                ("elapsedMs", JsonValue::number(0)),
                ("cacheHit", JsonValue::bool(true)),
                (
                    "cacheTtlMs",
                    JsonValue::number(TOOL_LIST_CACHE_TTL.as_millis()),
                ),
                ("toolCount", JsonValue::number(count)),
                ("tools", raw_tools),
            ]));
        } else {
            missing_count = missing_count.saturating_add(1);
            results.push(JsonValue::object([
                ("name", JsonValue::string(&server.name)),
                ("ok", JsonValue::bool(false)),
                ("enabled", JsonValue::bool(server.enabled)),
                ("sourceType", JsonValue::string(&server.source_type)),
                ("runtimeCallable", JsonValue::bool(true)),
                ("status", JsonValue::string("cache-miss")),
                ("elapsedMs", JsonValue::number(0)),
                ("cacheHit", JsonValue::bool(false)),
                (
                    "cacheTtlMs",
                    JsonValue::number(TOOL_LIST_CACHE_TTL.as_millis()),
                ),
                ("toolCount", JsonValue::number(0)),
                ("tools", JsonValue::array([])),
                (
                    "error",
                    JsonValue::string(
                        "no fresh tools/list cache entry; broker fallback remains available",
                    ),
                ),
            ]));
        }
    }

    Ok(JsonValue::object([
        ("ok", JsonValue::bool(missing_count == 0)),
        ("mode", JsonValue::string("raw-callable-tools-cache")),
        (
            "summary",
            JsonValue::string(
                "Read cached raw tools/list definitions for runtime-callable upstream MCP servers without launching upstream processes.",
            ),
        ),
        ("serverCount", JsonValue::number(results.len())),
        ("okCount", JsonValue::number(ok_count)),
        ("failedCount", JsonValue::number(missing_count)),
        ("cacheMissCount", JsonValue::number(missing_count)),
        ("toolCount", JsonValue::number(tool_count)),
        ("elapsedMs", JsonValue::number(started.elapsed().as_millis())),
        ("servers", JsonValue::array(results)),
    ]))
}

pub fn warm_tool_list_cache(
    root_path: &Path,
    timeout_ms: Option<u64>,
    refresh: bool,
) -> Result<JsonValue, String> {
    callable_tools_raw_catalog(root_path, timeout_ms, refresh)
}

pub fn warm_tool_list_cache_background(root_path: PathBuf, timeout_ms: Option<u64>, refresh: bool) {
    let _ = thread::Builder::new()
        .name("mcpace-tool-list-cache-warmup".to_string())
        .spawn(move || {
            let _ = warm_tool_list_cache(&root_path, timeout_ms, refresh);
        });
}

fn catalog_server_raw(
    root_path: &Path,
    server: &UpstreamServerConfig,
    timeout_ms: Option<u64>,
    refresh: bool,
) -> JsonValue {
    let started = Instant::now();
    let effective_timeout = probe_timeout_for(server, timeout_ms);

    match cached_tools_list(root_path, server, effective_timeout, refresh) {
        Ok((raw_tools, cache_hit)) => {
            let count = raw_tools.as_array().map(|items| items.len()).unwrap_or(0);
            JsonValue::object([
                ("name", JsonValue::string(&server.name)),
                ("ok", JsonValue::bool(true)),
                ("enabled", JsonValue::bool(server.enabled)),
                ("sourceType", JsonValue::string(&server.source_type)),
                ("runtimeCallable", JsonValue::bool(true)),
                ("status", JsonValue::string("listed-tools")),
                (
                    "timeoutMs",
                    JsonValue::number(effective_timeout.as_millis()),
                ),
                (
                    "elapsedMs",
                    JsonValue::number(started.elapsed().as_millis()),
                ),
                ("cacheHit", JsonValue::bool(cache_hit)),
                (
                    "cacheTtlMs",
                    JsonValue::number(TOOL_LIST_CACHE_TTL.as_millis()),
                ),
                ("toolCount", JsonValue::number(count)),
                ("tools", raw_tools),
            ])
        }
        Err(error) => JsonValue::object([
            ("name", JsonValue::string(&server.name)),
            ("ok", JsonValue::bool(false)),
            ("enabled", JsonValue::bool(server.enabled)),
            ("sourceType", JsonValue::string(&server.source_type)),
            ("runtimeCallable", JsonValue::bool(true)),
            ("status", JsonValue::string("catalog-failed")),
            (
                "timeoutMs",
                JsonValue::number(effective_timeout.as_millis()),
            ),
            (
                "elapsedMs",
                JsonValue::number(started.elapsed().as_millis()),
            ),
            ("cacheHit", JsonValue::bool(false)),
            (
                "cacheTtlMs",
                JsonValue::number(TOOL_LIST_CACHE_TTL.as_millis()),
            ),
            ("toolCount", JsonValue::number(0)),
            ("tools", JsonValue::array([])),
            ("error", JsonValue::string(error)),
        ]),
    }
}

fn tool_summary(tool: &JsonValue) -> JsonValue {
    let name = json_helpers::string_at_path(tool, &["name"])
        .unwrap_or("<unnamed>")
        .to_string();
    let title = json_helpers::string_at_path(tool, &["title"]).map(str::to_string);
    let description = json_helpers::string_at_path(tool, &["description"])
        .or(title.as_deref())
        .unwrap_or("")
        .to_string();
    JsonValue::object([
        ("name", JsonValue::string(name)),
        (
            "title",
            title.map(JsonValue::string).unwrap_or(JsonValue::Null),
        ),
        ("description", JsonValue::string(description)),
    ])
}

fn probe_server(
    root_path: &Path,
    server: &UpstreamServerConfig,
    timeout_ms: Option<u64>,
    refresh: bool,
) -> JsonValue {
    let started = Instant::now();
    let (runtime_callable, resolved_command, command_error) =
        server_runtime_callable(root_path, server);
    let effective_timeout = probe_timeout_for(server, timeout_ms);
    if !runtime_callable {
        let status = upstream_blocked_status(server);
        return JsonValue::object([
            ("name", JsonValue::string(&server.name)),
            ("ok", JsonValue::bool(false)),
            ("enabled", JsonValue::bool(server.enabled)),
            ("sourceType", JsonValue::string(&server.source_type)),
            ("runtimeCallable", JsonValue::bool(false)),
            ("status", JsonValue::string(status)),
            (
                "command",
                server
                    .command
                    .as_ref()
                    .map(|value| JsonValue::string(redact_command(value)))
                    .unwrap_or(JsonValue::Null),
            ),
            (
                "resolvedCommand",
                resolved_command
                    .as_ref()
                    .map(|value| JsonValue::string(value.display().to_string()))
                    .unwrap_or(JsonValue::Null),
            ),
            (
                "url",
                server
                    .url
                    .as_ref()
                    .map(JsonValue::string)
                    .unwrap_or(JsonValue::Null),
            ),
            ("toolCount", JsonValue::Null),
            (
                "elapsedMs",
                JsonValue::number(started.elapsed().as_millis()),
            ),
            ("cacheHit", JsonValue::bool(false)),
            (
                "cacheTtlMs",
                JsonValue::number(TOOL_LIST_CACHE_TTL.as_millis()),
            ),
            (
                "error",
                command_error.map(JsonValue::string).unwrap_or_else(|| {
                    JsonValue::string(
                        server
                            .disabled_reason
                            .as_deref()
                            .unwrap_or(status)
                            .to_string(),
                    )
                }),
            ),
        ]);
    }

    match cached_tools_list(root_path, server, effective_timeout, refresh) {
        Ok((tools, cache_hit)) => {
            let tool_names = tools
                .as_array()
                .unwrap_or(&[])
                .iter()
                .filter_map(|tool| json_helpers::string_at_path(tool, &["name"]))
                .map(JsonValue::string)
                .collect::<Vec<_>>();
            JsonValue::object([
                ("name", JsonValue::string(&server.name)),
                ("ok", JsonValue::bool(true)),
                ("enabled", JsonValue::bool(server.enabled)),
                ("sourceType", JsonValue::string(&server.source_type)),
                ("runtimeCallable", JsonValue::bool(true)),
                ("status", JsonValue::string("listed-tools")),
                (
                    "timeoutMs",
                    JsonValue::number(effective_timeout.as_millis()),
                ),
                (
                    "elapsedMs",
                    JsonValue::number(started.elapsed().as_millis()),
                ),
                ("cacheHit", JsonValue::bool(cache_hit)),
                (
                    "cacheTtlMs",
                    JsonValue::number(TOOL_LIST_CACHE_TTL.as_millis()),
                ),
                (
                    "command",
                    server
                        .command
                        .as_ref()
                        .map(|value| JsonValue::string(redact_command(value)))
                        .unwrap_or(JsonValue::Null),
                ),
                (
                    "resolvedCommand",
                    resolved_command
                        .as_ref()
                        .map(|value| JsonValue::string(value.display().to_string()))
                        .unwrap_or(JsonValue::Null),
                ),
                (
                    "toolCount",
                    JsonValue::number(tools.as_array().map(|items| items.len()).unwrap_or(0)),
                ),
                ("toolNames", JsonValue::array(tool_names)),
            ])
        }
        Err(error) => JsonValue::object([
            ("name", JsonValue::string(&server.name)),
            ("ok", JsonValue::bool(false)),
            ("enabled", JsonValue::bool(server.enabled)),
            ("sourceType", JsonValue::string(&server.source_type)),
            ("runtimeCallable", JsonValue::bool(true)),
            ("status", JsonValue::string("probe-failed")),
            (
                "timeoutMs",
                JsonValue::number(effective_timeout.as_millis()),
            ),
            (
                "elapsedMs",
                JsonValue::number(started.elapsed().as_millis()),
            ),
            ("cacheHit", JsonValue::bool(false)),
            (
                "cacheTtlMs",
                JsonValue::number(TOOL_LIST_CACHE_TTL.as_millis()),
            ),
            (
                "command",
                server
                    .command
                    .as_ref()
                    .map(|value| JsonValue::string(redact_command(value)))
                    .unwrap_or(JsonValue::Null),
            ),
            (
                "resolvedCommand",
                resolved_command
                    .as_ref()
                    .map(|value| JsonValue::string(value.display().to_string()))
                    .unwrap_or(JsonValue::Null),
            ),
            ("toolCount", JsonValue::Null),
            ("error", JsonValue::string(error)),
        ]),
    }
}

fn audit_server(
    root_path: &Path,
    server: &UpstreamServerConfig,
    timeout_ms: Option<u64>,
    refresh: bool,
) -> JsonValue {
    let started = Instant::now();
    let (runtime_callable, resolved_command, command_error) =
        server_runtime_callable(root_path, server);
    let effective_timeout = probe_timeout_for(server, timeout_ms);
    let declared_policies = tool_policy_summaries(&server.tool_policies);
    if !runtime_callable {
        let status = upstream_blocked_status(server);
        return JsonValue::object([
            ("name", JsonValue::string(&server.name)),
            ("ok", JsonValue::bool(false)),
            ("enabled", JsonValue::bool(server.enabled)),
            ("sourceType", JsonValue::string(&server.source_type)),
            ("runtimeCallable", JsonValue::bool(false)),
            ("status", JsonValue::string(status)),
            (
                "command",
                server
                    .command
                    .as_ref()
                    .map(|value| JsonValue::string(redact_command(value)))
                    .unwrap_or(JsonValue::Null),
            ),
            (
                "resolvedCommand",
                resolved_command
                    .as_ref()
                    .map(|value| JsonValue::string(value.display().to_string()))
                    .unwrap_or(JsonValue::Null),
            ),
            (
                "url",
                server
                    .url
                    .as_ref()
                    .map(JsonValue::string)
                    .unwrap_or(JsonValue::Null),
            ),
            (
                "declaredPolicyCount",
                JsonValue::number(declared_policies.len()),
            ),
            ("declaredPolicies", JsonValue::array(declared_policies)),
            ("toolCount", JsonValue::number(0)),
            ("annotatedToolCount", JsonValue::number(0)),
            ("unannotatedToolCount", JsonValue::number(0)),
            ("advisoryRiskToolCount", JsonValue::number(0)),
            ("guardRecommendedToolCount", JsonValue::number(0)),
            ("policyCoveredToolCount", JsonValue::number(0)),
            ("unprotectedAdvisoryRiskToolCount", JsonValue::number(0)),
            ("unprotectedGuardRecommendedToolCount", JsonValue::number(0)),
            ("unknownSemanticsToolCount", JsonValue::number(0)),
            ("reviewRecommendedToolCount", JsonValue::number(0)),
            ("tools", JsonValue::array([])),
            (
                "elapsedMs",
                JsonValue::number(started.elapsed().as_millis()),
            ),
            ("cacheHit", JsonValue::bool(false)),
            (
                "cacheTtlMs",
                JsonValue::number(TOOL_LIST_CACHE_TTL.as_millis()),
            ),
            (
                "error",
                command_error.map(JsonValue::string).unwrap_or_else(|| {
                    JsonValue::string(
                        server
                            .disabled_reason
                            .as_deref()
                            .unwrap_or(status)
                            .to_string(),
                    )
                }),
            ),
        ]);
    }

    match cached_tools_list(root_path, server, effective_timeout, refresh) {
        Ok((raw_tools, cache_hit)) => {
            let audits = raw_tools
                .as_array()
                .unwrap_or(&[])
                .iter()
                .map(|tool| audit_tool(server, tool))
                .collect::<Vec<_>>();
            let annotated_tool_count = audits.iter().filter(|item| item.has_annotations).count();
            let unannotated_tool_count = audits.len().saturating_sub(annotated_tool_count);
            let advisory_risk_tool_count =
                audits.iter().filter(|item| item.has_advisory_risk).count();
            let guard_recommended_tool_count =
                audits.iter().filter(|item| item.guard_recommended).count();
            let policy_covered_tool_count =
                audits.iter().filter(|item| item.policy_covered).count();
            let unprotected_advisory_risk_tool_count = audits
                .iter()
                .filter(|item| item.has_advisory_risk && !item.policy_covered)
                .count();
            let unprotected_guard_recommended_tool_count = audits
                .iter()
                .filter(|item| item.guard_recommended && !item.policy_covered)
                .count();
            let unknown_semantics_tool_count =
                audits.iter().filter(|item| item.unknown_semantics).count();
            let review_recommended_tool_count =
                audits.iter().filter(|item| item.review_recommended).count();
            let tools = audits
                .into_iter()
                .map(|item| item.value)
                .collect::<Vec<_>>();

            JsonValue::object([
                ("name", JsonValue::string(&server.name)),
                ("ok", JsonValue::bool(true)),
                (
                    "policyOk",
                    JsonValue::bool(unprotected_guard_recommended_tool_count == 0),
                ),
                ("enabled", JsonValue::bool(server.enabled)),
                ("sourceType", JsonValue::string(&server.source_type)),
                ("runtimeCallable", JsonValue::bool(true)),
                ("status", JsonValue::string("audited-tools")),
                (
                    "timeoutMs",
                    JsonValue::number(effective_timeout.as_millis()),
                ),
                (
                    "elapsedMs",
                    JsonValue::number(started.elapsed().as_millis()),
                ),
                ("cacheHit", JsonValue::bool(cache_hit)),
                (
                    "cacheTtlMs",
                    JsonValue::number(TOOL_LIST_CACHE_TTL.as_millis()),
                ),
                (
                    "command",
                    server
                        .command
                        .as_ref()
                        .map(|value| JsonValue::string(redact_command(value)))
                        .unwrap_or(JsonValue::Null),
                ),
                (
                    "resolvedCommand",
                    resolved_command
                        .as_ref()
                        .map(|value| JsonValue::string(value.display().to_string()))
                        .unwrap_or(JsonValue::Null),
                ),
                (
                    "declaredPolicyCount",
                    JsonValue::number(declared_policies.len()),
                ),
                ("declaredPolicies", JsonValue::array(declared_policies)),
                ("toolCount", JsonValue::number(tools.len())),
                (
                    "annotatedToolCount",
                    JsonValue::number(annotated_tool_count),
                ),
                (
                    "unannotatedToolCount",
                    JsonValue::number(unannotated_tool_count),
                ),
                (
                    "advisoryRiskToolCount",
                    JsonValue::number(advisory_risk_tool_count),
                ),
                (
                    "guardRecommendedToolCount",
                    JsonValue::number(guard_recommended_tool_count),
                ),
                (
                    "policyCoveredToolCount",
                    JsonValue::number(policy_covered_tool_count),
                ),
                (
                    "unprotectedAdvisoryRiskToolCount",
                    JsonValue::number(unprotected_advisory_risk_tool_count),
                ),
                (
                    "unprotectedGuardRecommendedToolCount",
                    JsonValue::number(unprotected_guard_recommended_tool_count),
                ),
                (
                    "unknownSemanticsToolCount",
                    JsonValue::number(unknown_semantics_tool_count),
                ),
                (
                    "reviewRecommendedToolCount",
                    JsonValue::number(review_recommended_tool_count),
                ),
                ("tools", JsonValue::array(tools)),
            ])
        }
        Err(error) => JsonValue::object([
            ("name", JsonValue::string(&server.name)),
            ("ok", JsonValue::bool(false)),
            ("enabled", JsonValue::bool(server.enabled)),
            ("sourceType", JsonValue::string(&server.source_type)),
            ("runtimeCallable", JsonValue::bool(true)),
            ("status", JsonValue::string("audit-failed")),
            (
                "timeoutMs",
                JsonValue::number(effective_timeout.as_millis()),
            ),
            (
                "elapsedMs",
                JsonValue::number(started.elapsed().as_millis()),
            ),
            ("cacheHit", JsonValue::bool(false)),
            (
                "cacheTtlMs",
                JsonValue::number(TOOL_LIST_CACHE_TTL.as_millis()),
            ),
            (
                "command",
                server
                    .command
                    .as_ref()
                    .map(|value| JsonValue::string(redact_command(value)))
                    .unwrap_or(JsonValue::Null),
            ),
            (
                "resolvedCommand",
                resolved_command
                    .as_ref()
                    .map(|value| JsonValue::string(value.display().to_string()))
                    .unwrap_or(JsonValue::Null),
            ),
            (
                "declaredPolicyCount",
                JsonValue::number(declared_policies.len()),
            ),
            ("declaredPolicies", JsonValue::array(declared_policies)),
            ("toolCount", JsonValue::number(0)),
            ("annotatedToolCount", JsonValue::number(0)),
            ("unannotatedToolCount", JsonValue::number(0)),
            ("advisoryRiskToolCount", JsonValue::number(0)),
            ("guardRecommendedToolCount", JsonValue::number(0)),
            ("policyCoveredToolCount", JsonValue::number(0)),
            ("unprotectedAdvisoryRiskToolCount", JsonValue::number(0)),
            ("unprotectedGuardRecommendedToolCount", JsonValue::number(0)),
            ("unknownSemanticsToolCount", JsonValue::number(0)),
            ("reviewRecommendedToolCount", JsonValue::number(0)),
            ("tools", JsonValue::array([])),
            ("error", JsonValue::string(error)),
        ]),
    }
}

fn server_inventory_item(root_path: &Path, server: &UpstreamServerConfig) -> JsonValue {
    let (runtime_callable, resolved_command, command_error) =
        server_runtime_callable(root_path, server);
    let status = if !server.enabled {
        "disabled"
    } else if runtime_callable && server.source_type == "http" {
        "callable-http"
    } else if runtime_callable {
        "callable-stdio"
    } else {
        upstream_blocked_status(server)
    };
    let reason = if !server.enabled {
        server
            .disabled_reason
            .clone()
            .unwrap_or_else(|| "server is disabled by source or policy".to_string())
    } else if runtime_callable && server.source_type == "http" {
        "enabled plain HTTP server; list with upstream_tools and call with upstream_call"
            .to_string()
    } else if runtime_callable {
        "enabled stdio server; list with upstream_tools and call with upstream_call".to_string()
    } else if let Some(error) = command_error {
        error
    } else if server.source_type == "http" {
        "HTTP upstream is configured but not callable; use http:// for direct local/plain HTTP or bridge HTTPS through stdio"
            .to_string()
    } else if server.source_type == "legacy-sse" {
        "legacy HTTP+SSE upstreams are not forwarded directly; use a stdio compatibility adapter or migrate to Streamable HTTP"
            .to_string()
    } else {
        "server does not have a callable stdio command".to_string()
    };

    JsonValue::object([
        ("name", JsonValue::string(&server.name)),
        ("enabled", JsonValue::bool(server.enabled)),
        ("sourceType", JsonValue::string(&server.source_type)),
        ("runtimeCallable", JsonValue::bool(runtime_callable)),
        ("status", JsonValue::string(status)),
        ("reason", JsonValue::string(reason)),
        (
            "command",
            server
                .command
                .as_ref()
                .map(|value| JsonValue::string(redact_command(value)))
                .unwrap_or(JsonValue::Null),
        ),
        (
            "resolvedCommand",
            resolved_command
                .as_ref()
                .map(|value| JsonValue::string(value.display().to_string()))
                .unwrap_or(JsonValue::Null),
        ),
        ("argCount", JsonValue::number(server.args.len())),
        (
            "url",
            server
                .url
                .as_ref()
                .map(JsonValue::string)
                .unwrap_or(JsonValue::Null),
        ),
        (
            "cwd",
            server
                .cwd
                .as_ref()
                .map(|value| JsonValue::string(value.display().to_string()))
                .unwrap_or(JsonValue::Null),
        ),
    ])
}

fn batch_tool_call_error(
    server_name: &str,
    completed: usize,
    total: usize,
    error: impl std::fmt::Display,
) -> String {
    format!(
        "upstream server '{}' batch stopped at call {} of {} after {} completed call(s); earlier calls may already have side effects: {}",
        server_name,
        completed.saturating_add(1),
        total,
        completed,
        error
    )
}

fn validate_tool_call_result(
    server_name: &str,
    tool_name: &str,
    result: &JsonValue,
) -> Result<bool, String> {
    let object = result.as_object().ok_or_else(|| {
        format!(
            "upstream server '{}' tool '{}' returned a non-object tools/call result",
            server_name, tool_name
        )
    })?;
    if !matches!(object.get("content"), Some(JsonValue::Array(_))) {
        return Err(format!(
            "upstream server '{}' tool '{}' tools/call result must contain a content array",
            server_name, tool_name
        ));
    }
    match object.get("isError") {
        Some(JsonValue::Bool(value)) => Ok(*value),
        Some(_) => Err(format!(
            "upstream server '{}' tool '{}' returned a non-boolean isError value",
            server_name, tool_name
        )),
        None => Ok(false),
    }
}

fn negotiated_protocol_version(
    server_name: &str,
    initialize_result: &JsonValue,
) -> Result<String, String> {
    let version = json_helpers::string_at_path(initialize_result, &["protocolVersion"])
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            format!(
                "upstream server '{}' initialize result did not contain protocolVersion",
                server_name
            )
        })?;
    if !mcp::is_supported_protocol_version(version) {
        return Err(format!(
            "upstream server '{}' selected unsupported MCP protocol version '{}'",
            server_name, version
        ));
    }
    if !matches!(
        json_helpers::value_at_path(initialize_result, &["capabilities"]),
        Some(JsonValue::Object(_))
    ) {
        return Err(format!(
            "upstream server '{}' initialize result must contain a capabilities object",
            server_name
        ));
    }
    let server_info =
        json_helpers::object_at_path(initialize_result, &["serverInfo"]).ok_or_else(|| {
            format!(
                "upstream server '{}' initialize result must contain a serverInfo object",
                server_name
            )
        })?;
    for field in ["name", "version"] {
        if !server_info
            .get(field)
            .and_then(JsonValue::as_str)
            .map(str::trim)
            .map(|value| !value.is_empty())
            .unwrap_or(false)
        {
            return Err(format!(
                "upstream server '{}' initialize serverInfo must contain non-empty '{}'",
                server_name, field
            ));
        }
    }
    Ok(version.to_string())
}

fn empty_object() -> JsonValue {
    json_helpers::empty_object()
}

#[cfg(test)]
mod tests;
