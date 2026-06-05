use super::response::{now_ms, sanitize_root_path};
use super::{
    run_json_command, run_json_command_vec, CachedHealth, CachedOverview, DashboardConfig,
    ServeSurface,
};
use crate::json::JsonValue;
use crate::json_helpers;
use crate::resources;
use std::collections::BTreeMap;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

pub(super) fn cached_health_json(
    config: &DashboardConfig,
    refresh: bool,
) -> Result<JsonValue, String> {
    if config.health_cache_ttl.is_zero() {
        return build_health_json(config).map(|value| {
            with_runtime_cache_metadata(
                value,
                config,
                CacheMetadata {
                    hit: false,
                    bypassed: true,
                    stale: false,
                    ttl: config.health_cache_ttl,
                    age: None,
                    refresh_error: None,
                },
            )
        });
    }

    let mut guard = config
        .health_cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if !refresh {
        if let Some(cached) = guard.as_ref() {
            let age = cached.stored_at.elapsed();
            if age <= config.health_cache_ttl {
                return Ok(with_runtime_cache_metadata(
                    cached.value.clone(),
                    config,
                    CacheMetadata {
                        hit: true,
                        bypassed: false,
                        stale: false,
                        ttl: config.health_cache_ttl,
                        age: Some(age),
                        refresh_error: None,
                    },
                ));
            }
        }
    }

    match build_health_json(config) {
        Ok(value) => {
            *guard = Some(CachedHealth {
                stored_at: Instant::now(),
                value: value.clone(),
            });
            Ok(with_runtime_cache_metadata(
                value,
                config,
                CacheMetadata {
                    hit: false,
                    bypassed: refresh,
                    stale: false,
                    ttl: config.health_cache_ttl,
                    age: Some(Duration::from_millis(0)),
                    refresh_error: None,
                },
            ))
        }
        Err(error) => {
            if let Some(cached) = guard.as_ref() {
                return Ok(with_runtime_cache_metadata(
                    cached.value.clone(),
                    config,
                    CacheMetadata {
                        hit: true,
                        bypassed: refresh,
                        stale: true,
                        ttl: config.health_cache_ttl,
                        age: Some(cached.stored_at.elapsed()),
                        refresh_error: Some(error),
                    },
                ));
            }
            Err(error)
        }
    }
}

pub(super) fn build_health_json(config: &DashboardConfig) -> Result<JsonValue, String> {
    let readiness = run_json_command(&config.root_path, &["verify", "readiness", "--json"])?;
    let ok = json_helpers::bool_at_path(&readiness, &["readyForRuntimeOps"]).unwrap_or(false);
    Ok(JsonValue::object([
        ("ok", JsonValue::bool(ok)),
        ("generatedAtMs", JsonValue::number(now_ms())),
        ("readiness", readiness),
    ]))
}

pub(super) fn cached_overview_json(
    config: &DashboardConfig,
    refresh: bool,
) -> Result<JsonValue, String> {
    if config.overview_cache_ttl.is_zero() {
        return build_overview_json(&config.root_path).map(|value| {
            with_runtime_cache_metadata(
                value,
                config,
                CacheMetadata {
                    hit: false,
                    bypassed: true,
                    stale: false,
                    ttl: config.overview_cache_ttl,
                    age: None,
                    refresh_error: None,
                },
            )
        });
    }

    let mut guard = config
        .overview_cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if !refresh {
        if let Some(cached) = guard.as_ref() {
            let age = cached.stored_at.elapsed();
            if age <= config.overview_cache_ttl {
                return Ok(with_runtime_cache_metadata(
                    cached.value.clone(),
                    config,
                    CacheMetadata {
                        hit: true,
                        bypassed: false,
                        stale: false,
                        ttl: config.overview_cache_ttl,
                        age: Some(age),
                        refresh_error: None,
                    },
                ));
            }
        }
    }

    match build_overview_json(&config.root_path) {
        Ok(value) => {
            *guard = Some(CachedOverview {
                stored_at: Instant::now(),
                value: value.clone(),
            });
            Ok(with_runtime_cache_metadata(
                value,
                config,
                CacheMetadata {
                    hit: false,
                    bypassed: refresh,
                    stale: false,
                    ttl: config.overview_cache_ttl,
                    age: Some(Duration::from_millis(0)),
                    refresh_error: None,
                },
            ))
        }
        Err(error) => {
            if let Some(cached) = guard.as_ref() {
                return Ok(with_runtime_cache_metadata(
                    cached.value.clone(),
                    config,
                    CacheMetadata {
                        hit: true,
                        bypassed: refresh,
                        stale: true,
                        ttl: config.overview_cache_ttl,
                        age: Some(cached.stored_at.elapsed()),
                        refresh_error: Some(error),
                    },
                ));
            }
            Err(error)
        }
    }
}

struct CacheMetadata {
    hit: bool,
    bypassed: bool,
    stale: bool,
    ttl: Duration,
    age: Option<Duration>,
    refresh_error: Option<String>,
}

fn with_runtime_cache_metadata(
    mut value: JsonValue,
    config: &DashboardConfig,
    cache: CacheMetadata,
) -> JsonValue {
    if let JsonValue::Object(map) = &mut value {
        map.insert("cache".to_string(), cache_metadata_json(cache));
        map.insert("runtime".to_string(), runtime_status_json(config));
    }
    value
}

fn cache_metadata_json(cache: CacheMetadata) -> JsonValue {
    let mut entries = vec![
        ("hit".to_string(), JsonValue::bool(cache.hit)),
        ("bypassed".to_string(), JsonValue::bool(cache.bypassed)),
        ("stale".to_string(), JsonValue::bool(cache.stale)),
        (
            "ttlMs".to_string(),
            JsonValue::number(cache.ttl.as_millis()),
        ),
    ];
    if let Some(age) = cache.age {
        entries.push(("ageMs".to_string(), JsonValue::number(age.as_millis())));
    }
    if let Some(error) = cache.refresh_error {
        entries.push(("refreshError".to_string(), JsonValue::string(error)));
    }
    JsonValue::object(entries)
}

pub(super) fn runtime_resources_response(config: &DashboardConfig) -> JsonValue {
    JsonValue::object([
        ("ok", JsonValue::bool(true)),
        ("generatedAtMs", JsonValue::number(now_ms())),
        ("runtime", runtime_status_json(config)),
    ])
}

