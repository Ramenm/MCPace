use crate::hub::leases::{self, RuntimeLeaseAcquireResult, RuntimeLeaseRequest};
use crate::json::{parse_str, JsonValue};
use crate::json_helpers;
use crate::mcp_protocol as mcp;
use crate::profile;
use crate::resources;
use std::collections::{hash_map::DefaultHasher, BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc::{self, Receiver},
    Arc, Condvar, Mutex, OnceLock,
};
use std::thread;
use std::time::{Duration, Instant, UNIX_EPOCH};

const DEFAULT_TIMEOUT_MS: u64 = 120_000;
const DEFAULT_PROBE_TIMEOUT_MS: u64 = 30_000;
const TOOL_LIST_CACHE_TTL: Duration = Duration::from_secs(30);
const TOOL_LIST_CACHE_MAX_ENTRIES: usize = 128;
const UPSTREAM_SESSION_IDLE_TTL: Duration = Duration::from_secs(300);
const STDERR_DIAGNOSTIC_MAX_LINES: usize = 6;
const STDERR_DIAGNOSTIC_MAX_CHARS_PER_LINE: usize = 320;
const DIAGNOSTIC_REDACTION: &str = "<redacted>";
const INITIALIZE_ID: i64 = 1;
const METHOD_ID: i64 = 2;

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
    cwd: Option<PathBuf>,
    url: Option<String>,
    timeout_ms: u64,
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
    tool_policies: Vec<ToolRiskPolicy>,
}

struct RunningServer {
    child: Child,
    stdin: Box<dyn Write + Send>,
    stdout_rx: Receiver<String>,
    stderr_rx: Receiver<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct UpstreamSessionKey {
    root_path: String,
    server_name: String,
    settings_modified_ms: u128,
    settings_len: u64,
    server_fingerprint: String,
    client_id: String,
    session_id: String,
    project_root: String,
    transport: String,
    metadata_fingerprint: String,
}

struct PooledUpstreamSession {
    running: RunningServer,
    created_at: Instant,
    last_used: Instant,
    next_request_id: i64,
    call_count: usize,
}

#[derive(Clone, Debug)]
struct UpstreamPoolCallOutcome {
    enabled: bool,
    hit: bool,
    session_call_count: usize,
    session_age_ms: u128,
    pool_size: usize,
    max_pool_size: usize,
    idle_ttl_ms: u128,
    evicted_idle_count: usize,
    evicted_capacity_count: usize,
}

struct UpstreamPoolInvocation<'a> {
    root_path: &'a Path,
    server: &'a UpstreamServerConfig,
    key: UpstreamSessionKey,
    timeout: Duration,
    lease_lost: Option<&'a AtomicBool>,
}

pub struct UpstreamSessionPool {
    sessions: BTreeMap<UpstreamSessionKey, PooledUpstreamSession>,
    max_sessions: usize,
}

