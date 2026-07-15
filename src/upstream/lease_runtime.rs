use super::diagnostics::stderr_suffix;
use super::http_runtime::{run_http_request, run_http_tool_calls};
use super::inventory::configured_inventory;
use super::lease_queue;
use super::process_config::child_process_path;
use super::server_config::{find_server, load_servers};
use super::session_pool::{
    UpstreamPoolCallOutcome, UpstreamPoolInvocation, UpstreamSessionCheckout, UpstreamSessionKey,
    UpstreamSessionPool,
};
use super::stdio_runtime::{run_stdio_request, run_stdio_tool_calls};
use super::tool_cache::cached_tools_list;
use super::{
    cache_root_path, context_string, ensure_callable_stdio, optional_json_string,
    server_fingerprint, server_runtime_callable, timeout_for, validate_tool_call_result,
    ToolRiskPolicy, UpstreamLeaseContext, UpstreamServerConfig, UpstreamToolCall,
    TOOL_LIST_CACHE_TTL,
};
use crate::execution::{ExecutionAffinityContext, ExecutionAffinityKey};
use crate::hub::leases::{self, RuntimeLeaseAcquireResult, RuntimeLeaseRequest};
use crate::json::JsonValue;
use crate::json_helpers;
use crate::mcp_sources;
use std::collections::{hash_map::DefaultHasher, BTreeSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    mpsc::Receiver,
    Arc,
};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const ALLOW_UNKNOWN_TOOL_ARGUMENT: &str = "allowUnknownTool";
const ALLOW_UNKNOWN_UPSTREAM_TOOL_ARGUMENT: &str = "allowUnknownUpstreamTool";

static TOOL_AUDIT_SEQUENCE: AtomicU64 = AtomicU64::new(1);

fn remaining_upstream_timeout(
    deadline: Instant,
    server_name: &str,
    phase: &str,
) -> Result<Duration, String> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err(format!(
            "upstream server '{}' timed out before {}",
            server_name, phase
        ));
    }
    Ok(remaining)
}

struct UpstreamLeaseGuard {
    root_path: PathBuf,
    lease_id: String,
    lease: JsonValue,
    released: bool,
    heartbeat: Option<LeaseHeartbeat>,
    queue: LeaseQueueMetrics,
    affinity: ExecutionAffinityKey,
}

