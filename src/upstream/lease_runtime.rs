use super::diagnostics::stderr_suffix;
use super::http_runtime::run_http_request;
use super::inventory::configured_inventory;
use super::process_config::child_process_path;
use super::server_config::{find_server, load_servers};
use super::session_pool::{
    UpstreamPoolCallOutcome, UpstreamPoolInvocation, UpstreamSessionKey, UpstreamSessionPool,
};
use super::stdio_runtime::{run_stdio_request, run_stdio_tool_calls};
use super::tool_cache::cached_tools_list;
use super::{
    cache_root_path, context_string, ensure_callable_stdio, optional_json_string,
    server_fingerprint, server_runtime_callable, timeout_for, ToolRiskPolicy, UpstreamLeaseContext,
    UpstreamServerConfig, UpstreamToolCall, TOOL_LIST_CACHE_TTL,
};
use crate::hub::leases::{self, RuntimeLeaseAcquireResult, RuntimeLeaseRequest};
use crate::json::JsonValue;
use crate::json_helpers;
use crate::mcp_sources;
use std::collections::{hash_map::DefaultHasher, BTreeSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc::Receiver,
    Arc, Mutex,
};
use std::thread;
use std::time::{Duration, Instant};

const ALLOW_UNKNOWN_TOOL_ARGUMENT: &str = "allowUnknownTool";
const ALLOW_UNKNOWN_UPSTREAM_TOOL_ARGUMENT: &str = "allowUnknownUpstreamTool";

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

    pub(super) fn heartbeat_lost_flag(&self) -> Option<&AtomicBool> {
        self.heartbeat
            .as_ref()
            .map(|heartbeat| heartbeat.lost.as_ref())
    }
}

impl UpstreamLeaseAttachment {
    pub(super) fn heartbeat_lost_flag(&self) -> Option<&AtomicBool> {
        match self {
            UpstreamLeaseAttachment::Attached(guard) => guard.heartbeat_lost_flag(),
        }
    }
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
    if server.source_type == "http" {
        return run_http_request(server, method, params, effective_timeout);
    }
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
    let effective_timeout = timeout_for(server, timeout_ms);
    validate_upstream_tool_policy(server, tool_name, context)?;

    let lease = acquire_upstream_lease(root_path, server_name, context, effective_timeout)?;
    validate_upstream_tool_known(root_path, server, tool_name, effective_timeout, context)?;
    let heartbeat_lost = lease.heartbeat_lost_flag();
    let call_params = JsonValue::object([
        ("name", JsonValue::string(tool_name)),
        ("arguments", arguments.clone()),
    ]);
    let result = match if server.source_type == "http" {
        run_http_request(server, "tools/call", Some(call_params), effective_timeout)
    } else {
        run_stdio_request(
            root_path,
            server,
            "tools/call",
            Some(call_params),
            effective_timeout,
            heartbeat_lost,
        )
    } {
        Ok(value) => value,
        Err(error) => {
            log_tool_call_audit(
                root_path,
                server_name,
                tool_name,
                arguments,
                context,
                false,
                false,
                false,
                None,
                None,
                Some(&error),
            );
            return Err(error);
        }
    };
    let lease_outcome = finalize_upstream_lease(lease)?;
    let lease_id_for_audit = lease_outcome.lease_id.clone();
    let upstream_is_error = json_helpers::bool_at_path(&result, &["isError"]).unwrap_or(false);
    let upstream_ok = !upstream_is_error;
    log_tool_call_audit(
        root_path,
        server_name,
        tool_name,
        arguments,
        context,
        true,
        upstream_ok,
        false,
        lease_id_for_audit.as_deref(),
        None,
        None,
    );

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
    let effective_timeout = timeout_for(server, timeout_ms);
    validate_upstream_tool_policy(server, tool_name, context)?;
    if server.source_type == "http" {
        return call_tool_with_context(
            root_path,
            server_name,
            tool_name,
            arguments,
            timeout_ms,
            context,
        );
    }

