use super::args::ParsedArgs;
use super::runtime;
use crate::client::{self, RuntimePlanRequest};
use crate::diagnostics;
use crate::json::{parse_str, JsonValue};
use crate::json_helpers;
use crate::runtimepaths;
use crate::text_utils;
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const LEASE_STORE_VERSION: usize = 2;
const DEFAULT_LEASE_TTL_MS: u128 = 120_000;
const MAX_LEASE_TTL_MS: u128 = 3_600_000;
const LEASE_LOCK_ATTEMPTS: usize = 100;
const LEASE_LOCK_SLEEP_MS: u64 = 10;
const LEASE_LOCK_STALE_MS: u128 = 30_000;

#[derive(Debug, Clone)]
struct PlannedRoute {
    server_name: String,
    admission_state: String,
    request_strategy: String,
    request_mutex_key: Option<String>,
    session_affinity_key: Option<String>,
    process_scope_key: String,
    process_partition: String,
    project_binding_key: Option<String>,
    worktree_binding_key: Option<String>,
    conflict_domain: String,
    host_lock_key: Option<String>,
    state_profile_key: Option<String>,
    scheduler_lane: String,
    startup_strategy: String,
    upstream_transport: String,
    parallelism_limit: usize,
    warnings: Vec<String>,
    client_id: String,
    session_lease_id: String,
    session_id: Option<String>,
    project_root: Option<String>,
}

#[derive(Debug, Clone)]
struct LeaseCommandResult {
    json: JsonValue,
    exit_code: i32,
}

#[derive(Debug, Clone)]
struct SessionAccumulator {
    session_lease_id: String,
    client_id: String,
    session_id: Option<String>,
    project_root: Option<String>,
    started_at_ms: u128,
    last_lease_seen_at_ms: u128,
    active_lease_ids: Vec<String>,
    servers: Vec<String>,
}

impl SessionAccumulator {
    fn from_lease(lease: &JsonValue, refreshed_at_ms: u128) -> Option<Self> {
        let lease_id = required_string(lease, &["leaseId"]);
        if lease_id.is_empty() {
            return None;
        }
        let client_id = optional_string(lease, &["clientId"]).unwrap_or_else(|| "unknown".into());
        let session_id = optional_string(lease, &["sessionId"]);
        let session_lease_id = optional_string(lease, &["sessionLeaseId"]).unwrap_or_else(|| {
            let session_token = session_id
                .as_deref()
                .map(|value| non_empty_token(value, "adhoc"))
                .unwrap_or_else(|| non_empty_token(&lease_id, "adhoc"));
            format!(
                "session:{}:{}",
                non_empty_token(&client_id, "unknown"),
                session_token
            )
        });
        let started_at_ms = number_u128_at(lease, &["acquiredAtMs"]).unwrap_or(refreshed_at_ms);
        let last_lease_seen_at_ms = number_u128_at(lease, &["renewedAtMs"])
            .or_else(|| number_u128_at(lease, &["acquiredAtMs"]))
            .unwrap_or(refreshed_at_ms);
        let server = optional_string(lease, &["server"]);

        let mut accumulator = Self {
            session_lease_id,
            client_id,
            session_id,
            project_root: optional_string(lease, &["projectRoot"]),
            started_at_ms,
            last_lease_seen_at_ms,
            active_lease_ids: vec![lease_id],
            servers: Vec::new(),
        };
        if let Some(server) = server {
            accumulator.servers.push(server);
        }
        Some(accumulator)
    }

    fn add_lease(&mut self, lease: &JsonValue, refreshed_at_ms: u128) {
        let lease_id = required_string(lease, &["leaseId"]);
        if lease_id.is_empty() {
            return;
        }
        self.active_lease_ids.push(lease_id);
        if let Some(server) = optional_string(lease, &["server"]) {
            self.servers.push(server);
        }
        if self.project_root.is_none() {
            self.project_root = optional_string(lease, &["projectRoot"]);
        }
        let started_at_ms = number_u128_at(lease, &["acquiredAtMs"]).unwrap_or(refreshed_at_ms);
        self.started_at_ms = self.started_at_ms.min(started_at_ms);
        let last_lease_seen_at_ms = number_u128_at(lease, &["renewedAtMs"])
            .or_else(|| number_u128_at(lease, &["acquiredAtMs"]))
            .unwrap_or(refreshed_at_ms);
        self.last_lease_seen_at_ms = self.last_lease_seen_at_ms.max(last_lease_seen_at_ms);
    }