pub(super) fn runtime_status_json(config: &DashboardConfig) -> JsonValue {
    let metrics = config.metrics.snapshot();
    let mut upstream_pool_size = 0usize;
    let mut upstream_pool_max_size = 0usize;
    let mut upstream_pool_idle_ttl_ms = 0u128;
    let mut upstream_pool_locked_shards = 0usize;
    let mut upstream_pool_evicted_idle_count = 0usize;
    let mut upstream_session_snapshots = Vec::new();

    for pool_lock in &config.upstream_session_pools {
        if let Ok(mut pool) = pool_lock.lock() {
            upstream_pool_evicted_idle_count =
                upstream_pool_evicted_idle_count.saturating_add(pool.purge_idle_and_exited());
            upstream_pool_size = upstream_pool_size.saturating_add(pool.session_count());
            upstream_pool_max_size =
                upstream_pool_max_size.saturating_add(pool.max_session_count());
            upstream_pool_idle_ttl_ms = pool.idle_ttl_ms();
            upstream_pool_locked_shards = upstream_pool_locked_shards.saturating_add(1);
            upstream_session_snapshots.extend(
                pool.session_snapshots()
                    .into_iter()
                    .map(|snapshot| snapshot.to_json_value()),
            );
        }
    }

    let http_session_snapshot = config
        .http_session_store
        .lock()
        .map(|mut store| store.snapshot(now_ms()))
        .ok();

    JsonValue::object([
        (
            "surface",
            JsonValue::string(match config.surface {
                ServeSurface::Dashboard => "dashboard-http",
                ServeSurface::UnifiedServe => "unified-serve-http",
            }),
        ),
        (
            "availableParallelism",
            JsonValue::number(resources::available_parallelism()),
        ),
        (
            "http",
            JsonValue::object([
                ("maxConnections", JsonValue::number(config.max_connections)),
                (
                    "activeConnections",
                    JsonValue::number(metrics.active_connections),
                ),
                (
                    "acceptedConnections",
                    JsonValue::number(metrics.accepted_connections),
                ),
                (
                    "completedConnections",
                    JsonValue::number(metrics.completed_connections),
                ),
                (
                    "failedConnections",
                    JsonValue::number(metrics.failed_connections),
                ),
                (
                    "maxObservedActiveConnections",
                    JsonValue::number(metrics.max_active_connections),
                ),
                (
                    "requestDurationTotalMs",
                    JsonValue::number(metrics.total_request_duration_ms),
                ),
                (
                    "requestDurationAverageMs",
                    JsonValue::number(metrics.average_request_duration_ms),
                ),
                (
                    "requestDurationMaxMs",
                    JsonValue::number(metrics.max_request_duration_ms),
                ),
                (
                    "ioTimeoutMs",
                    JsonValue::number(config.io_timeout.as_millis()),
                ),
                ("maxBodyBytes", JsonValue::number(config.max_body_bytes)),
                (
                    "maxRequestLineBytes",
                    JsonValue::number(resources::MAX_HTTP_REQUEST_LINE_BYTES),
                ),
                (
                    "maxHeaderLineBytes",
                    JsonValue::number(resources::MAX_HTTP_HEADER_LINE_BYTES),
                ),
                (
                    "maxHeaderBytes",
                    JsonValue::number(resources::MAX_HTTP_HEADER_BYTES),
                ),
                (
                    "maxHeaderCount",
                    JsonValue::number(resources::MAX_HTTP_HEADER_COUNT),
                ),
            ]),
        ),
        (
            "caches",
            JsonValue::object([
                (
                    "overviewTtlMs",
                    JsonValue::number(config.overview_cache_ttl.as_millis()),
                ),
                (
                    "healthTtlMs",
                    JsonValue::number(config.health_cache_ttl.as_millis()),
                ),
            ]),
        ),
        (
            "httpSessionStore",
            JsonValue::object([
                (
                    "size",
                    JsonValue::number(
                        http_session_snapshot
                            .as_ref()
                            .map(|snapshot| snapshot.session_count)
                            .unwrap_or_default(),
                    ),
                ),
                (
                    "maxSize",
                    JsonValue::number(
                        http_session_snapshot
                            .as_ref()
                            .map(|snapshot| snapshot.max_sessions)
                            .unwrap_or_default(),
                    ),
                ),
                (
                    "ttlMs",
                    JsonValue::number(
                        http_session_snapshot
                            .as_ref()
                            .map(|snapshot| snapshot.ttl_ms)
                            .unwrap_or_default(),
                    ),
                ),
                (
                    "prunedExpiredSessions",
                    JsonValue::number(
                        http_session_snapshot
                            .as_ref()
                            .map(|snapshot| snapshot.pruned_expired_sessions)
                            .unwrap_or_default(),
                    ),
                ),
                (
                    "oldestCreatedAtMs",
                    http_session_snapshot
                        .as_ref()
                        .and_then(|snapshot| snapshot.oldest_created_at_ms)
                        .map(JsonValue::number)
                        .unwrap_or(JsonValue::Null),
                ),
                (
                    "newestLastSeenAtMs",
                    http_session_snapshot
                        .as_ref()
                        .and_then(|snapshot| snapshot.newest_last_seen_at_ms)
                        .map(JsonValue::number)
                        .unwrap_or(JsonValue::Null),
                ),
                (
                    "namedClientSessions",
                    JsonValue::number(
                        http_session_snapshot
                            .as_ref()
                            .map(|snapshot| snapshot.named_client_sessions)
                            .unwrap_or_default(),
                    ),
                ),
                (
                    "versionedClientSessions",
                    JsonValue::number(
                        http_session_snapshot
                            .as_ref()
                            .map(|snapshot| snapshot.versioned_client_sessions)
                            .unwrap_or_default(),
                    ),
                ),
                (
                    "mcpaceGeneratedSessions",
                    JsonValue::number(
                        http_session_snapshot
                            .as_ref()
                            .map(|snapshot| snapshot.mcpace_generated_sessions)
                            .unwrap_or_default(),
                    ),
                ),
                ("locked", JsonValue::bool(http_session_snapshot.is_some())),
            ]),
        ),
        (
            "upstreamSessionPool",
            JsonValue::object([
                ("size", JsonValue::number(upstream_pool_size)),
                ("maxSize", JsonValue::number(upstream_pool_max_size)),
                ("idleTtlMs", JsonValue::number(upstream_pool_idle_ttl_ms)),
                (
                    "evictedIdleCount",
                    JsonValue::number(upstream_pool_evicted_idle_count),
                ),
                (
                    "shardCount",
                    JsonValue::number(config.upstream_session_pools.len()),
                ),
                (
                    "lockedShardCount",
                    JsonValue::number(upstream_pool_locked_shards),
                ),
                (
                    "sessions",
                    JsonValue::array(upstream_session_snapshots.clone()),
                ),
            ]),
        ),
        (
            "serverResourceMonitoring",
            server_resource_monitoring_json(&upstream_session_snapshots),
        ),
    ])
}

#[derive(Default)]
struct ServerResourceRollup {
    sessions: usize,
    rss_bytes: u64,
    virtual_memory_bytes: u64,
    fd_count: u64,
    threads: u64,
    call_count: usize,
    pids: Vec<String>,
}

fn server_resource_monitoring_json(sessions: &[JsonValue]) -> JsonValue {
    let mut rollups = BTreeMap::<String, ServerResourceRollup>::new();
    for session in sessions {
        let server = json_string(session, "server", "unknown");
        let rollup = rollups.entry(server).or_default();
        rollup.sessions = rollup.sessions.saturating_add(1);
        rollup.call_count = rollup
            .call_count
            .saturating_add(json_usize(session, "callCount", 0));
        if let Some(pid) = session.get("pid").and_then(JsonValue::as_i64) {
            rollup.pids.push(pid.to_string());
        }
        if let Some(resource) = session.get("resource") {
            rollup.rss_bytes = rollup
                .rss_bytes
                .saturating_add(json_u64(resource, "rssBytes", 0));
            rollup.virtual_memory_bytes = rollup.virtual_memory_bytes.saturating_add(json_u64(
                resource,
                "virtualMemoryBytes",
                0,
            ));
            rollup.fd_count = rollup
                .fd_count
                .saturating_add(json_u64(resource, "fdCount", 0));
            rollup.threads = rollup
                .threads
                .saturating_add(json_u64(resource, "threads", 0));
        }
    }

    let items = rollups
        .into_iter()
        .map(|(server, rollup)| {
            JsonValue::object([
                ("server", JsonValue::string(server)),
                ("sessions", JsonValue::number(rollup.sessions)),
                (
                    "pids",
                    JsonValue::array(rollup.pids.into_iter().map(JsonValue::string)),
                ),
                ("rssBytes", JsonValue::number(rollup.rss_bytes)),
                (
                    "virtualMemoryBytes",
                    JsonValue::number(rollup.virtual_memory_bytes),
                ),
                ("fdCount", JsonValue::number(rollup.fd_count)),
                ("threads", JsonValue::number(rollup.threads)),
                ("callCount", JsonValue::number(rollup.call_count)),
            ])
        })
        .collect::<Vec<_>>();

    JsonValue::object([
        ("schema", JsonValue::string("mcpace.serverResourceMonitoring.v1")),
        (
            "status",
            JsonValue::string(if sessions.is_empty() {
                "waiting-for-live-upstream-sessions"
            } else {
                "live-pid-snapshot"
            }),
        ),
        ("sessionCount", JsonValue::number(sessions.len())),
        ("items", JsonValue::array(items)),
        (
            "note",
            JsonValue::string("Per-server resource rows are live for pooled stdio upstream sessions. Servers with no active process stay represented by overview policy and evidence, not by fake metrics."),
        ),
    ])
}

pub(super) fn query_bool_flag(query: &str, key: &str) -> bool {
    query.split('&').any(|part| {
        let (name, value) = part.split_once('=').unwrap_or((part, ""));
        if !name.eq_ignore_ascii_case(key) {
            return false;
        }
        value.is_empty()
            || value == "1"
            || value.eq_ignore_ascii_case("true")
            || value.eq_ignore_ascii_case("yes")
    })
}

