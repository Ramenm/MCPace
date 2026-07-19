use super::response::{now_ms, sanitize_root_path};
use super::{
    run_json_command, run_json_command_vec, CachedHealth, CachedOverview, CachedOverviewFailure,
    DashboardConfig, ServeSurface,
};
use crate::json::JsonValue;
use crate::json_helpers;
use crate::resources;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime};

#[cfg(test)]
static OVERVIEW_BUILD_INVOCATIONS: AtomicUsize = AtomicUsize::new(0);

#[cfg(test)]
pub(super) fn reset_overview_build_invocations() {
    OVERVIEW_BUILD_INVOCATIONS.store(0, Ordering::Release);
}

#[cfg(test)]
pub(super) fn overview_build_invocations() -> usize {
    OVERVIEW_BUILD_INVOCATIONS.load(Ordering::Acquire)
}

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

    if let Some(cached) = guard.entry.clone() {
        let age = cached.stored_at.elapsed();
        if guard.refreshing {
            drop(guard);
            return Ok(with_runtime_cache_metadata(
                (*cached.value).clone(),
                config,
                CacheMetadata {
                    hit: true,
                    bypassed: refresh,
                    stale: true,
                    ttl: config.overview_cache_ttl,
                    age: Some(age),
                    refresh_error: None,
                },
            ));
        }
        if !refresh && age <= config.overview_cache_ttl {
            drop(guard);
            return Ok(with_runtime_cache_metadata(
                (*cached.value).clone(),
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

        // Expired and explicit-refresh requests share one refresh generation.
        // Concurrent callers receive the last immutable snapshot instead of
        // multiplying the subprocess-backed overview rebuild.
        guard.refreshing = true;
        drop(guard);
        return refresh_overview_cache(config, Some(cached), refresh);
    }

    // Cold failures are retained briefly so callers that waited on the same
    // generation observe its result instead of rebuilding serially.
    let failure_ttl = config.overview_cache_ttl.min(Duration::from_secs(1));
    if let Some(failure) = guard.cold_failure.as_ref() {
        if failure.stored_at.elapsed() <= failure_ttl {
            return Err(failure.error.clone());
        }
    }
    guard.cold_failure = None;

    // Cold start stays single-flight under the lock. Once a snapshot exists,
    // later stale and explicit-refresh requests use the coalesced path above.
    match build_overview_json(&config.root_path) {
        Ok(value) => {
            guard.cold_failure = None;
            guard.entry = Some(CachedOverview {
                stored_at: Instant::now(),
                value: Arc::new(value.clone()),
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
            guard.cold_failure = Some(CachedOverviewFailure {
                stored_at: Instant::now(),
                error: error.clone(),
            });
            Err(error)
        }
    }
}

fn refresh_overview_cache(
    config: &DashboardConfig,
    previous: Option<CachedOverview>,
    bypassed: bool,
) -> Result<JsonValue, String> {
    let refresh_result = build_overview_json(&config.root_path);
    let mut guard = config
        .overview_cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.refreshing = false;

    match refresh_result {
        Ok(value) => {
            guard.entry = Some(CachedOverview {
                stored_at: Instant::now(),
                value: Arc::new(value.clone()),
            });
            Ok(with_runtime_cache_metadata(
                value,
                config,
                CacheMetadata {
                    hit: false,
                    bypassed,
                    stale: false,
                    ttl: config.overview_cache_ttl,
                    age: Some(Duration::from_millis(0)),
                    refresh_error: None,
                },
            ))
        }
        Err(error) => {
            let cached = guard.entry.clone().or(previous);
            drop(guard);
            if let Some(cached) = cached {
                return Ok(with_runtime_cache_metadata(
                    (*cached.value).clone(),
                    config,
                    CacheMetadata {
                        hit: true,
                        bypassed,
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
    let process_resource_snapshot = resources::process_resource_snapshot_json(std::process::id());
    let resource_governor_snapshot = config
        .resource_governor
        .snapshot_json(&process_resource_snapshot);
    let upstream_pool_evicted_idle_count = config.upstream_session_pool.purge_idle_and_exited();
    let upstream_pool_size = config.upstream_session_pool.session_count();
    let upstream_pool_max_size = config.upstream_session_pool.max_session_count();
    let upstream_pool_idle_ttl_ms = config.upstream_session_pool.idle_ttl_ms();
    let upstream_session_snapshots = config
        .upstream_session_pool
        .session_snapshots()
        .into_iter()
        .map(|snapshot| snapshot.to_json_value())
        .collect::<Vec<_>>();

    let http_session_snapshot = config
        .http_session_store
        .lock()
        .map(|mut store| store.snapshot(now_ms()))
        .ok();
    let http_latency_snapshot = config
        .request_latencies
        .lock()
        .map(|tracker| tracker.snapshot_json())
        .unwrap_or_else(|_| {
            JsonValue::object([("error", JsonValue::string("latency tracker lock poisoned"))])
        });
    let http_operation_snapshot = config
        .operation_traces
        .lock()
        .map(|tracker| tracker.snapshot_json())
        .unwrap_or_else(|_| {
            JsonValue::object([("error", JsonValue::string("operation trace lock poisoned"))])
        });
    let http_rate_limit_snapshot = config
        .rate_limiter
        .lock()
        .map(|limiter| limiter.snapshot_json())
        .unwrap_or_else(|_| {
            JsonValue::object([("error", JsonValue::string("rate limiter lock poisoned"))])
        });
    let http_admission_snapshot = config.admission.snapshot_json();

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
        ("processResource", process_resource_snapshot.clone()),
        ("resourceGovernor", resource_governor_snapshot.clone()),
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
                ("latency", http_latency_snapshot),
                ("operations", http_operation_snapshot),
                ("rateLimit", http_rate_limit_snapshot),
                ("admission", http_admission_snapshot),
                ("resourceGovernor", resource_governor_snapshot),
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
                (
                    "requestIdReplayBytes",
                    JsonValue::number(
                        http_session_snapshot
                            .as_ref()
                            .map(|snapshot| snapshot.request_id_replay_bytes)
                            .unwrap_or_default(),
                    ),
                ),
                (
                    "maxRequestIdReplayBytes",
                    JsonValue::number(
                        http_session_snapshot
                            .as_ref()
                            .map(|snapshot| snapshot.max_request_id_replay_bytes)
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
                ("managerCount", JsonValue::number(1)),
                ("busyManagerCount", JsonValue::number(0)),
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
    #[cfg(test)]
    if root_path.join(".count-overview-builds").is_file() {
        OVERVIEW_BUILD_INVOCATIONS.fetch_add(1, Ordering::AcqRel);
    }

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
    let dashboard_foundation = build_dashboard_foundation_json(
        &hub,
        &readiness,
        &servers,
        &clients,
        &operator_plan,
        &cached_tool_evidence,
    );
    let access_review = build_dashboard_access_review_json(&servers, &cached_tool_evidence);
    let runtime_control_plane = build_runtime_control_plane_json(
        &servers,
        &instances,
        &operator_plan,
        &cached_tool_evidence,
    );
    let automation = build_dashboard_automation_json(root_path, &cached_tool_evidence);
    let discovery_control = build_discovery_control_json(root_path);

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
        ("dashboardFoundation", dashboard_foundation),
        ("accessReview", access_review),
        ("automation", automation),
        ("discoveryControl", discovery_control),
        ("operatorPlan", operator_plan),
        ("userReadiness", user_readiness),
        ("runtimeControlPlane", runtime_control_plane),
    ]))
}

fn build_dashboard_foundation_json(
    hub: &JsonValue,
    readiness: &JsonValue,
    servers: &JsonValue,
    clients: &JsonValue,
    operator_plan: &JsonValue,
    cached_tool_evidence: &JsonValue,
) -> JsonValue {
    let server_items = json_items(servers, &["servers", "items"]);
    let client_items = json_items(clients, &["targets", "clients", "items"]);
    let client_configured = json_string_opt(clients, "configuredClientKeyName")
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    let summary = operator_plan.get("summary").unwrap_or(&JsonValue::Null);
    let total_servers = server_items.len();
    let enabled_servers = server_items
        .iter()
        .filter(|server| json_bool(server, "effectiveEnabled", false))
        .count();
    let parked_servers = total_servers.saturating_sub(enabled_servers);
    let blocked_servers = json_usize(summary, "blocked", 0);
    let policy_changes = json_usize(summary, "policyChanges", 0);
    let unchecked_servers = json_usize(summary, "unchecked", 0);
    let cached_ok = json_usize(cached_tool_evidence, "okCount", 0);
    let cached_miss = json_usize(cached_tool_evidence, "cacheMissCount", 0).max(json_usize(
        cached_tool_evidence,
        "failedCount",
        0,
    ));
    let tool_count = json_usize(cached_tool_evidence, "toolCount", 0);
    let runtime_ready = json_helpers::bool_at_path(readiness, &["runtimePrerequisitesReady"])
        .or_else(|| json_helpers::bool_at_path(hub, &["readyForRuntimeOps"]))
        .unwrap_or(false);
    let endpoint = json_string(hub, "endpoint", "/mcp");
    let client_count = client_items.len();
    let enabled_without_evidence = enabled_servers.saturating_sub(cached_ok);
    let remote_sources = server_items
        .iter()
        .filter(|server| server_source_is_remote(server))
        .count();
    let secret_bearing_sources = server_items
        .iter()
        .filter(|server| server_has_secret_boundary(server))
        .count();
    let safety_status = if blocked_servers > 0 {
        "bad"
    } else if enabled_without_evidence > 0
        || remote_sources > 0
        || secret_bearing_sources > 0
        || policy_changes > 0
    {
        "warn"
    } else {
        "good"
    };
    let safety_title = if blocked_servers > 0 {
        "Fix blockers before enabling"
    } else if enabled_without_evidence > 0 {
        "Test enabled sources before use"
    } else if remote_sources > 0 {
        "Review remote sources"
    } else if secret_bearing_sources > 0 {
        "Secrets referenced, values hidden"
    } else {
        "Source safety stays simple"
    };
    let safety_body = if blocked_servers > 0 {
        "Required source or runtime blockers are present. Keep affected servers off until the blocker is resolved."
    } else if enabled_without_evidence > 0 {
        "Enabled sources without tools/list evidence should be tested before a user relies on their tools."
    } else if remote_sources > 0 {
        "Remote HTTP sources need an explicit origin/auth review; do not treat reachability as trust."
    } else if secret_bearing_sources > 0 {
        "The dashboard may show secret names for review, but never raw environment or header values."
    } else {
        "No obvious enable-time blocker is visible in the current overview. Keep preview, save disabled, review, enable, then test as the default path."
    };

    // If /api/overview could be built, the dashboard backend is reachable.
    // Runtime prerequisites are a separate concern and belong to the routing/use step,
    // otherwise the first card says "backend problem" when the real next action is repair/setup.
    let backend_status = "good";
    let client_status = if client_configured { "good" } else { "warn" };
    let source_status = if total_servers > 0 { "good" } else { "warn" };
    let tools_status = if cached_ok > 0 { "good" } else { "warn" };
    let routing_ready = runtime_ready
        && enabled_servers > 0
        && cached_ok > 0
        && blocked_servers == 0
        && policy_changes == 0;
    let routing_status = if routing_ready { "good" } else { "warn" };

    let steps = vec![
        foundation_step(
            "backend",
            "Backend",
            backend_status,
            "Backend online",
            "The dashboard API returned overview data. Runtime readiness is checked later, before use.",
            "refresh",
            "Refresh",
        ),
        foundation_step(
            "client",
            "Client",
            client_status,
            if client_configured { "Client wired" } else { "Connect a client" },
            if client_configured {
                "A local client key is configured for the MCPace endpoint."
            } else if client_count > 0 {
                "Preview a client patch first. A target catalog is not the same as a wired client."
            } else {
                "Pick Claude, Cursor, VS Code, or a custom client and preview the patch first."
            },
            "clients",
            if client_configured { "Open client" } else { "Connect" },
        ),
        foundation_step(
            "source",
            "Source",
            source_status,
            if total_servers > 0 { "Server source saved" } else { "Add one source" },
            if total_servers > 0 {
                "Sources are tracked separately from enablement so they can stay parked."
            } else {
                "Import an existing MCP config first; otherwise discover or add a command manually."
            },
            if total_servers > 0 { "servers" } else { "import-server" },
            if total_servers > 0 { "Open sources" } else { "Import" },
        ),
        foundation_step(
            "tools",
            "Tools",
            tools_status,
            if cached_ok > 0 {
                "Tools evidence exists"
            } else {
                "Test enabled sources"
            },
            if cached_ok > 0 {
                "At least one source has cached tools/list evidence."
            } else if total_servers > 0 {
                "A saved source is not enough; run Test before trusting exposed tools."
            } else {
                "Tools can only be checked after a source exists."
            },
            "servers",
            "Open servers",
        ),
        foundation_step(
            "routing",
            "Routing",
            routing_status,
            if routing_status == "good" {
                "Safe by default"
            } else if !runtime_ready {
                "Repair runtime"
            } else {
                "Review policy"
            },
            if routing_status == "good" {
                "Tools evidence exists and no blocked source or obvious policy fix is waiting."
            } else if !runtime_ready {
                "Runtime prerequisites are not ready, so keep routing conservative until repair passes."
            } else if total_servers == 0 {
                "Routing becomes meaningful after at least one source is saved."
            } else if enabled_servers == 0 {
                "Saved sources are still parked. Review one source, enable deliberately, then run Test before use."
            } else if cached_ok == 0 {
                "Keep routing conservative until Test creates tools/list evidence."
            } else {
                "Keep routing conservative until blockers and recommended policy fixes are handled."
            },
            if !runtime_ready { "repair" } else { "servers" },
            if !runtime_ready {
                "Repair"
            } else if routing_status == "good" {
                "Open routing"
            } else if enabled_servers == 0 {
                "Enable"
            } else {
                "Review"
            },
        ),
    ];
    let complete = steps
        .iter()
        .filter(|step| step.get("status").and_then(JsonValue::as_str) == Some("good"))
        .count();
    let next_step = steps
        .iter()
        .find(|step| step.get("status").and_then(JsonValue::as_str) != Some("good"))
        .cloned()
        .unwrap_or_else(|| {
            foundation_step(
                "ready",
                "Ready",
                "good",
                "Base setup is ready",
                "Normal use can stay on the server rows; open setup tools only when changing config.",
                "servers",
                "Open servers",
            )
        });
    let status = if complete == steps.len() {
        "good"
    } else {
        "warn"
    };
    let next_step_key = json_string(&next_step, "key", "ready");
    let title = match next_step_key.as_str() {
        "backend" => "Start with the local backend",
        "client" => "Connect one local client",
        "source" => "Add one MCP server source",
        "tools" => "Test tools before use",
        "routing" if !runtime_ready => "Repair runtime before use",
        "routing" if enabled_servers == 0 => "Enable one reviewed source",
        "routing" => "Review routing before widening access",
        _ => "Base setup is ready",
    };
    let body = match next_step_key.as_str() {
        "backend" => "The safest base path is: start or reconnect the dashboard backend, then refresh before reading server state.",
        "client" => "MCPace is useful only when a client points at its local endpoint. Preview that patch before editing files.",
        "source" => "Bring in one source, save it parked, review it, enable deliberately, then run Test before normal use.",
        "tools" => "A saved or enabled source is not ready until Test confirms which tools it provides.",
        "routing" if !runtime_ready => "Runtime prerequisites are checked at the routing/use boundary. Repair them after client, source, and tool setup are clear.",
        "routing" if enabled_servers == 0 => "Saved sources are still parked. Review one, enable it deliberately, then run Test before normal routing.",
        "routing" => "Solve blockers and policy fixes before adding workers or exposing more servers.",
        _ => "Normal use can stay on the server rows. Open setup tools only for import, discovery, clients, or diagnostics.",
    };
    let primary_action_label = json_string(&next_step, "actionLabel", "Refresh");
    let primary_action_key = json_string(&next_step, "action", "refresh");
    let primary_action = foundation_action(&primary_action_label, &primary_action_key);

    JsonValue::object([
        ("schema", JsonValue::string("mcpace.dashboardFoundation.v1")),
        (
            "stateKey",
            JsonValue::string(json_string(&next_step, "key", "ready")),
        ),
        (
            "nextStepKey",
            JsonValue::string(json_string(&next_step, "key", "ready")),
        ),
        ("status", JsonValue::string(status)),
        ("title", JsonValue::string(title)),
        ("body", JsonValue::string(body)),
        ("complete", JsonValue::number(complete)),
        ("total", JsonValue::number(steps.len())),
        (
            "progressPct",
            JsonValue::number(complete.saturating_mul(100) / steps.len().max(1)),
        ),
        ("endpoint", JsonValue::string(endpoint)),
        (
            "counts",
            JsonValue::object([
                ("clients", JsonValue::number(client_count)),
                ("clientConfigured", JsonValue::bool(client_configured)),
                ("servers", JsonValue::number(total_servers)),
                ("enabledServers", JsonValue::number(enabled_servers)),
                ("parkedServers", JsonValue::number(parked_servers)),
                ("cachedToolServers", JsonValue::number(cached_ok)),
                ("cachedToolMisses", JsonValue::number(cached_miss)),
                ("cachedTools", JsonValue::number(tool_count)),
                ("blockedServers", JsonValue::number(blocked_servers)),
                ("uncheckedServers", JsonValue::number(unchecked_servers)),
                ("policyChanges", JsonValue::number(policy_changes)),
                ("runtimeReady", JsonValue::bool(runtime_ready)),
                ("routingReady", JsonValue::bool(routing_ready)),
                ("backendReachable", JsonValue::bool(true)),
            ]),
        ),
        ("nextStep", next_step),
        ("steps", JsonValue::array(steps)),
        ("actions", foundation_actions_json(primary_action)),
        (
            "displayRules",
            JsonValue::array([
                JsonValue::string("Show backend, client, source, tools, and routing before advanced controls."),
                JsonValue::string("Backend online only means /api/overview responded; runtime readiness is checked before routing/use."),
                JsonValue::string("Keep workers, raw env, headers, leases, and protocol diagnostics in their named task workspace."),
                JsonValue::string("Use preview, save disabled, review, enable, then test as the default change path."),
                JsonValue::string("Render secret values as names only; keep raw env, headers, and tokens hidden or inside the Source task."),
            ]),
        ),
        (
            "safety",
            JsonValue::object([
                ("schema", JsonValue::string("mcpace.dashboardSafety.v1")),
                ("status", JsonValue::string(safety_status)),
                ("title", JsonValue::string(safety_title)),
                ("body", JsonValue::string(safety_body)),
                (
                    "counts",
                    JsonValue::object([
                        (
                            "enabledWithoutEvidence",
                            JsonValue::number(enabled_without_evidence),
                        ),
                        ("remoteSources", JsonValue::number(remote_sources)),
                        (
                            "secretBearingSources",
                            JsonValue::number(secret_bearing_sources),
                        ),
                        ("blockedServers", JsonValue::number(blocked_servers)),
                        ("policyChanges", JsonValue::number(policy_changes)),
                    ]),
                ),
                (
                    "rules",
                    JsonValue::array([
                        JsonValue::string("names-only-secrets"),
                        JsonValue::string("preview-before-write"),
                        JsonValue::string("localhost-or-https-auth"),
                    ]),
                ),
            ]),
        ),
    ])
}

fn server_source_is_remote(server: &JsonValue) -> bool {
    let source_url = json_string(server, "sourceUrl", "").to_ascii_lowercase();
    let source_type = json_string(server, "sourceType", "").to_ascii_lowercase();
    let transport = json_string(server, "transport", "").to_ascii_lowercase();
    let url_is_remote = (source_url.starts_with("http://") || source_url.starts_with("https://"))
        && !source_url.starts_with("http://localhost")
        && !source_url.starts_with("https://localhost")
        && !source_url.starts_with("http://127.0.0.1")
        && !source_url.starts_with("https://127.0.0.1")
        && !source_url.starts_with("http://[::1]")
        && !source_url.starts_with("https://[::1]");
    url_is_remote
        || source_type.contains("http")
        || source_type.contains("sse")
        || transport.contains("http")
        || transport.contains("sse")
}

fn server_has_secret_boundary(server: &JsonValue) -> bool {
    let env_names = json_array_len(server, "sourceEnvNames");
    let header_names = json_array_len(server, "sourceHeaderNames");
    let credential_binding = json_string(server, "credentialBinding", "").to_ascii_lowercase();
    env_names > 0
        || header_names > 0
        || credential_binding.contains("credential")
        || credential_binding.contains("secret")
        || credential_binding.contains("token")
        || credential_binding.contains("auth")
}

fn json_array_len(value: &JsonValue, key: &str) -> usize {
    value
        .get(key)
        .and_then(JsonValue::as_array)
        .map(|items| items.len())
        .unwrap_or(0)
}

fn foundation_step(
    key: &str,
    label: &str,
    status: &str,
    title: &str,
    body: &str,
    action: &str,
    action_label: &str,
) -> JsonValue {
    JsonValue::object([
        ("key", JsonValue::string(key)),
        ("label", JsonValue::string(label)),
        ("status", JsonValue::string(status)),
        ("title", JsonValue::string(title)),
        ("body", JsonValue::string(body)),
        ("action", JsonValue::string(action)),
        ("actionLabel", JsonValue::string(action_label)),
    ])
}

fn foundation_action(label: &str, action: &str) -> JsonValue {
    JsonValue::object([
        ("label", JsonValue::string(label)),
        ("action", JsonValue::string(action)),
    ])
}

fn foundation_actions_json(primary_action: JsonValue) -> JsonValue {
    let mut actions = vec![primary_action];
    for action in [
        foundation_action("Import", "import-server"),
        foundation_action("Client", "clients"),
        foundation_action("Servers", "servers"),
    ] {
        let action_key = json_string(&action, "action", "");
        let already_present = actions
            .iter()
            .any(|existing| json_string(existing, "action", "") == action_key);
        if !already_present {
            actions.push(action);
        }
    }
    JsonValue::array(actions)
}

fn dashboard_config_json(root_path: &Path) -> Option<JsonValue> {
    json_helpers::read_json_file(&root_path.join("mcpace.config.json")).ok()
}

fn config_value<'a>(config: Option<&'a JsonValue>, path: &[&str]) -> Option<&'a JsonValue> {
    config.and_then(|value| json_helpers::value_at_path(value, path))
}

fn config_string(config: Option<&JsonValue>, path: &[&str], fallback: &str) -> String {
    config_value(config, path)
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| fallback.to_string())
}

fn config_bool(config: Option<&JsonValue>, path: &[&str], fallback: bool) -> bool {
    config_value(config, path)
        .and_then(JsonValue::as_bool)
        .unwrap_or(fallback)
}

fn config_usize(config: Option<&JsonValue>, path: &[&str], fallback: usize) -> usize {
    config_value(config, path)
        .and_then(JsonValue::as_i64)
        .filter(|value| *value >= 0)
        .map(|value| value as usize)
        .unwrap_or(fallback)
}

fn configured_string_count(config: Option<&JsonValue>, path: &[&str]) -> usize {
    config
        .and_then(|value| json_helpers::array_at_path(value, path))
        .map(|items| {
            items
                .iter()
                .filter_map(JsonValue::as_str)
                .filter(|value| !value.trim().is_empty())
                .count()
        })
        .unwrap_or(0)
}

fn string_array(values: &[&str]) -> JsonValue {
    JsonValue::array(values.iter().map(|value| JsonValue::string(*value)))
}

fn resolve_config_path(root_path: &Path, configured_path: &str) -> PathBuf {
    let path = PathBuf::from(configured_path);
    if path.is_absolute() {
        path
    } else {
        root_path.join(path)
    }
}

fn cache_age_ms(path: &Path) -> Option<u128> {
    let modified = fs::metadata(path).ok()?.modified().ok()?;
    SystemTime::now()
        .duration_since(modified)
        .ok()
        .map(|duration| duration.as_millis())
}

fn registry_cache_json(root_path: &Path, config: Option<&JsonValue>) -> JsonValue {
    let configured_path = config_string(
        config,
        &["dynamicDiscovery", "registryCachePath"],
        "./catalog/registry-cache.json",
    );
    let path = resolve_config_path(root_path, &configured_path);
    let exists = path.exists();
    JsonValue::object([
        ("configuredPath", JsonValue::string(configured_path)),
        ("exists", JsonValue::bool(exists)),
        (
            "ageMs",
            cache_age_ms(&path)
                .map(JsonValue::number)
                .unwrap_or(JsonValue::Null),
        ),
        (
            "ttlHours",
            JsonValue::number(config_usize(
                config,
                &["dynamicDiscovery", "registryCacheTtlHours"],
                24,
            )),
        ),
        (
            "autoRefresh",
            JsonValue::bool(config_bool(
                config,
                &["dynamicDiscovery", "autoRefreshRegistry"],
                false,
            )),
        ),
    ])
}

fn build_dashboard_automation_json(
    root_path: &Path,
    cached_tool_evidence: &JsonValue,
) -> JsonValue {
    let config_value = dashboard_config_json(root_path);
    let config = config_value.as_ref();
    let ui_refresh_ms = config_usize(config, &["uiSurface", "refreshIntervalMs"], 15_000);
    let dynamic_enabled = config_bool(config, &["dynamicDiscovery", "enabled"], false);
    let include_paths = configured_string_count(config, &["mcpSettings", "includePaths"]);
    let include_dirs = configured_string_count(config, &["mcpSettings", "includeDirs"]);
    let catalog_paths = configured_string_count(config, &["dynamicDiscovery", "catalogPaths"]);
    let registry_endpoints =
        configured_string_count(config, &["dynamicDiscovery", "registryEndpoints"]);
    let tool_server_count = json_usize(cached_tool_evidence, "serverCount", 0);
    let tool_count = json_usize(cached_tool_evidence, "toolCount", 0);
    let cache_hit = json_bool(cached_tool_evidence, "cacheHit", false);
    let cache_mode = json_string(cached_tool_evidence, "mode", "raw-callable-tools-cache");

    JsonValue::object([
        ("schema", JsonValue::string("mcpace.dashboardAutomation.v1")),
        (
            "overviewRefresh",
            JsonValue::object([
                ("kind", JsonValue::string("live")),
                ("intervalMs", JsonValue::number(ui_refresh_ms)),
                ("source", JsonValue::string("uiSurface.refreshIntervalMs")),
                ("userControlled", JsonValue::bool(true)),
            ]),
        ),
        (
            "serverSources",
            JsonValue::object([
                ("kind", JsonValue::string("config")),
                ("baseFile", JsonValue::string("mcp_settings.json")),
                ("includePathCount", JsonValue::number(include_paths)),
                ("includeDirCount", JsonValue::number(include_dirs)),
                ("importSupported", JsonValue::bool(true)),
                (
                    "importCommand",
                    JsonValue::string(
                        "mcpace advanced server import --from <mcp-settings.json> --dry-run",
                    ),
                ),
            ]),
        ),
        (
            "discoveryJob",
            JsonValue::object([
                ("kind", JsonValue::string("config")),
                ("enabled", JsonValue::bool(dynamic_enabled)),
                (
                    "mode",
                    JsonValue::string(config_string(
                        config,
                        &["dynamicDiscovery", "mode"],
                        "manual",
                    )),
                ),
                (
                    "autoInstall",
                    JsonValue::string(config_string(
                        config,
                        &["dynamicDiscovery", "autoInstall"],
                        "manual-only",
                    )),
                ),
                (
                    "unknownServers",
                    JsonValue::string(config_string(
                        config,
                        &["dynamicDiscovery", "installUnknown"],
                        "plan-only",
                    )),
                ),
                ("catalogPathCount", JsonValue::number(catalog_paths)),
                (
                    "registryEndpointCount",
                    JsonValue::number(registry_endpoints),
                ),
                ("registryCache", registry_cache_json(root_path, config)),
            ]),
        ),
        (
            "toolEvidenceCache",
            JsonValue::object([
                ("kind", JsonValue::string("live-cache")),
                ("mode", JsonValue::string(cache_mode)),
                ("cacheHit", JsonValue::bool(cache_hit)),
                ("serverCount", JsonValue::number(tool_server_count)),
                ("toolCount", JsonValue::number(tool_count)),
            ]),
        ),
        (
            "policyPlan",
            JsonValue::object([
                ("kind", JsonValue::string("derived")),
                (
                    "from",
                    string_array(&["source config", "runtime policy", "live tool evidence"]),
                ),
                (
                    "safeDefault",
                    JsonValue::string(config_string(
                        config,
                        &["executionDefaults", "mode"],
                        "serialized",
                    )),
                ),
                ("manualOverride", JsonValue::bool(true)),
            ]),
        ),
        (
            "fieldPolicy",
            JsonValue::object([
                (
                    "live",
                    string_array(&[
                        "backend link",
                        "runtime",
                        "tests",
                        "active locks",
                        "tool evidence",
                    ]),
                ),
                (
                    "config",
                    string_array(&[
                        "server source",
                        "enabled state",
                        "routing policy",
                        "discovery settings",
                    ]),
                ),
                (
                    "derived",
                    string_array(&[
                        "next step",
                        "server lane",
                        "policy plan",
                        "readiness confidence",
                    ]),
                ),
                (
                    "hidden",
                    string_array(&[
                        "secret values",
                        "raw env",
                        "raw headers",
                        "full internals until diagnostics",
                    ]),
                ),
            ]),
        ),
        (
            "setupFlow",
            string_array(&["Import", "Discover", "Test", "Enable", "Use"]),
        ),
    ])
}

fn build_discovery_control_json(root_path: &Path) -> JsonValue {
    let config_value = dashboard_config_json(root_path);
    let config = config_value.as_ref();
    JsonValue::object([
        ("schema", JsonValue::string("mcpace.discoveryControl.v1")),
        (
            "enabled",
            JsonValue::bool(config_bool(config, &["dynamicDiscovery", "enabled"], false)),
        ),
        (
            "mode",
            JsonValue::string(config_string(
                config,
                &["dynamicDiscovery", "mode"],
                "manual",
            )),
        ),
        (
            "defaultCommand",
            JsonValue::string(config_string(
                config,
                &["dynamicDiscovery", "defaultCommand"],
                "manual",
            )),
        ),
        (
            "autoInstall",
            JsonValue::string(config_string(
                config,
                &["dynamicDiscovery", "autoInstall"],
                "manual-only",
            )),
        ),
        (
            "installUnknown",
            JsonValue::string(config_string(
                config,
                &["dynamicDiscovery", "installUnknown"],
                "plan-only",
            )),
        ),
        (
            "maxAutoInstallsPerRun",
            JsonValue::number(config_usize(
                config,
                &["dynamicDiscovery", "maxAutoInstallsPerRun"],
                0,
            )),
        ),
        (
            "probeAfterInstall",
            JsonValue::bool(config_bool(
                config,
                &["dynamicDiscovery", "probeAfterInstall"],
                true,
            )),
        ),
        (
            "refreshRegistryOnDiscover",
            JsonValue::bool(config_bool(
                config,
                &["dynamicDiscovery", "refreshRegistryOnDiscover"],
                false,
            )),
        ),
        (
            "autoRefreshRegistry",
            JsonValue::bool(config_bool(
                config,
                &["dynamicDiscovery", "autoRefreshRegistry"],
                false,
            )),
        ),
        ("registryCache", registry_cache_json(root_path, config)),
        (
            "catalogPathCount",
            JsonValue::number(configured_string_count(
                config,
                &["dynamicDiscovery", "catalogPaths"],
            )),
        ),
        (
            "registryEndpointCount",
            JsonValue::number(configured_string_count(
                config,
                &["dynamicDiscovery", "registryEndpoints"],
            )),
        ),
        (
            "safeFlow",
            string_array(&["Preview", "Save disabled", "Review", "Enable", "Test"]),
        ),
        (
            "guardrail",
            JsonValue::string(
                "Dashboard discovery never enables a new server without an explicit user action.",
            ),
        ),
    ])
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

fn build_dashboard_access_review_json(
    servers: &JsonValue,
    cached_tool_evidence: &JsonValue,
) -> JsonValue {
    let server_items = json_items(servers, &["servers", "items"]);
    let mut enabled = 0usize;
    let mut remote_http = 0usize;
    let mut credential_sources = 0usize;
    let mut hidden_secret_names = 0usize;
    let mut approval_required = 0usize;
    let mut destructive = 0usize;
    let mut mutating_or_open_world = 0usize;
    let mut read_only = 0usize;
    let mut enabled_without_evidence = 0usize;
    let mut sensitive_without_evidence = 0usize;
    let mut category_counts = BTreeMap::<String, usize>::new();
    let mut rows = Vec::new();

    for server in server_items {
        let name = json_string(server, "name", "server");
        let effective_enabled = json_bool(server, "effectiveEnabled", false);
        if effective_enabled {
            enabled = enabled.saturating_add(1);
        }

        let source_text = format!(
            "{} {} {} {} {}",
            json_string(server, "sourceType", ""),
            json_string(server, "sourceCommand", ""),
            json_string(server, "sourceUrl", ""),
            json_string(server, "url", ""),
            json_string(server, "transportPreference", "")
        )
        .to_ascii_lowercase();
        let remote = source_text.contains("http://")
            || source_text.contains("https://")
            || source_text.contains("streamable-http")
            || source_text.contains("sse");
        if remote {
            remote_http = remote_http.saturating_add(1);
        }

        let secret_name_count = json_array(server, "sourceEnvNames")
            .len()
            .saturating_add(json_array(server, "sourceHeaderNames").len());
        hidden_secret_names = hidden_secret_names.saturating_add(secret_name_count);
        let credential_hint = secret_name_count > 0
            || json_string(server, "credentialBinding", "")
                .to_ascii_lowercase()
                .contains("credential")
            || source_text.contains("oauth")
            || source_text.contains("token")
            || source_text.contains("auth");
        if credential_hint {
            credential_sources = credential_sources.saturating_add(1);
        }

        let cached_tools = cached_tools_for_server(cached_tool_evidence, &name);
        let evidence_state = evidence_state_for_server(server, cached_tools.as_ref());
        let has_evidence = has_live_tool_evidence(&evidence_state);
        if effective_enabled && !has_evidence {
            enabled_without_evidence = enabled_without_evidence.saturating_add(1);
        }

        let tool_risk = infer_tool_risk(server, cached_tools.as_ref());
        for category in &tool_risk.categories {
            *category_counts.entry(category.clone()).or_default() += 1;
        }
        if tool_risk.approval_required {
            approval_required = approval_required.saturating_add(1);
        }
        match tool_risk.risk.as_str() {
            "destructive" => destructive = destructive.saturating_add(1),
            "read-only" => read_only = read_only.saturating_add(1),
            "mutating" | "credential" | "network" | "filesystem" | "unknown" => {
                mutating_or_open_world = mutating_or_open_world.saturating_add(1)
            }
            _ => {}
        }
        if effective_enabled
            && !has_evidence
            && (tool_risk.approval_required || remote || credential_hint)
        {
            sensitive_without_evidence = sensitive_without_evidence.saturating_add(1);
        }

        rows.push(JsonValue::object([
            ("name", JsonValue::string(name)),
            ("enabled", JsonValue::bool(effective_enabled)),
            ("evidenceState", JsonValue::string(evidence_state)),
            ("risk", JsonValue::string(tool_risk.risk)),
            (
                "approvalRequired",
                JsonValue::bool(tool_risk.approval_required),
            ),
            ("remote", JsonValue::bool(remote)),
            ("credentialHints", JsonValue::bool(credential_hint)),
            (
                "hiddenSecretNameCount",
                JsonValue::number(secret_name_count),
            ),
            (
                "categories",
                JsonValue::array(tool_risk.categories.into_iter().map(JsonValue::string)),
            ),
        ]));
    }

    let total = server_items.len();
    let status = if sensitive_without_evidence > 0 {
        "bad"
    } else if total == 0
        || enabled_without_evidence > 0
        || approval_required > 0
        || remote_http > 0
        || credential_sources > 0
    {
        "warn"
    } else {
        "good"
    };
    let title = if total == 0 {
        "Access review waits for one source"
    } else if sensitive_without_evidence > 0 {
        "Review access before enabling"
    } else if approval_required > 0 || remote_http > 0 || credential_sources > 0 {
        "Access needs explicit review"
    } else {
        "Access boundary looks quiet"
    };
    let body = if total == 0 {
        "Add or import one source first. MCPace should not describe permissions for tools that do not exist yet."
    } else if sensitive_without_evidence > 0 {
        "Some enabled sources look sensitive but have no tools/list evidence. Test them or park them before normal use."
    } else if approval_required > 0 {
        "Write, destructive, network, credential, or unknown tools stay approval-first; annotations are hints, not proof."
    } else {
        "No obvious sensitive tool category is waiting in the current overview. Keep secrets hidden and retest after config changes."
    };
    let primary_action = if total == 0 {
        foundation_action("Import config", "import-server")
    } else if enabled_without_evidence > 0 || sensitive_without_evidence > 0 {
        foundation_action("Open servers", "servers")
    } else {
        foundation_action("Refresh", "refresh")
    };

    JsonValue::object([
        ("schema", JsonValue::string("mcpace.dashboardAccessReview.v1")),
        ("status", JsonValue::string(status)),
        ("title", JsonValue::string(title)),
        ("body", JsonValue::string(body)),
        ("primaryAction", primary_action),
        (
            "counts",
            JsonValue::object([
                ("servers", JsonValue::number(total)),
                ("enabled", JsonValue::number(enabled)),
                ("remoteHttp", JsonValue::number(remote_http)),
                ("credentialSources", JsonValue::number(credential_sources)),
                ("hiddenSecretNames", JsonValue::number(hidden_secret_names)),
                ("approvalRequired", JsonValue::number(approval_required)),
                ("destructive", JsonValue::number(destructive)),
                ("mutatingOrOpenWorld", JsonValue::number(mutating_or_open_world)),
                ("readOnly", JsonValue::number(read_only)),
                ("enabledWithoutEvidence", JsonValue::number(enabled_without_evidence)),
                ("sensitiveWithoutEvidence", JsonValue::number(sensitive_without_evidence)),
            ]),
        ),
        (
            "categoryCounts",
            JsonValue::object(
                category_counts
                    .into_iter()
                    .map(|(key, value)| (key, JsonValue::number(value))),
            ),
        ),
        (
            "items",
            JsonValue::array([
                access_review_item(
                    "Approval",
                    approval_required,
                    if approval_required > 0 { "warn" } else { "good" },
                    "Write, destructive, open-world, credential, and unknown tools should ask before use.",
                ),
                access_review_item(
                    "Secrets",
                    hidden_secret_names,
                    if hidden_secret_names > 0 { "warn" } else { "good" },
                    "Show env/header names only. Never render secret values in the dashboard.",
                ),
                access_review_item(
                    "Remote/Auth",
                    remote_http.saturating_add(credential_sources),
                    if remote_http > 0 || credential_sources > 0 { "warn" } else { "good" },
                    "Remote HTTP and auth-backed sources need explicit origin and scope review.",
                ),
                access_review_item(
                    "Evidence",
                    enabled_without_evidence,
                    if enabled_without_evidence > 0 { "bad" } else { "good" },
                    "Enabled sources need initialize/tools-list evidence before normal routing.",
                ),
            ]),
        ),
        ("serverRows", JsonValue::array(rows)),
        (
            "displayRules",
            JsonValue::array([
                JsonValue::string("Surface access summary after the five basics, not before them."),
                JsonValue::string("Treat tool annotations as hints until source trust and live evidence exist."),
                JsonValue::string("Human approval is required for sampling, destructive tools, credentials, and unknown access."),
                JsonValue::string("Secret values stay hidden; only names and counts are visible."),
            ]),
        ),
    ])
}

fn access_review_item(label: &str, count: usize, status: &str, body: &str) -> JsonValue {
    JsonValue::object([
        ("label", JsonValue::string(label)),
        ("count", JsonValue::number(count)),
        ("status", JsonValue::string(status)),
        ("body", JsonValue::string(body)),
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
                JsonValue::string(
                    "manual worker/policy controls unless the Routing task is opened",
                ),
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
                    ("description", JsonValue::string("Each server has a launch command or Streamable HTTP URL, stored disabled until explicitly tested/enabled.")),
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
            "Keep off until a workflow needs it, then test and enable",
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
        &format!("mcpace advanced server capabilities {} --json", quoted),
    ));
    if !enabled || lane == "off" {
        commands.push(command_json(
            "Enable",
            &format!("mcpace advanced server enable {} --json", quoted),
        ));
    }
    if enabled || lane == "off" {
        commands.push(command_json(
            "Test",
            &format!("mcpace advanced server test {} --refresh --json", quoted),
        ));
    }
    if needs_policy_change {
        commands.push(command_json(
            "Apply policy",
            &format!(
                "mcpace advanced server set-policy {} --mode {} --max-workers {} --max-in-flight-per-worker {} --json",
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

    let mut handles = Vec::with_capacity(commands.len());
    for (name, args) in commands {
        let root_path = root_path.to_path_buf();
        match thread::Builder::new()
            .name(format!("mcpace-overview-{name}"))
            .spawn(move || {
                run_json_command_vec(&root_path, args.into_iter().map(str::to_string).collect())
            }) {
            Ok(handle) => handles.push((name, handle)),
            Err(error) => {
                for (_, handle) in handles {
                    let _ = handle.join();
                }
                return Err(format!(
                    "{}: failed to start command worker: {}",
                    name, error
                ));
            }
        }
    }

    join_json_command_handles(handles).map_err(|error| error.to_string())
}

#[derive(Debug)]
pub(super) struct JsonCommandJoinError(String);

impl std::fmt::Display for JsonCommandJoinError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

pub(super) fn join_json_command_handles<E>(
    handles: Vec<(&'static str, thread::JoinHandle<Result<JsonValue, E>>)>,
) -> Result<BTreeMap<&'static str, JsonValue>, JsonCommandJoinError>
where
    E: std::fmt::Display,
{
    let mut results = BTreeMap::new();
    let mut first_error = None;
    for (name, handle) in handles {
        match handle.join() {
            Ok(Ok(value)) => {
                results.insert(name, value);
            }
            Ok(Err(error)) => {
                if first_error.is_none() {
                    first_error = Some(format!("{}: {}", name, error));
                }
            }
            Err(_) => {
                if first_error.is_none() {
                    first_error = Some(format!("{}: command worker panicked", name));
                }
            }
        }
    }
    if let Some(error) = first_error {
        Err(JsonCommandJoinError(error))
    } else {
        Ok(results)
    }
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