    let lease = acquire_upstream_lease(root_path, server_name, context, effective_timeout)?;
    let heartbeat_lost = lease.heartbeat_lost_flag();
    let pool_key = upstream_session_key(root_path, server, context);
    let audit_pool_key = pool_key.clone();
    let (result, pool_outcome) = {
        let mut pool = pool
            .lock()
            .map_err(|_| "upstream session pool lock was poisoned".to_string())?;
        let initial_pool_hit = pool.session_exists(&pool_key);
        validate_upstream_tool_known_with_pool(
            root_path,
            server,
            tool_name,
            effective_timeout,
            context,
            heartbeat_lost,
            &pool_key,
            &mut pool,
        )?;
        let (result, mut outcome) = match pool.call_tool(
            UpstreamPoolInvocation {
                root_path,
                server,
                key: pool_key,
                timeout: effective_timeout,
                lease_lost: heartbeat_lost,
            },
            tool_name,
            arguments,
        ) {
            Ok(value) => value,
            Err(error) => {
                log_tool_call_audit(
                    root_path,
                    server_name,
                    tool_name,
                    arguments,
                    context,
                    false,
                    false,
                    true,
                    None,
                    Some(&audit_pool_key),
                    Some(&error),
                );
                return Err(error);
            }
        };
        outcome.hit = initial_pool_hit;
        (result, outcome)
    };
    let lease_outcome = finalize_upstream_lease(lease)?;
    let lease_id_for_audit = lease_outcome.lease_id.clone();
    let upstream_is_error = json_helpers::bool_at_path(&result, &["isError"]).unwrap_or(false);
    let upstream_ok = !upstream_is_error;
    log_tool_call_audit(
        root_path,
        server_name,
        tool_name,
        arguments,
        context,
        true,
        upstream_ok,
        true,
        lease_id_for_audit.as_deref(),
        Some(&audit_pool_key),
        None,
    );

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
    let effective_timeout = timeout_for(server, timeout_ms);
    validate_upstream_batch_tool_policy(server, calls, context)?;

    let lease = acquire_upstream_lease(root_path, server_name, context, effective_timeout)?;
    validate_upstream_batch_tools_known(root_path, server, calls, effective_timeout, context)?;
    let heartbeat_lost = lease.heartbeat_lost_flag();
    let results = if server.source_type == "http" {
        let mut results = Vec::new();
        for (index, call) in calls.iter().enumerate() {
            let result = match run_http_request(
                server,
                "tools/call",
                Some(JsonValue::object([
                    ("name", JsonValue::string(call.tool.clone())),
                    ("arguments", call.arguments.clone()),
                ])),
                effective_timeout,
            ) {
                Ok(value) => value,
                Err(error) => {
                    log_tool_batch_audit(
                        root_path,
                        server_name,
                        calls,
                        context,
                        false,
                        false,
                        None,
                        None,
                        0,
                        calls.len(),
                        Some(&error),
                    );
                    return Err(error);
                }
            };
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
        }
        results
    } else {
        match run_stdio_tool_calls(root_path, server, calls, effective_timeout, heartbeat_lost) {
            Ok(value) => value,
            Err(error) => {
                log_tool_batch_audit(
                    root_path,
                    server_name,
                    calls,
                    context,
                    false,
                    false,
                    None,
                    None,
                    0,
                    calls.len(),
                    Some(&error),
                );
                return Err(error);
            }
        }
    };
    let lease_outcome = finalize_upstream_lease(lease)?;
    let lease_id_for_audit = lease_outcome.lease_id.clone();
    let upstream_ok_count = results
        .iter()
        .filter(|item| json_helpers::bool_at_path(item, &["upstreamOk"]).unwrap_or(false))
        .count();
    let upstream_failed_count = results.len().saturating_sub(upstream_ok_count);
    let upstream_ok = upstream_failed_count == 0;
    log_tool_batch_audit(
        root_path,
        server_name,
        calls,
        context,
        true,
        false,
        lease_id_for_audit.as_deref(),
        None,
        upstream_ok_count,
        upstream_failed_count,
        None,
    );

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
    let effective_timeout = timeout_for(server, timeout_ms);
    validate_upstream_batch_tool_policy(server, calls, context)?;
    if server.source_type == "http" {
        return call_tools_with_context(root_path, server_name, calls, timeout_ms, context);
    }