pub(super) fn build_overview_json(root_path: &Path) -> Result<JsonValue, String> {
    let mut results = run_json_commands_parallel(
        root_path,
        vec![
            ("clients", vec!["client", "list", "--json"]),
            ("doctor", vec!["doctor", "--json"]),
            ("hub", vec!["hub", "status", "--json"]),
            ("instances", vec!["server", "instances", "--json"]),
            ("leases", vec!["hub", "lease", "list", "--json"]),
            ("readiness", vec!["verify", "readiness", "--json"]),
            ("servers", vec!["server", "list", "--json"]),
        ],
    )?;

    let doctor = take_parallel_result(&mut results, "doctor")?;
    let hub = take_parallel_result(&mut results, "hub")?;
    let readiness = take_parallel_result(&mut results, "readiness")?;
    let servers = take_parallel_result(&mut results, "servers")?;
    let instances = take_parallel_result(&mut results, "instances")?;
    let clients = take_parallel_result(&mut results, "clients")?;
    let leases = take_parallel_result(&mut results, "leases")?;
    let operator_plan = build_operator_plan_json(&servers, &instances, &readiness, &leases);
    let user_readiness =
        build_user_readiness_json(&operator_plan, &servers, &clients, &hub, &readiness);
    let cached_tool_evidence = crate::upstream::callable_tools_cached_catalog(root_path)
        .unwrap_or_else(cached_tool_evidence_error_json);
    let runtime_control_plane = build_runtime_control_plane_json(
        &servers,
        &instances,
        &operator_plan,
        &cached_tool_evidence,
    );

    Ok(JsonValue::object([
        ("generatedAtMs", JsonValue::number(now_ms())),
        (
            "rootPath",
            JsonValue::string(sanitize_root_path(&root_path.display().to_string())),
        ),
        ("doctor", doctor),
        ("hub", hub),
        ("readiness", readiness),
        ("servers", servers),
        ("instances", instances),
        ("clients", clients),
        ("leases", leases),
        ("cachedToolEvidence", cached_tool_evidence),
        ("operatorPlan", operator_plan),
        ("userReadiness", user_readiness),
        ("runtimeControlPlane", runtime_control_plane),
    ]))
}

fn cached_tool_evidence_error_json(error: String) -> JsonValue {
    JsonValue::object([
        ("ok", JsonValue::bool(false)),
        ("mode", JsonValue::string("raw-callable-tools-cache")),
        ("serverCount", JsonValue::number(0)),
        ("okCount", JsonValue::number(0)),
        ("failedCount", JsonValue::number(0)),
        ("cacheMissCount", JsonValue::number(0)),
        ("toolCount", JsonValue::number(0)),
        ("servers", JsonValue::array([])),
        ("error", JsonValue::string(error)),
    ])
}

fn build_runtime_control_plane_json(
    servers: &JsonValue,
    instances: &JsonValue,
    operator_plan: &JsonValue,
    cached_tool_evidence: &JsonValue,
) -> JsonValue {
    let server_items = json_items(servers, &["servers", "items"]);
    let instance_items = json_items(instances, &["instances", "items"]);
    let plan_items = json_items(operator_plan, &["items"]);
    let mut summary = RuntimeControlSummary::default();
    let mut items = Vec::new();

    for server in server_items {
        let name = json_string(server, "name", "server");
        let related = related_instances_for_server(server, instance_items);
        let operator = plan_items
            .iter()
            .find(|item| json_string(item, "name", "") == name);
        let cached_tools = cached_tools_for_server(cached_tool_evidence, &name);
        let item =
            runtime_control_item_for_server(server, &related, operator, cached_tools.as_ref());
        summary.count(&item);
        items.push(item.to_json_value());
    }

    JsonValue::object([
        ("schema", JsonValue::string("mcpace.runtimeControlPlane.v1")),
        ("summary", summary.to_json_value()),
        ("items", JsonValue::array(items)),
        (
            "decisionOrder",
            JsonValue::array([
                JsonValue::string("liveEvidence"),
                JsonValue::string("toolRisk"),
                JsonValue::string("parallelism"),
                JsonValue::string("isolation"),
                JsonValue::string("resourceBudget"),
            ]),
        ),
        (
            "statement",
            JsonValue::string("Static server metadata proposes a route; live initialize/tools-list evidence and observed resource use decide whether MCPace can safely widen concurrency or isolation."),
        ),
    ])
}

#[derive(Default)]
struct RuntimeControlSummary {
    total: usize,
    no_live_evidence: usize,
    read_only: usize,
    mutating: usize,
    destructive: usize,
    approval_required: usize,
    shared_ok: usize,
    serialized: usize,
    container_optional: usize,
    container_required: usize,
    native_restricted: usize,
    external_http: usize,
}

impl RuntimeControlSummary {
    fn count(&mut self, item: &RuntimeControlItem) {
        self.total = self.total.saturating_add(1);
        if !has_live_tool_evidence(&item.evidence_state) {
            self.no_live_evidence = self.no_live_evidence.saturating_add(1);
        }
        match item.tool_risk.risk.as_str() {
            "read-only" => self.read_only = self.read_only.saturating_add(1),
            "destructive" => self.destructive = self.destructive.saturating_add(1),
            "mutating" | "credential" | "network" | "filesystem" => {
                self.mutating = self.mutating.saturating_add(1)
            }
            _ => {}
        }
        if item.tool_risk.approval_required {
            self.approval_required = self.approval_required.saturating_add(1);
        }
        if item.parallelism.mode == "shared" || item.parallelism.mode == "pool" {
            self.shared_ok = self.shared_ok.saturating_add(1);
        } else {
            self.serialized = self.serialized.saturating_add(1);
        }
        match item.isolation.mode.as_str() {
            "container-optional" => {
                self.container_optional = self.container_optional.saturating_add(1)
            }
            "container-required" => {
                self.container_required = self.container_required.saturating_add(1)
            }
            "native-restricted" | "native-guarded" => {
                self.native_restricted = self.native_restricted.saturating_add(1)
            }
            "external-http" => self.external_http = self.external_http.saturating_add(1),
            _ => {}
        }
    }

    fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("total", JsonValue::number(self.total)),
            ("noLiveEvidence", JsonValue::number(self.no_live_evidence)),
            ("readOnly", JsonValue::number(self.read_only)),
            ("mutating", JsonValue::number(self.mutating)),
            ("destructive", JsonValue::number(self.destructive)),
            (
                "approvalRequired",
                JsonValue::number(self.approval_required),
            ),
            ("sharedOk", JsonValue::number(self.shared_ok)),
            ("serialized", JsonValue::number(self.serialized)),
            (
                "containerOptional",
                JsonValue::number(self.container_optional),
            ),
            (
                "containerRequired",
                JsonValue::number(self.container_required),
            ),
            (
                "nativeRestricted",
                JsonValue::number(self.native_restricted),
            ),
            ("externalHttp", JsonValue::number(self.external_http)),
        ])
    }
}

struct RuntimeControlItem {
    name: String,
    enabled: bool,
    evidence_state: String,
    tool_risk: ToolRiskDecision,
    parallelism: ParallelismDecision,
    isolation: IsolationDecision,
    resource_budget: ResourceBudgetDecision,
    next_gate: String,
    why: String,
}

impl RuntimeControlItem {
    fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("name", JsonValue::string(self.name.clone())),
            ("enabled", JsonValue::bool(self.enabled)),
            (
                "evidenceState",
                JsonValue::string(self.evidence_state.clone()),
            ),
            ("toolRisk", self.tool_risk.to_json_value()),
            ("parallelism", self.parallelism.to_json_value()),
            ("isolation", self.isolation.to_json_value()),
            ("resourceBudget", self.resource_budget.to_json_value()),
            ("nextGate", JsonValue::string(self.next_gate.clone())),
            ("why", JsonValue::string(self.why.clone())),
        ])
    }
}

struct ToolRiskDecision {
    risk: String,
    approval_required: bool,
    categories: Vec<String>,
    signals: Vec<String>,
}

impl ToolRiskDecision {
    fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("risk", JsonValue::string(self.risk.clone())),
            ("approvalRequired", JsonValue::bool(self.approval_required)),
            (
                "categories",
                JsonValue::array(self.categories.iter().cloned().map(JsonValue::string)),
            ),
            (
                "signals",
                JsonValue::array(self.signals.iter().cloned().map(JsonValue::string)),
            ),
        ])
    }
}