    fn into_json(mut self, refreshed_at_ms: u128) -> JsonValue {
        self.active_lease_ids.sort();
        self.active_lease_ids.dedup();
        self.servers.sort();
        self.servers.dedup();
        JsonValue::object([
            (
                "sessionLeaseId",
                JsonValue::string(self.session_lease_id.clone()),
            ),
            ("clientId", JsonValue::string(self.client_id.clone())),
            (
                "sessionId",
                json_helpers::json_string_or_null(self.session_id.clone()),
            ),
            (
                "projectRoot",
                json_helpers::json_string_or_null(self.project_root.clone()),
            ),
            ("startedAtMs", JsonValue::number(self.started_at_ms)),
            (
                "lastLeaseSeenAtMs",
                JsonValue::number(self.last_lease_seen_at_ms),
            ),
            ("refreshedAtMs", JsonValue::number(refreshed_at_ms)),
            (
                "activeLeaseCount",
                JsonValue::number(self.active_lease_ids.len()),
            ),
            (
                "activeLeaseIds",
                JsonValue::array(self.active_lease_ids.into_iter().map(JsonValue::string)),
            ),
            (
                "servers",
                JsonValue::array(self.servers.into_iter().map(JsonValue::string)),
            ),
        ])
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RuntimeLeaseRequest {
    pub(crate) server_name: String,
    pub(crate) client_id: Option<String>,
    pub(crate) session_id: Option<String>,
    pub(crate) project_root: Option<String>,
    pub(crate) transport: Option<String>,
    pub(crate) metadata_json: Option<String>,
    pub(crate) ttl_ms: Option<u128>,
    pub(crate) takeover: bool,
}

#[derive(Debug, Clone)]
pub(crate) enum RuntimeLeaseAcquireResult {
    Acquired { lease_id: String, json: JsonValue },
    Blocked { json: JsonValue },
}

struct LeaseStoreGuard {
    path: PathBuf,
}

impl Drop for LeaseStoreGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub(super) fn run(
    root_path: &Path,
    parsed: &ParsedArgs,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let result = match parsed.lease_action.as_deref().unwrap_or("list") {
        "list" => run_list(root_path),
        "acquire" => run_acquire(root_path, parsed),
        "renew" => run_renew(root_path, parsed),
        "release" => run_release(root_path, parsed),
        other => Err(format!("unsupported hub lease action: {}", other)),
    };

    match result {
        Ok(result) => {
            if parsed.json_output {
                let _ = writeln!(stdout, "{}", result.json.to_pretty_string());
            } else {
                write_text_result(&result.json, stdout);
            }
            result.exit_code
        }
        Err(error) => {
            diagnostics::stderr_line(stderr, format_args!("{}", error));
            1
        }
    }
}

fn run_list(root_path: &Path) -> Result<LeaseCommandResult, String> {
    mutate_store(root_path, |store, now_ms, expired_purged_count| {
        let leases = sorted_leases(store);
        let sessions = sorted_sessions(store);
        Ok(LeaseCommandResult {
            json: JsonValue::object([
                ("status", JsonValue::string("listed")),
                ("version", JsonValue::number(LEASE_STORE_VERSION)),
                ("nowMs", JsonValue::number(now_ms)),
                ("activeLeaseCount", JsonValue::number(leases.len())),
                ("activeSessionCount", JsonValue::number(sessions.len())),
                (
                    "expiredPurgedCount",
                    JsonValue::number(expired_purged_count),
                ),
                ("leases", JsonValue::array(leases)),
                ("sessions", JsonValue::array(sessions)),
            ]),
            exit_code: 0,
        })
    })
}

fn run_acquire(root_path: &Path, parsed: &ParsedArgs) -> Result<LeaseCommandResult, String> {
    let server_name = parsed
        .server_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "hub lease acquire requires --server <name>".to_string())?
        .to_string();
    acquire_runtime_lease_command(
        root_path,
        RuntimeLeaseRequest {
            server_name,
            client_id: parsed.client_id.clone(),
            session_id: parsed.session_id.clone(),
            project_root: parsed.project_root.clone(),
            transport: parsed.transport.clone(),
            metadata_json: parsed.metadata_json.clone(),
            ttl_ms: parsed.ttl_ms,
            takeover: parsed.takeover,
        },
    )
}

pub(crate) fn acquire_runtime_lease(
    root_path: &Path,
    request: RuntimeLeaseRequest,
) -> Result<RuntimeLeaseAcquireResult, String> {
    let server_name = clean_required_server_name(&request.server_name)?;
    let route = route_for_request(root_path, &request, &server_name)?
        .unwrap_or_else(|| conservative_settings_only_route(&request, &server_name));
    let result = acquire_lease_for_route(
        root_path,
        route,
        normalize_ttl(request.ttl_ms),
        request.takeover,
    )?;
    if result.exit_code == 0 {
        let lease_id = json_helpers::string_at_path(&result.json, &["leaseId"])
            .unwrap_or("")
            .to_string();
        Ok(RuntimeLeaseAcquireResult::Acquired {
            lease_id,
            json: result.json,
        })
    } else {
        Ok(RuntimeLeaseAcquireResult::Blocked { json: result.json })
    }
}

fn conservative_settings_only_route(
    request: &RuntimeLeaseRequest,
    server_name: &str,
) -> PlannedRoute {
    let server_token = non_empty_token(server_name, "server");
    let client_id = text_utils::trimmed_non_empty_owned(request.client_id.as_ref())
        .unwrap_or_else(|| "mcpace-upstream-bridge".to_string());
    let session_id = text_utils::trimmed_non_empty_owned(request.session_id.as_ref());
    let project_root = text_utils::trimmed_non_empty_owned(request.project_root.as_ref());
    let project_token = project_root
        .as_deref()
        .map(|value| non_empty_token(value, "global"))
        .unwrap_or_else(|| "global".to_string());
    let session_token = session_id
        .as_deref()
        .map(|value| non_empty_token(value, "adhoc"))
        .unwrap_or_else(|| format!("settings-only-{}", server_token));
    let upstream_transport = text_utils::trimmed_non_empty_owned(request.transport.as_ref())
        .unwrap_or_else(|| "stdio".to_string());

    PlannedRoute {
        server_name: server_name.to_string(),
        admission_state: "configured-source".to_string(),
        request_strategy: "single-writer".to_string(),
        request_mutex_key: Some(format!(
            "settings-only:{}:{}",
            server_token, project_token
        )),
        session_affinity_key: session_id
            .as_ref()
            .map(|_| format!("settings-only:{}:{}", server_token, session_token)),
        process_scope_key: format!("settings-only:{}:{}", server_token, project_token),
        process_partition: project_token,
        project_binding_key: project_root
            .as_ref()
            .map(|_| format!("project:settings-only:{}", server_token)),
        worktree_binding_key: None,
        conflict_domain: format!("settings-only:{}", server_token),
        host_lock_key: None,
        state_profile_key: None,
        scheduler_lane: "settings-only-conservative".to_string(),
        startup_strategy: "per-request".to_string(),
        upstream_transport,
        parallelism_limit: 1,
        warnings: vec![format!(
            "server '{}' exists in the merged MCP settings registry but is not declared in mcpace.config.json; MCPace assigned a conservative single-writer request lease instead of bypassing scheduling",
            server_name
        )],
        client_id: client_id.clone(),
        session_lease_id: format!(
            "session:{}:{}",
            non_empty_token(&client_id, "client"),
            session_token
        ),
        session_id,
        project_root,
    }
}

fn acquire_runtime_lease_command(
    root_path: &Path,
    request: RuntimeLeaseRequest,
) -> Result<LeaseCommandResult, String> {
    let server_name = clean_required_server_name(&request.server_name)?;
    let route = route_for_request(root_path, &request, &server_name)?.ok_or_else(|| {
        format!(
            "server '{}' was not found in the client routing plan",
            server_name
        )
    })?;
    acquire_lease_for_route(
        root_path,
        route,
        normalize_ttl(request.ttl_ms),
        request.takeover,
    )
}

fn route_for_request(
    root_path: &Path,
    request: &RuntimeLeaseRequest,
    server_name: &str,
) -> Result<Option<PlannedRoute>, String> {
    let plan_json = client::runtime_plan_json(
        root_path,
        RuntimePlanRequest {
            client_id: request.client_id.clone(),
            session_id: request.session_id.clone(),
            project_root: request.project_root.clone(),
            transport: request.transport.clone(),
            metadata_json: request.metadata_json.clone(),
        },
    )?;
    if find_server_plan(&plan_json, server_name).is_none() {
        return Ok(None);
    }
    planned_route_from_plan(&plan_json, server_name).map(Some)
}

fn acquire_lease_for_route(
    root_path: &Path,
    route: PlannedRoute,
    ttl_ms: u128,
    takeover: bool,
) -> Result<LeaseCommandResult, String> {
    mutate_store(root_path, |store, now_ms, expired_purged_count| {
        let blockers = route_blockers(&route);
        if !blockers.is_empty() {
            return Ok(LeaseCommandResult {
                json: acquire_blocked_json(
                    &route,
                    now_ms,
                    expired_purged_count,
                    blockers.join(" | "),
                    None,
                    active_lease_count(store),
                ),
                exit_code: 1,
            });
        }

        let taken_over_lease = if let Some(conflict) = find_conflict(store, &route) {
            if takeover_allowed(takeover, &conflict.lease, &route) {
                let lease_id = required_string(&conflict.lease, &["leaseId"]);
                let removed = leases_map_mut(store).remove(&lease_id);
                if let Some(removed_lease) = &removed {
                    let _ = runtime::append_log(
                        root_path,
                        "warn",
                        "lease_taken_over",
                        &[
                            ("leaseId", JsonValue::string(lease_id)),
                            ("server", JsonValue::string(route.server_name.clone())),
                            (
                                "sessionLeaseId",
                                JsonValue::string(route.session_lease_id.clone()),
                            ),
                        ],
                    );
                    Some(removed_lease.clone())
                } else {
                    Some(conflict.lease)
                }
            } else {
                return Ok(LeaseCommandResult {
                    json: acquire_blocked_json(
                        &route,
                        now_ms,
                        expired_purged_count,
                        conflict.reason,
                        Some(conflict.lease),
                        active_lease_count(store),
                    ),
                    exit_code: 1,
                });
            }
        } else {
            None
        };

        let lease_id = generate_lease_id(&route, now_ms);
        let acquired_at_ms = now_ms;
        let expires_at_ms = now_ms.saturating_add(ttl_ms);
        let lease_json =
            lease_record_json(&lease_id, &route, acquired_at_ms, expires_at_ms, ttl_ms);
        leases_map_mut(store).insert(lease_id.clone(), lease_json.clone());
        rebuild_sessions(store, now_ms);
        let active_count = active_lease_count(store);
        let active_session_count = active_session_count(store);
        let _ = runtime::append_log(
            root_path,
            "info",
            "lease_acquired",
            &[
                ("leaseId", JsonValue::string(lease_id.clone())),
                ("server", JsonValue::string(route.server_name.clone())),
                (
                    "schedulerLane",
                    JsonValue::string(route.scheduler_lane.clone()),
                ),
                ("ttlMs", JsonValue::number(ttl_ms)),
            ],
        );

        Ok(LeaseCommandResult {
            json: JsonValue::object([
                ("status", JsonValue::string("acquired")),
                ("leaseId", JsonValue::string(lease_id)),
                ("server", JsonValue::string(route.server_name.clone())),
                ("clientId", JsonValue::string(route.client_id.clone())),
                (
                    "sessionLeaseId",
                    JsonValue::string(route.session_lease_id.clone()),
                ),
                ("ttlMs", JsonValue::number(ttl_ms)),
                ("acquiredAtMs", JsonValue::number(acquired_at_ms)),
                ("expiresAtMs", JsonValue::number(expires_at_ms)),
                ("activeLeaseCount", JsonValue::number(active_count)),
                (
                    "activeSessionCount",
                    JsonValue::number(active_session_count),
                ),
                (
                    "expiredPurgedCount",
                    JsonValue::number(expired_purged_count),
                ),
                ("route", route_json(&route)),
                ("lease", lease_json),
                ("takeover", JsonValue::bool(taken_over_lease.is_some())),
                (
                    "takenOverLease",
                    taken_over_lease.unwrap_or(JsonValue::Null),
                ),
                (
                    "warnings",
                    JsonValue::array(route.warnings.iter().cloned().map(JsonValue::string)),
                ),
            ]),
            exit_code: 0,
        })
    })
}

fn run_renew(root_path: &Path, parsed: &ParsedArgs) -> Result<LeaseCommandResult, String> {
    let lease_id = parsed
        .lease_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "hub lease renew requires --lease-id <id>".to_string())?
        .to_string();

