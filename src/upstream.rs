use crate::hub::leases::{self, RuntimeLeaseAcquireResult, RuntimeLeaseRequest};
use crate::json::{parse_str, JsonValue};
use crate::json_helpers;
use crate::mcp_protocol as mcp;
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc::{self, Receiver},
    Arc, Mutex, OnceLock,
};
use std::thread;
use std::time::{Duration, Instant, UNIX_EPOCH};

const DEFAULT_TIMEOUT_MS: u64 = 120_000;
const DEFAULT_PROBE_TIMEOUT_MS: u64 = 30_000;
const TOOL_LIST_CACHE_TTL: Duration = Duration::from_secs(30);
const INITIALIZE_ID: i64 = 1;
const METHOD_ID: i64 = 2;

#[derive(Clone, Debug)]
struct UpstreamServerConfig {
    name: String,
    enabled: bool,
    source_type: String,
    command: Option<String>,
    args: Vec<String>,
    env: BTreeMap<String, String>,
    url: Option<String>,
    timeout_ms: u64,
}

struct RunningServer {
    child: Child,
    stdin: Box<dyn Write + Send>,
    stdout_rx: Receiver<String>,
    stderr_rx: Receiver<String>,
}

struct UpstreamLeaseGuard {
    root_path: PathBuf,
    lease_id: String,
    lease: JsonValue,
    released: bool,
    heartbeat: Option<LeaseHeartbeat>,
}

struct LeaseHeartbeat {
    stop: Arc<AtomicBool>,
    lost: Arc<AtomicBool>,
    renewal_count: Arc<AtomicUsize>,
    failure_count: Arc<AtomicUsize>,
    handle: Option<thread::JoinHandle<()>>,
}

enum UpstreamLeaseAttachment {
    Attached(UpstreamLeaseGuard),
}