struct ParallelismDecision {
    mode: String,
    max_workers: usize,
    max_in_flight_per_worker: usize,
    admission: String,
    lock_scope: String,
    reason: String,
}

impl ParallelismDecision {
    fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("mode", JsonValue::string(self.mode.clone())),
            ("maxWorkers", JsonValue::number(self.max_workers)),
            (
                "maxInFlightPerWorker",
                JsonValue::number(self.max_in_flight_per_worker),
            ),
            ("admission", JsonValue::string(self.admission.clone())),
            ("lockScope", JsonValue::string(self.lock_scope.clone())),
            ("reason", JsonValue::string(self.reason.clone())),
        ])
    }
}

struct IsolationDecision {
    mode: String,
    container_compatible: bool,
    reason: String,
    required_before_enable: bool,
}

impl IsolationDecision {
    fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("mode", JsonValue::string(self.mode.clone())),
            (
                "containerCompatible",
                JsonValue::bool(self.container_compatible),
            ),
            ("reason", JsonValue::string(self.reason.clone())),
            (
                "requiredBeforeEnable",
                JsonValue::bool(self.required_before_enable),
            ),
        ])
    }
}

struct ResourceBudgetDecision {
    class: String,
    memory_hint_mb: usize,
    cpu_hint: String,
    monitor: String,
}

impl ResourceBudgetDecision {
    fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("class", JsonValue::string(self.class.clone())),
            ("memoryHintMb", JsonValue::number(self.memory_hint_mb)),
            ("cpuHint", JsonValue::string(self.cpu_hint.clone())),
            ("monitor", JsonValue::string(self.monitor.clone())),
        ])
    }
}

fn runtime_control_item_for_server(
    server: &JsonValue,
    instances: &[JsonValue],
    operator: Option<&JsonValue>,
    cached_tools: Option<&JsonValue>,
) -> RuntimeControlItem {
    let name = json_string(server, "name", "server");
    let enabled = json_bool(server, "effectiveEnabled", false);
    let evidence_state = evidence_state_for_server(server, cached_tools);
    let tool_risk = infer_tool_risk(server, cached_tools);
    let parallelism = infer_parallelism(server, instances, &tool_risk, &evidence_state);
    let isolation = infer_isolation(server, &tool_risk, &parallelism);
    let resource_budget = infer_resource_budget(server, &tool_risk, &parallelism, &isolation);
    let next_gate = if !has_live_tool_evidence(&evidence_state) {
        "probe-tools-list"
    } else if isolation.required_before_enable {
        "choose-isolation"
    } else if tool_risk.approval_required {
        "approval-policy"
    } else if parallelism.admission != "open" {
        "keep-serialized"
    } else {
        "usable"
    }
    .to_string();
    let why = operator
        .and_then(|value| json_string_opt(value, "rationale"))
        .unwrap_or_else(|| {
            format!(
                "{} evidence, {} risk, {} routing, {} isolation",
                evidence_state, tool_risk.risk, parallelism.mode, isolation.mode
            )
        });

    RuntimeControlItem {
        name,
        enabled,
        evidence_state,
        tool_risk,
        parallelism,
        isolation,
        resource_budget,
        next_gate,
        why,
    }
}

fn evidence_state_for_server(server: &JsonValue, cached_tools: Option<&JsonValue>) -> String {
    if json_usize(server, "toolCount", 0) > 0
        || !json_array(server, "toolNames").is_empty()
        || !json_array(server, "tools").is_empty()
    {
        "live-tools".to_string()
    } else if let Some(tools) = cached_tools {
        if tools
            .as_array()
            .map(|items| !items.is_empty())
            .unwrap_or(false)
        {
            return "cached-tools".to_string();
        }
        if json_bool(server, "effectiveEnabled", false) {
            "enabled-unprobed".to_string()
        } else {
            "parked-unprobed".to_string()
        }
    } else if json_bool(server, "effectiveEnabled", false) {
        "enabled-unprobed".to_string()
    } else {
        "parked-unprobed".to_string()
    }
}

fn has_live_tool_evidence(evidence_state: &str) -> bool {
    matches!(evidence_state, "live-tools" | "cached-tools")
}

fn cached_tools_for_server(
    cached_tool_evidence: &JsonValue,
    server_name: &str,
) -> Option<JsonValue> {
    json_items(cached_tool_evidence, &["servers"])
        .iter()
        .find(|item| {
            json_string(item, "name", "") == server_name
                && json_bool(item, "ok", false)
                && json_bool(item, "cacheHit", false)
        })
        .and_then(|item| item.get("tools").cloned())
}

fn infer_tool_risk(server: &JsonValue, cached_tools: Option<&JsonValue>) -> ToolRiskDecision {
    let mut categories = Vec::new();
    let mut signals = Vec::new();
    fn add_signal(
        categories: &mut Vec<String>,
        signals: &mut Vec<String>,
        category: &str,
        signal: String,
    ) {
        if !categories.iter().any(|item| item == category) {
            categories.push(category.to_string());
        }
        signals.push(signal);
    }

    let fields = [
        json_string(server, "runtimeType", ""),
        json_string(server, "stateClass", ""),
        json_string(server, "effectClass", ""),
        json_string(server, "credentialBinding", ""),
        json_string(server, "scopeClass", ""),
        json_string(server, "sourceType", ""),
        json_string(server, "sourceCommand", ""),
    ]
    .join(" ")
    .to_ascii_lowercase();

    for (needle, category) in [
        ("delete", "destructive"),
        ("remove", "destructive"),
        ("write", "mutating"),
        ("update", "mutating"),
        ("create", "mutating"),
        ("insert", "mutating"),
        ("exec", "process"),
        ("shell", "process"),
        ("run", "process"),
        ("credential", "credential"),
        ("token", "credential"),
        ("secret", "credential"),
        ("network", "network"),
        ("external", "network"),
        ("http", "network"),
        ("file", "filesystem"),
        ("filesystem", "filesystem"),
        ("sqlite", "stateful"),
        ("stateful", "stateful"),
        ("browser", "desktop"),
        ("playwright", "desktop"),
        ("desktop", "desktop"),
        ("unknown", "unknown"),
    ] {
        if fields.contains(needle) {
            add_signal(
                &mut categories,
                &mut signals,
                category,
                format!("metadata:{needle}"),
            );
        }
    }

    for tool in tools_for_server(server, cached_tools) {
        let tool_name = json_string(&tool, "name", "tool");
        let lower = tool_descriptor_text(&tool);
        if json_helpers::bool_at_path(&tool, &["annotations", "destructiveHint"]) == Some(true) {
            add_signal(
                &mut categories,
                &mut signals,
                "destructive",
                format!("annotation:destructiveHint:{tool_name}"),
            );
        }
        if json_helpers::bool_at_path(&tool, &["annotations", "readOnlyHint"]) == Some(false) {
            add_signal(
                &mut categories,
                &mut signals,
                "mutating",
                format!("annotation:readOnlyHint=false:{tool_name}"),
            );
        }
        if json_helpers::bool_at_path(&tool, &["annotations", "openWorldHint"]) == Some(true) {
            add_signal(
                &mut categories,
                &mut signals,
                "network",
                format!("annotation:openWorldHint:{tool_name}"),
            );
        }
        for (needle, category) in [
            ("delete", "destructive"),
            ("remove", "destructive"),
            ("destroy", "destructive"),
            ("drop", "destructive"),
            ("write", "mutating"),
            ("update", "mutating"),
            ("create", "mutating"),
            ("patch", "mutating"),
            ("insert", "mutating"),
            ("exec", "process"),
            ("shell", "process"),
            ("command", "process"),
            ("run", "process"),
            ("login", "credential"),
            ("password", "credential"),
            ("token", "credential"),
            ("secret", "credential"),
            ("api_key", "credential"),
            ("apikey", "credential"),
            ("fetch", "network"),
            ("http", "network"),
            ("url", "network"),
            ("request", "network"),
            ("read_file", "filesystem"),
            ("write_file", "filesystem"),
            ("path", "filesystem"),
            ("filename", "filesystem"),
            ("browser", "desktop"),
            ("click", "desktop"),
        ] {
            if lower.contains(needle) {
                add_signal(
                    &mut categories,
                    &mut signals,
                    category,
                    format!("tool:{tool_name}:{needle}"),
                );
            }
        }
    }

    if categories.is_empty() {
        if fields.contains("read-only")
            || fields.contains("readonly")
            || fields.contains("stateless")
        {
            categories.push("read-only".to_string());
            signals.push("metadata:read-only-or-stateless".to_string());
        } else {
            categories.push("unknown".to_string());
            signals.push("no-live-tool-risk-signal".to_string());
        }
    }

    let risk = if categories.iter().any(|item| item == "destructive") {
        "destructive"
    } else if categories.iter().any(|item| item == "credential") {
        "credential"
    } else if categories
        .iter()
        .any(|item| item == "process" || item == "mutating" || item == "stateful")
    {
        "mutating"
    } else if categories.iter().any(|item| item == "filesystem") {
        "filesystem"
    } else if categories
        .iter()
        .any(|item| item == "network" || item == "desktop")
    {
        "network"
    } else if categories.iter().any(|item| item == "read-only") {
        "read-only"
    } else {
        "unknown"
    }
    .to_string();

    let approval_required =
        !matches!(risk.as_str(), "read-only") || categories.iter().any(|item| item == "unknown");

    ToolRiskDecision {
        risk,
        approval_required,
        categories,
        signals,
    }
}