impl Default for UpstreamSessionPool {
    fn default() -> Self {
        Self::with_max_sessions(max_pooled_upstream_sessions())
    }
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
    pub allow_arguments: BTreeSet<String>,
    pub allowed_tool_risk_classes: BTreeSet<String>,
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
static TOOL_LIST_INFLIGHT: OnceLock<(Mutex<BTreeSet<ToolListCacheKey>>, Condvar)> = OnceLock::new();

impl Drop for RunningServer {
    fn drop(&mut self) {
        let _ = self.stdin.flush();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl RunningServer {
    fn has_exited(&mut self) -> bool {
        match self.child.try_wait() {
            Ok(Some(_)) | Err(_) => true,
            Ok(None) => false,
        }
    }
}

impl PooledUpstreamSession {
    fn new(
        root_path: &Path,
        server: &UpstreamServerConfig,
        timeout: Duration,
        lease_lost: Option<&AtomicBool>,
    ) -> Result<Self, String> {
        let mut running = spawn_stdio_server(root_path, server)?;
        let deadline = Instant::now() + timeout;

        initialize_running_server(&mut running, server, deadline, lease_lost)?;

        let now = Instant::now();
        Ok(Self {
            running,
            created_at: now,
            last_used: now,
            next_request_id: METHOD_ID,
            call_count: 0,
        })
    }

    fn next_request_id(&mut self) -> i64 {
        let request_id = self.next_request_id;
        self.next_request_id = self.next_request_id.saturating_add(1);
        if self.next_request_id <= INITIALIZE_ID {
            self.next_request_id = METHOD_ID;
        }
        request_id
    }

    fn call_tool(
        &mut self,
        server: &UpstreamServerConfig,
        tool_name: &str,
        arguments: &JsonValue,
        deadline: Instant,
        lease_lost: Option<&AtomicBool>,
    ) -> Result<JsonValue, String> {
        let request_id = self.next_request_id();
        write_jsonrpc(
            &mut self.running.stdin,
            JsonValue::object([
                ("jsonrpc", JsonValue::string("2.0")),
                ("id", JsonValue::number(request_id)),
                ("method", JsonValue::string("tools/call")),
                (
                    "params",
                    JsonValue::object([
                        ("name", JsonValue::string(tool_name)),
                        ("arguments", arguments.clone()),
                    ]),
                ),
            ]),
        )?;
        let result = read_response(
            &self.running.stdout_rx,
            &self.running.stderr_rx,
            request_id,
            deadline,
            &server.name,
            "tools/call",
            lease_lost,
        )?;
        self.call_count = self.call_count.saturating_add(1);
        self.last_used = Instant::now();
        Ok(result)
    }

    fn call_tools(
        &mut self,
        server: &UpstreamServerConfig,
        calls: &[UpstreamToolCall],
        deadline: Instant,
        lease_lost: Option<&AtomicBool>,
    ) -> Result<Vec<JsonValue>, String> {
        let mut results = Vec::new();
        for (index, call) in calls.iter().enumerate() {
            let request_id = self.next_request_id();
            write_jsonrpc(
                &mut self.running.stdin,
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
                &self.running.stdout_rx,
                &self.running.stderr_rx,
                request_id,
                deadline,
                &server.name,
                "tools/call",
                lease_lost,
            )?;
            let upstream_is_error =
                json_helpers::bool_at_path(&result, &["isError"]).unwrap_or(false);
            let upstream_ok = !upstream_is_error;
            results.push(JsonValue::object([
                ("index", JsonValue::number(index)),
                ("ok", JsonValue::bool(upstream_ok)),
                ("upstreamOk", JsonValue::bool(upstream_ok)),
                ("upstreamIsError", JsonValue::bool(upstream_is_error)),
                ("tool", JsonValue::string(call.tool.clone())),
                ("upstreamResult", result),
            ]));
            self.call_count = self.call_count.saturating_add(1);
            self.last_used = Instant::now();
        }
        Ok(results)
    }
}

impl UpstreamSessionPool {
    pub fn with_max_sessions(max_sessions: usize) -> Self {
        Self {
            sessions: BTreeMap::new(),
            max_sessions: max_sessions.max(1),
        }
    }

    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    pub fn max_session_count(&self) -> usize {
        self.max_sessions
    }

    pub fn idle_ttl_ms(&self) -> u128 {
        UPSTREAM_SESSION_IDLE_TTL.as_millis()
    }

    fn call_tool(
        &mut self,
        invocation: UpstreamPoolInvocation<'_>,
        tool_name: &str,
        arguments: &JsonValue,
    ) -> Result<(JsonValue, UpstreamPoolCallOutcome), String> {
        let deadline = Instant::now() + invocation.timeout;
        let key = invocation.key;
        let (evicted_idle_count, evicted_capacity_count) = self.prepare_for_key(&key);
        let hit = self.sessions.contains_key(&key);
        if !hit {
            let session = PooledUpstreamSession::new(
                invocation.root_path,
                invocation.server,
                invocation.timeout,
                invocation.lease_lost,
            )?;
            self.sessions.insert(key.clone(), session);
        }

        let call_result = self
            .sessions
            .get_mut(&key)
            .ok_or_else(|| "upstream session pool lost its session entry".to_string())?
            .call_tool(
                invocation.server,
                tool_name,
                arguments,
                deadline,
                invocation.lease_lost,
            );

        match call_result {
            Ok(result) => {
                let session = self.sessions.get(&key).ok_or_else(|| {
                    "upstream session pool lost its completed session".to_string()
                })?;
                let outcome = UpstreamPoolCallOutcome {
                    enabled: true,
                    hit,
                    session_call_count: session.call_count,
                    session_age_ms: session.created_at.elapsed().as_millis(),
                    pool_size: self.sessions.len(),
                    max_pool_size: self.max_session_count(),
                    idle_ttl_ms: UPSTREAM_SESSION_IDLE_TTL.as_millis(),
                    evicted_idle_count,
                    evicted_capacity_count,
                };
                Ok((result, outcome))
            }
            Err(error) => {
                self.sessions.remove(&key);
                Err(error)
            }
        }
    }

    fn call_tools(
        &mut self,
        invocation: UpstreamPoolInvocation<'_>,
        calls: &[UpstreamToolCall],
    ) -> Result<(Vec<JsonValue>, UpstreamPoolCallOutcome), String> {
        let deadline = Instant::now() + invocation.timeout;
        let key = invocation.key;
        let (evicted_idle_count, evicted_capacity_count) = self.prepare_for_key(&key);
        let hit = self.sessions.contains_key(&key);
        if !hit {
            let session = PooledUpstreamSession::new(
                invocation.root_path,
                invocation.server,
                invocation.timeout,
                invocation.lease_lost,
            )?;
            self.sessions.insert(key.clone(), session);
        }

        let call_result = self
            .sessions
            .get_mut(&key)
            .ok_or_else(|| "upstream session pool lost its session entry".to_string())?
            .call_tools(invocation.server, calls, deadline, invocation.lease_lost);

        match call_result {
            Ok(results) => {
                let session = self.sessions.get(&key).ok_or_else(|| {
                    "upstream session pool lost its completed session".to_string()
                })?;
                let outcome = UpstreamPoolCallOutcome {
                    enabled: true,
                    hit,
                    session_call_count: session.call_count,
                    session_age_ms: session.created_at.elapsed().as_millis(),
                    pool_size: self.sessions.len(),
                    max_pool_size: self.max_session_count(),
                    idle_ttl_ms: UPSTREAM_SESSION_IDLE_TTL.as_millis(),
                    evicted_idle_count,
                    evicted_capacity_count,
                };
                Ok((results, outcome))
            }
            Err(error) => {
                self.sessions.remove(&key);
                Err(error)
            }
        }
    }

    fn prepare_for_key(&mut self, key: &UpstreamSessionKey) -> (usize, usize) {
        let now = Instant::now();
        let mut evicted_idle_count = 0usize;
        self.sessions.retain(|_, session| {
            let idle = now.duration_since(session.last_used) > UPSTREAM_SESSION_IDLE_TTL;
            let exited = session.running.has_exited();
            if idle || exited {
                evicted_idle_count = evicted_idle_count.saturating_add(1);
                false
            } else {
                true
            }
        });

        let mut evicted_capacity_count = 0usize;
        while !self.sessions.contains_key(key) && self.sessions.len() >= self.max_session_count() {
            let Some(oldest_key) = self
                .sessions
                .iter()
                .min_by_key(|(_, session)| session.last_used)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            self.sessions.remove(&oldest_key);
            evicted_capacity_count = evicted_capacity_count.saturating_add(1);
        }

        (evicted_idle_count, evicted_capacity_count)
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
        .map(|server| server_inventory_item(root_path, server))
        .collect::<Vec<_>>();
    let callable_stdio_count = servers
        .values()
        .filter(|server| server_runtime_callable(root_path, server).0)
        .count();

    Ok(JsonValue::object([
        ("ok", JsonValue::bool(true)),
        ("mode", JsonValue::string("inventory")),
        (
            "summary",
            JsonValue::string(
                "Use upstream_tools with a server name to list a configured stdio upstream; use upstream_call with server/tool/arguments for one call or upstream_batch for stateful sequences. HTTP upstream fan-out remains inventory-only until an HTTP forwarding adapter is configured.",
            ),
        ),
        ("stdioForwardingImplemented", JsonValue::bool(true)),
        ("callableConfiguredStdioServerCount", JsonValue::number(callable_stdio_count)),
        ("servers", JsonValue::array(items)),
    ]))
}

pub fn surface_manifest(
    root_path: &Path,
    transport: &str,
    top_level_tools: Vec<String>,
    include_live_catalog: bool,
    timeout_ms: Option<u64>,
    refresh: bool,
) -> Result<JsonValue, String> {
    let upstream_inventory = configured_inventory(root_path)?;
    let configured_server_count = json_helpers::array_at_path(&upstream_inventory, &["servers"])
        .map_or(0, |items| items.len());
    let callable_stdio_count =
        json_helpers::value_at_path(&upstream_inventory, &["callableConfiguredStdioServerCount"])
            .and_then(JsonValue::as_i64)
            .unwrap_or(0);

    let live_catalog = if include_live_catalog {
        Some(catalog_tools(root_path, None, timeout_ms, refresh)?)
    } else {
        None
    };
    let live_catalog_tool_count = live_catalog
        .as_ref()
        .and_then(|catalog| json_helpers::value_at_path(catalog, &["toolCount"]))
        .and_then(JsonValue::as_i64);

    Ok(JsonValue::object([
        ("ok", JsonValue::bool(true)),
        ("mode", JsonValue::string("surface-manifest")),
        (
            "summary",
            JsonValue::string(
                "Transparent MCPace surface contract: only MCPace management and wrapper tools are advertised as top-level MCP tools. Configured upstream tools are still real MCP tools, but they remain upstream and are discovered/called through explicit wrapper tools instead of being disguised as native MCPace tools.",
            ),
        ),
        (
            "transport",
            JsonValue::string(transport),
        ),
        (
            "configurationModel",
            JsonValue::object([
                ("name", JsonValue::string("bring-your-own-mcp-servers")),
                (
                    "serverSourceOfTruth",
                    JsonValue::string("mcp_settings.json.mcpServers"),
                ),
                (
                    "policyOverlay",
                    JsonValue::string(
                        "mcpace.config.json.servers is optional metadata for routing, concurrency, platform gates, required commands, and tool risk policies.",
                    ),
                ),
                (
                    "packagedDefaults",
                    JsonValue::object([
                        ("upstreamServersEnabled", JsonValue::bool(false)),
                        ("candidateRecommendations", JsonValue::bool(false)),
                        ("requiresHardcodedServerNames", JsonValue::bool(false)),
                    ]),
                ),
                ("arbitraryServerNames", JsonValue::bool(true)),
                ("requiresRecompileForNewServers", JsonValue::bool(false)),
                ("installsUpstreamPackages", JsonValue::bool(false)),
                (
                    "userInstallResponsibility",
                    JsonValue::string(
                        "Users install any upstream MCP server package or binary they want, then reference its command/url in their own mcp_settings.json.",
                    ),
                ),
                ("stdioAutoDiscovery", JsonValue::bool(true)),
                ("httpUpstreamForwardingImplemented", JsonValue::bool(false)),
            ]),
        ),
        (
            "topLevelTools",
            JsonValue::object([
                (
                    "claim",
                    JsonValue::string(
                        "These are the exact tool names returned by this MCPace endpoint's tools/list.",
                    ),
                ),
                ("count", JsonValue::number(top_level_tools.len())),
                (
                    "names",
                    JsonValue::array(top_level_tools.into_iter().map(JsonValue::string)),
                ),
            ]),
        ),
        (
            "upstreamTools",
            JsonValue::object([
                (
                    "claim",
                    JsonValue::string(
                        "MCPace can advertise configured upstream tools as projected native top-level tools when the live catalog fits the configured budget. Broker tools remain available for large catalogs, strict clients, and explicit routing.",
                    ),
                ),
                ("configuredServerCount", JsonValue::number(configured_server_count)),
                (
                    "callableConfiguredStdioServerCount",
                    JsonValue::number(callable_stdio_count),
                ),
                (
                    "liveCatalogIncluded",
                    JsonValue::bool(include_live_catalog),
                ),
                (
                    "liveCatalogToolCount",
                    live_catalog_tool_count
                        .map(JsonValue::number)
                        .unwrap_or(JsonValue::Null),
                ),
                (
                    "directTopLevelProjection",
                    JsonValue::object([
                        ("enabled", JsonValue::bool(true)),
                        ("default", JsonValue::string("auto")),
                        ("mode", JsonValue::string("budgeted-live-catalog")),
                        (
                            "reason",
                            JsonValue::string(
                                "Projection is decided from the live tools/list catalog, protocol-compatible tool schemas, and the MCPACE_TOOL_BUDGET/MCPACE_TOOL_EXPOSURE settings. Direct projected calls still go through MCPace leases and declarative tool policies.",
                            ),
                        ),
                    ]),
                ),
            ]),
        ),
        (
            "routingAndSafety",
            JsonValue::object([
                ("schedulerLeases", JsonValue::bool(true)),
                ("sessionAwareUpstreamBatch", JsonValue::bool(true)),
                ("configDrivenToolPolicies", JsonValue::bool(true)),
                ("autoPolicyWeakening", JsonValue::bool(false)),
                (
                    "policyStatement",
                    JsonValue::string(
                        "Policy suggestions are dry-run evidence only; runtime enforcement changes only when mcpace.config.json toolPolicies are updated.",
                    ),
                ),
            ]),
        ),
        (
            "recommendedFlow",
            JsonValue::array([
                JsonValue::string("surface_manifest"),
                JsonValue::string("upstream_probe"),
                JsonValue::string("upstream_policy_audit"),
                JsonValue::string("upstream_catalog"),
                JsonValue::string("upstream_tools(server)"),
                JsonValue::string("upstream_call or upstream_batch"),
            ]),
        ),
        ("upstreamInventory", upstream_inventory),
        (
            "liveCatalog",
            live_catalog.unwrap_or(JsonValue::Null),
        ),
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

pub fn audit_tool_policies(
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
        move |root, server, timeout| audit_server(root, server, timeout, refresh),
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
    let tool_count = sum_i64_at_path(&results, &["toolCount"]);
    let annotated_tool_count = sum_i64_at_path(&results, &["annotatedToolCount"]);
    let unannotated_tool_count = sum_i64_at_path(&results, &["unannotatedToolCount"]);
    let advisory_risk_tool_count = sum_i64_at_path(&results, &["advisoryRiskToolCount"]);
    let guard_recommended_tool_count = sum_i64_at_path(&results, &["guardRecommendedToolCount"]);
    let policy_covered_tool_count = sum_i64_at_path(&results, &["policyCoveredToolCount"]);
    let unprotected_advisory_risk_tool_count =
        sum_i64_at_path(&results, &["unprotectedAdvisoryRiskToolCount"]);
    let unprotected_guard_recommended_tool_count =
        sum_i64_at_path(&results, &["unprotectedGuardRecommendedToolCount"]);
    let unknown_semantics_tool_count = sum_i64_at_path(&results, &["unknownSemanticsToolCount"]);
    let review_recommended_tool_count = sum_i64_at_path(&results, &["reviewRecommendedToolCount"]);
    let (cache_hit_count, cache_miss_count) = catalog_cache_counts(&results);

    Ok(JsonValue::object([
        ("ok", JsonValue::bool(failed_count == 0)),
        (
            "policyOk",
            JsonValue::bool(failed_count == 0 && unprotected_guard_recommended_tool_count == 0),
        ),
        ("mode", JsonValue::string("policy-audit")),
        (
            "summary",
            JsonValue::string(
                "Audited configured upstream MCP tools against MCP ToolAnnotations plus explicit mcpace.config.json toolPolicies. Annotation and name-based findings are advisory; MCPace still enforces only declarative toolPolicies, because the MCP protocol does not standardize parallel-safety or mutation semantics for every server.",
            ),
        ),
        ("serverCount", JsonValue::number(results.len())),
        ("okCount", JsonValue::number(ok_count)),
        ("skippedCount", JsonValue::number(skipped_count)),
        ("failedCount", JsonValue::number(failed_count)),
        ("toolCount", JsonValue::number(tool_count)),
        ("annotatedToolCount", JsonValue::number(annotated_tool_count)),
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
        ("cacheHitCount", JsonValue::number(cache_hit_count)),
        ("cacheMissCount", JsonValue::number(cache_miss_count)),
        ("elapsedMs", JsonValue::number(started.elapsed().as_millis())),
        ("servers", JsonValue::array(results)),
    ]))
}

pub fn suggest_tool_policies(
    root_path: &Path,
    server_name: Option<&str>,
    timeout_ms: Option<u64>,
    refresh: bool,
) -> Result<JsonValue, String> {
    let started = Instant::now();
    let audit = audit_tool_policies(root_path, server_name, timeout_ms, refresh)?;
    let report = policy_suggestion_report(&audit);

    Ok(JsonValue::object([
        ("ok", JsonValue::bool(true)),
        ("mode", JsonValue::string("policy-suggest")),
        (
            "summary",
            JsonValue::string(
                "Generated declarative mcpace.config.json toolPolicies suggestions from live upstream tools/list, MCP ToolAnnotations, and generic name signals. Suggestions are safe to review and copy; runtime enforcement still only changes when the declarative config is updated.",
            ),
        ),
        ("auditOk", audit_value_or_null(&audit, &["ok"])),
        ("auditPolicyOk", audit_value_or_null(&audit, &["policyOk"])),
        ("auditServerCount", audit_value_or_null(&audit, &["serverCount"])),
        ("auditToolCount", audit_value_or_null(&audit, &["toolCount"])),
        (
            "suggestedPolicyCount",
            audit_value_or_null(&report, &["suggestedPolicyCount"]),
        ),
        (
            "suggestedToolCount",
            audit_value_or_null(&report, &["suggestedToolCount"]),
        ),
        (
            "unknownReviewToolCount",
            audit_value_or_null(&report, &["unknownReviewToolCount"]),
        ),
        (
            "autoApplySafety",
            JsonValue::string(
                "dry-run-by-design: MCPace can infer policy candidates, but it should not silently weaken or mutate project policy without an explicit config update path.",
            ),
        ),
        (
            "suggestions",
            audit_value_or_null(&report, &["suggestions"]),
        ),
        ("servers", audit_value_or_null(&report, &["servers"])),
        ("elapsedMs", JsonValue::number(started.elapsed().as_millis())),
    ]))
}

fn audit_value_or_null(value: &JsonValue, path: &[&str]) -> JsonValue {
    json_helpers::value_at_path(value, path)
        .cloned()
        .unwrap_or(JsonValue::Null)
}

#[derive(Default)]
struct PolicySuggestionBucket {
    server: String,
    risk_class: String,
    allow_argument: String,
    tools: BTreeSet<String>,
    evidence: BTreeSet<String>,
    confidence_score: u8,
}

fn policy_suggestion_report(audit: &JsonValue) -> JsonValue {
    let mut buckets: BTreeMap<(String, String), PolicySuggestionBucket> = BTreeMap::new();
    let mut unknown_by_server: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for server in json_helpers::array_at_path(audit, &["servers"]).unwrap_or(&[]) {
        let Some(server_name) = json_helpers::string_at_path(server, &["name"])
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        for tool in json_helpers::array_at_path(server, &["tools"]).unwrap_or(&[]) {
            let Some(tool_name) = json_helpers::string_at_path(tool, &["name"])
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                continue;
            };

            if json_helpers::string_at_path(tool, &["policyStatus"])
                == Some("review-unknown-semantics")
            {
                unknown_by_server
                    .entry(server_name.to_string())
                    .or_default()
                    .insert(tool_name.to_string());
            }

            let guard_recommended =
                json_helpers::bool_at_path(tool, &["guardRecommended"]).unwrap_or(false);
            let policy_covered =
                json_helpers::bool_at_path(tool, &["policyCovered"]).unwrap_or(false);
            if !guard_recommended || policy_covered {
                continue;
            }

            let classes = json_helpers::strings_from_array(json_helpers::array_at_path(
                tool,
                &["advisoryRiskClasses"],
            ));
            let Some(risk_class) = suggested_policy_risk_class(server_name, &classes) else {
                continue;
            };
            let key = (server_name.to_string(), risk_class.clone());
            let bucket = buckets
                .entry(key)
                .or_insert_with(|| PolicySuggestionBucket {
                    server: server_name.to_string(),
                    allow_argument: allow_argument_for_risk_class(&risk_class),
                    risk_class,
                    ..Default::default()
                });
            bucket.tools.insert(tool_name.to_string());
            for signal in json_helpers::strings_from_array(json_helpers::array_at_path(
                tool,
                &["advisorySignals"],
            )) {
                bucket.evidence.insert(signal.clone());
                bucket.confidence_score = bucket
                    .confidence_score
                    .max(policy_suggestion_signal_score(&signal));
            }
            if bucket.confidence_score == 0 {
                bucket.confidence_score = 1;
            }
        }
    }

    let suggestions = buckets
        .values()
        .map(policy_suggestion_to_json)
        .collect::<Vec<_>>();
    let suggested_tool_count = buckets
        .values()
        .map(|bucket| bucket.tools.len())
        .sum::<usize>();
    let unknown_review_tool_count = unknown_by_server.values().map(BTreeSet::len).sum::<usize>();
    let servers = policy_suggestion_servers(&buckets, &unknown_by_server);

    JsonValue::object([
        ("suggestedPolicyCount", JsonValue::number(suggestions.len())),
        (
            "suggestedToolCount",
            JsonValue::number(suggested_tool_count),
        ),
        (
            "unknownReviewToolCount",
            JsonValue::number(unknown_review_tool_count),
        ),
        ("suggestions", JsonValue::array(suggestions)),
        ("servers", JsonValue::array(servers)),
    ])
}

fn policy_suggestion_to_json(bucket: &PolicySuggestionBucket) -> JsonValue {
    let policy = JsonValue::object([
        (
            "tools",
            JsonValue::array(bucket.tools.iter().cloned().map(JsonValue::string)),
        ),
        ("riskClass", JsonValue::string(&bucket.risk_class)),
        ("allowArgument", JsonValue::string(&bucket.allow_argument)),
        (
            "description",
            JsonValue::string(policy_suggestion_description(
                &bucket.server,
                &bucket.risk_class,
            )),
        ),
    ]);
    JsonValue::object([
        ("server", JsonValue::string(&bucket.server)),
        (
            "applyPath",
            JsonValue::string(format!("servers.{}.toolPolicies", bucket.server)),
        ),
        (
            "confidence",
            JsonValue::string(policy_suggestion_confidence(bucket.confidence_score)),
        ),
        (
            "evidence",
            JsonValue::array(bucket.evidence.iter().cloned().map(JsonValue::string)),
        ),
        ("policy", policy),
    ])
}

fn policy_suggestion_servers(
    buckets: &BTreeMap<(String, String), PolicySuggestionBucket>,
    unknown_by_server: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<JsonValue> {
    let mut server_names = BTreeSet::new();
    server_names.extend(buckets.keys().map(|(server, _)| server.clone()));
    server_names.extend(unknown_by_server.keys().cloned());

    server_names
        .into_iter()
        .map(|server| {
            let suggestions = buckets
                .values()
                .filter(|bucket| bucket.server == server)
                .map(policy_suggestion_to_json)
                .collect::<Vec<_>>();
            let suggested_tool_count = suggestions
                .iter()
                .filter_map(|suggestion| {
                    json_helpers::array_at_path(suggestion, &["policy", "tools"])
                })
                .map(<[JsonValue]>::len)
                .sum::<usize>();
            let unknown_tools = unknown_by_server
                .get(&server)
                .map(|tools| {
                    tools
                        .iter()
                        .cloned()
                        .map(JsonValue::string)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            JsonValue::object([
                ("name", JsonValue::string(&server)),
                ("suggestedPolicyCount", JsonValue::number(suggestions.len())),
                (
                    "suggestedToolCount",
                    JsonValue::number(suggested_tool_count),
                ),
                (
                    "unknownReviewToolCount",
                    JsonValue::number(unknown_tools.len()),
                ),
                ("suggestions", JsonValue::array(suggestions)),
                ("unknownReviewTools", JsonValue::array(unknown_tools)),
            ])
        })
        .collect()
}

fn suggested_policy_risk_class(server_name: &str, classes: &[String]) -> Option<String> {
    for stable in [
        "interaction-control",
        "interaction-observation",
        "desktop-control",
        "desktop-observation",
        "system-control",
    ] {
        if classes.iter().any(|class| class == stable) {
            return Some(stable.to_string());
        }
    }

    if classes
        .iter()
        .any(|class| class == "mutation" || class == "not-readonly")
    {
        return Some(format!("{}-mutation", policy_slug(server_name)));
    }

    None
}

fn policy_slug(value: &str) -> String {
    let parts = value
        .trim()
        .to_ascii_lowercase()
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if parts.is_empty() {
        "upstream".to_string()
    } else {
        parts.join("-")
    }
}

fn allow_argument_for_risk_class(risk_class: &str) -> String {
    let mut output = String::from("allow");
    for part in risk_class
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
    {
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            output.push(first.to_ascii_uppercase());
            for character in chars {
                output.push(character.to_ascii_lowercase());
            }
        }
    }
    if output == "allow" {
        output.push_str("UpstreamRisk");
    }
    output
}

fn policy_suggestion_signal_score(signal: &str) -> u8 {
    if signal.starts_with("mcp.destructiveHint") || signal.starts_with("mcp.readOnlyHint=false") {
        3
    } else if signal.starts_with("name-token:") || signal.starts_with("name-pattern:") {
        2
    } else {
        1
    }
}

fn policy_suggestion_confidence(score: u8) -> &'static str {
    match score {
        3..=u8::MAX => "high",
        2 => "medium",
        _ => "low",
    }
}

fn policy_suggestion_description(server_name: &str, risk_class: &str) -> String {
    format!(
        "Suggested by upstream_policy_suggest from live tools/list annotations and name signals for server '{}'; review semantics, then keep this declarative '{}' guard if these tools mutate or control state.",
        server_name, risk_class
    )
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

fn sum_i64_at_path(items: &[JsonValue], path: &[&str]) -> i64 {
    items
        .iter()
        .filter_map(|item| json_helpers::value_at_path(item, path))
        .filter_map(JsonValue::as_i64)
        .sum()
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpstreamProjectionSafety {
    /// Project only tools that look read-only or otherwise low-risk from MCP annotations/policies.
    Safe,
    /// Project low-risk tools plus tools with unknown semantics; keep policy-guarded tools hidden.
    Review,
    /// Project every callable upstream tool. Runtime policy checks still apply at call time.
    All,
}

impl UpstreamProjectionSafety {
    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().replace('_', "-").as_str() {
            "all" | "unsafe" | "maximum" | "max" => Self::All,
            "review" | "unknown" | "balanced" => Self::Review,
            _ => Self::Safe,
        }
    }
}

pub fn decode_projected_tool_name(name: &str) -> Option<(String, String)> {
    let rest = name.strip_prefix("u_")?;
    let (server, tool) = rest.split_once('_')?;
    let server = decode_projected_component(server)?;
    let tool = decode_projected_component(tool)?;
    if server.trim().is_empty() || tool.trim().is_empty() {
        return None;
    }
    Some((server, tool))
}

pub fn projected_tool_catalog(
    root_path: &Path,
    timeout_ms: Option<u64>,
    refresh: bool,
    safety: UpstreamProjectionSafety,
    max_tools: Option<usize>,
) -> Result<JsonValue, String> {
    let started = Instant::now();
    let servers = load_servers(root_path)?;
    let mut projected = Vec::new();
    let mut server_count = 0usize;
    let mut ok_server_count = 0usize;
    let mut skipped_server_count = 0usize;
    let mut raw_tool_count = 0usize;
    let mut projected_tool_count = 0usize;
    let mut skipped_guarded_count = 0usize;
    let mut skipped_unknown_count = 0usize;
    let mut skipped_name_count = 0usize;
    let mut truncated = false;

    for server in servers.values() {
        server_count = server_count.saturating_add(1);
        let (runtime_callable, _, _) = server_runtime_callable(root_path, server);
        if !runtime_callable {
            skipped_server_count = skipped_server_count.saturating_add(1);
            continue;
        }
        let effective_timeout = probe_timeout_for(server, timeout_ms);
        let tools = match cached_tools_list(root_path, server, effective_timeout, refresh) {
            Ok((tools, _)) => tools,
            Err(_) => {
                skipped_server_count = skipped_server_count.saturating_add(1);
                continue;
            }
        };
        ok_server_count = ok_server_count.saturating_add(1);
        for tool in tools.as_array().unwrap_or(&[]) {
            raw_tool_count = raw_tool_count.saturating_add(1);
            let Some(original_tool_name) = json_helpers::string_at_path(tool, &["name"])
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                skipped_name_count = skipped_name_count.saturating_add(1);
                continue;
            };

            let audit = audit_tool(server, tool);
            let policy_covered =
                json_helpers::bool_at_path(&audit.value, &["policyCovered"]).unwrap_or(false);
            let guard_recommended =
                json_helpers::bool_at_path(&audit.value, &["guardRecommended"]).unwrap_or(false);
            let unknown_semantics =
                json_helpers::bool_at_path(&audit.value, &["unknownSemantics"]).unwrap_or(false);
            let should_skip = match safety {
                UpstreamProjectionSafety::All => false,
                UpstreamProjectionSafety::Review => policy_covered || guard_recommended,
                UpstreamProjectionSafety::Safe => {
                    policy_covered || guard_recommended || unknown_semantics
                }
            };
            if should_skip {
                if unknown_semantics {
                    skipped_unknown_count = skipped_unknown_count.saturating_add(1);
                } else {
                    skipped_guarded_count = skipped_guarded_count.saturating_add(1);
                }
                continue;
            }

            let Some(projected_name) = encode_projected_tool_name(&server.name, original_tool_name)
            else {
                skipped_name_count = skipped_name_count.saturating_add(1);
                continue;
            };
            if let Some(max_tools) = max_tools {
                if projected_tool_count >= max_tools {
                    truncated = true;
                    continue;
                }
            }
            projected.push(project_tool_definition(
                &server.name,
                original_tool_name,
                &projected_name,
                tool,
                &audit.value,
            ));
            projected_tool_count = projected_tool_count.saturating_add(1);
        }
    }

    Ok(JsonValue::object([
        ("ok", JsonValue::bool(true)),
        ("mode", JsonValue::string("projected-upstream-tools")),
        ("serverCount", JsonValue::number(server_count)),
        ("okServerCount", JsonValue::number(ok_server_count)),
        (
            "skippedServerCount",
            JsonValue::number(skipped_server_count),
        ),
        ("rawToolCount", JsonValue::number(raw_tool_count)),
        (
            "projectedToolCount",
            JsonValue::number(projected_tool_count),
        ),
        (
            "skippedGuardedToolCount",
            JsonValue::number(skipped_guarded_count),
        ),
        (
            "skippedUnknownToolCount",
            JsonValue::number(skipped_unknown_count),
        ),
        (
            "skippedNameToolCount",
            JsonValue::number(skipped_name_count),
        ),
        ("truncated", JsonValue::bool(truncated)),
        (
            "safety",
            JsonValue::string(match safety {
                UpstreamProjectionSafety::Safe => "safe",
                UpstreamProjectionSafety::Review => "review",
                UpstreamProjectionSafety::All => "all",
            }),
        ),
        (
            "maxTools",
            max_tools.map(JsonValue::number).unwrap_or(JsonValue::Null),
        ),
        (
            "elapsedMs",
            JsonValue::number(started.elapsed().as_millis()),
        ),
        ("tools", JsonValue::array(projected)),
    ]))
}

pub fn encode_projected_tool_name(server: &str, tool: &str) -> Option<String> {
    let server = encode_projected_component(server.trim());
    let tool = encode_projected_component(tool.trim());
    if server.is_empty() || tool.is_empty() {
        return None;
    }
    let name = format!("u_{}_{}", server, tool);
    if projected_tool_name_is_recommended_shape(&name) {
        Some(name)
    } else {
        None
    }
}

fn encode_projected_component(value: &str) -> String {
    value
        .as_bytes()
        .iter()
        .map(|byte| format!("{:02x}", byte))
        .collect::<String>()
}

fn decode_projected_component(value: &str) -> Option<String> {
    if value.is_empty() || !value.len().is_multiple_of(2) {
        return None;
    }
    let mut bytes = Vec::new();
    for index in (0..value.len()).step_by(2) {
        let byte = u8::from_str_radix(&value[index..index + 2], 16).ok()?;
        bytes.push(byte);
    }
    String::from_utf8(bytes).ok()
}

fn projected_tool_name_is_recommended_shape(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && !name.contains("__")
        && name.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '_' || character == '-'
        })
}

fn project_tool_definition(
    server_name: &str,
    original_tool_name: &str,
    projected_name: &str,
    tool: &JsonValue,
    audit: &JsonValue,
) -> JsonValue {
    let mut object = tool.as_object().cloned().unwrap_or_default();
    let title = json_helpers::string_at_path(tool, &["title"])
        .or_else(|| json_helpers::string_at_path(tool, &["name"]))
        .unwrap_or(original_tool_name)
        .to_string();
    let description = json_helpers::string_at_path(tool, &["description"])
        .unwrap_or("")
        .trim()
        .to_string();
    object.insert("name".to_string(), JsonValue::string(projected_name));
    object.insert(
        "title".to_string(),
        JsonValue::string(format!("{} · {}", server_name, title)),
    );
    object.insert(
        "description".to_string(),
        JsonValue::string(projected_description(
            server_name,
            original_tool_name,
            &description,
        )),
    );
    object
        .entry("inputSchema".to_string())
        .or_insert_with(default_tool_input_schema);
    let mut meta = json_helpers::object_at_path(tool, &["_meta"])
        .cloned()
        .unwrap_or_default();
    meta.insert(
        "mcpace/upstreamServer".to_string(),
        JsonValue::string(server_name),
    );
    meta.insert(
        "mcpace/upstreamTool".to_string(),
        JsonValue::string(original_tool_name),
    );
    meta.insert("mcpace/projected".to_string(), JsonValue::bool(true));
    meta.insert(
        "mcpace/policyStatus".to_string(),
        json_helpers::string_at_path(audit, &["policyStatus"])
            .map(JsonValue::string)
            .unwrap_or(JsonValue::Null),
    );
    object.insert("_meta".to_string(), JsonValue::Object(meta));
    JsonValue::Object(object)
}

fn projected_description(server_name: &str, original_tool_name: &str, description: &str) -> String {
    let prefix = format!(
        "Upstream MCP tool `{}` on server `{}` routed through MCPace. ",
        original_tool_name, server_name
    );
    if description.is_empty() {
        prefix
    } else {
        format!("{}{}", prefix, description)
    }
}

fn default_tool_input_schema() -> JsonValue {
    JsonValue::object([("type", JsonValue::string("object"))])
}

pub fn callable_server_names(root_path: &Path) -> Result<Vec<String>, String> {
    let servers = load_servers(root_path)?;
    Ok(servers
        .values()
        .filter(|server| server_runtime_callable(root_path, server).0)
        .map(|server| server.name.clone())
        .collect())
}

pub fn request_once(
    root_path: &Path,
    server_name: &str,
    method: &str,
    params: Option<JsonValue>,
    timeout_ms: Option<u64>,
) -> Result<JsonValue, String> {
    let server_name = server_name.trim();
    let method = method.trim();
    if server_name.is_empty() {
        return Err("upstream request requires a non-empty server name".to_string());
    }
    if method.is_empty() {
        return Err("upstream request requires a non-empty method".to_string());
    }
    let servers = load_servers(root_path)?;
    let server = find_server(&servers, server_name)
        .ok_or_else(|| format!("upstream server '{}' is not configured", server_name))?;
    ensure_callable_stdio(root_path, server)?;
    let effective_timeout = timeout_for(server, timeout_ms);
    run_stdio_request(root_path, server, method, params, effective_timeout, None)
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
    let server = find_server(&servers, server_name)
        .ok_or_else(|| format!("upstream server '{}' is not configured", server_name))?;
    ensure_callable_stdio(root_path, server)?;

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
    let server = find_server(&servers, server_name)
        .ok_or_else(|| format!("upstream server '{}' is not configured", server_name))?;
    ensure_callable_stdio(root_path, server)?;
    validate_upstream_tool_policy(server, tool_name, context)?;

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

pub fn call_tool_with_pooled_context(
    root_path: &Path,
    server_name: &str,
    tool_name: &str,
    arguments: &JsonValue,
    timeout_ms: Option<u64>,
    context: Option<&UpstreamLeaseContext>,
    pool: &Mutex<UpstreamSessionPool>,
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
    let server = find_server(&servers, server_name)
        .ok_or_else(|| format!("upstream server '{}' is not configured", server_name))?;
    ensure_callable_stdio(root_path, server)?;
    validate_upstream_tool_policy(server, tool_name, context)?;

    let effective_timeout = timeout_for(server, timeout_ms);
    let lease = acquire_upstream_lease(root_path, server_name, context, effective_timeout)?;
    let heartbeat_lost = lease.heartbeat_lost_flag();
    let pool_key = upstream_session_key(root_path, server, context);
    let (result, pool_outcome) = {
        let mut pool = pool
            .lock()
            .map_err(|_| "upstream session pool lock was poisoned".to_string())?;
        pool.call_tool(
            UpstreamPoolInvocation {
                root_path,
                server,
                key: pool_key,
                timeout: effective_timeout,
                lease_lost: heartbeat_lost,
            },
            tool_name,
            arguments,
        )?
    };
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
    entries.extend(upstream_pool_entries(pool_outcome));
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
    let server = find_server(&servers, server_name)
        .ok_or_else(|| format!("upstream server '{}' is not configured", server_name))?;
    ensure_callable_stdio(root_path, server)?;
    validate_upstream_batch_tool_policy(server, calls, context)?;

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

pub fn call_tools_with_pooled_context(
    root_path: &Path,
    server_name: &str,
    calls: &[UpstreamToolCall],
    timeout_ms: Option<u64>,
    context: Option<&UpstreamLeaseContext>,
    pool: &Mutex<UpstreamSessionPool>,
) -> Result<JsonValue, String> {
    let server_name = server_name.trim();
    if server_name.is_empty() {
        return Err("upstream_batch requires non-empty 'server'".to_string());
    }
    if calls.is_empty() {
        return Err("upstream_batch requires at least one call".to_string());
    }
    let servers = load_servers(root_path)?;
    let server = find_server(&servers, server_name)
        .ok_or_else(|| format!("upstream server '{}' is not configured", server_name))?;
    ensure_callable_stdio(root_path, server)?;
    validate_upstream_batch_tool_policy(server, calls, context)?;

    let effective_timeout = timeout_for(server, timeout_ms);
    let lease = acquire_upstream_lease(root_path, server_name, context, effective_timeout)?;
    let heartbeat_lost = lease.heartbeat_lost_flag();
    let pool_key = upstream_session_key(root_path, server, context);
    let (results, pool_outcome) = {
        let mut pool = pool
            .lock()
            .map_err(|_| "upstream session pool lock was poisoned".to_string())?;
        pool.call_tools(
            UpstreamPoolInvocation {
                root_path,
                server,
                key: pool_key,
                timeout: effective_timeout,
                lease_lost: heartbeat_lost,
            },
            calls,
        )?
    };
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
    entries.extend(upstream_pool_entries(pool_outcome));
    entries.extend(upstream_lease_entries(lease_outcome));
    Ok(JsonValue::object(entries))
}

pub fn tool_policy_info(
    root_path: &Path,
    server_name: &str,
    tool_name: &str,
) -> Result<JsonValue, String> {
    let server_name = server_name.trim();
    let tool_name = tool_name.trim();
    if server_name.is_empty() {
        return Err("tool_policy_info requires a non-empty server name".to_string());
    }
    if tool_name.is_empty() {
        return Err("tool_policy_info requires a non-empty tool name".to_string());
    }
    let servers = load_servers(root_path)?;
    let server = find_server(&servers, server_name)
        .ok_or_else(|| format!("upstream server '{}' is not configured", server_name))?;
    let mut policies = Vec::new();
    for policy in &server.tool_policies {
        if !policy.matches_tool(tool_name) {
            continue;
        }
        policies.push(JsonValue::object([
            (
                "riskClass",
                policy
                    .risk_class
                    .as_ref()
                    .map(|value| JsonValue::string(value.clone()))
                    .unwrap_or(JsonValue::Null),
            ),
            (
                "allowArgument",
                policy
                    .allow_argument
                    .as_ref()
                    .map(|value| JsonValue::string(value.clone()))
                    .unwrap_or(JsonValue::Null),
            ),
            (
                "description",
                policy
                    .description
                    .as_ref()
                    .map(|value| JsonValue::string(value.clone()))
                    .unwrap_or(JsonValue::Null),
            ),
        ]));
    }
    Ok(JsonValue::object([
        ("server", JsonValue::string(server_name)),
        ("tool", JsonValue::string(tool_name)),
        ("guardRequired", JsonValue::bool(!policies.is_empty())),
        ("policyCount", JsonValue::number(policies.len())),
        ("policies", JsonValue::array(policies)),
    ]))
}

fn validate_upstream_batch_tool_policy(
    server: &UpstreamServerConfig,
    calls: &[UpstreamToolCall],
    context: Option<&UpstreamLeaseContext>,
) -> Result<(), String> {
    for (index, call) in calls.iter().enumerate() {
        let tool_name = call.tool.trim();
        if tool_name.is_empty() {
            return Err(format!(
                "upstream_batch calls[{}] requires non-empty 'tool'",
                index
            ));
        }
        validate_upstream_tool_policy(server, tool_name, context)?;
    }
    Ok(())
}

fn validate_upstream_tool_policy(
    server: &UpstreamServerConfig,
    tool_name: &str,
    context: Option<&UpstreamLeaseContext>,
) -> Result<(), String> {
    for policy in &server.tool_policies {
        if !policy.matches_tool(tool_name) || policy.is_allowed(context) {
            continue;
        }
        return Err(policy.blocked_message(&server.name, tool_name));
    }

    Ok(())
}

pub fn collect_allow_arguments(arguments: &JsonValue) -> Result<BTreeSet<String>, String> {
    let mut values = BTreeSet::new();
    let Some(object) = arguments.as_object() else {
        return Ok(values);
    };

    for (key, value) in object {
        if !key.starts_with("allow") || key == "allowToolRiskClasses" {
            continue;
        }
        if key == "allowArguments" {
            collect_allow_argument_list(value, &mut values)?;
            continue;
        }
        match value {
            JsonValue::Bool(true) => {
                values.insert(key.clone());
            }
            JsonValue::Bool(false) | JsonValue::Null => {}
            _ => return Err(format!("{} must be a boolean", key)),
        }
    }

    Ok(values)
}

pub fn collect_allowed_tool_risk_classes(
    arguments: &JsonValue,
) -> Result<BTreeSet<String>, String> {
    match json_helpers::value_at_path(arguments, &["allowToolRiskClasses"]) {
        Some(JsonValue::Array(values)) => {
            let mut normalized = BTreeSet::new();
            for value in values {
                let Some(value) = value.as_str() else {
                    return Err("allowToolRiskClasses must be an array of strings".to_string());
                };
                let value = value.trim().to_ascii_lowercase();
                if !value.is_empty() {
                    normalized.insert(value);
                }
            }
            Ok(normalized)
        }
        Some(JsonValue::Null) | None => Ok(BTreeSet::new()),
        Some(_) => Err("allowToolRiskClasses must be an array of strings".to_string()),
    }
}

fn collect_allow_argument_list(
    value: &JsonValue,
    values: &mut BTreeSet<String>,
) -> Result<(), String> {
    match value {
        JsonValue::Array(items) => {
            for item in items {
                let Some(item) = item.as_str() else {
                    return Err("allowArguments must be an array of strings".to_string());
                };
                let item = item.trim();
                if !item.is_empty() {
                    values.insert(item.to_string());
                }
            }
            Ok(())
        }
        JsonValue::Null => Ok(()),
        _ => Err("allowArguments must be an array of strings".to_string()),
    }
}

impl ToolRiskPolicy {
    fn matches_tool(&self, tool_name: &str) -> bool {
        self.tools
            .iter()
            .any(|pattern| tool_pattern_matches(pattern, tool_name))
    }

    fn is_allowed(&self, context: Option<&UpstreamLeaseContext>) -> bool {
        let Some(context) = context else {
            return false;
        };

        self.allow_argument
            .as_ref()
            .map(|argument| context.allow_arguments.contains(argument))
            .unwrap_or(false)
            || self
                .risk_class
                .as_ref()
                .map(|risk_class| context.allowed_tool_risk_classes.contains(risk_class))
                .unwrap_or(false)
    }

    fn blocked_message(&self, server_name: &str, tool_name: &str) -> String {
        let mut grants = Vec::new();
        if let Some(argument) = &self.allow_argument {
            grants.push(format!("set {}=true", argument));
        }
        if let Some(risk_class) = &self.risk_class {
            grants.push(format!("include '{}' in allowToolRiskClasses", risk_class));
        }
        let grant_hint = if grants.is_empty() {
            "declare an explicit allow rule in mcpace.config.json".to_string()
        } else {
            grants.join(" or ")
        };
        let detail = self
            .description
            .as_ref()
            .map(|value| format!(" ({})", value))
            .unwrap_or_default();

        format!(
            "upstream server '{}' tool '{}' requires explicit risk authorization: {} on upstream_call/upstream_batch{}",
            server_name, tool_name, grant_hint, detail
        )
    }
}

fn tool_pattern_matches(pattern: &str, tool_name: &str) -> bool {
    let pattern = pattern.trim().to_ascii_lowercase();
    let tool_name = tool_name.trim().to_ascii_lowercase();
    if pattern.is_empty() {
        return false;
    }
    if pattern == "*" || pattern == tool_name {
        return true;
    }
    wildcard_match(&pattern, &tool_name)
}

fn wildcard_match(pattern: &str, value: &str) -> bool {
    if !pattern.contains('*') {
        return false;
    }

    let mut rest = value;
    let mut first = true;
    for segment in pattern.split('*').filter(|segment| !segment.is_empty()) {
        let Some(index) = rest.find(segment) else {
            return false;
        };
        if first && !pattern.starts_with('*') && index != 0 {
            return false;
        }
        rest = &rest[index + segment.len()..];
        first = false;
    }

    pattern.ends_with('*') || rest.is_empty()
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
        takeover: false,
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

fn upstream_pool_entries(outcome: UpstreamPoolCallOutcome) -> Vec<(String, JsonValue)> {
    vec![
        (
            "sessionPoolEnabled".to_string(),
            JsonValue::bool(outcome.enabled),
        ),
        ("sessionPoolHit".to_string(), JsonValue::bool(outcome.hit)),
        (
            "sessionPoolReused".to_string(),
            JsonValue::bool(outcome.hit),
        ),
        (
            "sessionPoolSessionCallCount".to_string(),
            JsonValue::number(outcome.session_call_count),
        ),
        (
            "sessionPoolSessionAgeMs".to_string(),
            JsonValue::number(outcome.session_age_ms),
        ),
        (
            "sessionPoolSize".to_string(),
            JsonValue::number(outcome.pool_size),
        ),
        (
            "sessionPoolMaxSize".to_string(),
            JsonValue::number(outcome.max_pool_size),
        ),
        (
            "sessionPoolIdleTtlMs".to_string(),
            JsonValue::number(outcome.idle_ttl_ms),
        ),
        (
            "sessionPoolEvictedIdleCount".to_string(),
            JsonValue::number(outcome.evicted_idle_count),
        ),
        (
            "sessionPoolEvictedCapacityCount".to_string(),
            JsonValue::number(outcome.evicted_capacity_count),
        ),
    ]
}

fn upstream_session_key(
    root_path: &Path,
    server: &UpstreamServerConfig,
    context: Option<&UpstreamLeaseContext>,
) -> UpstreamSessionKey {
    let settings_path = root_path.join("mcp_settings.json");
    let (settings_modified_ms, settings_len) = settings_metadata(&settings_path);
    UpstreamSessionKey {
        root_path: cache_root_path(root_path),
        server_name: server.name.clone(),
        settings_modified_ms,
        settings_len,
        server_fingerprint: server_fingerprint(server),
        client_id: context_string(context.and_then(|value| value.client_id.as_ref()))
            .unwrap_or_else(|| "mcpace-upstream-bridge".to_string()),
        session_id: context_string(context.and_then(|value| value.session_id.as_ref()))
            .unwrap_or_else(|| "anonymous-session".to_string()),
        project_root: context_string(context.and_then(|value| value.project_root.as_ref()))
            .unwrap_or_else(|| child_process_path(root_path)),
        transport: context_string(context.and_then(|value| value.transport.as_ref()))
            .unwrap_or_else(|| "stdio".to_string()),
        metadata_fingerprint: context
            .and_then(|value| value.metadata.as_ref())
            .map(JsonValue::to_compact_string)
            .unwrap_or_default(),
    }
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

fn normalize_policy_token(value: &str) -> String {
    value.trim().to_ascii_lowercase()
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
    let server_policies = load_upstream_server_policies(root_path)?;

    let mut parsed = BTreeMap::new();
    for (name, raw) in servers {
        let source_enabled = json_helpers::bool_at_path(raw, &["enabled"]).unwrap_or(true);
        let policy = server_policies.get(&name.trim().to_ascii_lowercase());
        let disabled_reason = if !source_enabled {
            Some("server is disabled in mcp_settings.json".to_string())
        } else if policy
            .map(|policy| !policy.platform_supported)
            .unwrap_or(false)
        {
            Some(format!(
                "server is not declared for the current platform '{}'",
                current_platform_alias()
            ))
        } else if policy
            .map(|policy| !policy.profile_enabled)
            .unwrap_or(false)
        {
            Some("server is disabled by the active MCPace runtime profile".to_string())
        } else {
            None
        };
        let enabled = disabled_reason.is_none();
        let command = json_helpers::string_at_path(raw, &["command"])
            .map(|value| expand_template(value, root_path));
        let args = json_helpers::strings_from_array(json_helpers::array_at_path(raw, &["args"]))
            .into_iter()
            .map(|value| expand_template(&value, root_path))
            .collect::<Vec<_>>();
        let url = json_helpers::string_at_path(raw, &["url"]).map(str::to_string);
        let source_type = infer_source_type(
            json_helpers::string_at_path(raw, &["type"]).unwrap_or(""),
            command.as_deref().unwrap_or(""),
            url.as_deref().unwrap_or(""),
        );
        let mut env_values = BTreeMap::new();
        if let Some(env_object) = json_helpers::object_at_path(raw, &["env"]) {
            for (key, value) in env_object {
                if let Some(text) = value.as_str() {
                    env_values.insert(key.clone(), expand_template(text, root_path));
                }
            }
        }
        for key in env_var_names_from_array(json_helpers::array_at_path(raw, &["env_vars"])) {
            if env_values.contains_key(&key) {
                continue;
            }
            if let Ok(value) = env::var(&key) {
                env_values.insert(key, value);
            }
        }
        let cwd = json_helpers::string_at_path(raw, &["cwd"])
            .map(|value| expand_template(value, root_path))
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from);
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
                disabled_reason,
                source_type,
                command,
                args,
                env: env_values,
                cwd,
                url,
                timeout_ms,
                tool_policies: policy
                    .map(|policy| policy.tool_policies.clone())
                    .unwrap_or_default(),
            },
        );
    }

    Ok(parsed)
}

fn infer_source_type(raw_source_type: &str, command: &str, url: &str) -> String {
    let normalized = normalize_source_type(raw_source_type);
    if !normalized.is_empty() {
        return normalized;
    }
    if !command.trim().is_empty() {
        "stdio".to_string()
    } else if !url.trim().is_empty() {
        "http".to_string()
    } else {
        "stdio".to_string()
    }
}

fn normalize_source_type(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "" => String::new(),
        "streamablehttp" | "streamable-http" | "http-stream" | "remote-http" | "remote-sse"
        | "remote" | "http" | "sse" => "http".to_string(),
        "stdio" | "local" | "local-stdio" | "local-command" | "command" => "stdio".to_string(),
        other => other.to_string(),
    }
}

fn env_var_names_from_array(value: Option<&[JsonValue]>) -> Vec<String> {
    value
        .unwrap_or(&[])
        .iter()
        .filter_map(|item| {
            if let Some(name) = item.as_str() {
                return Some(name.trim().to_string());
            }

            let object = item.as_object()?;
            let source = object
                .get("source")
                .and_then(JsonValue::as_str)
                .unwrap_or("local")
                .trim()
                .to_ascii_lowercase();
            if !source.is_empty() && source != "local" {
                return None;
            }
            object
                .get("name")
                .and_then(JsonValue::as_str)
                .map(|name| name.trim().to_string())
        })
        .filter(|name| !name.is_empty())
        .collect()
}

fn load_upstream_server_policies(
    root_path: &Path,
) -> Result<BTreeMap<String, UpstreamServerPolicy>, String> {
    let config_path = root_path.join("mcpace.config.json");
    if !config_path.is_file() {
        return Ok(BTreeMap::new());
    }

    let value = json_helpers::read_json_file(&config_path)?;
    let runtime_profile = profile::load_runtime_profile_selection(root_path)?;
    let mut policies = BTreeMap::new();
    let Some(servers) = json_helpers::object_at_path(&value, &["servers"]) else {
        return Ok(policies);
    };

    for (server_name, raw_server) in servers {
        let required = json_helpers::bool_at_path(raw_server, &["required"]).unwrap_or(false);
        let default_enabled =
            json_helpers::bool_at_path(raw_server, &["defaultEnabled"]).unwrap_or(false);
        let override_enabled = runtime_profile
            .server_overrides
            .get(&server_name.trim().to_ascii_lowercase())
            .copied();
        let profile_enabled = if required {
            true
        } else {
            override_enabled.unwrap_or(default_enabled)
        };
        let platform_supported =
            server_supports_current_platform(&json_helpers::strings_from_array(
                json_helpers::array_at_path(raw_server, &["platforms"]),
            ));
        let mut tool_policies = Vec::new();
        if let Some(raw_policies) = json_helpers::array_at_path(raw_server, &["toolPolicies"]) {
            for raw_policy in raw_policies {
                if let Some(policy) = parse_tool_policy(raw_policy) {
                    tool_policies.push(policy);
                }
            }
        }
        policies.insert(
            server_name.trim().to_ascii_lowercase(),
            UpstreamServerPolicy {
                profile_enabled,
                platform_supported,
                tool_policies,
            },
        );
    }

    Ok(policies)
}

fn parse_tool_policy(raw: &JsonValue) -> Option<ToolRiskPolicy> {
    let tools = json_helpers::strings_from_array(json_helpers::array_at_path(raw, &["tools"]))
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    if tools.is_empty() {
        return None;
    }

    let risk_class = json_helpers::string_at_path(raw, &["riskClass"])
        .map(normalize_policy_token)
        .filter(|value| !value.is_empty());
    let allow_argument = json_helpers::string_at_path(raw, &["allowArgument"])
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    if risk_class.is_none() && allow_argument.is_none() {
        return None;
    }

    Some(ToolRiskPolicy {
        tools,
        risk_class,
        allow_argument,
        description: json_helpers::string_at_path(raw, &["description"])
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
    })
}

fn server_supports_current_platform(platforms: &[String]) -> bool {
    if platforms.is_empty() {
        return true;
    }
    let current = current_platform_alias();
    platforms.iter().any(|platform| {
        let normalized = normalize_platform(platform);
        normalized == current || normalized == "any" || normalized == "all" || normalized == "*"
    })
}

fn current_platform_alias() -> &'static str {
    match std::env::consts::OS {
        "macos" => "macos",
        "windows" => "windows",
        "linux" => "linux",
        other => other,
    }
}

fn normalize_platform(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "darwin" | "mac" | "osx" | "macos" => "macos".to_string(),
        "win" | "windows" => "windows".to_string(),
        "linux" => "linux".to_string(),
        other => other.to_string(),
    }
}

fn find_server<'a>(
    servers: &'a BTreeMap<String, UpstreamServerConfig>,
    server_name: &str,
) -> Option<&'a UpstreamServerConfig> {
    servers.get(server_name).or_else(|| {
        servers
            .values()
            .find(|server| server.name.eq_ignore_ascii_case(server_name))
    })
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
    let total = servers.len();
    if total <= 1 {
        return servers
            .iter()
            .map(|server| task(root_path, server, timeout_ms))
            .collect();
    }

    let worker_limit = resources::default_worker_limit(total);
    let names = servers
        .iter()
        .map(|server| server.name.clone())
        .collect::<Vec<_>>();
    let pending = Arc::new(Mutex::new(servers.into_iter().enumerate()));
    let (tx, rx) = mpsc::channel();
    let mut handles = Vec::with_capacity(worker_limit);

    for _ in 0..worker_limit {
        let root_path = root_path.to_path_buf();
        let pending = Arc::clone(&pending);
        let tx = tx.clone();
        handles.push(thread::spawn(move || loop {
            let next = {
                let mut guard = pending
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                guard.next()
            };
            let Some((index, server)) = next else {
                break;
            };
            let name = server.name.clone();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                task(&root_path, &server, timeout_ms)
            }))
            .unwrap_or_else(|_| upstream_worker_panic_result(name));
            if tx.send((index, result)).is_err() {
                break;
            }
        }));
    }
    drop(tx);

    let mut results = (0..total).map(|_| None).collect::<Vec<_>>();
    for (index, result) in rx {
        if let Some(slot) = results.get_mut(index) {
            *slot = Some(result);
        }
    }
    for handle in handles {
        let _ = handle.join();
    }

    results
        .into_iter()
        .enumerate()
        .map(|(index, result)| {
            result.unwrap_or_else(|| {
                upstream_worker_panic_result(
                    names
                        .get(index)
                        .cloned()
                        .unwrap_or_else(|| "unknown".to_string()),
                )
            })
        })
        .collect()
}