struct UpstreamLeaseOutcome {
    attached: bool,
    lease_id: Option<String>,
    lease: JsonValue,
    released: bool,
    release: JsonValue,
    bypass_reason: Option<String>,
    heartbeat_started: bool,
    heartbeat_renewal_count: usize,
    heartbeat_lost: bool,
    heartbeat_failure_count: usize,
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
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct ToolListCacheKey {
    root_path: String,
    server_name: String,
    settings_modified_ms: u128,
    settings_len: u64,
    server_fingerprint: String,
}

#[derive(Clone, Debug)]
struct CachedToolList {
    stored_at: Instant,
    tools: JsonValue,
}

static TOOL_LIST_CACHE: OnceLock<Mutex<BTreeMap<ToolListCacheKey, CachedToolList>>> =
    OnceLock::new();

impl Drop for RunningServer {
    fn drop(&mut self) {
        let _ = self.stdin.flush();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for UpstreamLeaseGuard {
    fn drop(&mut self) {
        if !self.released {
            self.stop_heartbeat();
            let _ = leases::release_runtime_lease(&self.root_path, &self.lease_id);
            self.released = true;
        }
    }
}

impl UpstreamLeaseGuard {
    fn release(&mut self) -> Result<JsonValue, String> {
        if self.released {
            return Ok(JsonValue::Null);
        }
        self.stop_heartbeat();
        let release = leases::release_runtime_lease(&self.root_path, &self.lease_id)?;
        self.released = true;
        Ok(release)
    }

    fn stop_heartbeat(&mut self) {
        if let Some(mut heartbeat) = self.heartbeat.take() {
            heartbeat.stop.store(true, Ordering::SeqCst);
            if let Some(handle) = heartbeat.handle.take() {
                let _ = handle.join();
            }
        }
    }

    fn heartbeat_started(&self) -> bool {
        self.heartbeat.is_some()
    }

    fn heartbeat_renewal_count(&self) -> usize {
        self.heartbeat
            .as_ref()
            .map(|heartbeat| heartbeat.renewal_count.load(Ordering::SeqCst))
            .unwrap_or(0)
    }

    fn heartbeat_lost(&self) -> bool {
        self.heartbeat
            .as_ref()
            .map(|heartbeat| heartbeat.lost.load(Ordering::SeqCst))
            .unwrap_or(false)
    }

    fn heartbeat_failure_count(&self) -> usize {
        self.heartbeat
            .as_ref()
            .map(|heartbeat| heartbeat.failure_count.load(Ordering::SeqCst))
            .unwrap_or(0)
    }

    fn heartbeat_lost_flag(&self) -> Option<&AtomicBool> {
        self.heartbeat
            .as_ref()
            .map(|heartbeat| heartbeat.lost.as_ref())
    }
}

impl UpstreamLeaseAttachment {
    fn heartbeat_lost_flag(&self) -> Option<&AtomicBool> {
        match self {
            UpstreamLeaseAttachment::Attached(guard) => guard.heartbeat_lost_flag(),
        }
    }
}

pub fn configured_inventory(root_path: &Path) -> Result<JsonValue, String> {
    let servers = load_servers(root_path)?;
    let items = servers
        .values()
        .map(server_inventory_item)
        .collect::<Vec<_>>();
    let callable_stdio_count = servers
        .values()
        .filter(|server| server_runtime_callable(server).0)
        .count();

    Ok(JsonValue::object([
        ("ok", JsonValue::bool(true)),
        ("mode", JsonValue::string("inventory")),
        (
            "summary",
            JsonValue::string(
                "Use upstream_tools with a server name to list a configured stdio upstream; use upstream_call with server/tool/arguments for one call or upstream_batch for stateful sequences. Browser is callable when configured as the Agent Browser Protocol stdio MCP server; HTTP upstreams remain inventory-only.",
            ),
        ),
        ("stdioForwardingImplemented", JsonValue::bool(true)),
        ("callableConfiguredStdioServerCount", JsonValue::number(callable_stdio_count)),
        ("servers", JsonValue::array(items)),
    ]))
}

pub fn probe_servers(
    root_path: &Path,
    server_name: Option<&str>,
    timeout_ms: Option<u64>,
    refresh: bool,
) -> Result<JsonValue, String> {
    let servers = load_servers(root_path)?;
    let selected = server_name
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let selected_servers = select_servers(&servers, selected.as_deref());
    if let Some(name) = selected.as_deref() {
        if selected_servers.is_empty() {
            return Err(format!("upstream server '{}' is not configured", name));
        }
    }

    let results = run_server_tasks(
        root_path,
        selected_servers,
        timeout_ms,
        move |root, server, timeout| probe_server(root, server, timeout, refresh),
    );
    let ok_count = results
        .iter()
        .filter(|item| json_helpers::bool_at_path(item, &["ok"]).unwrap_or(false))
        .count();
    let skipped_count = results
        .iter()
        .filter(|item| json_helpers::string_at_path(item, &["status"]) == Some("disabled"))
        .count();
    let failed_count = results
        .len()
        .saturating_sub(ok_count)
        .saturating_sub(skipped_count);
    let (cache_hit_count, cache_miss_count) = catalog_cache_counts(&results);

    Ok(JsonValue::object([
        ("ok", JsonValue::bool(failed_count == 0)),
        ("mode", JsonValue::string("probe")),
        (
            "summary",
            JsonValue::string(
                "Probed configured upstream MCP servers from mcp_settings.json without hardcoded server names. Callable stdio servers use the short successful tools/list cache unless refresh=true is supplied; fresh probes launch the helper, request tools/list, and clean it up.",
            ),
        ),
        ("serverCount", JsonValue::number(results.len())),
        ("okCount", JsonValue::number(ok_count)),
        ("skippedCount", JsonValue::number(skipped_count)),
        ("failedCount", JsonValue::number(failed_count)),
        ("cacheHitCount", JsonValue::number(cache_hit_count)),
        ("cacheMissCount", JsonValue::number(cache_miss_count)),
        ("results", JsonValue::array(results)),
    ]))
}

pub fn catalog_tools(
    root_path: &Path,
    server_name: Option<&str>,
    timeout_ms: Option<u64>,
    refresh: bool,
) -> Result<JsonValue, String> {
    let started = Instant::now();
    let servers = load_servers(root_path)?;
    let selected = server_name
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let selected_servers = select_servers(&servers, selected.as_deref());
    if let Some(name) = selected.as_deref() {
        if selected_servers.is_empty() {
            return Err(format!("upstream server '{}' is not configured", name));
        }
    }

    let results = run_server_tasks(
        root_path,
        selected_servers,
        timeout_ms,
        move |root, server, timeout| catalog_server(root, server, timeout, refresh),
    );
    let ok_count = results
        .iter()
        .filter(|item| json_helpers::bool_at_path(item, &["ok"]).unwrap_or(false))
        .count();
    let skipped_count = results
        .iter()
        .filter(|item| json_helpers::string_at_path(item, &["status"]) == Some("disabled"))
        .count();
    let failed_count = results
        .len()
        .saturating_sub(ok_count)
        .saturating_sub(skipped_count);
    let tool_count = results
        .iter()
        .filter_map(|item| json_helpers::value_at_path(item, &["toolCount"]))
        .filter_map(JsonValue::as_i64)
        .sum::<i64>();
    let (cache_hit_count, cache_miss_count) = catalog_cache_counts(&results);
    let tools = flatten_catalog_tools(&results);

    Ok(JsonValue::object([
        ("ok", JsonValue::bool(failed_count == 0)),
        ("mode", JsonValue::string("catalog")),
        (
            "summary",
            JsonValue::string(
                "Discovered configured upstream MCP tools from mcp_settings.json and upstream tools/list responses without hardcoded server or tool names. The top-level tools array is a flat server-qualified catalog; use each call object with upstream_call or use upstream_batch for a stateful sequence.",
            ),
        ),
        ("serverCount", JsonValue::number(results.len())),
        ("okCount", JsonValue::number(ok_count)),
        ("skippedCount", JsonValue::number(skipped_count)),
        ("failedCount", JsonValue::number(failed_count)),
        ("toolCount", JsonValue::number(tool_count)),
        ("cacheHitCount", JsonValue::number(cache_hit_count)),
        ("cacheMissCount", JsonValue::number(cache_miss_count)),
        ("elapsedMs", JsonValue::number(started.elapsed().as_millis())),
        ("tools", JsonValue::array(tools)),
        ("servers", JsonValue::array(results)),
    ]))
}

fn catalog_cache_counts(results: &[JsonValue]) -> (usize, usize) {
    let mut hit_count = 0;
    let mut miss_count = 0;
    for item in results {
        if !json_helpers::bool_at_path(item, &["ok"]).unwrap_or(false) {
            continue;
        }
        if json_helpers::bool_at_path(item, &["cacheHit"]).unwrap_or(false) {
            hit_count += 1;
        } else {
            miss_count += 1;
        }
    }
    (hit_count, miss_count)
}

fn flatten_catalog_tools(results: &[JsonValue]) -> Vec<JsonValue> {
    let mut flattened = Vec::new();
    for server in results {
        if !json_helpers::bool_at_path(server, &["ok"]).unwrap_or(false) {
            continue;
        }
        let Some(server_name) = json_helpers::string_at_path(server, &["name"])
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        for tool in json_helpers::array_at_path(server, &["tools"]).unwrap_or(&[]) {
            let tool_name = json_helpers::string_at_path(tool, &["name"])
                .unwrap_or("<unnamed>")
                .to_string();
            let title = json_helpers::value_at_path(tool, &["title"])
                .cloned()
                .unwrap_or(JsonValue::Null);
            let description = json_helpers::string_at_path(tool, &["description"])
                .unwrap_or("")
                .to_string();
            flattened.push(JsonValue::object([
                ("server", JsonValue::string(server_name)),
                ("name", JsonValue::string(tool_name.clone())),
                (
                    "qualifiedName",
                    JsonValue::string(format!("{}.{}", server_name, tool_name)),
                ),
                ("title", title),
                ("description", JsonValue::string(description)),
                (
                    "call",
                    JsonValue::object([
                        ("tool", JsonValue::string("upstream_call")),
                        (
                            "arguments",
                            JsonValue::object([
                                ("server", JsonValue::string(server_name)),
                                ("tool", JsonValue::string(tool_name)),
                            ]),
                        ),
                    ]),
                ),
            ]));
        }
    }
    flattened
}

pub fn browser_status(root_path: &Path) -> Result<JsonValue, String> {
    let servers = load_servers(root_path)?;
    let Some(server) = servers.get("browser") else {
        return Ok(JsonValue::object([
            ("ok", JsonValue::bool(false)),
            ("server", JsonValue::string("browser")),
            ("status", JsonValue::string("missing")),
            ("runtimeCallable", JsonValue::bool(false)),
            (
                "reason",
                JsonValue::string("No browser server is configured in mcp_settings.json."),
            ),
        ]));
    };

    let (runtime_callable, resolved_command, command_error) = server_runtime_callable(server);
    if runtime_callable {
        return Ok(JsonValue::object([
            ("ok", JsonValue::bool(true)),
            ("server", JsonValue::string("browser")),
            ("enabled", JsonValue::bool(server.enabled)),
            ("sourceType", JsonValue::string(&server.source_type)),
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
            ("runtimeCallable", JsonValue::bool(true)),
            ("status", JsonValue::string("callable-stdio-abp")),
            (
                "reason",
                JsonValue::string(
                    "The browser entry is configured as an Agent Browser Protocol stdio MCP server. MCPace can list it with upstream_tools and call it with upstream_call. On Windows the helper process is launched with the no-console path; ABP_HEADLESS=0 intentionally uses a visible host browser window.",
                ),
            ),
            (
                "nextSafeAction",
                JsonValue::string(
                    "Call upstream_tools with server=browser, use upstream_call for browser_get_status, and use upstream_batch for stateful ABP sequences such as browser_navigate followed by browser_text/browser_action.",
                ),
            ),
        ]));
    }

    let (status, reason, next_safe_action) = if !server.enabled {
        (
            "disabled",
            "The browser entry exists but is disabled in mcp_settings.json.",
            "Enable the browser entry and configure it as stdio with npx agent-browser-protocol --mcp, or keep it disabled intentionally.",
        )
    } else if server.source_type == "http" {
        (
            "blocked-http-browser-bridge",
            "The configured browser entry is HTTP/host-bridge inventory. MCPace can report it, but this Rust HTTP adapter currently forwards stdio MCP upstreams only.",
            "Configure browser as stdio ABP (npx agent-browser-protocol --mcp) or add a real HTTP upstream proxy before using HTTP browser calls.",
        )
    } else {
        (
            "blocked-missing-browser-command",
            command_error
                .as_deref()
                .unwrap_or("The browser entry is not a callable stdio MCP server because it has no command or uses an unsupported transport."),
            "Configure browser with type=stdio, command=npx, and args for agent-browser-protocol --mcp.",
        )
    };

    Ok(JsonValue::object([
        ("ok", JsonValue::bool(false)),
        ("server", JsonValue::string("browser")),
        ("enabled", JsonValue::bool(server.enabled)),
        ("sourceType", JsonValue::string(&server.source_type)),
        (
            "url",
            server
                .url
                .as_ref()
                .map(JsonValue::string)
                .unwrap_or(JsonValue::Null),
        ),
        ("runtimeCallable", JsonValue::bool(false)),
        ("status", JsonValue::string(status)),
        ("reason", JsonValue::string(reason)),
        ("nextSafeAction", JsonValue::string(next_safe_action)),
    ]))
}

pub fn list_tools(
    root_path: &Path,
    server_name: Option<&str>,
    timeout_ms: Option<u64>,
    refresh: bool,
) -> Result<JsonValue, String> {
    let Some(server_name) = server_name.map(str::trim).filter(|value| !value.is_empty()) else {
        return configured_inventory(root_path);
    };
    let servers = load_servers(root_path)?;
    let server = servers
        .get(server_name)
        .ok_or_else(|| format!("upstream server '{}' is not configured", server_name))?;
    ensure_callable_stdio(server)?;

    let effective_timeout = timeout_for(server, timeout_ms);
    let (tools, cache_hit) = cached_tools_list(root_path, server, effective_timeout, refresh)?;
    let count = tools.as_array().map(|items| items.len()).unwrap_or(0);

    Ok(JsonValue::object([
        ("ok", JsonValue::bool(true)),
        ("server", JsonValue::string(server_name)),
        ("sourceType", JsonValue::string(&server.source_type)),
        (
            "timeoutMs",
            JsonValue::number(effective_timeout.as_millis()),
        ),
        ("cacheHit", JsonValue::bool(cache_hit)),
        (
            "cacheTtlMs",
            JsonValue::number(TOOL_LIST_CACHE_TTL.as_millis()),
        ),
        ("toolCount", JsonValue::number(count)),
        ("tools", tools),
    ]))
}

pub fn call_tool(
    root_path: &Path,
    server_name: &str,
    tool_name: &str,
    arguments: &JsonValue,
    timeout_ms: Option<u64>,
) -> Result<JsonValue, String> {
    call_tool_with_context(
        root_path,
        server_name,
        tool_name,
        arguments,
        timeout_ms,
        None,
    )
}

pub fn call_tool_with_context(
    root_path: &Path,
    server_name: &str,
    tool_name: &str,
    arguments: &JsonValue,
    timeout_ms: Option<u64>,
    context: Option<&UpstreamLeaseContext>,
) -> Result<JsonValue, String> {
    let server_name = server_name.trim();
    let tool_name = tool_name.trim();
    if server_name.is_empty() {
        return Err("upstream_call requires non-empty 'server'".to_string());
    }
    if tool_name.is_empty() {
        return Err("upstream_call requires non-empty 'tool'".to_string());
    }

    let servers = load_servers(root_path)?;
    let server = servers
        .get(server_name)
        .ok_or_else(|| format!("upstream server '{}' is not configured", server_name))?;
    ensure_callable_stdio(server)?;

    let effective_timeout = timeout_for(server, timeout_ms);
    let lease = acquire_upstream_lease(root_path, server_name, context, effective_timeout)?;
    let heartbeat_lost = lease.heartbeat_lost_flag();
    let result = run_stdio_request(
        root_path,
        server,
        "tools/call",
        Some(JsonValue::object([
            ("name", JsonValue::string(tool_name)),
            ("arguments", arguments.clone()),
        ])),
        effective_timeout,
        heartbeat_lost,
    )?;
    let lease_outcome = finalize_upstream_lease(lease)?;
    let upstream_is_error = json_helpers::bool_at_path(&result, &["isError"]).unwrap_or(false);
    let upstream_ok = !upstream_is_error;

    let mut entries = vec![
        ("ok".to_string(), JsonValue::bool(upstream_ok)),
        ("bridgeOk".to_string(), JsonValue::bool(true)),
        ("upstreamOk".to_string(), JsonValue::bool(upstream_ok)),
        (
            "upstreamIsError".to_string(),
            JsonValue::bool(upstream_is_error),
        ),
        ("server".to_string(), JsonValue::string(server_name)),
        ("tool".to_string(), JsonValue::string(tool_name)),
        (
            "timeoutMs".to_string(),
            JsonValue::number(effective_timeout.as_millis()),
        ),
        ("upstreamResult".to_string(), result),
    ];
    entries.extend(upstream_lease_entries(lease_outcome));
    Ok(JsonValue::object(entries))
}

pub fn call_tools(
    root_path: &Path,
    server_name: &str,
    calls: &[UpstreamToolCall],
    timeout_ms: Option<u64>,
) -> Result<JsonValue, String> {
    call_tools_with_context(root_path, server_name, calls, timeout_ms, None)
}

pub fn call_tools_with_context(
    root_path: &Path,
    server_name: &str,
    calls: &[UpstreamToolCall],
    timeout_ms: Option<u64>,
    context: Option<&UpstreamLeaseContext>,
) -> Result<JsonValue, String> {
    let server_name = server_name.trim();
    if server_name.is_empty() {
        return Err("upstream_batch requires non-empty 'server'".to_string());
    }
    if calls.is_empty() {
        return Err("upstream_batch requires at least one call".to_string());
    }

    let servers = load_servers(root_path)?;
    let server = servers
        .get(server_name)
        .ok_or_else(|| format!("upstream server '{}' is not configured", server_name))?;
    ensure_callable_stdio(server)?;

    let effective_timeout = timeout_for(server, timeout_ms);
    let lease = acquire_upstream_lease(root_path, server_name, context, effective_timeout)?;
    let heartbeat_lost = lease.heartbeat_lost_flag();
    let results =
        run_stdio_tool_calls(root_path, server, calls, effective_timeout, heartbeat_lost)?;
    let lease_outcome = finalize_upstream_lease(lease)?;
    let upstream_ok_count = results
        .iter()
        .filter(|item| json_helpers::bool_at_path(item, &["upstreamOk"]).unwrap_or(false))
        .count();
    let upstream_failed_count = results.len().saturating_sub(upstream_ok_count);
    let upstream_ok = upstream_failed_count == 0;

    let mut entries = vec![
        ("ok".to_string(), JsonValue::bool(upstream_ok)),
        ("bridgeOk".to_string(), JsonValue::bool(true)),
        ("upstreamOk".to_string(), JsonValue::bool(upstream_ok)),
        ("server".to_string(), JsonValue::string(server_name)),
        (
            "timeoutMs".to_string(),
            JsonValue::number(effective_timeout.as_millis()),
        ),
        ("callCount".to_string(), JsonValue::number(results.len())),
        (
            "upstreamOkCount".to_string(),
            JsonValue::number(upstream_ok_count),
        ),
        (
            "upstreamFailedCount".to_string(),
            JsonValue::number(upstream_failed_count),
        ),
        ("results".to_string(), JsonValue::array(results)),
    ];
    entries.extend(upstream_lease_entries(lease_outcome));
    Ok(JsonValue::object(entries))
}

fn acquire_upstream_lease(
    root_path: &Path,
    server_name: &str,
    context: Option<&UpstreamLeaseContext>,
    timeout: Duration,
) -> Result<UpstreamLeaseAttachment, String> {
    let effective_ttl_ms = context
        .and_then(|value| value.ttl_ms)
        .filter(|value| *value > 0)
        .unwrap_or_else(|| timeout.as_millis().saturating_add(5_000));
    let request = RuntimeLeaseRequest {
        server_name: server_name.to_string(),
        client_id: Some(
            context_string(context.and_then(|value| value.client_id.as_ref()))
                .unwrap_or_else(|| "mcpace-upstream-bridge".to_string()),
        ),
        session_id: context_string(context.and_then(|value| value.session_id.as_ref())),
        project_root: context_string(context.and_then(|value| value.project_root.as_ref()))
            .or_else(|| Some(child_process_path(root_path))),
        transport: Some(
            context_string(context.and_then(|value| value.transport.as_ref()))
                .unwrap_or_else(|| "stdio".to_string()),
        ),
        metadata_json: context
            .and_then(|value| value.metadata.as_ref())
            .map(JsonValue::to_compact_string),
        ttl_ms: Some(effective_ttl_ms),
    };

    match leases::acquire_runtime_lease(root_path, request)? {
        RuntimeLeaseAcquireResult::Acquired { lease_id, json } => {
            let lease = json_helpers::value_at_path(&json, &["lease"])
                .cloned()
                .unwrap_or_else(|| json.clone());
            let heartbeat = should_heartbeat_lease(effective_ttl_ms, timeout)
                .then(|| start_lease_heartbeat(root_path, &lease_id, effective_ttl_ms));
            Ok(UpstreamLeaseAttachment::Attached(UpstreamLeaseGuard {
                root_path: root_path.to_path_buf(),
                lease_id,
                lease,
                released: false,
                heartbeat,
            }))
        }
        RuntimeLeaseAcquireResult::Blocked { json } => Err(runtime_lease_blocked_error(
            server_name,
            json_helpers::string_at_path(&json, &["reason"])
                .unwrap_or("runtime lease acquisition was blocked"),
            &json,
        )),
    }
}

fn finalize_upstream_lease(
    attachment: UpstreamLeaseAttachment,
) -> Result<UpstreamLeaseOutcome, String> {
    match attachment {
        UpstreamLeaseAttachment::Attached(mut guard) => {
            let lease_id = guard.lease_id.clone();
            let lease = guard.lease.clone();
            let heartbeat_started = guard.heartbeat_started();
            let heartbeat_renewal_count = guard.heartbeat_renewal_count();
            let heartbeat_lost = guard.heartbeat_lost();
            let heartbeat_failure_count = guard.heartbeat_failure_count();
            let release = guard.release()?;
            Ok(UpstreamLeaseOutcome {
                attached: true,
                lease_id: Some(lease_id),
                lease,
                released: true,
                release,
                bypass_reason: None,
                heartbeat_started,
                heartbeat_renewal_count,
                heartbeat_lost,
                heartbeat_failure_count,
            })
        }
    }
}

fn upstream_lease_entries(outcome: UpstreamLeaseOutcome) -> Vec<(String, JsonValue)> {
    vec![
        (
            "leaseAttached".to_string(),
            JsonValue::bool(outcome.attached),
        ),
        (
            "leaseId".to_string(),
            optional_json_string(outcome.lease_id),
        ),
        ("lease".to_string(), outcome.lease),
        (
            "leaseReleased".to_string(),
            JsonValue::bool(outcome.released),
        ),
        ("leaseRelease".to_string(), outcome.release),
        (
            "leaseBypassReason".to_string(),
            optional_json_string(outcome.bypass_reason),
        ),
        (
            "leaseHeartbeatStarted".to_string(),
            JsonValue::bool(outcome.heartbeat_started),
        ),
        (
            "leaseHeartbeatRenewalCount".to_string(),
            JsonValue::number(outcome.heartbeat_renewal_count),
        ),
        (
            "leaseHeartbeatLost".to_string(),
            JsonValue::bool(outcome.heartbeat_lost),
        ),
        (
            "leaseHeartbeatFailureCount".to_string(),
            JsonValue::number(outcome.heartbeat_failure_count),
        ),
    ]
}

fn should_heartbeat_lease(ttl_ms: u128, timeout: Duration) -> bool {
    ttl_ms <= timeout.as_millis().saturating_add(1_000)
}

fn start_lease_heartbeat(root_path: &Path, lease_id: &str, ttl_ms: u128) -> LeaseHeartbeat {
    let stop = Arc::new(AtomicBool::new(false));
    let lost = Arc::new(AtomicBool::new(false));
    let renewal_count = Arc::new(AtomicUsize::new(0));
    let failure_count = Arc::new(AtomicUsize::new(0));
    let thread_stop = Arc::clone(&stop);
    let thread_lost = Arc::clone(&lost);
    let thread_renewal_count = Arc::clone(&renewal_count);
    let thread_failure_count = Arc::clone(&failure_count);
    let thread_root_path = root_path.to_path_buf();
    let thread_lease_id = lease_id.to_string();
    let interval = lease_heartbeat_interval(ttl_ms);

    let handle = thread::spawn(move || {
        while !thread_stop.load(Ordering::SeqCst) {
            sleep_interruptibly(interval, &thread_stop);
            if thread_stop.load(Ordering::SeqCst) {
                break;
            }
            match leases::renew_runtime_lease(&thread_root_path, &thread_lease_id, Some(ttl_ms)) {
                Ok(json) if json_helpers::string_at_path(&json, &["status"]) == Some("renewed") => {
                    thread_renewal_count.fetch_add(1, Ordering::SeqCst);
                }
                Ok(_) | Err(_) => {
                    thread_failure_count.fetch_add(1, Ordering::SeqCst);
                    thread_lost.store(true, Ordering::SeqCst);
                    break;
                }
            }
        }
    });

    LeaseHeartbeat {
        stop,
        lost,
        renewal_count,
        failure_count,
        handle: Some(handle),
    }
}

fn sleep_interruptibly(duration: Duration, stop: &AtomicBool) {
    let deadline = Instant::now() + duration;
    while !stop.load(Ordering::SeqCst) {
        let now = Instant::now();
        if now >= deadline {
            break;
        }
        thread::sleep(
            deadline
                .saturating_duration_since(now)
                .min(Duration::from_millis(25)),
        );
    }
}

fn lease_heartbeat_interval(ttl_ms: u128) -> Duration {
    let ttl_ms = u64::try_from(ttl_ms).unwrap_or(u64::MAX);
    let upper_bound = ttl_ms.saturating_sub(1).max(1);
    let interval_ms = (ttl_ms / 3).clamp(50, 30_000).min(upper_bound);
    Duration::from_millis(interval_ms)
}

fn runtime_lease_blocked_error(server_name: &str, reason: &str, json: &JsonValue) -> String {
    format!(
        "runtime lease blocked for upstream server '{}': {} | {}",
        server_name,
        reason,
        json.to_compact_string()
    )
}

fn runtime_lease_lost_error(
    server_name: &str,
    method: &str,
    stderr_rx: &Receiver<String>,
) -> String {
    format!(
        "runtime lease lost while waiting for upstream server '{}' response to {}; upstream process was cancelled before using a stale result{}",
        server_name,
        method,
        stderr_suffix(stderr_rx)
    )
}

fn context_string(value: Option<&String>) -> Option<String> {
    value
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn optional_json_string(value: Option<String>) -> JsonValue {
    match value {
        Some(value) => JsonValue::string(value),
        None => JsonValue::Null,
    }
}

fn load_servers(root_path: &Path) -> Result<BTreeMap<String, UpstreamServerConfig>, String> {
    let settings_path = root_path.join("mcp_settings.json");
    let value = json_helpers::read_json_file(&settings_path)?;
    let servers = json_helpers::object_at_path(&value, &["mcpServers"])
        .ok_or_else(|| format!("{} does not contain mcpServers", settings_path.display()))?;

    let mut parsed = BTreeMap::new();
    for (name, raw) in servers {
        let enabled = json_helpers::bool_at_path(raw, &["enabled"]).unwrap_or(true);
        let source_type = json_helpers::string_at_path(raw, &["type"])
            .unwrap_or("stdio")
            .to_string();
        let command = json_helpers::string_at_path(raw, &["command"])
            .map(|value| expand_template(value, root_path));
        let args = json_helpers::strings_from_array(json_helpers::array_at_path(raw, &["args"]))
            .into_iter()
            .map(|value| expand_template(&value, root_path))
            .collect::<Vec<_>>();
        let mut env_values = BTreeMap::new();
        if let Some(env_object) = json_helpers::object_at_path(raw, &["env"]) {
            for (key, value) in env_object {
                if let Some(text) = value.as_str() {
                    env_values.insert(key.clone(), expand_template(text, root_path));
                }
            }
        }
        let url = json_helpers::string_at_path(raw, &["url"]).map(str::to_string);
        let timeout_ms = json_helpers::value_at_path(raw, &["options", "timeout"])
            .and_then(JsonValue::as_i64)
            .or_else(|| {
                json_helpers::value_at_path(raw, &["initTimeout"]).and_then(JsonValue::as_i64)
            })
            .filter(|value| *value > 0)
            .map(|value| value as u64)
            .unwrap_or(DEFAULT_TIMEOUT_MS);

        parsed.insert(
            name.clone(),
            UpstreamServerConfig {
                name: name.clone(),
                enabled,
                source_type,
                command,
                args,
                env: env_values,
                url,
                timeout_ms,
            },
        );
    }

    Ok(parsed)
}

fn select_servers(
    servers: &BTreeMap<String, UpstreamServerConfig>,
    selected: Option<&str>,
) -> Vec<UpstreamServerConfig> {
    servers
        .values()
        .filter(|server| {
            selected
                .map(|name| server.name.eq_ignore_ascii_case(name))
                .unwrap_or(true)
        })
        .cloned()
        .collect()
}

fn run_server_tasks<F>(
    root_path: &Path,
    servers: Vec<UpstreamServerConfig>,
    timeout_ms: Option<u64>,
    task: F,
) -> Vec<JsonValue>
where
    F: Fn(&Path, &UpstreamServerConfig, Option<u64>) -> JsonValue + Copy + Send + 'static,
{
    if servers.len() <= 1 {
        return servers
            .iter()
            .map(|server| task(root_path, server, timeout_ms))
            .collect();
    }

    let handles = servers
        .into_iter()
        .map(|server| {
            let name = server.name.clone();
            let root_path = root_path.to_path_buf();
            (
                name,
                thread::spawn(move || task(&root_path, &server, timeout_ms)),
            )
        })
        .collect::<Vec<_>>();

    handles
        .into_iter()
        .map(|(name, handle)| {
            handle.join().unwrap_or_else(|_| {
                JsonValue::object([
                    ("name", JsonValue::string(name)),
                    ("ok", JsonValue::bool(false)),
                    ("status", JsonValue::string("worker-panicked")),
                    (
                        "error",
                        JsonValue::string("internal upstream discovery worker panicked"),
                    ),
                ])
            })
        })
        .collect()
}

fn cached_tools_list(
    root_path: &Path,
    server: &UpstreamServerConfig,
    timeout: Duration,
    refresh: bool,
) -> Result<(JsonValue, bool), String> {
    let key = tool_list_cache_key(root_path, server);
    if !refresh {
        if let Some(tools) = read_cached_tools(&key) {
            return Ok((tools, true));
        }
    }

    let result = run_stdio_request(root_path, server, "tools/list", None, timeout, None)?;
    let tools = json_helpers::value_at_path(&result, &["tools"])
        .cloned()
        .unwrap_or_else(|| JsonValue::array([]));
    write_cached_tools(key, tools.clone());
    Ok((tools, false))
}

fn read_cached_tools(key: &ToolListCacheKey) -> Option<JsonValue> {
    let cache = TOOL_LIST_CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut guard = cache.lock().ok()?;
    let entry = guard.get(key)?;
    if entry.stored_at.elapsed() <= TOOL_LIST_CACHE_TTL {
        return Some(entry.tools.clone());
    }
    guard.remove(key);
    None
}

fn write_cached_tools(key: ToolListCacheKey, tools: JsonValue) {
    let cache = TOOL_LIST_CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));
    if let Ok(mut guard) = cache.lock() {
        guard.insert(
            key,
            CachedToolList {
                stored_at: Instant::now(),
                tools,
            },
        );
    }
}

fn tool_list_cache_key(root_path: &Path, server: &UpstreamServerConfig) -> ToolListCacheKey {
    let settings_path = root_path.join("mcp_settings.json");
    let (settings_modified_ms, settings_len) = settings_metadata(&settings_path);
    ToolListCacheKey {
        root_path: cache_root_path(root_path),
        server_name: server.name.clone(),
        settings_modified_ms,
        settings_len,
        server_fingerprint: server_fingerprint(server),
    }
}

fn settings_metadata(settings_path: &Path) -> (u128, u64) {
    let Ok(metadata) = fs::metadata(settings_path) else {
        return (0, 0);
    };
    let modified_ms = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|value| value.as_millis())
        .unwrap_or(0);
    (modified_ms, metadata.len())
}

fn cache_root_path(root_path: &Path) -> String {
    root_path
        .canonicalize()
        .unwrap_or_else(|_| root_path.to_path_buf())
        .display()
        .to_string()
}

fn server_fingerprint(server: &UpstreamServerConfig) -> String {
    let env_values = server
        .env
        .iter()
        .map(|(key, value)| format!("{}={}", key, value))
        .collect::<Vec<_>>()
        .join("\u{1f}");
    format!(
        "protocol={}|enabled={}|type={}|command={}|args={}|env={}|url={}|timeout={}",
        mcp::CURRENT_PROTOCOL_VERSION,
        server.enabled,
        server.source_type,
        server.command.as_deref().unwrap_or_default(),
        server.args.join("\u{1f}"),
        env_values,
        server.url.as_deref().unwrap_or_default(),
        server.timeout_ms
    )
}

fn ensure_callable_stdio(server: &UpstreamServerConfig) -> Result<(), String> {
    let (runtime_callable, _resolved_command, command_error) = server_runtime_callable(server);
    if runtime_callable {
        return Ok(());
    }
    if !server.enabled {
        return Err(format!(
            "upstream server '{}' is disabled in mcp_settings.json",
            server.name
        ));
    }
    if server.source_type != "stdio" {
        return Err(format!(
            "upstream server '{}' uses '{}' transport. This MCPace bridge currently forwards stdio upstreams only; configure browser/host bridges as stdio or call browser_status/runtime_diagnostics for exact status.",
            server.name, server.source_type
        ));
    }
    if let Some(error) = command_error {
        return Err(error);
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

fn server_runtime_callable(
    server: &UpstreamServerConfig,
) -> (bool, Option<PathBuf>, Option<String>) {
    if !server.enabled {
        return (false, None, None);
    }
    if server.source_type != "stdio" {
        return (false, None, None);
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

    match resolve_command(command) {
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

fn run_stdio_request(
    root_path: &Path,
    server: &UpstreamServerConfig,
    method: &str,
    params: Option<JsonValue>,
    timeout: Duration,
    lease_lost: Option<&AtomicBool>,
) -> Result<JsonValue, String> {
    let mut running = spawn_stdio_server(root_path, server)?;
    let deadline = Instant::now() + timeout;

    write_jsonrpc(
        &mut running.stdin,
        JsonValue::object([
            ("jsonrpc", JsonValue::string("2.0")),
            ("id", JsonValue::number(INITIALIZE_ID)),
            ("method", JsonValue::string("initialize")),
            (
                "params",
                JsonValue::object([
                    (
                        "protocolVersion",
                        JsonValue::string(mcp::CURRENT_PROTOCOL_VERSION),
                    ),
                    ("capabilities", empty_object()),
                    (
                        "clientInfo",
                        JsonValue::object([
                            ("name", JsonValue::string("mcpace-upstream-bridge")),
                            ("version", JsonValue::string(env!("CARGO_PKG_VERSION"))),
                        ]),
                    ),
                ]),
            ),
        ]),
    )?;
    let _initialize_result = read_response(
        &running.stdout_rx,
        &running.stderr_rx,
        INITIALIZE_ID,
        deadline,
        &server.name,
        "initialize",
        lease_lost,
    )?;

    write_jsonrpc(
        &mut running.stdin,
        JsonValue::object([
            ("jsonrpc", JsonValue::string("2.0")),
            ("method", JsonValue::string("notifications/initialized")),
        ]),
    )?;

    let mut request_entries = vec![
        ("jsonrpc", JsonValue::string("2.0")),
        ("id", JsonValue::number(METHOD_ID)),
        ("method", JsonValue::string(method)),
    ];
    if let Some(params) = params {
        request_entries.push(("params", params));
    }
    write_jsonrpc(&mut running.stdin, JsonValue::object(request_entries))?;
    read_response(
        &running.stdout_rx,
        &running.stderr_rx,
        METHOD_ID,
        deadline,
        &server.name,
        method,
        lease_lost,
    )
}

fn run_stdio_tool_calls(
    root_path: &Path,
    server: &UpstreamServerConfig,
    calls: &[UpstreamToolCall],
    timeout: Duration,
    lease_lost: Option<&AtomicBool>,
) -> Result<Vec<JsonValue>, String> {
    let mut running = spawn_stdio_server(root_path, server)?;
    let deadline = Instant::now() + timeout;

    write_jsonrpc(
        &mut running.stdin,
        JsonValue::object([
            ("jsonrpc", JsonValue::string("2.0")),
            ("id", JsonValue::number(INITIALIZE_ID)),
            ("method", JsonValue::string("initialize")),
            (
                "params",
                JsonValue::object([
                    (
                        "protocolVersion",
                        JsonValue::string(mcp::CURRENT_PROTOCOL_VERSION),
                    ),
                    ("capabilities", empty_object()),
                    (
                        "clientInfo",
                        JsonValue::object([
                            ("name", JsonValue::string("mcpace-upstream-bridge")),
                            ("version", JsonValue::string(env!("CARGO_PKG_VERSION"))),
                        ]),
                    ),
                ]),
            ),
        ]),
    )?;
    let _initialize_result = read_response(
        &running.stdout_rx,
        &running.stderr_rx,
        INITIALIZE_ID,
        deadline,
        &server.name,
        "initialize",
        lease_lost,
    )?;

    write_jsonrpc(
        &mut running.stdin,
        JsonValue::object([
            ("jsonrpc", JsonValue::string("2.0")),
            ("method", JsonValue::string("notifications/initialized")),
        ]),
    )?;

    let mut results = Vec::new();
    for (index, call) in calls.iter().enumerate() {
        let request_id = METHOD_ID + index as i64;
        write_jsonrpc(
            &mut running.stdin,
            JsonValue::object([
                ("jsonrpc", JsonValue::string("2.0")),
                ("id", JsonValue::number(request_id)),
                ("method", JsonValue::string("tools/call")),
                (
                    "params",
                    JsonValue::object([
                        ("name", JsonValue::string(call.tool.clone())),
                        ("arguments", call.arguments.clone()),
                    ]),
                ),
            ]),
        )?;
        let result = read_response(
            &running.stdout_rx,
            &running.stderr_rx,
            request_id,
            deadline,
            &server.name,
            "tools/call",
            lease_lost,
        )?;
        let upstream_is_error = json_helpers::bool_at_path(&result, &["isError"]).unwrap_or(false);
        let upstream_ok = !upstream_is_error;
        results.push(JsonValue::object([
            ("index", JsonValue::number(index)),
            ("ok", JsonValue::bool(upstream_ok)),
            ("upstreamOk", JsonValue::bool(upstream_ok)),
            ("upstreamIsError", JsonValue::bool(upstream_is_error)),
            ("tool", JsonValue::string(call.tool.clone())),
            ("upstreamResult", result),
        ]));
    }

    Ok(results)
}

fn spawn_stdio_server(
    root_path: &Path,
    server: &UpstreamServerConfig,
) -> Result<RunningServer, String> {
    let command_name = server.command.as_deref().unwrap_or_default();
    let program = resolve_command(command_name).map_err(|error| {
        format!(
            "failed to resolve command '{}' for upstream server '{}': {}",
            command_name, server.name, error
        )
    })?;

    let mut command = Command::new(&program);
    command
        .args(&server.args)
        .current_dir(root_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("MCPACE_PRIMARY_WORKSPACE", child_process_path(root_path))
        .env(
            "MCPACE_MANAGER_DATA",
            child_process_path(&manager_data_path(root_path)),
        );
    for (key, value) in &server.env {
        command.env(key, value);
    }
    #[cfg(windows)]
    crate::windows_process::configure_no_window(&mut command);

    let mut child = command.spawn().map_err(|error| {
        format!(
            "failed to start upstream server '{}' with '{}': {}",
            server.name,
            program.display(),
            error
        )
    })?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| format!("upstream server '{}' stdin was unavailable", server.name))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| format!("upstream server '{}' stdout was unavailable", server.name))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| format!("upstream server '{}' stderr was unavailable", server.name))?;

    let (stdout_tx, stdout_rx) = mpsc::channel();
    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            match line {
                Ok(line) => {
                    if stdout_tx.send(line).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let (stderr_tx, stderr_rx) = mpsc::channel();
    thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            match line {
                Ok(line) => {
                    if stderr_tx.send(line).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    Ok(RunningServer {
        child,
        stdin: Box::new(stdin),
        stdout_rx,
        stderr_rx,
    })
}

fn write_jsonrpc(stdin: &mut dyn Write, message: JsonValue) -> Result<(), String> {
    writeln!(stdin, "{}", message.to_compact_string())
        .map_err(|error| format!("failed to write upstream JSON-RPC message: {}", error))?;
    stdin
        .flush()
        .map_err(|error| format!("failed to flush upstream JSON-RPC message: {}", error))
}

fn read_response(
    stdout_rx: &Receiver<String>,
    stderr_rx: &Receiver<String>,
    expected_id: i64,
    deadline: Instant,
    server_name: &str,
    method: &str,
    lease_lost: Option<&AtomicBool>,
) -> Result<JsonValue, String> {
    loop {
        if lease_lost
            .map(|value| value.load(Ordering::SeqCst))
            .unwrap_or(false)
        {
            return Err(runtime_lease_lost_error(server_name, method, stderr_rx));
        }
        let now = Instant::now();
        if now >= deadline {
            return Err(format!(
                "timed out waiting for upstream server '{}' response to {}.{}{}",
                server_name,
                method,
                format_expected_id(expected_id),
                stderr_suffix(stderr_rx)
            ));
        }
        let remaining = deadline.saturating_duration_since(now);
        match stdout_rx.recv_timeout(remaining.min(Duration::from_millis(250))) {
            Ok(line) => {
                if lease_lost
                    .map(|value| value.load(Ordering::SeqCst))
                    .unwrap_or(false)
                {
                    return Err(runtime_lease_lost_error(server_name, method, stderr_rx));
                }
                let trimmed = line.trim();
                if trimmed.is_empty() || !trimmed.starts_with('{') {
                    continue;
                }
                let message = match parse_str(trimmed) {
                    Ok(value) => value,
                    Err(_) => continue,
                };
                let id_matches = json_helpers::value_at_path(&message, &["id"])
                    .and_then(JsonValue::as_i64)
                    .map(|id| id == expected_id)
                    .unwrap_or(false);
                if !id_matches {
                    continue;
                }
                if lease_lost
                    .map(|value| value.load(Ordering::SeqCst))
                    .unwrap_or(false)
                {
                    return Err(runtime_lease_lost_error(server_name, method, stderr_rx));
                }
                if let Some(error) = json_helpers::value_at_path(&message, &["error"]) {
                    return Err(format!(
                        "upstream server '{}' returned JSON-RPC error for {}: {}{}",
                        server_name,
                        method,
                        error.to_compact_string(),
                        stderr_suffix(stderr_rx)
                    ));
                }
                return json_helpers::value_at_path(&message, &["result"])
                    .cloned()
                    .ok_or_else(|| {
                        format!(
                            "upstream server '{}' response to {} did not contain result{}",
                            server_name,
                            method,
                            stderr_suffix(stderr_rx)
                        )
                    });
            }
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(format!(
                    "upstream server '{}' closed stdout before responding to {}{}",
                    server_name,
                    method,
                    stderr_suffix(stderr_rx)
                ));
            }
        }
    }
}

fn format_expected_id(expected_id: i64) -> String {
    format!(" (id {})", expected_id)
}

fn stderr_suffix(stderr_rx: &Receiver<String>) -> String {
    let mut lines = Vec::new();
    while let Ok(line) = stderr_rx.try_recv() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            lines.push(trimmed.to_string());
        }
        if lines.len() >= 6 {
            break;
        }
    }
    if lines.is_empty() {
        String::new()
    } else {
        format!("; stderr: {}", lines.join(" | "))
    }
}

fn catalog_server(
    root_path: &Path,
    server: &UpstreamServerConfig,
    timeout_ms: Option<u64>,
    refresh: bool,
) -> JsonValue {
    let started = Instant::now();
    let (runtime_callable, resolved_command, command_error) = server_runtime_callable(server);
    let effective_timeout = probe_timeout_for(server, timeout_ms);
    if !runtime_callable {
        let status = if !server.enabled {
            "disabled"
        } else if server.source_type != "stdio" {
            "blocked-non-stdio"
        } else {
            "blocked-command-not-found"
        };
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
                command_error
                    .map(JsonValue::string)
                    .unwrap_or_else(|| JsonValue::string(status)),
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
    let (runtime_callable, resolved_command, command_error) = server_runtime_callable(server);
    let effective_timeout = probe_timeout_for(server, timeout_ms);
    if !runtime_callable {
        let status = if !server.enabled {
            "disabled"
        } else if server.source_type != "stdio" {
            "blocked-non-stdio"
        } else {
            "blocked-command-not-found"
        };
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
                command_error
                    .map(JsonValue::string)
                    .unwrap_or_else(|| JsonValue::string(status)),
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

fn server_inventory_item(server: &UpstreamServerConfig) -> JsonValue {
    let (runtime_callable, resolved_command, command_error) = server_runtime_callable(server);
    let status = if !server.enabled {
        "disabled"
    } else if runtime_callable {
        "callable-stdio"
    } else if server.source_type == "http" {
        "blocked-http-upstream"
    } else if server.source_type == "stdio" {
        "blocked-command-not-found"
    } else {
        "blocked-missing-command"
    };
    let reason = if !server.enabled {
        "server is disabled in mcp_settings.json".to_string()
    } else if runtime_callable {
        "enabled stdio server; list with upstream_tools and call with upstream_call".to_string()
    } else if let Some(error) = command_error {
        error
    } else if server.name == "browser" || server.source_type == "http" {
        "non-stdio browser/HTTP upstream fan-out is not implemented in this stdio bridge"
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
    ])
}

fn redact_command(command: &str) -> String {
    Path::new(command)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(command)
        .to_string()
}

fn expand_template(input: &str, root_path: &Path) -> String {
    let mut output = String::new();
    let mut rest = input;
    while let Some(start) = rest.find("${") {
        output.push_str(&rest[..start]);
        let after_start = &rest[start + 2..];
        let Some(end) = after_start.find('}') else {
            output.push_str(&rest[start..]);
            return output;
        };
        let expression = &after_start[..end];
        output.push_str(&resolve_placeholder(expression, root_path));
        rest = &after_start[end + 1..];
    }
    output.push_str(rest);
    output
}

fn resolve_placeholder(expression: &str, root_path: &Path) -> String {
    let (name, fallback) = expression
        .split_once(":-")
        .map(|(name, fallback)| (name, Some(fallback)))
        .unwrap_or((expression, None));
    match name {
        "MCPACE_PRIMARY_WORKSPACE" => child_process_path(root_path),
        "MCPACE_MANAGER_DATA" => child_process_path(&manager_data_path(root_path)),
        other => env::var(other)
            .ok()
            .or_else(|| fallback.map(str::to_string))
            .unwrap_or_default(),
    }
}

fn manager_data_path(root_path: &Path) -> PathBuf {
    root_path.join("data").join("runtime")
}

fn child_process_path(path: &Path) -> String {
    let value = path.display().to_string();
    if let Some(rest) = value.strip_prefix("\\\\?\\UNC\\") {
        return format!("\\\\{}", rest);
    }
    value.strip_prefix("\\\\?\\").unwrap_or(&value).to_string()
}

fn empty_object() -> JsonValue {
    JsonValue::object::<String, Vec<(String, JsonValue)>>(Vec::new())
}

fn resolve_command(command: &str) -> Result<PathBuf, String> {
    let command = command.trim();
    if command.is_empty() {
        return Err("empty command".to_string());
    }
    let raw = PathBuf::from(command);
    if (raw.components().count() > 1 || raw.extension().is_some()) && raw.exists() {
        return Ok(raw);
    }

    #[cfg(windows)]
    {
        if raw.extension().is_none() && raw.components().count() == 1 {
            if let Some(resolved) = resolve_windows_pathext(command) {
                return Ok(resolved);
            }
        }
    }

    which::which(command).map_err(|error| error.to_string())
}

#[cfg(windows)]
fn resolve_windows_pathext(command: &str) -> Option<PathBuf> {
    let path_var = env::var_os("PATH")?;
    let pathext = env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string());
    let extensions = pathext
        .split(';')
        .filter(|item| !item.trim().is_empty())
        .map(|item| item.trim().to_string())
        .collect::<Vec<_>>();
    for directory in env::split_paths(&path_var) {
        for extension in &extensions {
            let candidate = directory.join(format!("{}{}", command, extension));
            if candidate.is_file() {
                return Some(candidate);
            }
            let lower_candidate =
                directory.join(format!("{}{}", command, extension.to_ascii_lowercase()));
            if lower_candidate.is_file() {
                return Some(lower_candidate);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root() -> PathBuf {
        let unique = format!(
            "mcpace-upstream-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let path = env::temp_dir().join(unique);
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn expands_workspace_and_fallback_placeholders() {
        let root = PathBuf::from(r"C:\workspace\project");
        let expanded = expand_template(
            "${MCPACE_PRIMARY_WORKSPACE}|${MCPACE_MANAGER_DATA}|${MISSING_TEST_ENV:-fallback}",
            &root,
        );
        assert!(expanded.contains(r"C:\workspace\project"));
        assert!(expanded.contains(r"data\runtime"));
        assert!(expanded.ends_with("fallback"));
    }

    #[test]
    fn child_process_paths_strip_windows_extended_prefixes() {
        let drive_root = PathBuf::from(r"\\?\C:\workspace\project");
        assert_eq!(
            expand_template("${MCPACE_PRIMARY_WORKSPACE}", &drive_root),
            r"C:\workspace\project"
        );

        let unc_root = PathBuf::from(r"\\?\UNC\server\share\project");
        assert_eq!(
            expand_template("${MCPACE_PRIMARY_WORKSPACE}", &unc_root),
            r"\\server\share\project"
        );
    }

    #[test]
    fn inventory_marks_stdio_callable_and_http_blocked() {
        let root = temp_root();
        let command = std::env::current_exe()
            .unwrap()
            .display()
            .to_string()
            .replace('\\', "\\\\");
        fs::write(
            root.join("mcp_settings.json"),
            r#"{
  "mcpServers": {
    "memory": { "enabled": true, "type": "stdio", "command": "__COMMAND__", "args": ["-y", "server"] },
    "browser": { "enabled": true, "type": "http", "url": "http://127.0.0.1:39022/mcp" },
    "off": { "enabled": false, "type": "stdio", "command": "uvx", "args": [] }
  }
}"#
            .replace("__COMMAND__", &command),
        )
        .unwrap();

        let inventory = configured_inventory(&root).expect("inventory");
        let servers = json_helpers::array_at_path(&inventory, &["servers"]).unwrap();
        let memory = servers
            .iter()
            .find(|item| json_helpers::string_at_path(item, &["name"]) == Some("memory"))
            .unwrap();
        let browser = servers
            .iter()
            .find(|item| json_helpers::string_at_path(item, &["name"]) == Some("browser"))
            .unwrap();
        assert_eq!(
            json_helpers::string_at_path(memory, &["status"]),
            Some("callable-stdio")
        );
        assert_eq!(
            json_helpers::string_at_path(browser, &["status"]),
            Some("blocked-http-upstream")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn browser_status_reports_stdio_abp_callable() {
        let root = temp_root();
        let command = std::env::current_exe()
            .unwrap()
            .display()
            .to_string()
            .replace('\\', "\\\\");
        fs::write(
            root.join("mcp_settings.json"),
            r#"{
  "mcpServers": {
    "browser": {
      "enabled": true,
      "type": "stdio",
      "command": "__COMMAND__",
      "args": ["-y", "agent-browser-protocol@0.1.10", "--mcp"],
      "env": { "ABP_HEADLESS": "0" }
    }
  }
}"#
            .replace("__COMMAND__", &command),
        )
        .unwrap();

        let status = browser_status(&root).expect("browser status");
        assert_eq!(json_helpers::bool_at_path(&status, &["ok"]), Some(true));
        assert_eq!(
            json_helpers::bool_at_path(&status, &["runtimeCallable"]),
            Some(true)
        );
        assert_eq!(
            json_helpers::string_at_path(&status, &["status"]),
            Some("callable-stdio-abp")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn inventory_and_probe_report_missing_future_server_commands() {
        let root = temp_root();
        fs::write(
            root.join("mcp_settings.json"),
            r#"{
  "mcpServers": {
    "future-tool": {
      "enabled": true,
      "type": "stdio",
      "command": "definitely-missing-mcpace-test-binary",
      "args": []
    }
  }
}"#,
        )
        .unwrap();

        let inventory = configured_inventory(&root).expect("inventory");
        assert_eq!(
            json_helpers::value_at_path(&inventory, &["callableConfiguredStdioServerCount"])
                .and_then(JsonValue::as_i64),
            Some(0)
        );
        let servers = json_helpers::array_at_path(&inventory, &["servers"]).unwrap();
        let future = servers
            .iter()
            .find(|item| json_helpers::string_at_path(item, &["name"]) == Some("future-tool"))
            .unwrap();
        assert_eq!(
            json_helpers::bool_at_path(future, &["runtimeCallable"]),
            Some(false)
        );
        assert_eq!(
            json_helpers::string_at_path(future, &["status"]),
            Some("blocked-command-not-found")
        );

        let probe = probe_servers(&root, None, Some(1_000), false).expect("probe");
        assert_eq!(json_helpers::bool_at_path(&probe, &["ok"]), Some(false));
        assert_eq!(
            json_helpers::value_at_path(&probe, &["failedCount"]).and_then(JsonValue::as_i64),
            Some(1)
        );
        assert_eq!(
            json_helpers::value_at_path(&probe, &["cacheHitCount"]).and_then(JsonValue::as_i64),
            Some(0)
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn probe_reuses_successful_tool_list_cache_for_callable_stdio_servers() {
        let root = temp_root();
        let command = std::env::current_exe()
            .unwrap()
            .display()
            .to_string()
            .replace('\\', "\\\\");
        fs::write(
            root.join("mcp_settings.json"),
            r#"{
  "mcpServers": {
    "cached-probe": { "enabled": true, "type": "stdio", "command": "__COMMAND__", "args": [] }
  }
}"#
            .replace("__COMMAND__", &command),
        )
        .unwrap();
        let servers = load_servers(&root).expect("servers");
        let server = servers.get("cached-probe").unwrap();
        write_cached_tools(
            tool_list_cache_key(&root, server),
            JsonValue::array([JsonValue::object([(
                "name",
                JsonValue::string("cached_probe_tool"),
            )])]),
        );

        let probe =
            probe_servers(&root, Some("cached-probe"), Some(1_000), false).expect("cached probe");

        assert_eq!(json_helpers::bool_at_path(&probe, &["ok"]), Some(true));
        assert_eq!(
            json_helpers::value_at_path(&probe, &["cacheHitCount"]).and_then(JsonValue::as_i64),
            Some(1)
        );
        let results = json_helpers::array_at_path(&probe, &["results"]).unwrap();
        assert_eq!(
            json_helpers::bool_at_path(&results[0], &["cacheHit"]),
            Some(true)
        );
        let tool_names = json_helpers::array_at_path(&results[0], &["toolNames"]).unwrap();
        assert_eq!(tool_names[0].as_str(), Some("cached_probe_tool"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn catalog_reports_arbitrary_configured_server_names_without_whitelist() {
        let root = temp_root();
        fs::write(
            root.join("mcp_settings.json"),
            r#"{
  "mcpServers": {
    "alpha-telemetry": {
      "enabled": true,
      "type": "stdio",
      "command": "definitely-missing-mcpace-test-binary",
      "args": []
    },
    "zeta-ops": {
      "enabled": false,
      "type": "stdio",
      "command": "also-missing-mcpace-test-binary",
      "args": []
    }
  }
}"#,
        )
        .unwrap();

        let catalog = catalog_tools(&root, None, Some(1_000), false).expect("catalog");
        assert_eq!(
            json_helpers::string_at_path(&catalog, &["mode"]),
            Some("catalog")
        );
        let servers = json_helpers::array_at_path(&catalog, &["servers"]).unwrap();
        assert!(servers
            .iter()
            .any(|item| json_helpers::string_at_path(item, &["name"]) == Some("alpha-telemetry")));
        assert!(servers
            .iter()
            .any(|item| json_helpers::string_at_path(item, &["name"]) == Some("zeta-ops")));

        let selected = catalog_tools(&root, Some("ALPHA-TELEMETRY"), Some(1_000), false)
            .expect("selected catalog");
        let selected_servers = json_helpers::array_at_path(&selected, &["servers"]).unwrap();
        assert_eq!(selected_servers.len(), 1);
        assert_eq!(
            json_helpers::string_at_path(&selected_servers[0], &["name"]),
            Some("alpha-telemetry")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn tool_list_cache_key_tracks_settings_metadata() {
        let root = temp_root();
        let settings_path = root.join("mcp_settings.json");
        fs::write(
            &settings_path,
            r#"{
  "mcpServers": {
    "alpha": { "enabled": true, "type": "stdio", "command": "node", "args": ["a"] }
  }
}"#,
        )
        .unwrap();
        let servers = load_servers(&root).expect("servers");
        let key_before = tool_list_cache_key(&root, servers.get("alpha").unwrap());

        fs::write(
            &settings_path,
            r#"{
  "mcpServers": {
    "alpha": { "enabled": true, "type": "stdio", "command": "node", "args": ["a", "b"] }
  }
}"#,
        )
        .unwrap();
        let servers = load_servers(&root).expect("updated servers");
        let key_after = tool_list_cache_key(&root, servers.get("alpha").unwrap());

        assert_ne!(key_before, key_after);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn tool_list_cache_returns_fresh_entries_and_drops_stale_entries() {
        let key = ToolListCacheKey {
            root_path: format!("test-root-{}", std::process::id()),
            server_name: "alpha-cache".to_string(),
            settings_modified_ms: 1,
            settings_len: 2,
            server_fingerprint: "fingerprint".to_string(),
        };
        let tools = JsonValue::array([JsonValue::object([(
            "name",
            JsonValue::string("cached_tool"),
        )])]);

        write_cached_tools(key.clone(), tools.clone());
        assert_eq!(read_cached_tools(&key), Some(tools));

        let cache = TOOL_LIST_CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));
        cache.lock().unwrap().insert(
            key.clone(),
            CachedToolList {
                stored_at: Instant::now() - TOOL_LIST_CACHE_TTL - Duration::from_millis(1),
                tools: JsonValue::array([JsonValue::string("stale")]),
            },
        );
        assert_eq!(read_cached_tools(&key), None);
    }

    #[test]
    fn flat_catalog_tools_include_server_and_upstream_call_arguments() {
        let results = vec![JsonValue::object([
            ("name", JsonValue::string("alpha")),
            ("ok", JsonValue::bool(true)),
            ("cacheHit", JsonValue::bool(true)),
            (
                "tools",
                JsonValue::array([JsonValue::object([
                    ("name", JsonValue::string("read_item")),
                    ("title", JsonValue::string("Read item")),
                    ("description", JsonValue::string("Read one item.")),
                ])]),
            ),
        ])];

        let tools = flatten_catalog_tools(&results);

        assert_eq!(tools.len(), 1);
        let tool = &tools[0];
        assert_eq!(
            json_helpers::string_at_path(tool, &["server"]),
            Some("alpha")
        );
        assert_eq!(
            json_helpers::string_at_path(tool, &["qualifiedName"]),
            Some("alpha.read_item")
        );
        assert_eq!(
            json_helpers::string_at_path(tool, &["call", "tool"]),
            Some("upstream_call")
        );
        assert_eq!(
            json_helpers::string_at_path(tool, &["call", "arguments", "server"]),
            Some("alpha")
        );
        assert_eq!(
            json_helpers::string_at_path(tool, &["call", "arguments", "tool"]),
            Some("read_item")
        );
    }

    #[test]
    fn catalog_cache_counts_ignore_failed_servers() {
        let results = vec![
            JsonValue::object([
                ("name", JsonValue::string("hit")),
                ("ok", JsonValue::bool(true)),
                ("cacheHit", JsonValue::bool(true)),
            ]),
            JsonValue::object([
                ("name", JsonValue::string("miss")),
                ("ok", JsonValue::bool(true)),
                ("cacheHit", JsonValue::bool(false)),
            ]),
            JsonValue::object([
                ("name", JsonValue::string("failed")),
                ("ok", JsonValue::bool(false)),
                ("cacheHit", JsonValue::bool(false)),
            ]),
        ];

        assert_eq!(catalog_cache_counts(&results), (1, 1));
    }

    #[test]
    fn tool_summary_uses_upstream_name_title_and_description() {
        let summary = tool_summary(&JsonValue::object([
            ("name", JsonValue::string("alpha_tool")),
            ("title", JsonValue::string("Alpha tool")),
            ("description", JsonValue::string("Short alpha description")),
            (
                "inputSchema",
                JsonValue::object([("type", JsonValue::string("object"))]),
            ),
        ]));

        assert_eq!(
            json_helpers::string_at_path(&summary, &["name"]),
            Some("alpha_tool")
        );
        assert_eq!(
            json_helpers::string_at_path(&summary, &["title"]),
            Some("Alpha tool")
        );
        assert_eq!(
            json_helpers::string_at_path(&summary, &["description"]),
            Some("Short alpha description")
        );
        assert!(json_helpers::value_at_path(&summary, &["inputSchema"]).is_none());
    }
}