fn infer_parallelism(
    server: &JsonValue,
    instances: &[JsonValue],
    risk: &ToolRiskDecision,
    evidence_state: &str,
) -> ParallelismDecision {
    let current = route_mode_for_server(server, instances);
    let configured_workers = json_usize(server, "maxWorkers", 1);
    let configured_in_flight = json_usize(server, "maxInFlightPerWorker", 1);
    let lock_domains = json_array(server, "lockDomains")
        .iter()
        .filter_map(JsonValue::as_str)
        .collect::<Vec<_>>();
    let host_lock = json_string(server, "hostLock", "none");
    let lock_scope = if !lock_domains.is_empty() {
        lock_domains.join(",")
    } else if host_lock != "none" {
        host_lock
    } else {
        json_string(server, "conflictDomain", "none")
    };

    if current == "disabled" {
        return ParallelismDecision {
            mode: "disabled".to_string(),
            max_workers: 0,
            max_in_flight_per_worker: 0,
            admission: "disabled".to_string(),
            lock_scope,
            reason: "Disabled servers must not receive work or reserve worker capacity."
                .to_string(),
        };
    }

    if evidence_state != "live-tools" {
        return ParallelismDecision {
            mode: "serialized".to_string(),
            max_workers: 1,
            max_in_flight_per_worker: 1,
            admission: "probe-before-widening".to_string(),
            lock_scope,
            reason: "Live initialize/tools-list evidence is required before widening concurrency."
                .to_string(),
        };
    }

    if risk.approval_required || current == "serialized" || lock_scope != "none" {
        return ParallelismDecision {
            mode: if current == "project-isolated" || current == "session-isolated" {
                current
            } else {
                "serialized".to_string()
            },
            max_workers: 1,
            max_in_flight_per_worker: 1,
            admission: "lease-gated".to_string(),
            lock_scope,
            reason: "Risk, state, credentials, or lock domains require one-at-a-time admission."
                .to_string(),
        };
    }

    ParallelismDecision {
        mode: if current == "pool" {
            current
        } else {
            "shared".to_string()
        },
        max_workers: configured_workers.max(1),
        max_in_flight_per_worker: configured_in_flight.max(1),
        admission: "open".to_string(),
        lock_scope,
        reason:
            "Read-only/stateless evidence can use shared routing within configured worker limits."
                .to_string(),
    }
}

fn infer_isolation(
    server: &JsonValue,
    risk: &ToolRiskDecision,
    parallelism: &ParallelismDecision,
) -> IsolationDecision {
    let source_type = json_string(server, "sourceType", "unknown").to_ascii_lowercase();
    let command = json_string(server, "sourceCommand", "").to_ascii_lowercase();
    let fields = format!(
        "{} {} {} {}",
        source_type,
        command,
        risk.categories.join(" "),
        json_string(server, "launcherKind", "")
    )
    .to_ascii_lowercase();

    if source_type.contains("http") {
        return IsolationDecision {
            mode: "external-http".to_string(),
            container_compatible: false,
            reason: "HTTP upstreams are already outside MCPace's process tree; enforce transport/auth/origin boundaries instead of wrapping.".to_string(),
            required_before_enable: false,
        };
    }
    if fields.contains("browser") || fields.contains("playwright") || fields.contains("desktop") {
        return IsolationDecision {
            mode: "native-guarded".to_string(),
            container_compatible: false,
            reason: "Browser/desktop servers often need host UI, profile, or sockets and usually break in Docker.".to_string(),
            required_before_enable: false,
        };
    }
    if risk.risk == "destructive" || fields.contains("exec") || fields.contains("shell") {
        return IsolationDecision {
            mode: "container-required".to_string(),
            container_compatible: true,
            reason:
                "Process execution or destructive tools should not run unrestricted on the host."
                    .to_string(),
            required_before_enable: true,
        };
    }
    if risk.risk == "unknown"
        || fields.contains("npx")
        || fields.contains("uvx")
        || fields.contains("package")
    {
        return IsolationDecision {
            mode: "container-optional".to_string(),
            container_compatible: true,
            reason: "Unknown package sources benefit from optional sandboxing when the command is compatible.".to_string(),
            required_before_enable: false,
        };
    }
    if risk.approval_required || parallelism.mode != "shared" {
        return IsolationDecision {
            mode: "native-restricted".to_string(),
            container_compatible: false,
            reason: "Keep the server native but restrict routing, locks, and approval until stronger evidence exists.".to_string(),
            required_before_enable: false,
        };
    }
    IsolationDecision {
        mode: "native".to_string(),
        container_compatible: false,
        reason:
            "Low-risk, read-only, or stateless servers do not need container overhead by default."
                .to_string(),
        required_before_enable: false,
    }
}

fn infer_resource_budget(
    server: &JsonValue,
    risk: &ToolRiskDecision,
    parallelism: &ParallelismDecision,
    isolation: &IsolationDecision,
) -> ResourceBudgetDecision {
    let fields = format!(
        "{} {} {} {}",
        json_string(server, "sourceCommand", ""),
        json_string(server, "runtimeType", ""),
        json_string(server, "kind", ""),
        risk.categories.join(" ")
    )
    .to_ascii_lowercase();

    if fields.contains("browser") || fields.contains("playwright") {
        return ResourceBudgetDecision {
            class: "heavy".to_string(),
            memory_hint_mb: 768,
            cpu_hint: "bursty".to_string(),
            monitor: "require-live-pid-or-external-browser-health".to_string(),
        };
    }
    if isolation.mode == "container-required" || isolation.mode == "container-optional" {
        return ResourceBudgetDecision {
            class: "sandboxed".to_string(),
            memory_hint_mb: 256,
            cpu_hint: "quota-recommended".to_string(),
            monitor: "container-stats-if-containerized-procfs-if-native".to_string(),
        };
    }
    if parallelism.mode == "shared" || parallelism.mode == "pool" {
        return ResourceBudgetDecision {
            class: "light".to_string(),
            memory_hint_mb: 128,
            cpu_hint: "shared-low".to_string(),
            monitor: "procfs-when-live-session-exists".to_string(),
        };
    }
    ResourceBudgetDecision {
        class: "guarded".to_string(),
        memory_hint_mb: 256,
        cpu_hint: "single-worker".to_string(),
        monitor: "procfs-when-live-session-exists".to_string(),
    }
}

fn tools_for_server(server: &JsonValue, cached_tools: Option<&JsonValue>) -> Vec<JsonValue> {
    let mut tools = json_array(server, "tools").to_vec();
    if let Some(cached) = cached_tools.and_then(JsonValue::as_array) {
        tools.extend(cached.iter().cloned());
    }
    for name in json_array(server, "toolNames")
        .iter()
        .filter_map(JsonValue::as_str)
    {
        tools.push(JsonValue::object([("name", JsonValue::string(name))]));
    }
    tools
}