    renew_runtime_lease_command(root_path, &lease_id, parsed.ttl_ms)
}

pub(crate) fn renew_runtime_lease(
    root_path: &Path,
    lease_id: &str,
    ttl_ms: Option<u128>,
) -> Result<JsonValue, String> {
    let result = renew_runtime_lease_command(root_path, lease_id, ttl_ms)?;
    if result.exit_code == 0 {
        Ok(result.json)
    } else {
        Err(result.json.to_compact_string())
    }
}

fn renew_runtime_lease_command(
    root_path: &Path,
    lease_id: &str,
    ttl_ms: Option<u128>,
) -> Result<LeaseCommandResult, String> {
    let lease_id = lease_id.trim().to_string();
    if lease_id.is_empty() {
        return Err("hub lease renew requires --lease-id <id>".to_string());
    }
    let ttl_ms = normalize_ttl(ttl_ms);

    mutate_store(root_path, |store, now_ms, expired_purged_count| {
        let expires_at_ms = now_ms.saturating_add(ttl_ms);
        let renewed = {
            let leases = leases_map_mut(store);
            if let Some(lease) = leases.get_mut(&lease_id) {
                if let JsonValue::Object(map) = lease {
                    map.insert("renewedAtMs".to_string(), JsonValue::number(now_ms));
                    map.insert("expiresAtMs".to_string(), JsonValue::number(expires_at_ms));
                    map.insert("ttlMs".to_string(), JsonValue::number(ttl_ms));
                }
                Some(lease.clone())
            } else {
                None
            }
        };
        rebuild_sessions(store, now_ms);
        let active_count = active_lease_count(store);
        let active_session_count = active_session_count(store);
        let renewed_status = renewed.is_some();
        if renewed_status {
            let _ = runtime::append_log(
                root_path,
                "info",
                "lease_renewed",
                &[
                    ("leaseId", JsonValue::string(lease_id.clone())),
                    ("ttlMs", JsonValue::number(ttl_ms)),
                ],
            );
        }

        Ok(LeaseCommandResult {
            json: JsonValue::object([
                (
                    "status",
                    JsonValue::string(if renewed_status {
                        "renewed"
                    } else {
                        "not-found"
                    }),
                ),
                ("leaseId", JsonValue::string(lease_id.clone())),
                ("nowMs", JsonValue::number(now_ms)),
                ("ttlMs", JsonValue::number(ttl_ms)),
                ("expiresAtMs", JsonValue::number(expires_at_ms)),
                ("activeLeaseCount", JsonValue::number(active_count)),
                (
                    "activeSessionCount",
                    JsonValue::number(active_session_count),
                ),
                (
                    "expiredPurgedCount",
                    JsonValue::number(expired_purged_count),
                ),
                ("renewedLease", renewed.unwrap_or(JsonValue::Null)),
            ]),
            exit_code: if renewed_status { 0 } else { 1 },
        })
    })
}