fn upstream_worker_panic_result(name: String) -> JsonValue {
    JsonValue::object([
        ("name", JsonValue::string(name)),
        ("ok", JsonValue::bool(false)),
        ("status", JsonValue::string("worker-panicked")),
        (
            "error",
            JsonValue::string("internal upstream discovery worker panicked"),
        ),
    ])
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
        match acquire_tool_list_load_permit_or_cached(&key) {
            ToolListCacheAcquire::Cached(tools) => return Ok((tools, true)),
            ToolListCacheAcquire::Load(permit) => {
                let result =
                    run_stdio_request(root_path, server, "tools/list", None, timeout, None)?;
                let tools = json_helpers::value_at_path(&result, &["tools"])
                    .cloned()
                    .unwrap_or_else(|| JsonValue::array([]));
                write_cached_tools(key, tools.clone());
                drop(permit);
                return Ok((tools, false));
            }
        }
    }

    let result = run_stdio_request(root_path, server, "tools/list", None, timeout, None)?;
    let tools = json_helpers::value_at_path(&result, &["tools"])
        .cloned()
        .unwrap_or_else(|| JsonValue::array([]));
    write_cached_tools(key, tools.clone());
    Ok((tools, false))
}

enum ToolListCacheAcquire {
    Cached(JsonValue),
    Load(ToolListLoadPermit),
}