fn tool_descriptor_text(tool: &JsonValue) -> String {
    let mut text = Vec::new();
    for key in ["name", "title", "description"] {
        if let Some(value) = json_helpers::string_at_path(tool, &[key]) {
            text.push(value.to_string());
        }
    }
    if let Some(schema) = json_helpers::value_at_path(tool, &["inputSchema"]) {
        text.push(schema.to_compact_string());
    }
    text.join(" ").to_ascii_lowercase()
}

fn build_user_readiness_json(
    operator_plan: &JsonValue,
    servers: &JsonValue,
    clients: &JsonValue,
    hub: &JsonValue,
    readiness: &JsonValue,
) -> JsonValue {
    let summary = operator_plan.get("summary").unwrap_or(&JsonValue::Null);
    let total = json_usize(
        summary,
        "total",
        json_items(servers, &["servers", "items"]).len(),
    );
    let enabled = json_usize(summary, "enabled", 0);
    let blocked = json_usize(summary, "blocked", 0);
    let unchecked = json_usize(summary, "unchecked", 0);
    let guarded = json_usize(summary, "guarded", 0);
    let ready = json_usize(summary, "ready", 0);
    let off = json_usize(summary, "off", total.saturating_sub(enabled));
    let policy_changes = json_usize(summary, "policyChanges", 0);
    let clients_count = json_items(clients, &["clients", "items"]).len();
    let runtime_ready = json_helpers::bool_at_path(readiness, &["runtimePrerequisitesReady"])
        .or_else(|| json_helpers::bool_at_path(hub, &["readyForRuntimeOps"]))
        .unwrap_or(false);

    let mut missing = Vec::new();
    if !runtime_ready {
        missing.push("runtime prerequisites are not ready".to_string());
    }
    if total == 0 {
        missing.push("no MCP server source is configured yet".to_string());
    }
    if total > 0 && enabled == 0 {
        missing.push("all configured servers are parked/off".to_string());
    }
    if blocked > 0 {
        missing.push(format!(
            "{} blocked server{}",
            blocked,
            if blocked == 1 { "" } else { "s" }
        ));
    }
    if unchecked > 0 {
        missing.push(format!(
            "{} server{} still need Test/tools-list evidence",
            unchecked,
            if unchecked == 1 { "" } else { "s" }
        ));
    }
    if clients_count == 0 {
        missing.push("no client install/export evidence in the current overview".to_string());
    }
    if policy_changes > 0 {
        missing.push(format!(
            "{} recommended policy fix{} not applied",
            policy_changes,
            if policy_changes == 1 { "" } else { "es" }
        ));
    }

    let (headline, body, primary_action, primary_reason, confidence) = if !runtime_ready {
        (
            "Not usable yet",
            "The dashboard is reachable, but runtime prerequisites are not ready enough for normal MCP use.",
            "Fix runtime setup first",
            "Without a ready local runtime, clients cannot rely on /mcp.",
            0.25,
        )
    } else if blocked > 0 {
        (
            "Do not trust it yet",
            "One or more selected servers are blocked. Fix those before enabling more tools.",
            "Fix blocked servers",
            "Blocked sources make the operator view ambiguous for a normal user.",
            0.4,
        )
    } else if ready > 0 {
        (
            "Usable with visible evidence",
            "At least one server has enough evidence for brokered use. Guarded servers should stay conservative.",
            "Use ready servers; keep testing unchecked ones",
            "The user can see status, launch command, evidence, and next action without opening raw diagnostics.",
            if unchecked == 0 && guarded == 0 && policy_changes == 0 { 0.9 } else { 0.72 },
        )
    } else if enabled > 0 {
        (
            "Almost usable, but evidence is weak",
            "Servers are enabled, but live tools/list evidence or policy cleanup is still missing.",
            "Run Test on enabled servers",
            "The dashboard should not claim capabilities until the server proves its tool list.",
            0.56,
        )
    } else if total > 0 {
        (
            "Configured but parked",
            "Server sources exist, but they are intentionally off. Enable only the source needed for the current workflow, then test it.",
            "Enable one server and run Test",
            "Parked sources are safer than silently exposing tools to every client.",
            0.62,
        )
    } else {
        (
            "Clean empty state",
            "No servers are configured yet. The next useful action is adding one source, keeping it disabled, then testing it.",
            "Add a server command",
            "A user should see a safe add-server path instead of raw internals.",
            0.58,
        )
    };

    JsonValue::object([
        ("schema", JsonValue::string("mcpace.userReadiness.v1")),
        ("headline", JsonValue::string(headline)),
        ("body", JsonValue::string(body)),
        ("confidence", JsonValue::number(confidence)),
        ("primaryAction", JsonValue::string(primary_action)),
        ("primaryReason", JsonValue::string(primary_reason)),
        ("endpoint", JsonValue::string("/mcp")),
        (
            "summary",
            JsonValue::object([
                ("servers", JsonValue::number(total)),
                ("enabled", JsonValue::number(enabled)),
                ("ready", JsonValue::number(ready)),
                ("guarded", JsonValue::number(guarded)),
                ("unchecked", JsonValue::number(unchecked)),
                ("blocked", JsonValue::number(blocked)),
                ("off", JsonValue::number(off)),
                ("clients", JsonValue::number(clients_count)),
            ]),
        ),
        (
            "shouldSee",
            JsonValue::array([
                JsonValue::string("overall ready/not-ready status"),
                JsonValue::string("local MCP endpoint /mcp"),
                JsonValue::string("server launch command or URL"),
                JsonValue::string("live tools/list evidence and failures"),
                JsonValue::string("one recommended next action per server"),
            ]),
        ),
        (
            "shouldHide",
            JsonValue::array([
                JsonValue::string("environment variable values"),
                JsonValue::string("HTTP header values"),
                JsonValue::string("raw JSON and logs unless diagnostics are opened"),
                JsonValue::string("manual worker/policy controls unless Details is opened"),
                JsonValue::string("disabled server tools as if they were usable"),
            ]),
        ),
        (
            "missing",
            JsonValue::array(missing.into_iter().map(JsonValue::string)),
        ),
    ])
}

fn build_operator_plan_json(
    servers: &JsonValue,
    instances: &JsonValue,
    readiness: &JsonValue,
    leases: &JsonValue,
) -> JsonValue {
    let server_items = json_items(servers, &["servers", "items"]);
    let instance_items = json_items(instances, &["instances", "items"]);
    let lease_items = json_items(leases, &["leases", "activeLeases", "items"]);
    let missing_commands = json_helpers::array_at_path(readiness, &["missingRequiredCommands"])
        .unwrap_or(&[])
        .len();
    let missing_required_sources =
        json_helpers::array_at_path(readiness, &["missingRequiredSourceEnablement"])
            .unwrap_or(&[])
            .len();

    let mut summary = OperatorPlanSummary {
        total: server_items.len(),
        active_leases: lease_items.len(),
        missing_commands,
        missing_required_sources,
        ..OperatorPlanSummary::default()
    };

    let mut items = Vec::new();
    for server in server_items {
        let related = related_instances_for_server(server, instance_items);
        let plan = operator_plan_for_server(server, &related);
        summary.count(&plan);
        items.push(plan.to_json_value());
    }

    JsonValue::object([
        ("schema", JsonValue::string("mcpace.operatorPlan.v1")),
        ("summary", summary.to_json_value()),
        ("items", JsonValue::array(items)),
        (
            "flow",
            JsonValue::array([
                JsonValue::object([
                    ("stage", JsonValue::string("client")),
                    ("description", JsonValue::string("MCP clients point at the local /mcp endpoint instead of each upstream server.")),
                ]),
                JsonValue::object([
                    ("stage", JsonValue::string("broker")),
                    ("description", JsonValue::string("MCPace lists/searches tools, then routes upstream_call through server policy and lease context.")),
                ]),
                JsonValue::object([
                    ("stage", JsonValue::string("source")),
                    ("description", JsonValue::string("Each server has a launch command or Streamable HTTP URL, stored disabled until explicitly enabled/tested.")),
                ]),
                JsonValue::object([
                    ("stage", JsonValue::string("evidence")),
                    ("description", JsonValue::string("initialize + tools/list evidence is required before the dashboard treats a source as operational.")),
                ]),
            ]),
        ),
    ])
}