fn run_release(root_path: &Path, parsed: &ParsedArgs) -> Result<LeaseCommandResult, String> {
    let lease_id = parsed
        .lease_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "hub lease release requires --lease-id <id>".to_string())?
        .to_string();

    release_runtime_lease_command(root_path, &lease_id)
}

pub(crate) fn release_runtime_lease(root_path: &Path, lease_id: &str) -> Result<JsonValue, String> {
    Ok(release_runtime_lease_command(root_path, lease_id)?.json)
}

fn release_runtime_lease_command(
    root_path: &Path,
    lease_id: &str,
) -> Result<LeaseCommandResult, String> {
    let lease_id = lease_id.trim().to_string();
    if lease_id.is_empty() {
        return Err("hub lease release requires --lease-id <id>".to_string());
    }

    mutate_store(root_path, |store, now_ms, expired_purged_count| {
        let removed = leases_map_mut(store).remove(&lease_id);
        rebuild_sessions(store, now_ms);
        let active_count = active_lease_count(store);
        let active_session_count = active_session_count(store);
        let released = removed.is_some();
        if released {
            let _ = runtime::append_log(
                root_path,
                "info",
                "lease_released",
                &[("leaseId", JsonValue::string(lease_id.clone()))],
            );
        }
        Ok(LeaseCommandResult {
            json: JsonValue::object([
                (
                    "status",
                    JsonValue::string(if released { "released" } else { "not-found" }),
                ),
                ("leaseId", JsonValue::string(lease_id.clone())),
                ("nowMs", JsonValue::number(now_ms)),
                ("activeLeaseCount", JsonValue::number(active_count)),
                (
                    "activeSessionCount",
                    JsonValue::number(active_session_count),
                ),
                (
                    "expiredPurgedCount",
                    JsonValue::number(expired_purged_count),
                ),
                ("releasedLease", removed.unwrap_or(JsonValue::Null)),
            ]),
            exit_code: if released { 0 } else { 1 },
        })
    })
}