    let lease = acquire_upstream_lease(root_path, server_name, context, effective_timeout)?;
    let heartbeat_lost = lease.heartbeat_lost_flag();
    let pool_key = upstream_session_key(root_path, server, context);
    let audit_pool_key = pool_key.clone();
    let (results, pool_outcome) = {
        let mut pool = pool
            .lock()
            .map_err(|_| "upstream session pool lock was poisoned".to_string())?;
        let initial_pool_hit = pool.session_exists(&pool_key);
        validate_upstream_batch_tools_known_with_pool(
            root_path,
            server,
            calls,
            effective_timeout,
            context,
            heartbeat_lost,
            &pool_key,
            &mut pool,
        )?;
        let (results, mut outcome) = match pool.call_tools(
            UpstreamPoolInvocation {
                root_path,
                server,
                key: pool_key,
                timeout: effective_timeout,
                lease_lost: heartbeat_lost,
            },
            calls,
        ) {
            Ok(value) => value,
            Err(error) => {
                log_tool_batch_audit(
                    root_path,
                    server_name,
                    calls,
                    context,
                    false,
                    true,
                    None,
                    Some(&audit_pool_key),
                    0,
                    calls.len(),
                    Some(&error),
                );
                return Err(error);
            }
        };
        outcome.hit = initial_pool_hit;
        (results, outcome)
    };
    let lease_outcome = finalize_upstream_lease(lease)?;
    let lease_id_for_audit = lease_outcome.lease_id.clone();
    let upstream_ok_count = results
        .iter()
        .filter(|item| json_helpers::bool_at_path(item, &["upstreamOk"]).unwrap_or(false))
        .count();
    let upstream_failed_count = results.len().saturating_sub(upstream_ok_count);
    let upstream_ok = upstream_failed_count == 0;
    log_tool_batch_audit(
        root_path,
        server_name,
        calls,
        context,
        true,
        true,
        lease_id_for_audit.as_deref(),
        Some(&audit_pool_key),
        upstream_ok_count,
        upstream_failed_count,
        None,
    );

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

fn validate_upstream_batch_tools_known(
    root_path: &Path,
    server: &UpstreamServerConfig,
    calls: &[UpstreamToolCall],
    timeout: Duration,
    context: Option<&UpstreamLeaseContext>,
) -> Result<(), String> {
    if calls.is_empty() {
        return Ok(());
    }
    let tools = verified_tool_names_for_call(root_path, server, timeout, context)?;
    for call in calls {
        ensure_tool_name_in_verified_set(server, call.tool.trim(), &tools)?;
    }
    Ok(())
}

fn validate_upstream_tool_known(
    root_path: &Path,
    server: &UpstreamServerConfig,
    tool_name: &str,
    timeout: Duration,
    context: Option<&UpstreamLeaseContext>,
) -> Result<(), String> {
    let tools = verified_tool_names_for_call(root_path, server, timeout, context)?;
    ensure_tool_name_in_verified_set(server, tool_name, &tools)
}

#[allow(clippy::too_many_arguments)]
fn validate_upstream_tool_known_with_pool(
    root_path: &Path,
    server: &UpstreamServerConfig,
    tool_name: &str,
    timeout: Duration,
    context: Option<&UpstreamLeaseContext>,
    lease_lost: Option<&AtomicBool>,
    key: &UpstreamSessionKey,
    pool: &mut UpstreamSessionPool,
) -> Result<(), String> {
    let tools = verified_tool_names_for_call_with_pool(
        root_path, server, timeout, context, lease_lost, key, pool,
    )?;
    ensure_tool_name_in_verified_set(server, tool_name, &tools)
}

#[allow(clippy::too_many_arguments)]
fn validate_upstream_batch_tools_known_with_pool(
    root_path: &Path,
    server: &UpstreamServerConfig,
    calls: &[UpstreamToolCall],
    timeout: Duration,
    context: Option<&UpstreamLeaseContext>,
    lease_lost: Option<&AtomicBool>,
    key: &UpstreamSessionKey,
    pool: &mut UpstreamSessionPool,
) -> Result<(), String> {
    if calls.is_empty() {
        return Ok(());
    }
    let tools = verified_tool_names_for_call_with_pool(
        root_path, server, timeout, context, lease_lost, key, pool,
    )?;
    for call in calls {
        ensure_tool_name_in_verified_set(server, call.tool.trim(), &tools)?;
    }
    Ok(())
}

fn verified_tool_names_for_call(
    root_path: &Path,
    server: &UpstreamServerConfig,
    timeout: Duration,
    context: Option<&UpstreamLeaseContext>,
) -> Result<BTreeSet<String>, String> {
    if !known_tool_validation_required(context) {
        return Ok(BTreeSet::new());
    }
    let (tools, cache_hit) = cached_tools_list(root_path, server, timeout, false).map_err(|error| {
        format!(
            "refusing to call upstream server '{}' because tools/list could not verify the requested tool: {}. Retry after upstream_tools/upstream_probe succeeds, or pass {}=true only when the server intentionally supports dynamic hidden tools.",
            server.name, error, ALLOW_UNKNOWN_TOOL_ARGUMENT
        )
    })?;
    tool_names_from_tools_list(server, &tools, if cache_hit { " from cache" } else { "" })
}

fn verified_tool_names_for_call_with_pool(
    root_path: &Path,
    server: &UpstreamServerConfig,
    timeout: Duration,
    context: Option<&UpstreamLeaseContext>,
    lease_lost: Option<&AtomicBool>,
    key: &UpstreamSessionKey,
    pool: &mut UpstreamSessionPool,
) -> Result<BTreeSet<String>, String> {
    if !known_tool_validation_required(context) {
        return Ok(BTreeSet::new());
    }
    if let Some(tools) = super::tool_cache::read_cached_tools(
        &super::tool_cache::tool_list_cache_key(root_path, server),
    ) {
        return tool_names_from_tools_list(server, &tools, " from cache");
    }
    let (result, _) = pool.list_tools(UpstreamPoolInvocation {
        root_path,
        server,
        key: key.clone(),
        timeout,
        lease_lost,
    })?;
    let tools = json_helpers::value_at_path(&result, &["tools"])
        .cloned()
        .unwrap_or_else(|| JsonValue::array([]));
    tool_names_from_tools_list(server, &tools, " from pooled session")
}

fn tool_names_from_tools_list(
    server: &UpstreamServerConfig,
    tools: &JsonValue,
    source_hint: &str,
) -> Result<BTreeSet<String>, String> {
    let advertised = tools
        .as_array()
        .unwrap_or(&[])
        .iter()
        .filter_map(|tool| json_helpers::string_at_path(tool, &["name"]))
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .collect::<BTreeSet<_>>();
    if advertised.is_empty() {
        return Err(format!(
            "refusing to call upstream server '{}' because its tools/list returned no named tools{}; pass {}=true only for explicitly trusted dynamic hidden tools",
            server.name,
            source_hint,
            ALLOW_UNKNOWN_TOOL_ARGUMENT
        ));
    }
    Ok(advertised)
}

fn ensure_tool_name_in_verified_set(
    server: &UpstreamServerConfig,
    tool_name: &str,
    advertised: &BTreeSet<String>,
) -> Result<(), String> {
    if advertised.is_empty() {
        return Ok(());
    }
    if advertised.contains(tool_name) {
        return Ok(());
    }
    Err(format!(
        "refusing to call upstream server '{}' tool '{}' because it is not present in the server's current tools/list; use upstream_search/upstream_tools to refresh discovery, or pass {}=true only for explicitly trusted dynamic hidden tools",
        server.name, tool_name, ALLOW_UNKNOWN_TOOL_ARGUMENT
    ))
}

fn context_allows_unknown_tool(context: Option<&UpstreamLeaseContext>) -> bool {
    let Some(context) = context else {
        return false;
    };
    context
        .allow_arguments
        .contains(ALLOW_UNKNOWN_TOOL_ARGUMENT)
        || context
            .allow_arguments
            .contains(ALLOW_UNKNOWN_UPSTREAM_TOOL_ARGUMENT)
}

fn known_tool_validation_required(context: Option<&UpstreamLeaseContext>) -> bool {
    known_tool_validation_enabled() && !context_allows_unknown_tool(context)
}

fn known_tool_validation_enabled() -> bool {
    !std::env::var("MCPACE_ALLOW_UNKNOWN_UPSTREAM_TOOLS")
        .ok()
        .map(|value| {
            let value = value.trim().to_ascii_lowercase();
            matches!(value.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(false)
}

pub(super) fn validate_upstream_batch_tool_policy(
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

pub(super) fn validate_upstream_tool_policy(
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
    pub(super) fn matches_tool(&self, tool_name: &str) -> bool {
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

#[allow(clippy::too_many_arguments)]
fn log_tool_call_audit(
    root_path: &Path,
    server_name: &str,
    tool_name: &str,
    arguments: &JsonValue,
    context: Option<&UpstreamLeaseContext>,
    bridge_ok: bool,
    upstream_ok: bool,
    pooled: bool,
    lease_id: Option<&str>,
    pool_key: Option<&UpstreamSessionKey>,
    error: Option<&str>,
) {
    let upstream_is_error = bridge_ok && !upstream_ok;
    let level = if bridge_ok && upstream_ok {
        "info"
    } else {
        "warn"
    };
    let trace = upstream_session_trace(server_name, context, pool_key);
    let _ = crate::hub::runtime::append_log(
        root_path,
        level,
        "tool_call_audit",
        &[
            ("server", JsonValue::string(server_name)),
            ("tool", JsonValue::string(tool_name)),
            ("bridgeOk", JsonValue::bool(bridge_ok)),
            ("upstreamOk", JsonValue::bool(upstream_ok)),
            ("upstreamIsError", JsonValue::bool(upstream_is_error)),
            ("pooled", JsonValue::bool(pooled)),
            (
                "leaseId",
                optional_json_string(lease_id.map(str::to_string)),
            ),
            ("trace", JsonValue::string(trace)),
            (
                "argumentsFingerprint",
                JsonValue::string(argument_fingerprint(arguments)),
            ),
            ("clientId", context_field(context, "client_id")),
            ("sessionId", context_field(context, "session_id")),
            ("projectRoot", context_field(context, "project_root")),
            ("transport", context_field(context, "transport")),
            ("error", optional_json_string(error.map(str::to_string))),
        ],
    );
}

#[allow(clippy::too_many_arguments)]
fn log_tool_batch_audit(
    root_path: &Path,
    server_name: &str,
    calls: &[UpstreamToolCall],
    context: Option<&UpstreamLeaseContext>,
    bridge_ok: bool,
    pooled: bool,
    lease_id: Option<&str>,
    pool_key: Option<&UpstreamSessionKey>,
    upstream_ok_count: usize,
    upstream_failed_count: usize,
    error: Option<&str>,
) {
    let upstream_ok = bridge_ok && upstream_failed_count == 0;
    let level = if bridge_ok && upstream_ok {
        "info"
    } else {
        "warn"
    };
    let trace = upstream_session_trace(server_name, context, pool_key);
    let tools = calls
        .iter()
        .map(|call| JsonValue::string(call.tool.clone()));
    let fingerprints = calls
        .iter()
        .map(|call| JsonValue::string(argument_fingerprint(&call.arguments)));
    let _ = crate::hub::runtime::append_log(
        root_path,
        level,
        "tool_batch_audit",
        &[
            ("server", JsonValue::string(server_name)),
            ("bridgeOk", JsonValue::bool(bridge_ok)),
            ("upstreamOk", JsonValue::bool(upstream_ok)),
            ("upstreamOkCount", JsonValue::number(upstream_ok_count)),
            (
                "upstreamFailedCount",
                JsonValue::number(upstream_failed_count),
            ),
            ("callCount", JsonValue::number(calls.len())),
            ("tools", JsonValue::array(tools)),
            ("argumentsFingerprints", JsonValue::array(fingerprints)),
            ("pooled", JsonValue::bool(pooled)),
            (
                "leaseId",
                optional_json_string(lease_id.map(str::to_string)),
            ),
            ("trace", JsonValue::string(trace)),
            ("clientId", context_field(context, "client_id")),
            ("sessionId", context_field(context, "session_id")),
            ("projectRoot", context_field(context, "project_root")),
            ("transport", context_field(context, "transport")),
            ("error", optional_json_string(error.map(str::to_string))),
        ],
    );
}

fn argument_fingerprint(arguments: &JsonValue) -> String {
    let compact = arguments.to_compact_string();
    let mut hasher = DefaultHasher::new();
    compact.hash(&mut hasher);
    format!("len{}-{:016x}", compact.len(), hasher.finish())
}

fn upstream_session_trace(
    server_name: &str,
    context: Option<&UpstreamLeaseContext>,
    pool_key: Option<&UpstreamSessionKey>,
) -> String {
    if let Some(key) = pool_key {
        return format!(
            "chat={} project={} client={} -> {}#{}",
            compact_trace_value(&key.session_id),
            compact_trace_value(&key.project_root),
            compact_trace_value(&key.client_id),
            server_name,
            short_hash(&format!(
                "{}|{}|{}|{}",
                key.server_name, key.session_id, key.project_root, key.metadata_fingerprint
            ))
        );
    }
    let client = context
        .and_then(|value| value.client_id.as_ref())
        .map(String::as_str)
        .unwrap_or("mcpace-upstream-bridge");
    let session = context
        .and_then(|value| value.session_id.as_ref())
        .map(String::as_str)
        .unwrap_or("anonymous-session");
    let project = context
        .and_then(|value| value.project_root.as_ref())
        .map(String::as_str)
        .unwrap_or("unresolved-project");
    format!(
        "chat={} project={} client={} -> {}#{}",
        compact_trace_value(session),
        compact_trace_value(project),
        compact_trace_value(client),
        server_name,
        short_hash(&format!("{}|{}|{}", server_name, session, project))
    )
}

fn short_hash(value: &str) -> String {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    format!("{:08x}", hasher.finish() & 0xffff_ffff)
}

fn compact_trace_value(value: &str) -> String {
    let value = value.trim();
    let mut chars = value.chars();
    let prefix = chars.by_ref().take(48).collect::<String>();
    if chars.next().is_some() {
        format!("{}…", prefix)
    } else {
        prefix
    }
}

fn context_field(context: Option<&UpstreamLeaseContext>, key: &str) -> JsonValue {
    let value = match key {
        "client_id" => context.and_then(|context| context.client_id.clone()),
        "session_id" => context.and_then(|context| context.session_id.clone()),
        "project_root" => context.and_then(|context| context.project_root.clone()),
        "transport" => context.and_then(|context| context.transport.clone()),
        _ => None,
    };
    optional_json_string(value)
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
    let (settings_modified_ms, settings_len) = mcp_sources::mcp_settings_fingerprint(root_path);
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

pub(super) fn runtime_lease_lost_error(
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