#[derive(Default)]
struct OperatorPlanSummary {
    total: usize,
    enabled: usize,
    off: usize,
    blocked: usize,
    unchecked: usize,
    guarded: usize,
    ready: usize,
    policy_changes: usize,
    active_leases: usize,
    missing_commands: usize,
    missing_required_sources: usize,
}

impl OperatorPlanSummary {
    fn count(&mut self, plan: &ServerOperatorPlan) {
        if plan.enabled {
            self.enabled = self.enabled.saturating_add(1);
        } else {
            self.off = self.off.saturating_add(1);
        }
        match plan.lane.as_str() {
            "blocked" => self.blocked = self.blocked.saturating_add(1),
            "unchecked" => self.unchecked = self.unchecked.saturating_add(1),
            "guarded" => self.guarded = self.guarded.saturating_add(1),
            "ready" => self.ready = self.ready.saturating_add(1),
            "off" => {}
            _ => {}
        }
        if plan.needs_policy_change {
            self.policy_changes = self.policy_changes.saturating_add(1);
        }
    }

    fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("total", JsonValue::number(self.total)),
            ("enabled", JsonValue::number(self.enabled)),
            ("off", JsonValue::number(self.off)),
            ("blocked", JsonValue::number(self.blocked)),
            ("unchecked", JsonValue::number(self.unchecked)),
            ("guarded", JsonValue::number(self.guarded)),
            ("ready", JsonValue::number(self.ready)),
            ("policyChanges", JsonValue::number(self.policy_changes)),
            ("activeLeases", JsonValue::number(self.active_leases)),
            ("missingCommands", JsonValue::number(self.missing_commands)),
            (
                "missingRequiredSources",
                JsonValue::number(self.missing_required_sources),
            ),
        ])
    }
}

struct ServerOperatorPlan {
    name: String,
    enabled: bool,
    lane: String,
    tone: String,
    priority: usize,
    next_action: String,
    rationale: String,
    evidence: String,
    launch: String,
    source_type: String,
    current_mode: String,
    recommended_mode: String,
    recommended_workers: usize,
    recommended_in_flight: usize,
    needs_policy_change: bool,
    commands: Vec<JsonValue>,
    blockers: Vec<String>,
    safeguards: Vec<String>,
}

impl ServerOperatorPlan {
    fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("name", JsonValue::string(self.name.clone())),
            ("enabled", JsonValue::bool(self.enabled)),
            ("lane", JsonValue::string(self.lane.clone())),
            ("tone", JsonValue::string(self.tone.clone())),
            ("priority", JsonValue::number(self.priority)),
            ("nextAction", JsonValue::string(self.next_action.clone())),
            ("rationale", JsonValue::string(self.rationale.clone())),
            ("evidence", JsonValue::string(self.evidence.clone())),
            ("launch", JsonValue::string(self.launch.clone())),
            ("sourceType", JsonValue::string(self.source_type.clone())),
            ("currentMode", JsonValue::string(self.current_mode.clone())),
            (
                "recommendedPolicy",
                JsonValue::object([
                    ("mode", JsonValue::string(self.recommended_mode.clone())),
                    ("maxWorkers", JsonValue::number(self.recommended_workers)),
                    (
                        "maxInFlightPerWorker",
                        JsonValue::number(self.recommended_in_flight),
                    ),
                ]),
            ),
            (
                "needsPolicyChange",
                JsonValue::bool(self.needs_policy_change),
            ),
            ("commands", JsonValue::array(self.commands.clone())),
            (
                "blockers",
                JsonValue::array(self.blockers.iter().cloned().map(JsonValue::string)),
            ),
            (
                "safeguards",
                JsonValue::array(self.safeguards.iter().cloned().map(JsonValue::string)),
            ),
        ])
    }
}

fn operator_plan_for_server(server: &JsonValue, instances: &[JsonValue]) -> ServerOperatorPlan {
    let name = json_string(server, "name", "server");
    let source_enabled = json_bool(server, "sourceEnabled", true);
    let effective_enabled = json_bool(server, "effectiveEnabled", false);
    let profile_enabled = json_bool(server, "profileEnabled", false);
    let default_enabled = json_bool(server, "defaultEnabled", false);
    let required = json_bool(server, "required", false);
    let source_type = json_string(server, "sourceType", "unknown");
    let runtime_type = json_string(server, "runtimeType", "unknown");
    let state_class = json_string(server, "stateClass", "unknown");
    let effect_class = json_string(server, "effectClass", "unknown");
    let credential_binding = json_string(server, "credentialBinding", "unknown");
    let scope_class = json_string(server, "scopeClass", "unknown");
    let concurrency_policy = json_string(server, "concurrencyPolicy", "unknown");
    let host_lock = json_string(server, "hostLock", "none");
    let launch = launch_command_for_server(server);
    let current_mode = route_mode_for_server(server, instances);
    let mut blockers = Vec::new();
    let mut safeguards = Vec::new();

    if !source_enabled && (required || profile_enabled || default_enabled) {
        blockers.push(
            "profile or default policy selects this server while the source is disabled"
                .to_string(),
        );
    }
    if required && !effective_enabled {
        blockers.push("required server is not effectively enabled".to_string());
    }
    if launch.trim().is_empty() && source_type != "streamable-http" && source_type != "http" {
        safeguards.push(
            "no launch command is visible yet; import or install the source before probing"
                .to_string(),
        );
    }
    if !json_array(server, "sourceEnvNames").is_empty() {
        safeguards.push(
            "environment variables are present; values stay redacted in UI and JSON".to_string(),
        );
    }
    if !json_array(server, "sourceHeaderNames").is_empty() {
        safeguards
            .push("HTTP header names are present; values stay redacted in UI and JSON".to_string());
    }

    let fields = format!(
        "{} {} {} {} {} {} {}",
        runtime_type,
        state_class,
        effect_class,
        credential_binding,
        scope_class,
        concurrency_policy,
        host_lock
    )
    .to_ascii_lowercase();
    let has_tool_evidence = json_usize(server, "toolCount", 0) > 0
        || !json_array(server, "toolNames").is_empty()
        || !json_array(server, "tools").is_empty();
    let sensitive = fields.contains("credential")
        || fields.contains("external")
        || fields.contains("remote")
        || fields.contains("stateful")
        || fields.contains("session")
        || fields.contains("write")
        || fields.contains("host");
    let unknown = fields.contains("unknown") || scope_class == "configured-source";
    let stateless_readonly = runtime_type == "stateless"
        || state_class.contains("stateless")
        || effect_class.contains("read-only")
        || effect_class.contains("readonly");

    let (lane, tone, priority, next_action, rationale) = if !blockers.is_empty() {
        (
            "blocked",
            "bad",
            1,
            "Fix source enablement before testing",
            "A required or selected server cannot be used because its source state is inconsistent.",
        )
    } else if !effective_enabled {
        (
            "off",
            "warn",
            4,
            "Keep off until a workflow needs it, then enable and test",
            "Disabled sources do not expose tools to clients and should stay parked by default.",
        )
    } else if !has_tool_evidence || unknown {
        (
            "unchecked",
            "warn",
            2,
            "Run Test to collect initialize and tools/list evidence",
            "The server is enabled, but the dashboard has not seen enough live capability evidence yet.",
        )
    } else if sensitive || concurrency_policy.contains("single") || host_lock != "none" {
        (
            "guarded",
            "warn",
            3,
            "Use conservative routing and retest after config changes",
            "State, credentials, host locks, or side effects mean requests should not be shared freely.",
        )
    } else if stateless_readonly {
        (
            "ready",
            "good",
            5,
            "Ready for normal brokered use",
            "The server looks stateless/read-only and has enough evidence for low-risk shared routing.",
        )
    } else {
        (
            "guarded",
            "warn",
            3,
            "Keep serialized until stronger evidence exists",
            "The server is usable, but MCPace does not have enough proof to broaden concurrency.",
        )
    };

    if sensitive {
        safeguards.push(
            "lease-aware routing is required before upstream_call reaches this source".to_string(),
        );
    }
    if !has_tool_evidence && effective_enabled {
        safeguards.push(
            "no tools/list evidence is assumed; Test must run before trusting capabilities"
                .to_string(),
        );
    }

    let recommended_mode = if !effective_enabled {
        "disabled".to_string()
    } else if lane == "ready" {
        "shared".to_string()
    } else if current_mode == "project-isolated" || fields.contains("project") {
        "project-isolated".to_string()
    } else if current_mode == "session-isolated" || fields.contains("session") {
        "session-isolated".to_string()
    } else {
        "serialized".to_string()
    };
    let recommended_workers = 1usize;
    let recommended_in_flight = 1usize;
    let needs_policy_change =
        effective_enabled && current_mode != recommended_mode && recommended_mode != "disabled";

    let commands = operator_commands(
        &name,
        effective_enabled,
        lane,
        needs_policy_change,
        &recommended_mode,
        recommended_workers,
        recommended_in_flight,
    );
    let evidence = if has_tool_evidence {
        "tools/list evidence is present in the server record".to_string()
    } else if effective_enabled {
        "no live tools/list evidence in the current overview".to_string()
    } else {
        "server is off, so tools/list evidence is intentionally absent".to_string()
    };

    ServerOperatorPlan {
        name,
        enabled: effective_enabled,
        lane: lane.to_string(),
        tone: tone.to_string(),
        priority,
        next_action: next_action.to_string(),
        rationale: rationale.to_string(),
        evidence,
        launch,
        source_type,
        current_mode,
        recommended_mode,
        recommended_workers,
        recommended_in_flight,
        needs_policy_change,
        commands,
        blockers,
        safeguards,
    }
}