fn mutate_store<F>(root_path: &Path, mutator: F) -> Result<LeaseCommandResult, String>
where
    F: FnOnce(&mut BTreeMap<String, JsonValue>, u128, usize) -> Result<LeaseCommandResult, String>,
{
    runtime::ensure_runtime_layout(root_path)?;
    let state_root = runtimepaths::resolve_state_root(root_path);
    let lease_path = runtimepaths::hub_leases_path(&state_root);
    let _guard = acquire_lease_store_lock(&state_root)?;
    let mut store = read_store_map(&lease_path)?;
    normalize_store_shape(&mut store);
    let now_ms = runtime::now_ms();
    let expired_purged_count = purge_expired(&mut store, now_ms);
    rebuild_sessions(&mut store, now_ms);
    let result = mutator(&mut store, now_ms, expired_purged_count)?;
    rebuild_sessions(&mut store, runtime::now_ms());
    store.insert(
        "version".to_string(),
        JsonValue::number(LEASE_STORE_VERSION),
    );
    store.insert(
        "updatedAtMs".to_string(),
        JsonValue::number(runtime::now_ms()),
    );
    runtime::write_atomic(&lease_path, JsonValue::Object(store).to_pretty_string())?;
    Ok(result)
}

fn acquire_lease_store_lock(state_root: &Path) -> Result<LeaseStoreGuard, String> {
    runtimepaths::ensure_hub_dir(state_root)?;
    let lock_path = runtimepaths::hub_lease_lock_path(state_root);
    let payload = JsonValue::object([
        ("pid", JsonValue::number(std::process::id())),
        ("createdAtMs", JsonValue::number(runtime::now_ms())),
    ])
    .to_pretty_string();

    for _ in 0..LEASE_LOCK_ATTEMPTS {
        match OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&lock_path)
        {
            Ok(mut file) => {
                file.write_all(payload.as_bytes()).map_err(|error| {
                    format!("failed to write {}: {}", lock_path.display(), error)
                })?;
                return Ok(LeaseStoreGuard { path: lock_path });
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                if remove_stale_lease_store_lock(&lock_path)? {
                    continue;
                }
                thread::sleep(Duration::from_millis(LEASE_LOCK_SLEEP_MS));
            }
            Err(error) => {
                return Err(format!(
                    "failed to acquire lease store lock at {}: {}",
                    lock_path.display(),
                    error
                ))
            }
        }
    }

    Err(format!(
        "lease store lock is busy at {}; another MCPace runtime worker may be updating leases",
        lock_path.display()
    ))
}

fn remove_stale_lease_store_lock(lock_path: &Path) -> Result<bool, String> {
    let raw = match fs::read_to_string(lock_path) {
        Ok(value) => value,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(true),
        Err(error) => {
            return Err(format!(
                "failed to inspect lease store lock at {}: {}",
                lock_path.display(),
                error
            ))
        }
    };
    let created_at_ms = parse_str(&raw)
        .ok()
        .and_then(|value| number_u128_at(&value, &["createdAtMs"]));
    let Some(created_at_ms) = created_at_ms else {
        return Ok(false);
    };
    if runtime::now_ms().saturating_sub(created_at_ms) <= LEASE_LOCK_STALE_MS {
        return Ok(false);
    }
    match fs::remove_file(lock_path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(true),
        Err(error) => Err(format!(
            "failed to remove stale lease store lock at {}: {}",
            lock_path.display(),
            error
        )),
    }
}

fn read_store_map(path: &Path) -> Result<BTreeMap<String, JsonValue>, String> {
    if !path.is_file() {
        return Ok(default_store_map());
    }
    let raw = fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {}", path.display(), error))?;
    match parse_str(&raw)
        .map_err(|error| format!("failed to parse {}: {}", path.display(), error))?
    {
        JsonValue::Object(map) => Ok(map),
        _ => Err(format!(
            "lease store {} must be a JSON object",
            path.display()
        )),
    }
}

fn default_store_map() -> BTreeMap<String, JsonValue> {
    let mut map = BTreeMap::new();
    map.insert(
        "version".to_string(),
        JsonValue::number(LEASE_STORE_VERSION),
    );
    map.insert("leases".to_string(), JsonValue::Object(BTreeMap::new()));
    map.insert("sessions".to_string(), JsonValue::Object(BTreeMap::new()));
    map.insert(
        "updatedAtMs".to_string(),
        JsonValue::number(runtime::now_ms()),
    );
    map
}

fn normalize_store_shape(store: &mut BTreeMap<String, JsonValue>) {
    if !matches!(store.get("leases"), Some(JsonValue::Object(_))) {
        store.insert("leases".to_string(), JsonValue::Object(BTreeMap::new()));
    }
    if !matches!(store.get("sessions"), Some(JsonValue::Object(_))) {
        store.insert("sessions".to_string(), JsonValue::Object(BTreeMap::new()));
    }
}

fn leases_map_mut(store: &mut BTreeMap<String, JsonValue>) -> &mut BTreeMap<String, JsonValue> {
    if !matches!(store.get("leases"), Some(JsonValue::Object(_))) {
        store.insert("leases".to_string(), JsonValue::Object(BTreeMap::new()));
    }
    match store.get_mut("leases") {
        Some(JsonValue::Object(map)) => map,
        _ => unreachable!("leases was normalized to an object"),
    }
}