#[derive(Clone, Debug, Default)]
struct LeaseQueueMetrics {
    attempts: usize,
    wait_ms: u128,
    timeout_ms: u64,
    ticket: Option<u64>,
    depth_at_enqueue: usize,
    ahead_at_enqueue: usize,
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

struct PooledToolCallResult {
    result: JsonValue,
    pool_outcome: Option<UpstreamPoolCallOutcome>,
    validation: Result<bool, String>,
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
    queue: LeaseQueueMetrics,
}

impl Drop for UpstreamLeaseGuard {
    fn drop(&mut self) {
        if !self.released {
            self.stop_heartbeat();
            let _ = leases::release_runtime_lease(&self.root_path, &self.lease_id);
            lease_queue::notify_all_lanes();
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
        lease_queue::notify_all_lanes();
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

    fn affinity_key(&self) -> &ExecutionAffinityKey {
        match self {
            UpstreamLeaseAttachment::Attached(guard) => &guard.affinity,
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
        return run_http_request(server, method, params, effective_timeout).map_err(String::from);
    }
    run_stdio_request(root_path, server, method, params, effective_timeout, None)
        .map_err(String::from)
}
pub fn list_tools(
    root_path: &Path,
    server_name: Option<&str>,
    timeout_ms: Option<u64>,
    refresh: bool,
) -> Result<JsonValue, String> {
    let Some(server_name) = server_name.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(configured_inventory(root_path)?);
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
    let total_started = Instant::now();
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
    if let Err(error) = validate_upstream_tool_policy(server, tool_name, context) {
        let metrics = ToolAuditMetrics::for_single(
            arguments,
            None,
            context,
            0,
            0,
            total_started.elapsed().as_millis(),
        );
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
            &metrics,
            Some(&error),
        );
        return Err(error);
    }

    let queue_started = Instant::now();
    let lease = match acquire_upstream_lease(root_path, server, context, effective_timeout) {
        Ok(value) => value,
        Err(error) => {
            let metrics = ToolAuditMetrics::for_single(
                arguments,
                None,
                context,
                queue_started.elapsed().as_millis(),
                0,
                total_started.elapsed().as_millis(),
            );
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
                &metrics,
                Some(&error),
            );
            return Err(error);
        }
    };
    let queue_duration_ms = queue_started.elapsed().as_millis();
    let upstream_deadline = Instant::now() + effective_timeout;
    if let Err(error) =
        remaining_upstream_timeout(upstream_deadline, server_name, "tools/list verification")
            .and_then(|verification_timeout| {
                validate_upstream_tool_known(
                    root_path,
                    server,
                    tool_name,
                    verification_timeout,
                    context,
                )
            })
    {
        let metrics = ToolAuditMetrics::for_single(
            arguments,
            None,
            context,
            queue_duration_ms,
            0,
            total_started.elapsed().as_millis(),
        );
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
            &metrics,
            Some(&error),
        );
        return Err(error);
    }
    let heartbeat_lost = lease.heartbeat_lost_flag();
    let call_params = JsonValue::object([
        ("name", JsonValue::string(tool_name)),
        ("arguments", arguments.clone()),
    ]);
    let upstream_started = Instant::now();
    let call_result = remaining_upstream_timeout(upstream_deadline, server_name, "tools/call")
        .and_then(|call_timeout| {
            if server.source_type == "http" {
                run_http_request(server, "tools/call", Some(call_params), call_timeout)
                    .map_err(String::from)
            } else {
                run_stdio_request(
                    root_path,
                    server,
                    "tools/call",
                    Some(call_params),
                    call_timeout,
                    heartbeat_lost,
                )
                .map_err(String::from)
            }
        });
    let upstream_duration_ms = upstream_started.elapsed().as_millis();
    let result = match call_result {
        Ok(value) => value,
        Err(error) => {
            let metrics = ToolAuditMetrics::for_single(
                arguments,
                None,
                context,
                queue_duration_ms,
                upstream_duration_ms,
                total_started.elapsed().as_millis(),
            );
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
                &metrics,
                Some(&error),
            );
            return Err(error);
        }
    };
    let upstream_is_error = match validate_tool_call_result(&server.name, tool_name, &result) {
        Ok(value) => value,
        Err(error) => {
            let metrics = ToolAuditMetrics::for_single(
                arguments,
                Some(&result),
                context,
                queue_duration_ms,
                upstream_duration_ms,
                total_started.elapsed().as_millis(),
            );
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
                &metrics,
                Some(&error),
            );
            return Err(error);
        }
    };
    let lease_outcome = match finalize_upstream_lease(lease) {
        Ok(value) => value,
        Err(error) => {
            let metrics = ToolAuditMetrics::for_single(
                arguments,
                Some(&result),
                context,
                queue_duration_ms,
                upstream_duration_ms,
                total_started.elapsed().as_millis(),
            );
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
                &metrics,
                Some(&error),
            );
            return Err(error);
        }
    };
    let lease_id_for_audit = lease_outcome.lease_id.clone();
    let upstream_ok = !upstream_is_error;
    let metrics = ToolAuditMetrics::for_single(
        arguments,
        Some(&result),
        context,
        queue_duration_ms,
        upstream_duration_ms,
        total_started.elapsed().as_millis(),
    );
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
        &metrics,
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
        (
            "observability".to_string(),
            JsonValue::object(metrics.log_fields()),
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
    pool: &UpstreamSessionPool,
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
    let total_started = Instant::now();
    if let Err(error) = validate_upstream_tool_policy(server, tool_name, context) {
        let metrics = ToolAuditMetrics::for_single(
            arguments,
            None,
            context,
            0,
            0,
            total_started.elapsed().as_millis(),
        );
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
            None,
            &metrics,
            Some(&error),
        );
        return Err(error);
    }

    let queue_started = Instant::now();
    let lease = match acquire_upstream_lease(root_path, server, context, effective_timeout) {
        Ok(value) => value,
        Err(error) => {
            let metrics = ToolAuditMetrics::for_single(
                arguments,
                None,
                context,
                queue_started.elapsed().as_millis(),
                0,
                total_started.elapsed().as_millis(),
            );
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
                None,
                &metrics,
                Some(&error),
            );
            return Err(error);
        }
    };
    let queue_duration_ms = queue_started.elapsed().as_millis();
    let heartbeat_lost = lease.heartbeat_lost_flag();
    let pool_key = upstream_session_key(root_path, server, lease.affinity_key());
    let audit_pool_key = pool_key.clone();
    let upstream_started = Instant::now();
    let upstream_deadline = upstream_started + effective_timeout;
    let pooled_result = (|| -> Result<PooledToolCallResult, String> {
        let mut checkout = pool
            .checkout(UpstreamPoolInvocation {
                root_path,
                server,
                key: pool_key,
                timeout: upstream_deadline.saturating_duration_since(Instant::now()),
                lease_lost: heartbeat_lost,
            })
            .map_err(String::from)?;
        validate_upstream_tool_known_with_pool(
            root_path,
            server,
            tool_name,
            context,
            heartbeat_lost,
            upstream_deadline,
            &mut checkout,
        )?;
        let result = checkout
            .call_tool(
                server,
                tool_name,
                arguments,
                upstream_deadline,
                heartbeat_lost,
            )
            .map_err(String::from)?;
        let validation = validate_tool_call_result(&server.name, tool_name, &result);
        if validation.is_err() {
            checkout.invalidate();
        }
        let outcome = if validation.is_ok() {
            Some(checkout.outcome().map_err(String::from)?)
        } else {
            None
        };
        Ok(PooledToolCallResult {
            result,
            pool_outcome: outcome,
            validation,
        })
    })();
    let PooledToolCallResult {
        result,
        pool_outcome,
        validation,
    } = match pooled_result {
        Ok(value) => value,
        Err(error) => {
            let metrics = ToolAuditMetrics::for_single(
                arguments,
                None,
                context,
                queue_duration_ms,
                upstream_started.elapsed().as_millis(),
                total_started.elapsed().as_millis(),
            );
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
                &metrics,
                Some(&error),
            );
            return Err(error);
        }
    };
    let upstream_duration_ms = upstream_started.elapsed().as_millis();
    let upstream_is_error = match validation {
        Ok(value) => value,
        Err(error) => {
            let metrics = ToolAuditMetrics::for_single(
                arguments,
                Some(&result),
                context,
                queue_duration_ms,
                upstream_duration_ms,
                total_started.elapsed().as_millis(),
            );
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
                &metrics,
                Some(&error),
            );
            return Err(error);
        }
    };
    let pool_outcome = pool_outcome
        .ok_or_else(|| "validated pooled upstream call is missing its pool outcome".to_string())?;
    let lease_outcome = match finalize_upstream_lease(lease) {
        Ok(value) => value,
        Err(error) => {
            let metrics = ToolAuditMetrics::for_single(
                arguments,
                Some(&result),
                context,
                queue_duration_ms,
                upstream_duration_ms,
                total_started.elapsed().as_millis(),
            );
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
                &metrics,
                Some(&error),
            );
            return Err(error);
        }
    };
    let lease_id_for_audit = lease_outcome.lease_id.clone();
    let upstream_ok = !upstream_is_error;
    let metrics = ToolAuditMetrics::for_single(
        arguments,
        Some(&result),
        context,
        queue_duration_ms,
        upstream_duration_ms,
        total_started.elapsed().as_millis(),
    );
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
        &metrics,
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
        (
            "observability".to_string(),
            JsonValue::object(metrics.log_fields()),
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
    let total_started = Instant::now();
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
    if let Err(error) = validate_upstream_batch_tool_policy(server, calls, context) {
        let metrics = ToolAuditMetrics::for_batch(
            calls,
            None,
            context,
            0,
            0,
            total_started.elapsed().as_millis(),
        );
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
            &metrics,
            Some(&error),
        );
        return Err(error);
    }

    let queue_started = Instant::now();
    let lease = match acquire_upstream_lease(root_path, server, context, effective_timeout) {
        Ok(value) => value,
        Err(error) => {
            let metrics = ToolAuditMetrics::for_batch(
                calls,
                None,
                context,
                queue_started.elapsed().as_millis(),
                0,
                total_started.elapsed().as_millis(),
            );
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
                &metrics,
                Some(&error),
            );
            return Err(error);
        }
    };
    let queue_duration_ms = queue_started.elapsed().as_millis();
    let upstream_deadline = Instant::now() + effective_timeout;
    if let Err(error) =
        remaining_upstream_timeout(upstream_deadline, server_name, "tools/list verification")
            .and_then(|verification_timeout| {
                validate_upstream_batch_tools_known(
                    root_path,
                    server,
                    calls,
                    verification_timeout,
                    context,
                )
            })
    {
        let metrics = ToolAuditMetrics::for_batch(
            calls,
            None,
            context,
            queue_duration_ms,
            0,
            total_started.elapsed().as_millis(),
        );
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
            &metrics,
            Some(&error),
        );
        return Err(error);
    }
    let heartbeat_lost = lease.heartbeat_lost_flag();
    let upstream_started = Instant::now();
    let call_result =
        remaining_upstream_timeout(upstream_deadline, server_name, "tools/call batch").and_then(
            |call_timeout| {
                if server.source_type == "http" {
                    run_http_tool_calls(server, calls, call_timeout).map_err(String::from)
                } else {
                    run_stdio_tool_calls(root_path, server, calls, call_timeout, heartbeat_lost)
                        .map_err(String::from)
                }
            },
        );
    let upstream_duration_ms = upstream_started.elapsed().as_millis();
    let results = match call_result {
        Ok(value) => value,
        Err(error) => {
            let metrics = ToolAuditMetrics::for_batch(
                calls,
                None,
                context,
                queue_duration_ms,
                upstream_duration_ms,
                total_started.elapsed().as_millis(),
            );
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
                &metrics,
                Some(&error),
            );
            return Err(error);
        }
    };
    let lease_outcome = match finalize_upstream_lease(lease) {
        Ok(value) => value,
        Err(error) => {
            let metrics = ToolAuditMetrics::for_batch(
                calls,
                Some(&results),
                context,
                queue_duration_ms,
                upstream_duration_ms,
                total_started.elapsed().as_millis(),
            );
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
                &metrics,
                Some(&error),
            );
            return Err(error);
        }
    };
    let lease_id_for_audit = lease_outcome.lease_id.clone();
    let upstream_ok_count = results
        .iter()
        .filter(|item| json_helpers::bool_at_path(item, &["upstreamOk"]).unwrap_or(false))
        .count();
    let upstream_failed_count = results.len().saturating_sub(upstream_ok_count);
    let upstream_ok = upstream_failed_count == 0;
    let metrics = ToolAuditMetrics::for_batch(
        calls,
        Some(&results),
        context,
        queue_duration_ms,
        upstream_duration_ms,
        total_started.elapsed().as_millis(),
    );
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
        &metrics,
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
        (
            "observability".to_string(),
            JsonValue::object(metrics.log_fields()),
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
    pool: &UpstreamSessionPool,
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
    if server.source_type == "http" {
        return call_tools_with_context(root_path, server_name, calls, timeout_ms, context);
    }
    let total_started = Instant::now();
    if let Err(error) = validate_upstream_batch_tool_policy(server, calls, context) {
        let metrics = ToolAuditMetrics::for_batch(
            calls,
            None,
            context,
            0,
            0,
            total_started.elapsed().as_millis(),
        );
        log_tool_batch_audit(
            root_path,
            server_name,
            calls,
            context,
            false,
            true,
            None,
            None,
            0,
            calls.len(),
            &metrics,
            Some(&error),
        );
        return Err(error);
    }

    let queue_started = Instant::now();
    let lease = match acquire_upstream_lease(root_path, server, context, effective_timeout) {
        Ok(value) => value,
        Err(error) => {
            let metrics = ToolAuditMetrics::for_batch(
                calls,
                None,
                context,
                queue_started.elapsed().as_millis(),
                0,
                total_started.elapsed().as_millis(),
            );
            log_tool_batch_audit(
                root_path,
                server_name,
                calls,
                context,
                false,
                true,
                None,
                None,
                0,
                calls.len(),
                &metrics,
                Some(&error),
            );
            return Err(error);
        }
    };
    let queue_duration_ms = queue_started.elapsed().as_millis();
    let heartbeat_lost = lease.heartbeat_lost_flag();
    let pool_key = upstream_session_key(root_path, server, lease.affinity_key());
    let audit_pool_key = pool_key.clone();
    let upstream_started = Instant::now();
    let upstream_deadline = upstream_started + effective_timeout;
    let pooled_result = (|| -> Result<(Vec<JsonValue>, UpstreamPoolCallOutcome), String> {
        let mut checkout = pool
            .checkout(UpstreamPoolInvocation {
                root_path,
                server,
                key: pool_key,
                timeout: upstream_deadline.saturating_duration_since(Instant::now()),
                lease_lost: heartbeat_lost,
            })
            .map_err(String::from)?;
        validate_upstream_batch_tools_known_with_pool(
            root_path,
            server,
            calls,
            context,
            heartbeat_lost,
            upstream_deadline,
            &mut checkout,
        )?;
        let results = checkout
            .call_tools(server, calls, upstream_deadline, heartbeat_lost)
            .map_err(String::from)?;
        let outcome = checkout.outcome().map_err(String::from)?;
        Ok((results, outcome))
    })();
    let (results, pool_outcome) = match pooled_result {
        Ok(value) => value,
        Err(error) => {
            let metrics = ToolAuditMetrics::for_batch(
                calls,
                None,
                context,
                queue_duration_ms,
                upstream_started.elapsed().as_millis(),
                total_started.elapsed().as_millis(),
            );
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
                &metrics,
                Some(&error),
            );
            return Err(error);
        }
    };
    let upstream_duration_ms = upstream_started.elapsed().as_millis();
    let lease_outcome = match finalize_upstream_lease(lease) {
        Ok(value) => value,
        Err(error) => {
            let metrics = ToolAuditMetrics::for_batch(
                calls,
                Some(&results),
                context,
                queue_duration_ms,
                upstream_duration_ms,
                total_started.elapsed().as_millis(),
            );
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
                &metrics,
                Some(&error),
            );
            return Err(error);
        }
    };
    let lease_id_for_audit = lease_outcome.lease_id.clone();
    let upstream_ok_count = results
        .iter()
        .filter(|item| json_helpers::bool_at_path(item, &["upstreamOk"]).unwrap_or(false))
        .count();
    let upstream_failed_count = results.len().saturating_sub(upstream_ok_count);
    let upstream_ok = upstream_failed_count == 0;
    let metrics = ToolAuditMetrics::for_batch(
        calls,
        Some(&results),
        context,
        queue_duration_ms,
        upstream_duration_ms,
        total_started.elapsed().as_millis(),
    );
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
        &metrics,
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
        (
            "observability".to_string(),
            JsonValue::object(metrics.log_fields()),
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

fn validate_upstream_tool_known_with_pool(
    root_path: &Path,
    server: &UpstreamServerConfig,
    tool_name: &str,
    context: Option<&UpstreamLeaseContext>,
    lease_lost: Option<&AtomicBool>,
    deadline: Instant,
    checkout: &mut UpstreamSessionCheckout<'_>,
) -> Result<(), String> {
    let tools = verified_tool_names_for_call_with_pool(
        root_path, server, context, lease_lost, deadline, checkout,
    )?;
    ensure_tool_name_in_verified_set(server, tool_name, &tools)
}

fn validate_upstream_batch_tools_known_with_pool(
    root_path: &Path,
    server: &UpstreamServerConfig,
    calls: &[UpstreamToolCall],
    context: Option<&UpstreamLeaseContext>,
    lease_lost: Option<&AtomicBool>,
    deadline: Instant,
    checkout: &mut UpstreamSessionCheckout<'_>,
) -> Result<(), String> {
    if calls.is_empty() {
        return Ok(());
    }
    let tools = verified_tool_names_for_call_with_pool(
        root_path, server, context, lease_lost, deadline, checkout,
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
    context: Option<&UpstreamLeaseContext>,
    lease_lost: Option<&AtomicBool>,
    deadline: Instant,
    checkout: &mut UpstreamSessionCheckout<'_>,
) -> Result<BTreeSet<String>, String> {
    if !known_tool_validation_required(context) {
        return Ok(BTreeSet::new());
    }
    if let Some(tools) = super::tool_cache::read_cached_tools(
        &super::tool_cache::tool_list_cache_key(root_path, server),
    ) {
        return tool_names_from_tools_list(server, &tools, " from cache");
    }
    let result = checkout
        .list_tools(server, deadline, lease_lost)
        .map_err(String::from)?;
    let tools = super::tool_cache::validate_tools_list_result(server, &result)
        .map_err(|error| error.to_string())?;
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

#[derive(Clone, Debug, Default)]
struct ToolAuditMetrics {
    queue_duration_ms: u128,
    upstream_duration_ms: u128,
    total_duration_ms: u128,
    request_bytes: usize,
    response_bytes: usize,
    estimated_input_tokens: usize,
    estimated_output_tokens: usize,
    reported_input_tokens: Option<u128>,
    reported_output_tokens: Option<u128>,
    reported_total_tokens: Option<u128>,
    token_usage_source: Option<String>,
}

#[derive(Clone, Debug, Default)]
struct ReportedTokenUsage {
    input: Option<u128>,
    output: Option<u128>,
    total: Option<u128>,
    source: Option<String>,
}

impl ToolAuditMetrics {
    fn for_single(
        arguments: &JsonValue,
        result: Option<&JsonValue>,
        context: Option<&UpstreamLeaseContext>,
        queue_duration_ms: u128,
        upstream_duration_ms: u128,
        total_duration_ms: u128,
    ) -> Self {
        let request_bytes = arguments.to_compact_string().len();
        let response_bytes = result
            .map(JsonValue::to_compact_string)
            .map(|value| value.len())
            .unwrap_or(0);
        let usage = reported_token_usage(result, context);
        Self {
            queue_duration_ms,
            upstream_duration_ms,
            total_duration_ms,
            request_bytes,
            response_bytes,
            estimated_input_tokens: estimate_payload_tokens(request_bytes),
            estimated_output_tokens: estimate_payload_tokens(response_bytes),
            reported_input_tokens: usage.input,
            reported_output_tokens: usage.output,
            reported_total_tokens: usage.total,
            token_usage_source: usage.source,
        }
    }

    fn for_batch(
        calls: &[UpstreamToolCall],
        results: Option<&[JsonValue]>,
        context: Option<&UpstreamLeaseContext>,
        queue_duration_ms: u128,
        upstream_duration_ms: u128,
        total_duration_ms: u128,
    ) -> Self {
        let request_bytes = calls
            .iter()
            .map(|call| call.tool.len() + call.arguments.to_compact_string().len())
            .sum();
        let response_bytes = results
            .map(|items| {
                items
                    .iter()
                    .map(JsonValue::to_compact_string)
                    .map(|value| value.len())
                    .sum()
            })
            .unwrap_or(0);
        let usage = reported_token_usage_for_results(results.unwrap_or(&[]), context);
        Self {
            queue_duration_ms,
            upstream_duration_ms,
            total_duration_ms,
            request_bytes,
            response_bytes,
            estimated_input_tokens: estimate_payload_tokens(request_bytes),
            estimated_output_tokens: estimate_payload_tokens(response_bytes),
            reported_input_tokens: usage.input,
            reported_output_tokens: usage.output,
            reported_total_tokens: usage.total,
            token_usage_source: usage.source,
        }
    }

    fn log_fields(&self) -> Vec<(&'static str, JsonValue)> {
        vec![
            (
                "metricsSchema",
                JsonValue::string("mcpace.toolAuditMetrics.v1"),
            ),
            ("queueDurationMs", JsonValue::number(self.queue_duration_ms)),
            (
                "upstreamDurationMs",
                JsonValue::number(self.upstream_duration_ms),
            ),
            ("totalDurationMs", JsonValue::number(self.total_duration_ms)),
            ("requestBytes", JsonValue::number(self.request_bytes)),
            ("responseBytes", JsonValue::number(self.response_bytes)),
            (
                "estimatedInputTokens",
                JsonValue::number(self.estimated_input_tokens),
            ),
            (
                "estimatedOutputTokens",
                JsonValue::number(self.estimated_output_tokens),
            ),
            (
                "estimatedTotalTokens",
                JsonValue::number(
                    self.estimated_input_tokens
                        .saturating_add(self.estimated_output_tokens),
                ),
            ),
            ("tokenEstimateMethod", JsonValue::string("utf8-bytes-div-4")),
            (
                "reportedInputTokens",
                optional_json_u128(self.reported_input_tokens),
            ),
            (
                "reportedOutputTokens",
                optional_json_u128(self.reported_output_tokens),
            ),
            (
                "reportedTotalTokens",
                optional_json_u128(self.reported_total_tokens),
            ),
            (
                "tokenUsageSource",
                optional_json_string(self.token_usage_source.clone()),
            ),
        ]
    }
}

fn estimate_payload_tokens(bytes: usize) -> usize {
    if bytes == 0 {
        0
    } else {
        bytes.saturating_add(3) / 4
    }
}

fn optional_json_u128(value: Option<u128>) -> JsonValue {
    value.map(JsonValue::number).unwrap_or(JsonValue::Null)
}

fn reported_token_usage(
    result: Option<&JsonValue>,
    context: Option<&UpstreamLeaseContext>,
) -> ReportedTokenUsage {
    let result_usage = result
        .and_then(extract_reported_token_usage)
        .map(|mut usage| {
            usage.source = Some("upstream_result".to_string());
            usage
        })
        .unwrap_or_default();
    let context_usage = context
        .and_then(|value| value.metadata.as_ref())
        .and_then(extract_reported_token_usage)
        .map(|mut usage| {
            usage.source = Some("request_context".to_string());
            usage
        })
        .unwrap_or_default();
    merge_reported_token_usage(result_usage, context_usage)
}

fn reported_token_usage_for_results(
    results: &[JsonValue],
    context: Option<&UpstreamLeaseContext>,
) -> ReportedTokenUsage {
    let mut aggregate = ReportedTokenUsage::default();
    for result in results {
        let usage = reported_token_usage(Some(result), None);
        aggregate = sum_reported_token_usage(aggregate, usage);
    }
    let context_usage = reported_token_usage(None, context);
    merge_reported_token_usage(aggregate, context_usage)
}

fn extract_reported_token_usage(value: &JsonValue) -> Option<ReportedTokenUsage> {
    let candidates = [
        json_at_path(value, &["_meta", "usage"]),
        json_at_path(value, &["usage"]),
        json_at_path(value, &["metadata", "usage"]),
        json_at_path(value, &["upstreamResult", "_meta", "usage"]),
        json_at_path(value, &["upstreamResult", "usage"]),
        json_at_path(value, &["result", "_meta", "usage"]),
        json_at_path(value, &["result", "usage"]),
        json_at_path(value, &["_meta"]),
        json_at_path(value, &["metadata"]),
        Some(value),
    ];
    for candidate in candidates.into_iter().flatten() {
        let input = token_count(
            candidate,
            &[
                "inputTokens",
                "input_tokens",
                "promptTokens",
                "prompt_tokens",
            ],
        );
        let output = token_count(
            candidate,
            &[
                "outputTokens",
                "output_tokens",
                "completionTokens",
                "completion_tokens",
            ],
        );
        let explicit_total = token_count(candidate, &["totalTokens", "total_tokens"]);
        let total = explicit_total.or_else(|| match (input, output) {
            (Some(left), Some(right)) => Some(left.saturating_add(right)),
            _ => None,
        });
        if input.is_some() || output.is_some() || total.is_some() {
            return Some(ReportedTokenUsage {
                input,
                output,
                total,
                source: None,
            });
        }
    }
    None
}

fn json_at_path<'a>(value: &'a JsonValue, path: &[&str]) -> Option<&'a JsonValue> {
    let mut current = value;
    for key in path {
        current = current.get(key)?;
    }
    Some(current)
}

fn token_count(value: &JsonValue, keys: &[&str]) -> Option<u128> {
    keys.iter()
        .find_map(|key| value.get(key).and_then(json_u128))
}

fn json_u128(value: &JsonValue) -> Option<u128> {
    match value {
        JsonValue::Number(number) | JsonValue::String(number) => number.parse::<u128>().ok(),
        _ => None,
    }
}

fn merge_reported_token_usage(
    primary: ReportedTokenUsage,
    fallback: ReportedTokenUsage,
) -> ReportedTokenUsage {
    let used_primary =
        primary.input.is_some() || primary.output.is_some() || primary.total.is_some();
    let used_fallback = (!used_primary)
        || (primary.input.is_none() && fallback.input.is_some())
        || (primary.output.is_none() && fallback.output.is_some())
        || (primary.total.is_none() && fallback.total.is_some());
    ReportedTokenUsage {
        input: primary.input.or(fallback.input),
        output: primary.output.or(fallback.output),
        total: primary.total.or(fallback.total),
        source: match (used_primary, used_fallback) {
            (true, true) => Some("mixed".to_string()),
            (true, false) => primary.source,
            (false, true) => fallback.source,
            (false, false) => None,
        },
    }
}

fn sum_reported_token_usage(
    left: ReportedTokenUsage,
    right: ReportedTokenUsage,
) -> ReportedTokenUsage {
    let add = |first: Option<u128>, second: Option<u128>| match (first, second) {
        (Some(a), Some(b)) => Some(a.saturating_add(b)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    };
    let has_left = left.input.is_some() || left.output.is_some() || left.total.is_some();
    let has_right = right.input.is_some() || right.output.is_some() || right.total.is_some();
    ReportedTokenUsage {
        input: add(left.input, right.input),
        output: add(left.output, right.output),
        total: add(left.total, right.total),
        source: match (has_left, has_right) {
            (true, true) => Some("upstream_results".to_string()),
            (true, false) => left.source,
            (false, true) => right.source,
            (false, false) => None,
        },
    }
}

#[derive(Clone, Copy, Debug)]
struct ToolAuditOutcome {
    outcome: &'static str,
    error_kind: &'static str,
    failure_stage: &'static str,
}

fn next_tool_audit_id() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    let sequence = TOOL_AUDIT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("tc-{timestamp:x}-{sequence:x}")
}

fn classify_tool_audit_outcome(
    bridge_ok: bool,
    upstream_ok: bool,
    error: Option<&str>,
) -> ToolAuditOutcome {
    if bridge_ok && upstream_ok {
        return ToolAuditOutcome {
            outcome: "success",
            error_kind: "none",
            failure_stage: "complete",
        };
    }
    if bridge_ok {
        return ToolAuditOutcome {
            outcome: "tool_error",
            error_kind: "upstream_tool_error",
            failure_stage: "upstream",
        };
    }

    let message = error.unwrap_or_default().to_ascii_lowercase();
    if message.contains("policy")
        || message.contains("risk")
        || message.contains("allowunknown")
        || message.contains("not allowed")
        || message.contains("blocked")
        || message.contains("denied")
    {
        return ToolAuditOutcome {
            outcome: "denied",
            error_kind: "policy_denied",
            failure_stage: "policy",
        };
    }
    if message.contains("auth")
        || message.contains("credential")
        || message.contains("access token")
        || message.contains("api token")
        || message.contains("bearer token")
        || message.contains("missing token")
        || message.contains("invalid token")
        || message.contains("token expired")
        || message.contains("unauthorized")
        || message.contains("forbidden")
    {
        return ToolAuditOutcome {
            outcome: "denied",
            error_kind: "authorization",
            failure_stage: "authorization",
        };
    }
    if message.contains("timeout") || message.contains("timed out") {
        return ToolAuditOutcome {
            outcome: "timeout",
            error_kind: "timeout",
            failure_stage: if message.contains("queue") || message.contains("lease") {
                "queue"
            } else {
                "upstream"
            },
        };
    }
    if message.contains("queue")
        || message.contains("lease")
        || message.contains("capacity")
        || message.contains("busy")
    {
        return ToolAuditOutcome {
            outcome: "rejected",
            error_kind: "capacity",
            failure_stage: "queue",
        };
    }
    if message.contains("unknown tool")
        || message.contains("schema")
        || message.contains("argument")
        || message.contains("invalid")
        || message.contains("duplicate")
    {
        return ToolAuditOutcome {
            outcome: "invalid",
            error_kind: "validation",
            failure_stage: "validation",
        };
    }
    if message.contains("spawn")
        || message.contains("stdio")
        || message.contains("http")
        || message.contains("connect")
        || message.contains("transport")
        || message.contains("status")
        || message.contains("process")
    {
        return ToolAuditOutcome {
            outcome: "transport_error",
            error_kind: "transport",
            failure_stage: "upstream",
        };
    }
    ToolAuditOutcome {
        outcome: "bridge_error",
        error_kind: "internal",
        failure_stage: "bridge",
    }
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
    metrics: &ToolAuditMetrics,
    error: Option<&str>,
) {
    let upstream_is_error = bridge_ok && !upstream_ok;
    let level = if bridge_ok && upstream_ok {
        "info"
    } else {
        "warn"
    };
    let trace = upstream_session_trace(server_name, context, pool_key);
    let audit = classify_tool_audit_outcome(bridge_ok, upstream_ok, error);
    let mut fields = vec![
        ("auditSchema", JsonValue::string("mcpace.toolAudit.v2")),
        ("callId", JsonValue::string(next_tool_audit_id())),
        ("requestKind", JsonValue::string("tools/call")),
        ("callCount", JsonValue::number(1)),
        ("outcome", JsonValue::string(audit.outcome)),
        ("errorKind", JsonValue::string(audit.error_kind)),
        ("failureStage", JsonValue::string(audit.failure_stage)),
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
    ];
    fields.extend(metrics.log_fields());
    let _ = crate::hub::runtime::append_log(root_path, level, "tool_call_audit", &fields);
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
    metrics: &ToolAuditMetrics,
    error: Option<&str>,
) {
    let upstream_ok = bridge_ok && upstream_failed_count == 0;
    let level = if bridge_ok && upstream_ok {
        "info"
    } else {
        "warn"
    };
    let trace = upstream_session_trace(server_name, context, pool_key);
    let audit = classify_tool_audit_outcome(bridge_ok, upstream_ok, error);
    let tools = calls
        .iter()
        .map(|call| JsonValue::string(call.tool.clone()));
    let fingerprints = calls
        .iter()
        .map(|call| JsonValue::string(argument_fingerprint(&call.arguments)));
    let mut fields = vec![
        ("auditSchema", JsonValue::string("mcpace.toolAudit.v2")),
        ("callId", JsonValue::string(next_tool_audit_id())),
        ("requestKind", JsonValue::string("tools/call.batch")),
        ("outcome", JsonValue::string(audit.outcome)),
        ("errorKind", JsonValue::string(audit.error_kind)),
        ("failureStage", JsonValue::string(audit.failure_stage)),
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
    ];
    fields.extend(metrics.log_fields());
    let _ = crate::hub::runtime::append_log(root_path, level, "tool_batch_audit", &fields);
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
                key.server_name,
                key.execution_mode,
                key.affinity_fingerprint,
                key.server_fingerprint
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
    server: &UpstreamServerConfig,
    context: Option<&UpstreamLeaseContext>,
    timeout: Duration,
) -> Result<UpstreamLeaseAttachment, String> {
    if server.execution.is_disabled() {
        return Err(format!(
            "upstream server '{}' is disabled by execution policy",
            server.name
        ));
    }

    let affinity = execution_affinity_key(server, context)?;
    let effective_ttl_ms = context
        .and_then(|value| value.ttl_ms)
        .filter(|value| *value > 0)
        .unwrap_or_else(|| timeout.as_millis().saturating_add(5_000));
    let request = RuntimeLeaseRequest {
        server_name: server.name.clone(),
        client_id: Some(
            context_string(context.and_then(|value| value.client_id.as_ref()))
                .unwrap_or_else(|| "mcpace-upstream-bridge".to_string()),
        ),
        session_id: context_string(context.and_then(|value| value.session_id.as_ref())),
        project_root: context_string(context.and_then(|value| value.project_root.as_ref()))
            .or_else(|| Some(child_process_path(root_path))),
        transport: Some(
            context_string(context.and_then(|value| value.transport.as_ref()))
                .unwrap_or_else(|| server.source_type.clone()),
        ),
        metadata_json: context
            .and_then(|value| value.metadata.as_ref())
            .map(JsonValue::to_compact_string),
        ttl_ms: Some(effective_ttl_ms),
        takeover: false,
    };

    let queue_timeout_ms = server.execution.queue_timeout_ms;
    if queue_timeout_ms == 0 {
        return acquire_upstream_lease_once(
            root_path,
            server,
            request,
            effective_ttl_ms,
            LeaseQueueMetrics {
                attempts: 1,
                timeout_ms: 0,
                ..LeaseQueueMetrics::default()
            },
            affinity,
        );
    }

    let queued_at = Instant::now();
    let deadline = queued_at + Duration::from_millis(queue_timeout_ms);
    let lane_key = upstream_lease_lane_key(root_path, server, &affinity);
    let ticket = lease_queue::enqueue(lane_key, server.execution.max_queue_depth)
        .map_err(|error| error.to_string())?;
    let position = ticket.position().clone();
    ticket
        .wait_until_head(deadline)
        .map_err(|error| error.to_string())?;

    let mut attempts = 0usize;
    let blocked = loop {
        attempts = attempts.saturating_add(1);
        match leases::acquire_runtime_lease(root_path, request.clone())? {
            RuntimeLeaseAcquireResult::Acquired { lease_id, json } => {
                let lease = json_helpers::value_at_path(&json, &["lease"])
                    .cloned()
                    .unwrap_or_else(|| json.clone());
                let queue = LeaseQueueMetrics {
                    attempts,
                    wait_ms: queued_at.elapsed().as_millis(),
                    timeout_ms: queue_timeout_ms,
                    ticket: Some(position.ticket),
                    depth_at_enqueue: position.depth_at_enqueue,
                    ahead_at_enqueue: position.ahead_at_enqueue,
                };
                ticket.complete();
                let heartbeat = Some(start_lease_heartbeat(
                    root_path,
                    &lease_id,
                    effective_ttl_ms,
                ));
                return Ok(UpstreamLeaseAttachment::Attached(UpstreamLeaseGuard {
                    root_path: root_path.to_path_buf(),
                    lease_id,
                    lease,
                    released: false,
                    heartbeat,
                    queue,
                    affinity,
                }));
            }
            RuntimeLeaseAcquireResult::Blocked { json } => {
                if Instant::now() >= deadline {
                    break json;
                }
            }
        }

        ticket
            .wait_for_retry(Duration::from_millis(50), deadline)
            .map_err(|error| error.to_string())?;
    };

    let waited_ms = queued_at.elapsed().as_millis();
    let reason = json_helpers::string_at_path(&blocked, &["reason"])
        .unwrap_or("runtime lease acquisition remained blocked");
    Err(format!(
        "upstream lease queue timed out for server '{}' after {} ms and {} attempt(s): {} | {}",
        server.name,
        waited_ms,
        attempts,
        reason,
        blocked.to_compact_string()
    ))
}

fn acquire_upstream_lease_once(
    root_path: &Path,
    server: &UpstreamServerConfig,
    request: RuntimeLeaseRequest,
    effective_ttl_ms: u128,
    queue: LeaseQueueMetrics,
    affinity: ExecutionAffinityKey,
) -> Result<UpstreamLeaseAttachment, String> {
    match leases::acquire_runtime_lease(root_path, request)? {
        RuntimeLeaseAcquireResult::Acquired { lease_id, json } => {
            let lease = json_helpers::value_at_path(&json, &["lease"])
                .cloned()
                .unwrap_or_else(|| json.clone());
            let heartbeat = Some(start_lease_heartbeat(
                root_path,
                &lease_id,
                effective_ttl_ms,
            ));
            Ok(UpstreamLeaseAttachment::Attached(UpstreamLeaseGuard {
                root_path: root_path.to_path_buf(),
                lease_id,
                lease,
                released: false,
                heartbeat,
                queue,
                affinity,
            }))
        }
        RuntimeLeaseAcquireResult::Blocked { json } => Err(runtime_lease_blocked_error(
            &server.name,
            json_helpers::string_at_path(&json, &["reason"])
                .unwrap_or("runtime lease acquisition was blocked"),
            &json,
        )),
    }
}

fn execution_affinity_key(
    server: &UpstreamServerConfig,
    context: Option<&UpstreamLeaseContext>,
) -> Result<ExecutionAffinityKey, String> {
    server
        .execution
        .affinity_key(&ExecutionAffinityContext {
            client_id: context.and_then(|value| value.client_id.clone()),
            session_id: context.and_then(|value| value.session_id.clone()),
            project_root: context.and_then(|value| value.project_root.clone()),
            transport: context
                .and_then(|value| value.transport.clone())
                .or_else(|| Some(server.source_type.clone())),
            metadata: context.and_then(|value| value.metadata.clone()),
        })
        .map_err(String::from)
}

fn upstream_lease_lane_key(
    root_path: &Path,
    server: &UpstreamServerConfig,
    affinity: &ExecutionAffinityKey,
) -> String {
    format!(
        "root={}|server={}|affinity={}",
        short_hash(&cache_root_path(root_path)),
        short_hash(&server_fingerprint(server)),
        short_hash(&affinity.fingerprint)
    )
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
            let queue = guard.queue.clone();
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
                queue,
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
        (
            "leaseQueueAttempts".to_string(),
            JsonValue::number(outcome.queue.attempts),
        ),
        (
            "leaseQueueWaitMs".to_string(),
            JsonValue::number(outcome.queue.wait_ms),
        ),
        (
            "leaseQueueTimeoutMs".to_string(),
            JsonValue::number(outcome.queue.timeout_ms),
        ),
        (
            "leaseQueueTicket".to_string(),
            outcome
                .queue
                .ticket
                .map(JsonValue::number)
                .unwrap_or(JsonValue::Null),
        ),
        (
            "leaseQueueDepthAtEnqueue".to_string(),
            JsonValue::number(outcome.queue.depth_at_enqueue),
        ),
        (
            "leaseQueueAheadAtEnqueue".to_string(),
            JsonValue::number(outcome.queue.ahead_at_enqueue),
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
    affinity: &ExecutionAffinityKey,
) -> UpstreamSessionKey {
    let (settings_modified_ms, settings_len) = mcp_sources::mcp_settings_fingerprint(root_path);
    UpstreamSessionKey {
        root_path: cache_root_path(root_path),
        server_name: server.name.clone(),
        settings_modified_ms,
        settings_len,
        server_fingerprint: server_fingerprint(server),
        client_id: affinity.client_id.clone(),
        session_id: affinity.session_id.clone(),
        project_root: affinity.project_root.clone(),
        transport: affinity.transport.clone(),
        execution_mode: server.execution.mode.as_str().to_string(),
        affinity_fingerprint: affinity.fingerprint.clone(),
    }
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