fn operator_commands(
    name: &str,
    enabled: bool,
    lane: &str,
    needs_policy_change: bool,
    recommended_mode: &str,
    recommended_workers: usize,
    recommended_in_flight: usize,
) -> Vec<JsonValue> {
    let quoted = cli_word(name);
    let mut commands = Vec::new();
    commands.push(command_json(
        "Inspect",
        &format!("mcpace server capabilities {} --json", quoted),
    ));
    if !enabled || lane == "off" {
        commands.push(command_json(
            "Enable",
            &format!("mcpace server enable {} --json", quoted),
        ));
    }
    if enabled || lane == "off" {
        commands.push(command_json(
            "Test",
            &format!("mcpace server test {} --refresh --json", quoted),
        ));
    }
    if needs_policy_change {
        commands.push(command_json(
            "Apply policy",
            &format!(
                "mcpace server set-policy {} --mode {} --max-workers {} --max-in-flight-per-worker {} --json",
                quoted, recommended_mode, recommended_workers, recommended_in_flight
            ),
        ));
    }
    commands
}

fn command_json(label: &str, command: &str) -> JsonValue {
    JsonValue::object([
        ("label", JsonValue::string(label)),
        ("command", JsonValue::string(command)),
    ])
}

fn route_mode_for_server(server: &JsonValue, instances: &[JsonValue]) -> String {
    if let Some(mode) = instances
        .iter()
        .find_map(|instance| json_string_opt(instance, "mode"))
    {
        return normalize_route_mode(&mode);
    }
    let routing_group = json_string(server, "routingGroup", "").to_ascii_lowercase();
    let concurrency = json_string(server, "concurrencyPolicy", "").to_ascii_lowercase();
    let startup = json_string(server, "startupStrategy", "").to_ascii_lowercase();
    if startup == "disabled" || routing_group == "disabled" {
        return "disabled".to_string();
    }
    if routing_group.contains("pool") {
        return "pool".to_string();
    }
    if routing_group.contains("shared") || concurrency == "multi-reader" {
        return "shared".to_string();
    }
    if routing_group.contains("project") || concurrency == "isolated-per-project" {
        return "project-isolated".to_string();
    }
    if routing_group.contains("session") || concurrency == "single-session" {
        return "session-isolated".to_string();
    }
    "serialized".to_string()
}

fn normalize_route_mode(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "shared" => "shared".to_string(),
        "pool" => "pool".to_string(),
        "session-isolated" | "per-chat" | "session" => "session-isolated".to_string(),
        "project-isolated" | "per-project" | "project" => "project-isolated".to_string(),
        "disabled" => "disabled".to_string(),
        _ => "serialized".to_string(),
    }
}

fn related_instances_for_server(server: &JsonValue, instances: &[JsonValue]) -> Vec<JsonValue> {
    let name = json_string(server, "name", "");
    instances
        .iter()
        .filter(|instance| {
            json_string(instance, "server", "") == name
                || json_string(instance, "serverName", "") == name
                || json_string(instance, "name", "") == name
        })
        .cloned()
        .collect()
}

fn launch_command_for_server(server: &JsonValue) -> String {
    let command = json_string(server, "sourceCommand", "");
    if !command.trim().is_empty() {
        let mut parts = vec![cli_word(&command)];
        parts.extend(
            json_array(server, "sourceArgs")
                .iter()
                .filter_map(JsonValue::as_str)
                .map(cli_word),
        );
        return parts.join(" ");
    }
    json_string(server, "sourceUrl", "")
}

fn cli_word(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return "''".to_string();
    }
    if trimmed.chars().all(|ch| {
        ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | '/' | ':' | '@' | '=' | ',')
    }) {
        return trimmed.to_string();
    }
    format!("'{}'", trimmed.replace('\'', "'\\''"))
}

fn json_u64(value: &JsonValue, key: &str, fallback: u64) -> u64 {
    value
        .get(key)
        .and_then(JsonValue::as_i64)
        .filter(|value| *value >= 0)
        .map(|value| value as u64)
        .unwrap_or(fallback)
}

fn json_items<'a>(value: &'a JsonValue, keys: &[&str]) -> &'a [JsonValue] {
    if let Some(items) = value.as_array() {
        return items;
    }
    for key in keys {
        if let Some(items) = value.get(key).and_then(JsonValue::as_array) {
            return items;
        }
    }
    &[]
}

fn json_array<'a>(value: &'a JsonValue, key: &str) -> &'a [JsonValue] {
    value.get(key).and_then(JsonValue::as_array).unwrap_or(&[])
}

fn json_string(value: &JsonValue, key: &str, fallback: &str) -> String {
    json_string_opt(value, key).unwrap_or_else(|| fallback.to_string())
}

fn json_string_opt(value: &JsonValue, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn json_bool(value: &JsonValue, key: &str, fallback: bool) -> bool {
    value
        .get(key)
        .and_then(JsonValue::as_bool)
        .unwrap_or(fallback)
}

fn json_usize(value: &JsonValue, key: &str, fallback: usize) -> usize {
    value
        .get(key)
        .and_then(JsonValue::as_i64)
        .filter(|value| *value >= 0)
        .map(|value| value as usize)
        .unwrap_or(fallback)
}

pub(super) fn run_json_commands_parallel(
    root_path: &Path,
    commands: Vec<(&'static str, Vec<&'static str>)>,
) -> Result<BTreeMap<&'static str, JsonValue>, String> {
    if commands.len() <= 1 {
        let mut results = BTreeMap::new();
        for (name, args) in commands {
            results.insert(
                name,
                run_json_command_vec(root_path, args.into_iter().map(str::to_string).collect())
                    .map_err(|error| format!("{}: {}", name, error))?,
            );
        }
        return Ok(results);
    }

    let handles = commands
        .into_iter()
        .map(|(name, args)| {
            let root_path = root_path.to_path_buf();
            (
                name,
                thread::spawn(move || {
                    run_json_command_vec(&root_path, args.into_iter().map(str::to_string).collect())
                }),
            )
        })
        .collect::<Vec<_>>();

    let mut results = BTreeMap::new();
    for (name, handle) in handles {
        match handle.join() {
            Ok(Ok(value)) => {
                results.insert(name, value);
            }
            Ok(Err(error)) => return Err(format!("{}: {}", name, error)),
            Err(_) => return Err(format!("{}: command worker panicked", name)),
        }
    }
    Ok(results)
}

pub(super) fn take_parallel_result(
    results: &mut BTreeMap<&'static str, JsonValue>,
    name: &'static str,
) -> Result<JsonValue, String> {
    results
        .remove(name)
        .ok_or_else(|| format!("{}: command result missing", name))
}

pub(super) fn action_response(action: &str, result: JsonValue) -> JsonValue {
    JsonValue::object([
        ("action", JsonValue::string(action)),
        ("generatedAtMs", JsonValue::number(now_ms())),
        ("result", result),
    ])
}