fn leases_map(store: &BTreeMap<String, JsonValue>) -> Option<&BTreeMap<String, JsonValue>> {
    match store.get("leases") {
        Some(JsonValue::Object(map)) => Some(map),
        _ => None,
    }
}

fn purge_expired(store: &mut BTreeMap<String, JsonValue>, now_ms: u128) -> usize {
    let leases = leases_map_mut(store);
    let before = leases.len();
    leases.retain(|_, value| {
        expires_at_ms(value)
            .map(|expires_at| expires_at > now_ms)
            .unwrap_or(true)
    });
    before.saturating_sub(leases.len())
}

fn active_lease_count(store: &BTreeMap<String, JsonValue>) -> usize {
    leases_map(store).map(|leases| leases.len()).unwrap_or(0)
}

fn sorted_leases(store: &BTreeMap<String, JsonValue>) -> Vec<JsonValue> {
    leases_map(store)
        .map(|leases| leases.values().cloned().collect::<Vec<_>>())
        .unwrap_or_default()
}

fn sessions_map(store: &BTreeMap<String, JsonValue>) -> Option<&BTreeMap<String, JsonValue>> {
    match store.get("sessions") {
        Some(JsonValue::Object(map)) => Some(map),
        _ => None,
    }
}

fn active_session_count(store: &BTreeMap<String, JsonValue>) -> usize {
    sessions_map(store)
        .map(|sessions| sessions.len())
        .unwrap_or(0)
}

fn sorted_sessions(store: &BTreeMap<String, JsonValue>) -> Vec<JsonValue> {
    sessions_map(store)
        .map(|sessions| sessions.values().cloned().collect::<Vec<_>>())
        .unwrap_or_default()
}