struct ToolListLoadPermit {
    key: ToolListCacheKey,
    active: bool,
}

impl Drop for ToolListLoadPermit {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        let (lock, available) = TOOL_LIST_INFLIGHT.get_or_init(tool_list_inflight_state);
        let mut in_flight = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        in_flight.remove(&self.key);
        self.active = false;
        available.notify_all();
    }
}

fn acquire_tool_list_load_permit_or_cached(key: &ToolListCacheKey) -> ToolListCacheAcquire {
    loop {
        let (lock, available) = TOOL_LIST_INFLIGHT.get_or_init(tool_list_inflight_state);
        let mut in_flight = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if !in_flight.contains(key) {
            in_flight.insert(key.clone());
            return ToolListCacheAcquire::Load(ToolListLoadPermit {
                key: key.clone(),
                active: true,
            });
        }
        while in_flight.contains(key) {
            in_flight = available
                .wait(in_flight)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
        drop(in_flight);
        if let Some(tools) = read_cached_tools(key) {
            return ToolListCacheAcquire::Cached(tools);
        }
    }
}

fn tool_list_inflight_state() -> (Mutex<BTreeSet<ToolListCacheKey>>, Condvar) {
    (Mutex::new(BTreeSet::new()), Condvar::new())
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
        prune_tool_list_cache(&mut guard);
    }
}