fn rebuild_sessions(store: &mut BTreeMap<String, JsonValue>, refreshed_at_ms: u128) {
    let active_leases = leases_map(store)
        .map(|leases| leases.values().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    let mut sessions: BTreeMap<String, SessionAccumulator> = BTreeMap::new();
    for lease in active_leases {
        let Some(accumulator) = SessionAccumulator::from_lease(&lease, refreshed_at_ms) else {
            continue;
        };
        sessions
            .entry(accumulator.session_lease_id.clone())
            .and_modify(|existing| existing.add_lease(&lease, refreshed_at_ms))
            .or_insert(accumulator);
    }
    store.insert(
        "sessions".to_string(),
        JsonValue::Object(
            sessions
                .into_iter()
                .map(|(session_lease_id, session)| {
                    (session_lease_id, session.into_json(refreshed_at_ms))
                })
                .collect(),
        ),
    );
}

fn planned_route_from_plan(plan: &JsonValue, server_name: &str) -> Result<PlannedRoute, String> {
    let server = find_server_plan(plan, server_name).ok_or_else(|| {
        format!(
            "server '{}' was not found in the client routing plan",
            server_name
        )
    })?;
    let context = json_helpers::value_at_path(plan, &["context"])
        .ok_or_else(|| "client routing plan is missing context".to_string())?;

    Ok(PlannedRoute {
        server_name: required_string(&server, &["name"]),
        admission_state: required_string(&server, &["admissionState"]),
        request_strategy: required_string(&server, &["requestStrategy"]),
        request_mutex_key: optional_string(&server, &["requestMutexKey"]),
        session_affinity_key: optional_string(&server, &["sessionAffinityKey"]),
        process_scope_key: required_string(&server, &["processScopeKey"]),
        process_partition: required_string(&server, &["processPartition"]),
        project_binding_key: optional_string(&server, &["projectBindingKey"]),
        worktree_binding_key: optional_string(&server, &["worktreeBindingKey"]),
        conflict_domain: required_string(&server, &["conflictDomain"]),
        host_lock_key: optional_string(&server, &["hostLockKey"]),
        state_profile_key: optional_string(&server, &["stateProfileKey"]),
        scheduler_lane: required_string(&server, &["schedulerLane"]),
        startup_strategy: required_string(&server, &["startupStrategy"]),
        upstream_transport: required_string(&server, &["upstreamTransport"]),
        parallelism_limit: required_usize(&server, &["parallelismLimit"]),
        warnings: strings_at(&server, &["warnings"]),
        client_id: required_string(context, &["clientId"]),
        session_lease_id: required_string(context, &["sessionLeaseId"]),
        session_id: optional_string(context, &["sessionId"]),
        project_root: optional_string(context, &["projectRoot"]),
    })
}

fn find_server_plan(plan: &JsonValue, server_name: &str) -> Option<JsonValue> {
    let wanted = server_name.trim().to_ascii_lowercase();
    json_helpers::array_at_path(plan, &["servers"])?
        .iter()
        .find(|server| {
            json_helpers::string_at_path(server, &["name"])
                .map(|name| name.trim().eq_ignore_ascii_case(&wanted))
                .unwrap_or(false)
        })
        .cloned()
}

fn route_blockers(route: &PlannedRoute) -> Vec<String> {
    let mut blockers = Vec::new();
    if route.admission_state != "configured-source" {
        blockers.push(format!(
            "server '{}' is not routable because admissionState is '{}'",
            route.server_name, route.admission_state
        ));
    }
    if route.request_strategy == "disabled-no-route"
        || route.request_strategy == "legacy-compat-disabled"
        || route.scheduler_lane == "disabled"
        || route.scheduler_lane == "legacy-disabled"
        || route.startup_strategy == "disabled"
        || route.parallelism_limit == 0
    {
        blockers.push(format!(
            "server '{}' is not routable because strategy='{}' lane='{}' startup='{}' parallelismLimit={}",
            route.server_name,
            route.request_strategy,
            route.scheduler_lane,
            route.startup_strategy,
            route.parallelism_limit
        ));
    }
    if route
        .project_binding_key
        .as_deref()
        .map(|value| value.starts_with("project:pending:"))
        .unwrap_or(false)
    {
        blockers.push(format!(
            "server '{}' needs an explicit project root before a runtime lease can be granted",
            route.server_name
        ));
    }
    for warning in &route.warnings {
        if warning.contains("requires a project root but no project root was resolved")
            || warning.contains("no project root was resolved")
            || warning.contains("not declared for the current platform")
        {
            blockers.push(warning.clone());
        }
    }
    blockers.sort();
    blockers.dedup();
    blockers
}

#[derive(Debug, Clone)]
struct LeaseConflict {
    reason: String,
    lease: JsonValue,
}

fn find_conflict(
    store: &BTreeMap<String, JsonValue>,
    route: &PlannedRoute,
) -> Option<LeaseConflict> {
    let leases = leases_map(store)?;
    let candidate_capacity_key = route_capacity_key(route);
    if let Some(mutex_key) = &route.request_mutex_key {
        for lease in leases.values() {
            if optional_string(lease, &["route", "requestMutexKey"]).as_ref() == Some(mutex_key) {
                return Some(LeaseConflict {
                    reason: format!(
                        "mutex '{}' is already held by lease '{}'",
                        mutex_key,
                        required_string(lease, &["leaseId"])
                    ),
                    lease: lease.clone(),
                });
            }
        }
        return None;
    }

    if route.parallelism_limit > 0 {
        let mut matching = Vec::new();
        for lease in leases.values() {
            if optional_string(lease, &["route", "capacityKey"]).as_ref()
                == Some(&candidate_capacity_key)
            {
                matching.push(lease.clone());
            }
        }
        if matching.len() >= route.parallelism_limit {
            return Some(LeaseConflict {
                reason: format!(
                    "parallelism limit {} for '{}' is already exhausted",
                    route.parallelism_limit, candidate_capacity_key
                ),
                lease: matching.into_iter().next().unwrap_or(JsonValue::Null),
            });
        }
    }

    None
}

fn takeover_allowed(takeover: bool, conflicting_lease: &JsonValue, route: &PlannedRoute) -> bool {
    takeover
        && json_helpers::string_at_path(conflicting_lease, &["sessionLeaseId"])
            .map(|value| value == route.session_lease_id)
            .unwrap_or(false)
}

fn lease_record_json(
    lease_id: &str,
    route: &PlannedRoute,
    acquired_at_ms: u128,
    expires_at_ms: u128,
    ttl_ms: u128,
) -> JsonValue {
    JsonValue::object([
        ("leaseId", JsonValue::string(lease_id.to_string())),
        ("server", JsonValue::string(route.server_name.clone())),
        ("clientId", JsonValue::string(route.client_id.clone())),
        (
            "sessionLeaseId",
            JsonValue::string(route.session_lease_id.clone()),
        ),
        (
            "sessionId",
            json_helpers::json_string_or_null(route.session_id.clone()),
        ),
        (
            "projectRoot",
            json_helpers::json_string_or_null(route.project_root.clone()),
        ),
        ("acquiredAtMs", JsonValue::number(acquired_at_ms)),
        ("expiresAtMs", JsonValue::number(expires_at_ms)),
        ("ttlMs", JsonValue::number(ttl_ms)),
        ("route", route_json(route)),
    ])
}

fn route_json(route: &PlannedRoute) -> JsonValue {
    JsonValue::object([
        ("server", JsonValue::string(route.server_name.clone())),
        (
            "admissionState",
            JsonValue::string(route.admission_state.clone()),
        ),
        (
            "requestStrategy",
            JsonValue::string(route.request_strategy.clone()),
        ),
        (
            "requestMutexKey",
            json_helpers::json_string_or_null(route.request_mutex_key.clone()),
        ),
        (
            "sessionAffinityKey",
            json_helpers::json_string_or_null(route.session_affinity_key.clone()),
        ),
        ("capacityKey", JsonValue::string(route_capacity_key(route))),
        (
            "processScopeKey",
            JsonValue::string(route.process_scope_key.clone()),
        ),
        (
            "processPartition",
            JsonValue::string(route.process_partition.clone()),
        ),
        (
            "projectBindingKey",
            json_helpers::json_string_or_null(route.project_binding_key.clone()),
        ),
        (
            "worktreeBindingKey",
            json_helpers::json_string_or_null(route.worktree_binding_key.clone()),
        ),
        (
            "conflictDomain",
            JsonValue::string(route.conflict_domain.clone()),
        ),
        (
            "hostLockKey",
            json_helpers::json_string_or_null(route.host_lock_key.clone()),
        ),
        (
            "stateProfileKey",
            json_helpers::json_string_or_null(route.state_profile_key.clone()),
        ),
        (
            "schedulerLane",
            JsonValue::string(route.scheduler_lane.clone()),
        ),
        (
            "startupStrategy",
            JsonValue::string(route.startup_strategy.clone()),
        ),
        (
            "upstreamTransport",
            JsonValue::string(route.upstream_transport.clone()),
        ),
        (
            "parallelismLimit",
            JsonValue::number(route.parallelism_limit),
        ),
    ])
}

fn acquire_blocked_json(
    route: &PlannedRoute,
    now_ms: u128,
    expired_purged_count: usize,
    reason: String,
    conflicting_lease: Option<JsonValue>,
    active_lease_count: usize,
) -> JsonValue {
    JsonValue::object([
        ("status", JsonValue::string("blocked")),
        ("reason", JsonValue::string(reason)),
        ("server", JsonValue::string(route.server_name.clone())),
        ("clientId", JsonValue::string(route.client_id.clone())),
        (
            "sessionLeaseId",
            JsonValue::string(route.session_lease_id.clone()),
        ),
        ("nowMs", JsonValue::number(now_ms)),
        ("activeLeaseCount", JsonValue::number(active_lease_count)),
        (
            "expiredPurgedCount",
            JsonValue::number(expired_purged_count),
        ),
        ("route", route_json(route)),
        (
            "conflictingLease",
            conflicting_lease.unwrap_or(JsonValue::Null),
        ),
        (
            "warnings",
            JsonValue::array(route.warnings.iter().cloned().map(JsonValue::string)),
        ),
    ])
}

fn route_capacity_key(route: &PlannedRoute) -> String {
    route
        .request_mutex_key
        .clone()
        .unwrap_or_else(|| route.process_scope_key.clone())
}

fn number_u128_at(value: &JsonValue, path: &[&str]) -> Option<u128> {
    json_helpers::value_at_path(value, path).and_then(|value| match value {
        JsonValue::Number(text) => text.parse::<u128>().ok(),
        _ => None,
    })
}

fn required_string(value: &JsonValue, path: &[&str]) -> String {
    optional_string(value, path).unwrap_or_default()
}

fn optional_string(value: &JsonValue, path: &[&str]) -> Option<String> {
    json_helpers::value_at_path(value, path).and_then(|value| match value {
        JsonValue::String(text) => Some(text.clone()),
        JsonValue::Null => None,
        _ => None,
    })
}

fn required_usize(value: &JsonValue, path: &[&str]) -> usize {
    json_helpers::value_at_path(value, path)
        .and_then(JsonValue::as_i64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(1)
}

fn strings_at(value: &JsonValue, path: &[&str]) -> Vec<String> {
    json_helpers::array_at_path(value, path)
        .unwrap_or(&[])
        .iter()
        .filter_map(|item| item.as_str().map(|text| text.to_string()))
        .collect()
}

fn expires_at_ms(value: &JsonValue) -> Option<u128> {
    json_helpers::value_at_path(value, &["expiresAtMs"]).and_then(|value| match value {
        JsonValue::Number(text) => text.parse::<u128>().ok(),
        _ => None,
    })
}

fn normalize_ttl(ttl_ms: Option<u128>) -> u128 {
    ttl_ms
        .filter(|value| *value > 0)
        .map(|value| value.min(MAX_LEASE_TTL_MS))
        .unwrap_or(DEFAULT_LEASE_TTL_MS)
}

fn clean_required_server_name(server_name: &str) -> Result<String, String> {
    let server_name = server_name.trim();
    if server_name.is_empty() {
        return Err("hub lease acquire requires --server <name>".to_string());
    }
    if server_name.chars().any(|ch| ch.is_control()) {
        return Err("hub lease server name must not contain control characters".to_string());
    }
    Ok(server_name.to_string())
}

fn non_empty_token(value: &str, fallback: &str) -> String {
    let token = sanitize_token(value);
    if token.is_empty() {
        fallback.to_string()
    } else {
        token
    }
}

fn generate_lease_id(route: &PlannedRoute, now_ms: u128) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(now_ms);
    format!(
        "lease:{}:{}:{}:{}",
        sanitize_token(&route.server_name),
        now_ms,
        std::process::id(),
        nanos
    )
}

fn sanitize_token(value: &str) -> String {
    let mut output = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            output.push(ch.to_ascii_lowercase());
        } else if matches!(ch, '-' | '_' | '.') {
            output.push(ch);
        } else if !output.ends_with('-') {
            output.push('-');
        }
    }
    output.trim_matches('-').to_string()
}

fn write_text_result(value: &JsonValue, stdout: &mut dyn Write) {
    let status = json_helpers::string_at_path(value, &["status"]).unwrap_or("unknown");
    let _ = writeln!(stdout, "Lease status: {}", status);
    if let Some(lease_id) = json_helpers::string_at_path(value, &["leaseId"]) {
        let _ = writeln!(stdout, "Lease id: {}", lease_id);
    }
    if let Some(server) = json_helpers::string_at_path(value, &["server"]) {
        let _ = writeln!(stdout, "Server: {}", server);
    }
    if let Some(reason) = json_helpers::string_at_path(value, &["reason"]) {
        let _ = writeln!(stdout, "Reason: {}", reason);
    }
    if let Some(active) = json_helpers::value_at_path(value, &["activeLeaseCount"]) {
        if let Some(number) = active.as_i64() {
            let _ = writeln!(stdout, "Active leases: {}", number);
        }
    }
    if let Some(leases) = json_helpers::array_at_path(value, &["leases"]) {
        for lease in leases {
            let lease_id = json_helpers::string_at_path(lease, &["leaseId"]).unwrap_or("unknown");
            let server = json_helpers::string_at_path(lease, &["server"]).unwrap_or("unknown");
            let strategy = json_helpers::string_at_path(lease, &["route", "requestStrategy"])
                .unwrap_or("unknown");
            let _ = writeln!(stdout, "- {} :: {} :: {}", lease_id, server, strategy);
        }
    }
}