fn prune_tool_list_cache(cache: &mut BTreeMap<ToolListCacheKey, CachedToolList>) {
    while cache.len() > TOOL_LIST_CACHE_MAX_ENTRIES {
        let Some(oldest_key) = cache
            .iter()
            .min_by_key(|(_key, entry)| entry.stored_at)
            .map(|(key, _entry)| key.clone())
        else {
            break;
        };
        cache.remove(&oldest_key);
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

struct ToolPolicyAudit {
    value: JsonValue,
    has_annotations: bool,
    has_advisory_risk: bool,
    guard_recommended: bool,
    policy_covered: bool,
    unknown_semantics: bool,
    review_recommended: bool,
}

struct AdvisoryClassification {
    risk_classes: BTreeSet<String>,
    signals: Vec<String>,
}

fn audit_tool(server: &UpstreamServerConfig, tool: &JsonValue) -> ToolPolicyAudit {
    let name = json_helpers::string_at_path(tool, &["name"])
        .unwrap_or("<unnamed>")
        .to_string();
    let title = json_helpers::string_at_path(tool, &["title"]).map(str::to_string);
    let description = json_helpers::string_at_path(tool, &["description"])
        .or(title.as_deref())
        .unwrap_or("")
        .to_string();
    let annotation_keys = tool_annotation_keys(tool);
    let has_annotations = !annotation_keys.is_empty();
    let annotations = json_helpers::value_at_path(tool, &["annotations"])
        .cloned()
        .unwrap_or_else(empty_object);
    let classification = classify_tool_advisory(tool);
    let matching_policies = server
        .tool_policies
        .iter()
        .filter(|policy| policy.matches_tool(&name))
        .map(tool_policy_summary)
        .collect::<Vec<_>>();
    let policy_covered = !matching_policies.is_empty();
    let has_advisory_risk = !classification.risk_classes.is_empty();
    let guard_recommended = classification
        .risk_classes
        .iter()
        .any(|risk_class| risk_class_recommends_policy(risk_class));
    let unknown_semantics = !has_annotations && !has_advisory_risk && !policy_covered;
    let review_recommended =
        ((has_advisory_risk || guard_recommended) && !policy_covered) || unknown_semantics;
    let policy_status = if guard_recommended && !policy_covered {
        "unprotected-guard-recommended"
    } else if has_advisory_risk && !policy_covered {
        "unprotected-advisory-risk"
    } else if has_advisory_risk && policy_covered {
        "covered-advisory-risk"
    } else if unknown_semantics {
        "review-unknown-semantics"
    } else if json_helpers::bool_at_path(tool, &["annotations", "readOnlyHint"]) == Some(true) {
        "read-only-annotated"
    } else if policy_covered {
        "policy-covered"
    } else {
        "no-risk-detected"
    };
    let recommendation = audit_recommendation(
        policy_status,
        guard_recommended,
        policy_covered,
        unknown_semantics,
    );

    ToolPolicyAudit {
        value: JsonValue::object([
            ("name", JsonValue::string(&name)),
            (
                "title",
                title.map(JsonValue::string).unwrap_or(JsonValue::Null),
            ),
            ("description", JsonValue::string(description)),
            ("policyStatus", JsonValue::string(policy_status)),
            ("policyCovered", JsonValue::bool(policy_covered)),
            ("guardRecommended", JsonValue::bool(guard_recommended)),
            ("reviewRecommended", JsonValue::bool(review_recommended)),
            ("hasAnnotations", JsonValue::bool(has_annotations)),
            (
                "annotationKeys",
                JsonValue::array(annotation_keys.into_iter().map(JsonValue::string)),
            ),
            ("annotations", annotations),
            (
                "advisoryRiskClasses",
                JsonValue::array(
                    classification
                        .risk_classes
                        .iter()
                        .cloned()
                        .map(JsonValue::string),
                ),
            ),
            (
                "advisorySignals",
                JsonValue::array(classification.signals.into_iter().map(JsonValue::string)),
            ),
            ("matchingPolicies", JsonValue::array(matching_policies)),
            ("recommendation", JsonValue::string(recommendation)),
        ]),
        has_annotations,
        has_advisory_risk,
        guard_recommended,
        policy_covered,
        unknown_semantics,
        review_recommended,
    }
}

fn classify_tool_advisory(tool: &JsonValue) -> AdvisoryClassification {
    let mut risk_classes = BTreeSet::new();
    let mut signals = Vec::new();
    if json_helpers::bool_at_path(tool, &["annotations", "destructiveHint"]) == Some(true) {
        add_advisory_signal(
            &mut risk_classes,
            &mut signals,
            "mutation",
            "mcp.destructiveHint=true",
        );
    }
    if json_helpers::bool_at_path(tool, &["annotations", "readOnlyHint"]) == Some(false) {
        add_advisory_signal(
            &mut risk_classes,
            &mut signals,
            "not-readonly",
            "mcp.readOnlyHint=false",
        );
    }
    if json_helpers::bool_at_path(tool, &["annotations", "openWorldHint"]) == Some(true) {
        add_advisory_signal(
            &mut risk_classes,
            &mut signals,
            "open-world",
            "mcp.openWorldHint=true",
        );
    }

    if let Some(name) = json_helpers::string_at_path(tool, &["name"]) {
        add_name_based_advisory_signals(name, &mut risk_classes, &mut signals);
    }

    AdvisoryClassification {
        risk_classes,
        signals,
    }
}

fn add_advisory_signal(
    risk_classes: &mut BTreeSet<String>,
    signals: &mut Vec<String>,
    risk_class: &str,
    signal: &str,
) {
    risk_classes.insert(risk_class.to_string());
    signals.push(signal.to_string());
}

fn add_name_based_advisory_signals(
    tool_name: &str,
    risk_classes: &mut BTreeSet<String>,
    signals: &mut Vec<String>,
) {
    let lower = tool_name.trim().to_ascii_lowercase();
    let tokens = tool_name_tokens(&lower);

    for token in [
        "write", "create", "delete", "remove", "update", "edit", "move", "rename", "patch",
        "insert", "upsert", "append", "add", "commit", "checkout", "reset", "publish", "deploy",
        "install",
    ] {
        if tokens.contains(token) {
            add_advisory_signal(
                risk_classes,
                signals,
                "mutation",
                &format!("name-token:{}", token),
            );
        }
    }

    for token in [
        "powershell",
        "shell",
        "command",
        "exec",
        "execute",
        "process",
        "registry",
        "clipboard",
    ] {
        if tokens.contains(token)
            && !["read", "list", "describe"]
                .iter()
                .any(|safe| tokens.contains(*safe))
        {
            add_advisory_signal(
                risk_classes,
                signals,
                "system-control",
                &format!("name-token:{}", token),
            );
        }
    }

    if lower.contains("run_code") || lower.contains("run-code") {
        add_advisory_signal(
            risk_classes,
            signals,
            "system-control",
            "name-pattern:run_code",
        );
    }

    for token in [
        "click", "type", "press", "shortcut", "scroll", "drag", "hover", "select", "navigate",
        "resize", "tab", "tabs", "upload", "dialog", "evaluate", "fill", "close",
    ] {
        if tokens.contains(token) {
            let class = if lower.contains("page") || lower.contains("web") {
                "interaction-control"
            } else {
                "desktop-control"
            };
            add_advisory_signal(
                risk_classes,
                signals,
                class,
                &format!("name-token:{}", token),
            );
        }
    }

    for token in [
        "javascript",
        "cdp",
        "permission",
        "permissions",
        "downloads",
        "files",
        "action",
        "clear",
        "slider",
    ] {
        if tokens.contains(token) {
            add_advisory_signal(
                risk_classes,
                signals,
                "interaction-control",
                &format!("name-token:{}", token),
            );
        }
    }

    for token in ["screenshot", "snapshot", "scrape", "screen"] {
        if tokens.contains(token) {
            let class = if lower.contains("page") || lower.contains("web") {
                "interaction-observation"
            } else {
                "desktop-observation"
            };
            add_advisory_signal(
                risk_classes,
                signals,
                class,
                &format!("name-token:{}", token),
            );
        }
    }

    for token in ["fetch", "search", "http", "url", "web", "request"] {
        if tokens.contains(token) {
            add_advisory_signal(
                risk_classes,
                signals,
                "open-world",
                &format!("name-token:{}", token),
            );
        }
    }
}

fn tool_name_tokens(lower_name: &str) -> BTreeSet<String> {
    lower_name
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn risk_class_recommends_policy(risk_class: &str) -> bool {
    matches!(
        risk_class,
        "mutation"
            | "not-readonly"
            | "interaction-control"
            | "interaction-observation"
            | "desktop-control"
            | "desktop-observation"
            | "system-control"
    )
}

fn audit_recommendation(
    policy_status: &str,
    guard_recommended: bool,
    policy_covered: bool,
    unknown_semantics: bool,
) -> &'static str {
    if guard_recommended && !policy_covered {
        "Add an explicit mcpace.config.json toolPolicies entry before using this tool routinely; keep runtime enforcement declarative instead of hardcoding this tool in Rust."
    } else if policy_status == "unprotected-advisory-risk" {
        "Review the upstream tool semantics and add a toolPolicies guard if it can mutate local, remote, interactive, or host state."
    } else if unknown_semantics {
        "No MCP annotations or MCPace policy describe this tool; inspect the upstream server documentation before relying on parallel or unattended calls."
    } else if policy_covered {
        "Covered by declarative MCPace policy; callers must use the configured allow argument or risk-class opt-in for guarded calls."
    } else {
        "No guard is currently recommended from annotations or generic name heuristics."
    }
}

fn tool_annotation_keys(tool: &JsonValue) -> Vec<String> {
    json_helpers::object_at_path(tool, &["annotations"])
        .map(|object| object.keys().cloned().collect())
        .unwrap_or_default()
}

fn tool_policy_summaries(policies: &[ToolRiskPolicy]) -> Vec<JsonValue> {
    policies.iter().map(tool_policy_summary).collect()
}

fn tool_policy_summary(policy: &ToolRiskPolicy) -> JsonValue {
    JsonValue::object([
        (
            "tools",
            JsonValue::array(policy.tools.iter().cloned().map(JsonValue::string)),
        ),
        (
            "riskClass",
            policy
                .risk_class
                .as_ref()
                .map(JsonValue::string)
                .unwrap_or(JsonValue::Null),
        ),
        (
            "allowArgument",
            policy
                .allow_argument
                .as_ref()
                .map(JsonValue::string)
                .unwrap_or(JsonValue::Null),
        ),
        (
            "description",
            policy
                .description
                .as_ref()
                .map(JsonValue::string)
                .unwrap_or(JsonValue::Null),
        ),
    ])
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
        .map(|(key, value)| format!("{}:{}", key, fingerprint_env_value(value)))
        .collect::<Vec<_>>()
        .join("\u{1f}");
    format!(
        "protocol={}|enabled={}|type={}|command={}|args={}|env={}|cwd={}|url={}|timeout={}",
        mcp::CURRENT_PROTOCOL_VERSION,
        server.enabled,
        server.source_type,
        server.command.as_deref().unwrap_or_default(),
        server.args.join("\u{1f}"),
        env_values,
        server
            .cwd
            .as_ref()
            .map(|value| value.display().to_string())
            .unwrap_or_default(),
        server.url.as_deref().unwrap_or_default(),
        server.timeout_ms
    )
}

fn fingerprint_env_value(value: &str) -> String {
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
    if server.source_type != "stdio" {
        return Err(format!(
            "upstream server '{}' uses '{}' transport. This MCPace bridge currently forwards stdio upstreams only; configure a stdio adapter or call runtime_diagnostics for exact status.",
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
    root_path: &Path,
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

    initialize_running_server(&mut running, server, deadline, lease_lost)?;

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

    initialize_running_server(&mut running, server, deadline, lease_lost)?;

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

fn initialize_running_server(
    running: &mut RunningServer,
    server: &UpstreamServerConfig,
    deadline: Instant,
    lease_lost: Option<&AtomicBool>,
) -> Result<(), String> {
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
    )
}

fn spawn_stdio_server(
    root_path: &Path,
    server: &UpstreamServerConfig,
) -> Result<RunningServer, String> {
    let command_name = server.command.as_deref().unwrap_or_default();
    let cwd = server.cwd.as_deref().unwrap_or(root_path);
    if let Some(cwd_error) = validate_stdio_cwd(cwd, &server.name) {
        return Err(cwd_error);
    }
    let program = resolve_command_for_cwd(command_name, cwd).map_err(|error| {
        format!(
            "failed to resolve command '{}' for upstream server '{}': {}",
            command_name, server.name, error
        )
    })?;

    let mut command = Command::new(&program);
    command.env_clear();
    for (key, value) in default_child_process_environment() {
        command.env(key, value);
    }
    command
        .args(&server.args)
        .current_dir(cwd)
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

fn default_child_process_environment() -> BTreeMap<String, String> {
    let mut values = BTreeMap::new();
    let names: &[&str] = if cfg!(windows) {
        &[
            "PATH",
            "Path",
            "PATHEXT",
            "SystemRoot",
            "ComSpec",
            "TEMP",
            "TMP",
            "USERPROFILE",
            "APPDATA",
            "LOCALAPPDATA",
            "ProgramFiles",
            "ProgramFiles(x86)",
        ]
    } else {
        &[
            "PATH", "HOME", "TMPDIR", "TEMP", "TMP", "USER", "LOGNAME", "SHELL",
        ]
    };

    for name in names {
        if let Ok(value) = env::var(name) {
            values.insert((*name).to_string(), value);
        }
    }
    values
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
            lines.push(sanitize_stderr_diagnostic(trimmed));
        }
        if lines.len() >= STDERR_DIAGNOSTIC_MAX_LINES {
            break;
        }
    }
    if lines.is_empty() {
        String::new()
    } else {
        format!("; stderr: {}", lines.join(" | "))
    }
}

fn sanitize_stderr_diagnostic(value: &str) -> String {
    truncate_diagnostic_text(&redact_sensitive_assignments(&redact_bearer_tokens(value)))
}

fn redact_sensitive_assignments(value: &str) -> String {
    let chars = value.chars().collect::<Vec<_>>();
    let mut redacted = String::new();
    let mut index = 0;

    while index < chars.len() {
        let current = chars[index];
        if current == '=' || current == ':' {
            let key_end = diagnostic_key_end(&chars, index);
            let key_start = diagnostic_key_start(&chars, key_end);
            let key = chars[key_start..key_end]
                .iter()
                .collect::<String>()
                .trim()
                .trim_matches(|ch| ch == '\'' || ch == '"')
                .to_string();
            if diagnostic_key_is_sensitive(&key) {
                redacted.push(current);
                index += 1;
                while index < chars.len() && chars[index].is_whitespace() {
                    redacted.push(chars[index]);
                    index += 1;
                }
                redacted.push_str(DIAGNOSTIC_REDACTION);
                index = skip_sensitive_diagnostic_value(&chars, index);
                continue;
            }
        }
        redacted.push(current);
        index += 1;
    }

    redacted
}

fn diagnostic_key_end(chars: &[char], separator_index: usize) -> usize {
    let mut end = separator_index;
    while end > 0 && chars[end - 1].is_whitespace() {
        end -= 1;
    }
    end
}

fn diagnostic_key_start(chars: &[char], key_end: usize) -> usize {
    let mut start = key_end;
    while start > 0 {
        let previous = chars[start - 1];
        if previous.is_alphanumeric()
            || matches!(previous, '_' | '-' | '.')
            || (matches!(previous, '\'' | '"') && start == key_end)
        {
            start -= 1;
        } else {
            break;
        }
    }
    start
}

fn diagnostic_key_is_sensitive(key: &str) -> bool {
    let normalized = key
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .collect::<String>()
        .to_ascii_lowercase();
    if normalized.is_empty() {
        return false;
    }
    [
        "token",
        "secret",
        "password",
        "passwd",
        "api_key",
        "apikey",
        "access_key",
        "accesskey",
        "private_key",
        "privatekey",
        "authorization",
        "credential",
        "credentials",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
}

fn skip_sensitive_diagnostic_value(chars: &[char], mut index: usize) -> usize {
    if index >= chars.len() {
        return index;
    }

    if matches!(chars[index], '\'' | '"') {
        let quote = chars[index];
        index += 1;
        while index < chars.len() {
            let current = chars[index];
            index += 1;
            if current == quote {
                break;
            }
        }
        return index;
    }

    while index < chars.len()
        && !chars[index].is_whitespace()
        && !matches!(chars[index], ',' | ';' | '|' | ')' | '}')
    {
        index += 1;
    }
    index
}

fn redact_bearer_tokens(value: &str) -> String {
    let mut redacted = String::new();
    let lower = value.to_ascii_lowercase();
    let mut index = 0;

    while let Some(relative) = lower[index..].find("bearer ") {
        let start = index + relative;
        redacted.push_str(&value[index..start]);
        redacted.push_str("Bearer ");
        redacted.push_str(DIAGNOSTIC_REDACTION);
        index = skip_bearer_token(value, start + "bearer ".len());
    }

    redacted.push_str(&value[index..]);
    redacted
}

fn skip_bearer_token(value: &str, start: usize) -> usize {
    let mut end = start;
    for (relative, ch) in value[start..].char_indices() {
        if ch.is_whitespace() || matches!(ch, ',' | ';' | '|' | ')' | '}') {
            return start + relative;
        }
        end = start + relative + ch.len_utf8();
    }
    end
}

fn truncate_diagnostic_text(value: &str) -> String {
    if value.chars().count() <= STDERR_DIAGNOSTIC_MAX_CHARS_PER_LINE {
        return value.to_string();
    }

    let mut truncated = value
        .chars()
        .take(STDERR_DIAGNOSTIC_MAX_CHARS_PER_LINE)
        .collect::<String>();
    truncated.push_str("…<truncated>");
    truncated
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
        server
            .disabled_reason
            .clone()
            .unwrap_or_else(|| "server is disabled by source or policy".to_string())
    } else if runtime_callable {
        "enabled stdio server; list with upstream_tools and call with upstream_call".to_string()
    } else if let Some(error) = command_error {
        error
    } else if server.source_type == "http" {
        "non-stdio HTTP upstream fan-out is not implemented in this stdio bridge".to_string()
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

fn validate_stdio_cwd(cwd: &Path, server_name: &str) -> Option<String> {
    match fs::metadata(cwd) {
        Ok(metadata) if metadata.is_dir() => None,
        Ok(_) => Some(format!(
            "configured cwd '{}' for upstream server '{}' is not a directory",
            cwd.display(),
            server_name
        )),
        Err(error) => Some(format!(
            "configured cwd '{}' for upstream server '{}' is not accessible: {}",
            cwd.display(),
            server_name,
            error
        )),
    }
}

fn resolve_command_for_cwd(command: &str, cwd: &Path) -> Result<PathBuf, String> {
    let command = command.trim();
    if command.is_empty() {
        return Err("empty command".to_string());
    }

    let raw = PathBuf::from(command);
    if raw.is_absolute() {
        return if raw.exists() {
            Ok(raw.canonicalize().unwrap_or(raw))
        } else {
            Err(format!(
                "absolute command path '{}' does not exist",
                raw.display()
            ))
        };
    }

    let looks_path_like = raw.components().count() > 1 || raw.extension().is_some();
    if looks_path_like {
        let cwd_candidate = cwd.join(&raw);
        if cwd_candidate.exists() {
            return Ok(cwd_candidate.canonicalize().unwrap_or(cwd_candidate));
        }
        if raw.exists() {
            return Ok(raw.canonicalize().unwrap_or(raw));
        }
    }

    which::which(command).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
    fn config_declared_sensitive_tools_require_explicit_policy_flags() {
        let server = UpstreamServerConfig {
            name: "custom-desktop".to_string(),
            enabled: true,
            disabled_reason: None,
            source_type: "stdio".to_string(),
            command: Some("tool".to_string()),
            args: Vec::new(),
            env: BTreeMap::new(),
            cwd: None,
            url: None,
            timeout_ms: DEFAULT_TIMEOUT_MS,
            tool_policies: vec![
                ToolRiskPolicy {
                    tools: vec!["Screenshot".to_string(), "Snapshot".to_string()],
                    risk_class: Some("desktop-observation".to_string()),
                    allow_argument: Some("allowDesktopObservation".to_string()),
                    description: None,
                },
                ToolRiskPolicy {
                    tools: vec!["Type".to_string(), "Shortcut".to_string()],
                    risk_class: Some("desktop-control".to_string()),
                    allow_argument: Some("allowDesktopControl".to_string()),
                    description: None,
                },
                ToolRiskPolicy {
                    tools: vec!["Power*".to_string()],
                    risk_class: Some("system-control".to_string()),
                    allow_argument: Some("allowSystemControl".to_string()),
                    description: None,
                },
            ],
        };
        let ungated_server = UpstreamServerConfig {
            name: "ungated".to_string(),
            tool_policies: Vec::new(),
            ..server.clone()
        };

        assert!(validate_upstream_tool_policy(&server, "Wait", None).is_ok());
        assert!(validate_upstream_tool_policy(&ungated_server, "Type", None).is_ok());

        let blocked_screenshot = validate_upstream_tool_policy(&server, "Screenshot", None)
            .expect_err("screenshot should require observation opt-in");
        assert!(blocked_screenshot.contains("allowDesktopObservation=true"));
        assert!(blocked_screenshot.contains("desktop-observation"));

        let blocked_type = validate_upstream_tool_policy(&server, "Type", None)
            .expect_err("type should require desktop-control opt-in");
        assert!(blocked_type.contains("allowDesktopControl=true"));

        let blocked_powershell = validate_upstream_tool_policy(&server, "PowerShell", None)
            .expect_err("powershell should require system-control opt-in");
        assert!(blocked_powershell.contains("allowSystemControl=true"));

        let mut observation_args = BTreeSet::new();
        observation_args.insert("allowDesktopObservation".to_string());
        let observation = UpstreamLeaseContext {
            allow_arguments: observation_args,
            ..Default::default()
        };
        assert!(validate_upstream_tool_policy(&server, "Snapshot", Some(&observation)).is_ok());
        assert!(validate_upstream_tool_policy(&server, "Screenshot", Some(&observation)).is_ok());

        let mut desktop_risks = BTreeSet::new();
        desktop_risks.insert("desktop-control".to_string());
        let desktop_control = UpstreamLeaseContext {
            allowed_tool_risk_classes: desktop_risks,
            ..Default::default()
        };
        assert!(validate_upstream_tool_policy(&server, "Type", Some(&desktop_control)).is_ok());
        assert!(validate_upstream_tool_policy(&server, "Shortcut", Some(&desktop_control)).is_ok());

        let mut system_args = BTreeSet::new();
        system_args.insert("allowSystemControl".to_string());
        let system_control = UpstreamLeaseContext {
            allow_arguments: system_args,
            ..Default::default()
        };
        assert!(
            validate_upstream_tool_policy(&server, "PowerShell", Some(&system_control)).is_ok()
        );

        let batch = [UpstreamToolCall {
            tool: "Type".to_string(),
            arguments: mcp::empty_object(),
        }];
        assert!(validate_upstream_batch_tool_policy(&server, &batch, None).is_err());
        assert!(
            validate_upstream_batch_tool_policy(&server, &batch, Some(&desktop_control)).is_ok()
        );
    }

    #[test]
    fn allow_policy_argument_collectors_normalize_shared_bridge_inputs() {
        let args = parse_str(
            r#"{
              "allowDesktopObservation": true,
              "allowDesktopControl": false,
              "allowSystemControl": null,
              "allowArguments": [" allowCustomRisk ", "", "allowFilesystemMutation"],
              "allowToolRiskClasses": ["Desktop-Control", " filesystem-mutation ", ""]
            }"#,
        )
        .unwrap();

        let allow_arguments = collect_allow_arguments(&args).unwrap();
        assert!(allow_arguments.contains("allowDesktopObservation"));
        assert!(allow_arguments.contains("allowCustomRisk"));
        assert!(allow_arguments.contains("allowFilesystemMutation"));
        assert!(!allow_arguments.contains("allowDesktopControl"));
        assert!(!allow_arguments.contains("allowSystemControl"));
        assert!(!allow_arguments.contains("allowToolRiskClasses"));

        let risk_classes = collect_allowed_tool_risk_classes(&args).unwrap();
        assert_eq!(
            risk_classes,
            BTreeSet::from([
                "desktop-control".to_string(),
                "filesystem-mutation".to_string()
            ])
        );

        let bad_allow = parse_str(r#"{"allowArguments":[true]}"#).unwrap();
        assert_eq!(
            collect_allow_arguments(&bad_allow).unwrap_err(),
            "allowArguments must be an array of strings"
        );

        let bad_risk = parse_str(r#"{"allowToolRiskClasses":["ok", 1]}"#).unwrap();
        assert_eq!(
            collect_allowed_tool_risk_classes(&bad_risk).unwrap_err(),
            "allowToolRiskClasses must be an array of strings"
        );
    }

    #[test]
    fn server_fingerprint_does_not_embed_secret_env_values() {
        let mut env = BTreeMap::new();
        env.insert("API_TOKEN".to_string(), "secret-value".to_string());
        let server = UpstreamServerConfig {
            name: "secret-probe".to_string(),
            enabled: true,
            disabled_reason: None,
            source_type: "stdio".to_string(),
            command: Some("node".to_string()),
            args: Vec::new(),
            env,
            cwd: None,
            url: None,
            timeout_ms: 1_000,
            tool_policies: Vec::new(),
        };

        let fingerprint = server_fingerprint(&server);

        assert!(fingerprint.contains("API_TOKEN:"));
        assert!(!fingerprint.contains("secret-value"));
        assert!(fingerprint.contains("len12-hash"));
    }

    #[test]
    fn env_var_names_accept_codex_local_object_entries_and_skip_remote_entries() {
        let value = parse_str(
            r#"[
              "PLAIN_TOKEN",
              { "name": "LOCAL_OBJECT_TOKEN", "source": "local" },
              { "name": "DEFAULT_LOCAL_TOKEN" },
              { "name": "REMOTE_ONLY_TOKEN", "source": "remote" },
              { "source": "local" },
              ""
            ]"#,
        )
        .unwrap();
        let names = env_var_names_from_array(value.as_array());

        assert_eq!(
            names,
            vec![
                "PLAIN_TOKEN".to_string(),
                "LOCAL_OBJECT_TOKEN".to_string(),
                "DEFAULT_LOCAL_TOKEN".to_string(),
            ]
        );
    }

    #[test]
    fn stderr_suffix_redacts_sensitive_diagnostics_without_removing_context() {
        let (tx, rx) = mpsc::channel();
        tx.send(
            "startup failed TOKEN = abc123 Authorization: Bearer bearer-secret workspace=/tmp/work"
                .to_string(),
        )
        .unwrap();
        drop(tx);

        let suffix = stderr_suffix(&rx);

        assert!(suffix.contains("startup failed"));
        assert!(suffix.contains("workspace=/tmp/work"));
        assert!(suffix.contains(DIAGNOSTIC_REDACTION));
        assert!(!suffix.contains("abc123"));
        assert!(!suffix.contains("bearer-secret"));
    }

    #[test]
    fn stderr_suffix_bounds_diagnostic_line_count_and_length() {
        let (tx, rx) = mpsc::channel();
        for index in 0..8 {
            tx.send(format!("line-{index}: {}", "x".repeat(512)))
                .unwrap();
        }
        drop(tx);

        let suffix = stderr_suffix(&rx);

        assert!(suffix.contains("line-0"));
        assert!(suffix.contains("line-5"));
        assert!(!suffix.contains("line-6"));
        assert!(suffix.contains("<truncated>"));
        assert!(suffix.len() < 2_400);
    }

    #[cfg(unix)]
    #[test]
    fn spawn_stdio_server_does_not_forward_unspecified_parent_environment() {
        let root = temp_root();
        env::set_var("MCPACE_PARENT_SECRET_DO_NOT_FORWARD", "secret-value");
        env::set_var("MCPACE_ALLOWED_TOKEN_TEST", "allowed-value");
        let mut explicit_env = BTreeMap::new();
        explicit_env.insert("EXPLICIT_TOKEN".to_string(), "explicit-value".to_string());
        explicit_env.insert(
            "MCPACE_ALLOWED_TOKEN_TEST".to_string(),
            env::var("MCPACE_ALLOWED_TOKEN_TEST").unwrap(),
        );
        let server = UpstreamServerConfig {
            name: "env-probe".to_string(),
            enabled: true,
            disabled_reason: None,
            source_type: "stdio".to_string(),
            command: Some("sh".to_string()),
            args: vec![
                "-c".to_string(),
                r#"printf '%s|%s|%s|%s\n' "$MCPACE_PARENT_SECRET_DO_NOT_FORWARD" "$MCPACE_ALLOWED_TOKEN_TEST" "$EXPLICIT_TOKEN" "$MCPACE_PRIMARY_WORKSPACE""#.to_string(),
            ],
            env: explicit_env,
            cwd: None,
            url: None,
            timeout_ms: 1_000,
            tool_policies: Vec::new(),
        };

        let running = spawn_stdio_server(&root, &server).expect("spawn env probe");
        let line = running
            .stdout_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("env probe output");
        let fields = line.split('|').collect::<Vec<_>>();

        assert_eq!(fields[0], "");
        assert_eq!(fields[1], "allowed-value");
        assert_eq!(fields[2], "explicit-value");
        assert!(fields[3].contains(&root.display().to_string()));

        env::remove_var("MCPACE_PARENT_SECRET_DO_NOT_FORWARD");
        env::remove_var("MCPACE_ALLOWED_TOKEN_TEST");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn load_servers_attaches_declarative_tool_policies_from_project_config() {
        let root = temp_root();
        fs::write(
            root.join("mcp_settings.json"),
            r#"{
  "mcpServers": {
    "custom": { "enabled": true, "type": "stdio", "command": "node", "args": ["server.js"] }
  }
}"#,
        )
        .unwrap();
        fs::write(
            root.join("mcpace.config.json"),
            r#"{
  "servers": {
    "custom": {
      "toolPolicies": [
        {
          "riskClass": "custom-risk",
          "allowArgument": "allowCustomRisk",
          "tools": ["danger_*"]
        }
      ]
    }
  }
}"#,
        )
        .unwrap();

        let servers = load_servers(&root).expect("servers");
        let server = servers.get("custom").expect("custom server");

        let blocked = validate_upstream_tool_policy(server, "danger_write", None)
            .expect_err("config policy should block matching tool");
        assert!(blocked.contains("custom-risk"));
        assert!(blocked.contains("allowCustomRisk=true"));

        let mut allowed_risks = BTreeSet::new();
        allowed_risks.insert("custom-risk".to_string());
        let context = UpstreamLeaseContext {
            allowed_tool_risk_classes: allowed_risks,
            ..Default::default()
        };
        assert!(validate_upstream_tool_policy(server, "danger_write", Some(&context)).is_ok());
        assert!(validate_upstream_tool_policy(server, "safe_read", None).is_ok());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn load_servers_applies_profile_and_platform_policy_from_project_config() {
        let root = temp_root();
        let unsupported_platform = if current_platform_alias() == "windows" {
            "linux"
        } else {
            "windows"
        };
        fs::write(
            root.join("mcp_settings.json"),
            r#"{
  "mcpServers": {
    "profile-blocked": { "enabled": true, "type": "stdio", "command": "node", "args": ["server.js"] },
    "platform-blocked": { "enabled": true, "type": "stdio", "command": "node", "args": ["server.js"] }
  }
}"#,
        )
        .unwrap();
        fs::write(
            root.join("mcpace.config.json"),
            format!(
                r#"{{
  "profiles": {{
    "runtime": {{
      "default": "safe",
      "profiles": {{
        "safe": {{ "serverOverrides": {{}} }}
      }}
    }}
  }},
  "servers": {{
    "profile-blocked": {{ "required": false, "defaultEnabled": false }},
    "platform-blocked": {{ "required": false, "defaultEnabled": true, "platforms": ["{}"] }}
  }}
}}"#,
                unsupported_platform
            ),
        )
        .unwrap();

        let servers = load_servers(&root).expect("servers");
        let profile_blocked = servers.get("profile-blocked").expect("profile-blocked");
        assert!(!profile_blocked.enabled);
        assert!(profile_blocked
            .disabled_reason
            .as_deref()
            .unwrap_or_default()
            .contains("runtime profile"));

        let platform_blocked = servers.get("platform-blocked").expect("platform-blocked");
        assert!(!platform_blocked.enabled);
        assert!(platform_blocked
            .disabled_reason
            .as_deref()
            .unwrap_or_default()
            .contains("current platform"));

        let error = ensure_callable_stdio(&root, platform_blocked).expect_err("platform disabled");
        assert!(error.contains("disabled"));
        assert!(error.contains("current platform"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn server_runtime_callable_blocks_missing_cwd_before_spawn() {
        let root = temp_root();
        let missing_cwd = root.join("missing-workspace");
        let server = UpstreamServerConfig {
            name: "missing-cwd-probe".to_string(),
            enabled: true,
            disabled_reason: None,
            source_type: "stdio".to_string(),
            command: Some("node".to_string()),
            args: Vec::new(),
            env: BTreeMap::new(),
            cwd: Some(missing_cwd.clone()),
            url: None,
            timeout_ms: 1_000,
            tool_policies: Vec::new(),
        };

        let (callable, resolved, error) = server_runtime_callable(&root, &server);

        assert!(!callable);
        assert!(resolved.is_none());
        let error = error.expect("missing cwd error");
        assert!(error.contains("configured cwd"));
        assert!(error.contains(&missing_cwd.display().to_string()));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn relative_stdio_command_paths_resolve_against_configured_cwd() {
        let root = temp_root();
        let workspace = root.join("workspace");
        let bin_dir = workspace.join("bin");
        fs::create_dir_all(&bin_dir).unwrap();
        let executable = bin_dir.join("server-probe");
        fs::write(&executable, "#!/bin/sh\n").unwrap();

        let resolved = resolve_command_for_cwd("./bin/server-probe", &workspace)
            .expect("resolve relative command from cwd");

        assert_eq!(resolved, executable.canonicalize().unwrap());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn load_servers_accepts_source_only_standard_mcp_shape() {
        let root = temp_root();
        fs::create_dir_all(root.join("workspace")).unwrap();
        env::set_var("MCPACE_TEST_FORWARDED_ENV", "forwarded-value");
        fs::write(
            root.join("mcp_settings.json"),
            r#"{
  "mcpServers": {
    "Server From Settings": {
      "command": "node",
      "args": ["${MCPACE_TEST_ARG:-fallback-arg}"],
      "cwd": "${MCPACE_PRIMARY_WORKSPACE}/workspace",
      "env": { "STATIC_ROOT": "${MCPACE_PRIMARY_WORKSPACE}" },
      "env_vars": ["MCPACE_TEST_FORWARDED_ENV"]
    }
  }
}"#,
        )
        .unwrap();

        let servers = load_servers(&root).expect("source-only server config");
        let server = servers
            .get("Server From Settings")
            .expect("server from settings");
        assert!(server.enabled);
        assert_eq!(server.source_type, "stdio");
        assert_eq!(server.command.as_deref(), Some("node"));
        assert_eq!(server.args, vec!["fallback-arg".to_string()]);
        assert_eq!(
            server
                .env
                .get("MCPACE_TEST_FORWARDED_ENV")
                .map(String::as_str),
            Some("forwarded-value")
        );
        assert!(server
            .env
            .get("STATIC_ROOT")
            .map(|value| value.contains(&root.display().to_string()))
            .unwrap_or(false));
        assert!(server.cwd.as_ref().unwrap().ends_with("workspace"));

        env::remove_var("MCPACE_TEST_FORWARDED_ENV");
        let _ = fs::remove_dir_all(root);
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
    "remote-demo": { "enabled": true, "type": "http", "url": "http://127.0.0.1:39022/mcp" },
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
        let remote = servers
            .iter()
            .find(|item| json_helpers::string_at_path(item, &["name"]) == Some("remote-demo"))
            .unwrap();
        assert_eq!(
            json_helpers::string_at_path(memory, &["status"]),
            Some("callable-stdio")
        );
        assert_eq!(
            json_helpers::string_at_path(remote, &["status"]),
            Some("blocked-http-upstream")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn surface_manifest_is_explicit_about_wrapper_projection() {
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
    "memory": { "enabled": true, "type": "stdio", "command": "__COMMAND__", "args": ["-y", "server"] }
  }
}"#
            .replace("__COMMAND__", &command),
        )
        .unwrap();

        let manifest = surface_manifest(
            &root,
            "streamable-http",
            vec!["surface_manifest".to_string(), "upstream_call".to_string()],
            false,
            None,
            false,
        )
        .expect("surface manifest");
        assert_eq!(json_helpers::bool_at_path(&manifest, &["ok"]), Some(true));
        assert_eq!(
            json_helpers::bool_at_path(
                &manifest,
                &["upstreamTools", "directTopLevelProjection", "enabled"]
            ),
            Some(true)
        );
        assert_eq!(
            json_helpers::value_at_path(&manifest, &["topLevelTools", "count"])
                .and_then(JsonValue::as_i64),
            Some(2)
        );
        assert_eq!(
            json_helpers::bool_at_path(&manifest, &["upstreamTools", "liveCatalogIncluded"]),
            Some(false)
        );
        assert_eq!(
            json_helpers::string_at_path(&manifest, &["configurationModel", "name"]),
            Some("bring-your-own-mcp-servers")
        );
        assert_eq!(
            json_helpers::string_at_path(&manifest, &["configurationModel", "serverSourceOfTruth"]),
            Some("mcp_settings.json.mcpServers")
        );
        assert_eq!(
            json_helpers::bool_at_path(
                &manifest,
                &[
                    "configurationModel",
                    "packagedDefaults",
                    "upstreamServersEnabled"
                ]
            ),
            Some(false)
        );
        assert_eq!(
            json_helpers::bool_at_path(
                &manifest,
                &[
                    "configurationModel",
                    "packagedDefaults",
                    "requiresHardcodedServerNames"
                ]
            ),
            Some(false)
        );
        assert_eq!(
            json_helpers::bool_at_path(&manifest, &["configurationModel", "arbitraryServerNames"]),
            Some(true)
        );
        assert_eq!(
            json_helpers::bool_at_path(
                &manifest,
                &["configurationModel", "requiresRecompileForNewServers"]
            ),
            Some(false)
        );
        assert_eq!(
            json_helpers::bool_at_path(
                &manifest,
                &["configurationModel", "installsUpstreamPackages"]
            ),
            Some(false)
        );
        assert_eq!(
            json_helpers::bool_at_path(
                &manifest,
                &["configurationModel", "httpUpstreamForwardingImplemented"]
            ),
            Some(false)
        );
        assert!(json_helpers::string_at_path(&manifest, &["summary"])
            .unwrap_or_default()
            .contains("disguised as native MCPace tools"));
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
    fn bounded_server_task_runner_preserves_input_order() {
        fn server(name: &str) -> UpstreamServerConfig {
            UpstreamServerConfig {
                name: name.to_string(),
                enabled: true,
                disabled_reason: None,
                source_type: "stdio".to_string(),
                command: Some("tool".to_string()),
                args: Vec::new(),
                env: BTreeMap::new(),
                cwd: None,
                url: None,
                timeout_ms: DEFAULT_TIMEOUT_MS,
                tool_policies: Vec::new(),
            }
        }

        let root = temp_root();
        let results = run_server_tasks(
            &root,
            vec![server("zeta"), server("alpha"), server("middle")],
            None,
            |_root, server, _timeout| {
                JsonValue::object([("name", JsonValue::string(&server.name))])
            },
        );

        let names = results
            .iter()
            .filter_map(|item| json_helpers::string_at_path(item, &["name"]))
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["zeta", "alpha", "middle"]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn bounded_server_task_runner_reports_worker_panics() {
        fn server(name: &str) -> UpstreamServerConfig {
            UpstreamServerConfig {
                name: name.to_string(),
                enabled: true,
                disabled_reason: None,
                source_type: "stdio".to_string(),
                command: Some("tool".to_string()),
                args: Vec::new(),
                env: BTreeMap::new(),
                cwd: None,
                url: None,
                timeout_ms: DEFAULT_TIMEOUT_MS,
                tool_policies: Vec::new(),
            }
        }

        let root = temp_root();
        let results = run_server_tasks(
            &root,
            vec![server("ok"), server("panic")],
            None,
            |_root, server, _timeout| {
                if server.name == "panic" {
                    panic!("intentional task panic");
                }
                JsonValue::object([
                    ("name", JsonValue::string(&server.name)),
                    ("ok", JsonValue::bool(true)),
                ])
            },
        );

        assert_eq!(json_helpers::bool_at_path(&results[0], &["ok"]), Some(true));
        assert_eq!(
            json_helpers::string_at_path(&results[1], &["name"]),
            Some("panic")
        );
        assert_eq!(
            json_helpers::string_at_path(&results[1], &["status"]),
            Some("worker-panicked")
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
    fn tool_list_cache_prunes_oldest_entries_to_bound_memory() {
        let mut cache = BTreeMap::new();
        let now = Instant::now();
        for index in 0..(TOOL_LIST_CACHE_MAX_ENTRIES + 5) {
            cache.insert(
                ToolListCacheKey {
                    root_path: format!("test-prune-root-{}", std::process::id()),
                    server_name: format!("server-{index:03}"),
                    settings_modified_ms: index as u128,
                    settings_len: index as u64,
                    server_fingerprint: format!("fingerprint-{index:03}"),
                },
                CachedToolList {
                    stored_at: now + Duration::from_millis(index as u64),
                    tools: JsonValue::array([JsonValue::string(format!("tool-{index}"))]),
                },
            );
        }

        prune_tool_list_cache(&mut cache);

        assert_eq!(cache.len(), TOOL_LIST_CACHE_MAX_ENTRIES);
        assert!(cache
            .keys()
            .all(|key| key.server_name.as_str() >= "server-005"));
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
    fn tool_policy_audit_flags_unprotected_mutating_tools_without_enforcing_heuristics() {
        let server = UpstreamServerConfig {
            name: "audit-fixture".to_string(),
            enabled: true,
            disabled_reason: None,
            source_type: "stdio".to_string(),
            command: Some("tool".to_string()),
            args: Vec::new(),
            env: BTreeMap::new(),
            cwd: None,
            url: None,
            timeout_ms: DEFAULT_TIMEOUT_MS,
            tool_policies: Vec::new(),
        };
        let audit = audit_tool(
            &server,
            &JsonValue::object([
                ("name", JsonValue::string("delete_file")),
                (
                    "annotations",
                    JsonValue::object([
                        ("destructiveHint", JsonValue::bool(true)),
                        ("readOnlyHint", JsonValue::bool(false)),
                    ]),
                ),
            ]),
        );

        assert!(audit.has_annotations);
        assert!(audit.has_advisory_risk);
        assert!(audit.guard_recommended);
        assert!(!audit.policy_covered);
        assert_eq!(
            json_helpers::string_at_path(&audit.value, &["policyStatus"]),
            Some("unprotected-guard-recommended")
        );
        assert!(
            json_helpers::array_at_path(&audit.value, &["advisoryRiskClasses"])
                .unwrap()
                .iter()
                .any(|value| value.as_str() == Some("mutation"))
        );
    }

    #[test]
    fn tool_policy_audit_reports_declarative_policy_coverage() {
        let server = UpstreamServerConfig {
            name: "audit-fixture".to_string(),
            enabled: true,
            disabled_reason: None,
            source_type: "stdio".to_string(),
            command: Some("tool".to_string()),
            args: Vec::new(),
            env: BTreeMap::new(),
            cwd: None,
            url: None,
            timeout_ms: DEFAULT_TIMEOUT_MS,
            tool_policies: vec![ToolRiskPolicy {
                tools: vec!["write_*".to_string()],
                risk_class: Some("filesystem-mutation".to_string()),
                allow_argument: Some("allowFilesystemMutation".to_string()),
                description: Some("writes project files".to_string()),
            }],
        };
        let audit = audit_tool(
            &server,
            &JsonValue::object([("name", JsonValue::string("write_file"))]),
        );

        assert!(audit.has_advisory_risk);
        assert!(audit.guard_recommended);
        assert!(audit.policy_covered);
        assert!(!audit.review_recommended);
        assert_eq!(
            json_helpers::string_at_path(&audit.value, &["policyStatus"]),
            Some("covered-advisory-risk")
        );
        let policies = json_helpers::array_at_path(&audit.value, &["matchingPolicies"]).unwrap();
        assert_eq!(policies.len(), 1);
        assert_eq!(
            json_helpers::string_at_path(&policies[0], &["allowArgument"]),
            Some("allowFilesystemMutation")
        );
    }

    #[test]
    fn policy_suggestions_group_unprotected_guarded_tools_by_generated_risk_class() {
        let audit = JsonValue::object([(
            "servers",
            JsonValue::array([JsonValue::object([
                ("name", JsonValue::string("alpha-tools")),
                ("ok", JsonValue::bool(true)),
                (
                    "tools",
                    JsonValue::array([
                        JsonValue::object([
                            ("name", JsonValue::string("delete_item")),
                            ("guardRecommended", JsonValue::bool(true)),
                            ("policyCovered", JsonValue::bool(false)),
                            (
                                "policyStatus",
                                JsonValue::string("unprotected-guard-recommended"),
                            ),
                            (
                                "advisoryRiskClasses",
                                JsonValue::array([JsonValue::string("mutation")]),
                            ),
                            (
                                "advisorySignals",
                                JsonValue::array([JsonValue::string("name-token:delete")]),
                            ),
                        ]),
                        JsonValue::object([
                            ("name", JsonValue::string("write_item")),
                            ("guardRecommended", JsonValue::bool(true)),
                            ("policyCovered", JsonValue::bool(false)),
                            (
                                "policyStatus",
                                JsonValue::string("unprotected-guard-recommended"),
                            ),
                            (
                                "advisoryRiskClasses",
                                JsonValue::array([JsonValue::string("not-readonly")]),
                            ),
                            (
                                "advisorySignals",
                                JsonValue::array([JsonValue::string("mcp.readOnlyHint=false")]),
                            ),
                        ]),
                    ]),
                ),
            ])]),
        )]);

        let report = policy_suggestion_report(&audit);
        assert_eq!(
            json_helpers::value_at_path(&report, &["suggestedPolicyCount"])
                .and_then(JsonValue::as_i64),
            Some(1)
        );
        assert_eq!(
            json_helpers::value_at_path(&report, &["suggestedToolCount"])
                .and_then(JsonValue::as_i64),
            Some(2)
        );
        let suggestions = json_helpers::array_at_path(&report, &["suggestions"]).unwrap();
        let suggestion = &suggestions[0];
        assert_eq!(
            json_helpers::string_at_path(suggestion, &["server"]),
            Some("alpha-tools")
        );
        assert_eq!(
            json_helpers::string_at_path(suggestion, &["policy", "riskClass"]),
            Some("alpha-tools-mutation")
        );
        assert_eq!(
            json_helpers::string_at_path(suggestion, &["policy", "allowArgument"]),
            Some("allowAlphaToolsMutation")
        );
        assert_eq!(
            json_helpers::string_at_path(suggestion, &["confidence"]),
            Some("high")
        );
        let tools = json_helpers::array_at_path(suggestion, &["policy", "tools"]).unwrap();
        assert_eq!(tools.len(), 2);
    }

    #[test]
    fn policy_suggestions_keep_interaction_control_as_stable_cross_server_risk_class() {
        let audit = JsonValue::object([(
            "servers",
            JsonValue::array([JsonValue::object([
                ("name", JsonValue::string("interactive")),
                ("ok", JsonValue::bool(true)),
                (
                    "tools",
                    JsonValue::array([JsonValue::object([
                        ("name", JsonValue::string("page_navigate")),
                        ("guardRecommended", JsonValue::bool(true)),
                        ("policyCovered", JsonValue::bool(false)),
                        (
                            "policyStatus",
                            JsonValue::string("unprotected-guard-recommended"),
                        ),
                        (
                            "advisoryRiskClasses",
                            JsonValue::array([JsonValue::string("interaction-control")]),
                        ),
                        (
                            "advisorySignals",
                            JsonValue::array([JsonValue::string("name-token:navigate")]),
                        ),
                    ])]),
                ),
            ])]),
        )]);

        let report = policy_suggestion_report(&audit);
        let suggestions = json_helpers::array_at_path(&report, &["suggestions"]).unwrap();
        assert_eq!(
            json_helpers::string_at_path(&suggestions[0], &["policy", "riskClass"]),
            Some("interaction-control")
        );
        assert_eq!(
            json_helpers::string_at_path(&suggestions[0], &["policy", "allowArgument"]),
            Some("allowInteractionControl")
        );
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
