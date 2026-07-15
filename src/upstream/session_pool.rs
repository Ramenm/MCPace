use super::stdio_runtime::{
    initialize_running_server, read_response, spawn_stdio_server, write_jsonrpc, RunningServer,
    StdioRuntimeError,
};
use super::{
    batch_tool_call_error, max_pooled_upstream_sessions, validate_tool_call_result,
    ToolListPagination, UpstreamServerConfig, UpstreamToolCall, INITIALIZE_ID, METHOD_ID,
    UPSTREAM_SESSION_IDLE_TTL,
};
use crate::json::JsonValue;
use crate::resources;
use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, TryLockError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Debug)]
pub(super) enum UpstreamSessionPoolError {
    Stdio(StdioRuntimeError),
    State(String),
}

type UpstreamSessionPoolResult<T> = Result<T, UpstreamSessionPoolError>;

impl UpstreamSessionPoolError {
    fn state(message: impl Into<String>) -> Self {
        Self::State(message.into())
    }
}

impl fmt::Display for UpstreamSessionPoolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stdio(error) => write!(formatter, "{}", error),
            Self::State(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for UpstreamSessionPoolError {}

impl From<StdioRuntimeError> for UpstreamSessionPoolError {
    fn from(error: StdioRuntimeError) -> Self {
        Self::Stdio(error)
    }
}

impl From<String> for UpstreamSessionPoolError {
    fn from(message: String) -> Self {
        Self::State(message)
    }
}

impl From<&str> for UpstreamSessionPoolError {
    fn from(message: &str) -> Self {
        Self::State(message.to_string())
    }
}

impl From<UpstreamSessionPoolError> for String {
    fn from(error: UpstreamSessionPoolError) -> Self {
        error.to_string()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(super) struct UpstreamSessionKey {
    pub(super) root_path: String,
    pub(super) server_name: String,
    pub(super) settings_modified_ms: u128,
    pub(super) settings_len: u64,
    pub(super) server_fingerprint: String,
    pub(super) client_id: String,
    pub(super) session_id: String,
    pub(super) project_root: String,
    pub(super) transport: String,
    pub(super) execution_mode: String,
    pub(super) affinity_fingerprint: String,
}

struct PooledUpstreamSession {
    running: RunningServer,
    created_at: Instant,
    last_used: Instant,
    next_request_id: i64,
    call_count: usize,
}

struct PooledUpstreamWorker {
    session: Mutex<Option<PooledUpstreamSession>>,
    busy: AtomicBool,
    initialized: AtomicBool,
    last_used_ms: AtomicU64,
    idle_ttl_ms: AtomicU64,
}

#[derive(Default)]
struct UpstreamSessionPoolState {
    groups: BTreeMap<UpstreamSessionKey, Vec<Arc<PooledUpstreamWorker>>>,
}

#[derive(Clone, Debug)]
pub(super) struct UpstreamPoolCallOutcome {
    pub(super) enabled: bool,
    pub(super) hit: bool,
    pub(super) session_call_count: usize,
    pub(super) session_age_ms: u128,
    pub(super) pool_size: usize,
    pub(super) max_pool_size: usize,
    pub(super) idle_ttl_ms: u128,
    pub(super) evicted_idle_count: usize,
    pub(super) evicted_capacity_count: usize,
}

#[derive(Clone, Debug)]
pub struct UpstreamSessionSnapshot {
    pub server_name: String,
    pub pid: u32,
    pub client_id: String,
    pub session_id: String,
    pub project_root: String,
    pub transport: String,
    pub created_age_ms: u128,
    pub idle_ms: u128,
    pub call_count: usize,
    pub resource: JsonValue,
}

impl UpstreamSessionSnapshot {
    pub fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("server", JsonValue::string(self.server_name.clone())),
            ("pid", JsonValue::number(self.pid)),
            ("clientId", JsonValue::string(self.client_id.clone())),
            ("sessionId", JsonValue::string(self.session_id.clone())),
            ("projectRoot", JsonValue::string(self.project_root.clone())),
            ("transport", JsonValue::string(self.transport.clone())),
            ("ageMs", JsonValue::number(self.created_age_ms)),
            ("idleMs", JsonValue::number(self.idle_ms)),
            ("callCount", JsonValue::number(self.call_count)),
            ("resource", self.resource.clone()),
        ])
    }
}

pub(super) struct UpstreamPoolInvocation<'a> {
    pub(super) root_path: &'a Path,
    pub(super) server: &'a UpstreamServerConfig,
    pub(super) key: UpstreamSessionKey,
    pub(super) timeout: Duration,
    pub(super) lease_lost: Option<&'a AtomicBool>,
}

pub(super) struct UpstreamSessionCheckout<'a> {
    pool: &'a UpstreamSessionPool,
    key: UpstreamSessionKey,
    worker: Arc<PooledUpstreamWorker>,
    hit: bool,
    evicted_idle_count: usize,
    evicted_capacity_count: usize,
    failed: bool,
}

struct WorkerInitializationGuard<'a> {
    pool: &'a UpstreamSessionPool,
    key: &'a UpstreamSessionKey,
    worker: Arc<PooledUpstreamWorker>,
    armed: bool,
}

pub struct UpstreamSessionPool {
    state: Mutex<UpstreamSessionPoolState>,
    available: Condvar,
    max_sessions: usize,
}

impl Default for UpstreamSessionPool {
    fn default() -> Self {
        Self::with_max_sessions(max_pooled_upstream_sessions())
    }
}

impl PooledUpstreamWorker {
    fn reserved(idle_ttl_ms: u64) -> Self {
        Self {
            session: Mutex::new(None),
            busy: AtomicBool::new(true),
            initialized: AtomicBool::new(false),
            last_used_ms: AtomicU64::new(now_epoch_ms()),
            idle_ttl_ms: AtomicU64::new(idle_ttl_ms.max(1)),
        }
    }
}

impl PooledUpstreamSession {
    fn new(
        root_path: &Path,
        server: &UpstreamServerConfig,
        timeout: Duration,
        lease_lost: Option<&AtomicBool>,
    ) -> UpstreamSessionPoolResult<Self> {
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

    fn list_tools(
        &mut self,
        server: &UpstreamServerConfig,
        deadline: Instant,
        lease_lost: Option<&AtomicBool>,
    ) -> UpstreamSessionPoolResult<JsonValue> {
        let mut pagination = ToolListPagination::new();
        let mut cursor: Option<String> = None;
        loop {
            let request_id = self.next_request_id();
            let mut entries = vec![
                ("jsonrpc", JsonValue::string("2.0")),
                ("id", JsonValue::number(request_id)),
                ("method", JsonValue::string("tools/list")),
            ];
            if let Some(cursor) = cursor.as_ref() {
                entries.push((
                    "params",
                    JsonValue::object([("cursor", JsonValue::string(cursor.clone()))]),
                ));
            }
            write_jsonrpc(
                &self.running,
                JsonValue::object(entries),
                deadline,
                &server.name,
                "tools/list",
                lease_lost,
            )?;
            let result = read_response(
                &self.running.stdout_rx,
                &self.running.stderr_rx,
                request_id,
                deadline,
                &server.name,
                "tools/list",
                lease_lost,
            )?;
            cursor = pagination.add_page(&server.name, &result)?;
            self.last_used = Instant::now();
            if cursor.is_none() {
                return Ok(pagination.finish());
            }
        }
    }

    fn call_tool(
        &mut self,
        server: &UpstreamServerConfig,
        tool_name: &str,
        arguments: &JsonValue,
        deadline: Instant,
        lease_lost: Option<&AtomicBool>,
    ) -> UpstreamSessionPoolResult<JsonValue> {
        let request_id = self.next_request_id();
        write_jsonrpc(
            &self.running,
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
            deadline,
            &server.name,
            "tools/call",
            lease_lost,
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
    ) -> UpstreamSessionPoolResult<Vec<JsonValue>> {
        let mut results = Vec::new();
        for (index, call) in calls.iter().enumerate() {
            let request_id = self.next_request_id();
            let call_result = (|| {
                write_jsonrpc(
                    &self.running,
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
                    deadline,
                    &server.name,
                    "tools/call",
                    lease_lost,
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
                    validate_tool_call_result(&server.name, &call.tool, &result)?;
                Ok::<_, UpstreamSessionPoolError>((result, upstream_is_error))
            })();
            let (result, upstream_is_error) = call_result.map_err(|error| {
                UpstreamSessionPoolError::state(batch_tool_call_error(
                    &server.name,
                    index,
                    calls.len(),
                    error,
                ))
            })?;
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

impl UpstreamSessionCheckout<'_> {
    fn lock_session(
        &mut self,
    ) -> UpstreamSessionPoolResult<MutexGuard<'_, Option<PooledUpstreamSession>>> {
        match self.worker.session.lock() {
            Ok(guard) => Ok(guard),
            Err(poisoned) => {
                drop(poisoned.into_inner());
                self.failed = true;
                self.worker.initialized.store(false, Ordering::Release);
                Err(UpstreamSessionPoolError::state(
                    "checked-out upstream worker session state was poisoned",
                ))
            }
        }
    }

    pub(super) fn list_tools(
        &mut self,
        server: &UpstreamServerConfig,
        deadline: Instant,
        lease_lost: Option<&AtomicBool>,
    ) -> UpstreamSessionPoolResult<JsonValue> {
        let result = {
            let mut guard = self.lock_session()?;
            let session = guard.as_mut().ok_or_else(|| {
                UpstreamSessionPoolError::state("checked-out upstream worker is not initialized")
            })?;
            session.list_tools(server, deadline, lease_lost)
        };
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    pub(super) fn call_tool(
        &mut self,
        server: &UpstreamServerConfig,
        tool_name: &str,
        arguments: &JsonValue,
        deadline: Instant,
        lease_lost: Option<&AtomicBool>,
    ) -> UpstreamSessionPoolResult<JsonValue> {
        let result = {
            let mut guard = self.lock_session()?;
            let session = guard.as_mut().ok_or_else(|| {
                UpstreamSessionPoolError::state("checked-out upstream worker is not initialized")
            })?;
            session.call_tool(server, tool_name, arguments, deadline, lease_lost)
        };
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    pub(super) fn call_tools(
        &mut self,
        server: &UpstreamServerConfig,
        calls: &[UpstreamToolCall],
        deadline: Instant,
        lease_lost: Option<&AtomicBool>,
    ) -> UpstreamSessionPoolResult<Vec<JsonValue>> {
        let result = {
            let mut guard = self.lock_session()?;
            let session = guard.as_mut().ok_or_else(|| {
                UpstreamSessionPoolError::state("checked-out upstream worker is not initialized")
            })?;
            session.call_tools(server, calls, deadline, lease_lost)
        };
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    pub(super) fn invalidate(&mut self) {
        self.failed = true;
    }

    pub(super) fn outcome(&mut self) -> UpstreamSessionPoolResult<UpstreamPoolCallOutcome> {
        let hit = self.hit;
        let pool_size = self.pool.session_count();
        let max_pool_size = self.pool.max_session_count();
        let idle_ttl_ms = self.worker.idle_ttl_ms.load(Ordering::Acquire) as u128;
        let evicted_idle_count = self.evicted_idle_count;
        let evicted_capacity_count = self.evicted_capacity_count;
        let (session_call_count, session_age_ms) = {
            let guard = self.lock_session()?;
            let session = guard.as_ref().ok_or_else(|| {
                UpstreamSessionPoolError::state("checked-out upstream worker is not initialized")
            })?;
            (session.call_count, session.created_at.elapsed().as_millis())
        };
        Ok(UpstreamPoolCallOutcome {
            enabled: true,
            hit,
            session_call_count,
            session_age_ms,
            pool_size,
            max_pool_size,
            idle_ttl_ms,
            evicted_idle_count,
            evicted_capacity_count,
        })
    }
}

impl Drop for UpstreamSessionCheckout<'_> {
    fn drop(&mut self) {
        self.worker
            .last_used_ms
            .store(now_epoch_ms(), Ordering::Release);
        if self.failed {
            self.worker.initialized.store(false, Ordering::Release);
            let removed = self.pool.remove_worker(&self.key, &self.worker);
            self.worker.busy.store(false, Ordering::Release);
            self.pool.available.notify_all();
            drop(removed);
        } else {
            self.worker.busy.store(false, Ordering::Release);
            self.pool.available.notify_one();
        }
    }
}

impl Drop for WorkerInitializationGuard<'_> {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        self.worker.initialized.store(false, Ordering::Release);
        let removed = self.pool.remove_worker(self.key, &self.worker);
        self.worker.busy.store(false, Ordering::Release);
        self.pool.available.notify_all();
        drop(removed);
    }
}

impl UpstreamSessionPool {
    pub fn with_max_sessions(max_sessions: usize) -> Self {
        Self {
            state: Mutex::new(UpstreamSessionPoolState::default()),
            available: Condvar::new(),
            max_sessions: max_sessions.max(1),
        }
    }

    fn lock_state(&self) -> MutexGuard<'_, UpstreamSessionPoolState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn session_count(&self) -> usize {
        registered_session_count(&self.lock_state())
    }

    pub fn max_session_count(&self) -> usize {
        self.max_sessions
    }

    pub fn idle_ttl_ms(&self) -> u128 {
        let state = self.lock_state();
        state
            .groups
            .values()
            .flat_map(|workers| workers.iter())
            .map(|worker| worker.idle_ttl_ms.load(Ordering::Acquire) as u128)
            .max()
            .unwrap_or_else(|| UPSTREAM_SESSION_IDLE_TTL.as_millis())
    }

    pub(crate) fn purge_idle_and_exited(&self) -> usize {
        let now_ms = now_epoch_ms();
        let candidates = {
            let state = self.lock_state();
            state
                .groups
                .values()
                .flat_map(|workers| workers.iter().cloned())
                .collect::<Vec<_>>()
        };
        let stale = candidates
            .into_iter()
            .filter(|worker| {
                if worker.busy.load(Ordering::Acquire) {
                    return false;
                }
                let idle = now_ms.saturating_sub(worker.last_used_ms.load(Ordering::Acquire))
                    > worker.idle_ttl_ms.load(Ordering::Acquire);
                let exited_or_missing = match worker.session.try_lock() {
                    Ok(mut guard) => guard
                        .as_mut()
                        .map(|session| session.running.has_exited())
                        .unwrap_or(true),
                    Err(TryLockError::Poisoned(_)) => true,
                    Err(TryLockError::WouldBlock) => false,
                };
                idle || exited_or_missing
            })
            .collect::<Vec<_>>();
        let removed = self.remove_workers(&stale);
        let count = removed.len();
        if count > 0 {
            self.available.notify_all();
        }
        drop(removed);
        count
    }

    pub(crate) fn session_snapshots(&self) -> Vec<UpstreamSessionSnapshot> {
        let workers = {
            let state = self.lock_state();
            state
                .groups
                .iter()
                .flat_map(|(key, workers)| {
                    workers
                        .iter()
                        .cloned()
                        .map(|worker| (key.clone(), worker))
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        };
        let now = Instant::now();
        workers
            .into_iter()
            .filter_map(|(key, worker)| {
                if !worker.initialized.load(Ordering::Acquire) {
                    return None;
                }
                let mut guard = match worker.session.try_lock() {
                    Ok(guard) => guard,
                    Err(TryLockError::Poisoned(_)) => {
                        worker.initialized.store(false, Ordering::Release);
                        return None;
                    }
                    Err(TryLockError::WouldBlock) => return None,
                };
                let session = guard.as_mut()?;
                if session.running.has_exited() {
                    return None;
                }
                let pid = session.running.child.id();
                Some(UpstreamSessionSnapshot {
                    server_name: key.server_name,
                    pid,
                    client_id: key.client_id,
                    session_id: key.session_id,
                    project_root: key.project_root,
                    transport: key.transport,
                    created_age_ms: now.duration_since(session.created_at).as_millis(),
                    idle_ms: now.duration_since(session.last_used).as_millis(),
                    call_count: session.call_count,
                    resource: resources::process_resource_snapshot_json(pid),
                })
            })
            .collect()
    }

    pub(super) fn checkout<'a>(
        &'a self,
        invocation: UpstreamPoolInvocation<'_>,
    ) -> UpstreamSessionPoolResult<UpstreamSessionCheckout<'a>> {
        let deadline = Instant::now() + invocation.timeout;
        let key = invocation.key;
        let worker_limit = invocation.server.execution.worker_limit().max(1);
        let worker_idle_ttl_ms = invocation.server.execution.idle_ttl_ms.max(1);
        let mut evicted_idle_count = self.purge_idle_and_exited();
        let mut evicted_capacity_count = 0usize;

        loop {
            if invocation
                .lease_lost
                .is_some_and(|lost| lost.load(Ordering::Acquire))
            {
                return Err(UpstreamSessionPoolError::state(
                    "upstream lease was lost while waiting for a pooled worker",
                ));
            }

            let mut evicted_worker = None;
            let mut selected = None;
            let mut state = self.lock_state();
            if let Some(workers) = state.groups.get(&key) {
                for worker in workers.iter().take(worker_limit) {
                    if worker
                        .busy
                        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                        .is_ok()
                    {
                        worker
                            .idle_ttl_ms
                            .store(worker_idle_ttl_ms, Ordering::Release);
                        let hit = worker.initialized.load(Ordering::Acquire);
                        selected = Some((Arc::clone(worker), hit));
                        break;
                    }
                }
            }

            if selected.is_none() {
                let group_len = state.groups.get(&key).map_or(0, Vec::len);
                let total = registered_session_count(&state);
                if group_len < worker_limit && total < self.max_sessions {
                    let worker = Arc::new(PooledUpstreamWorker::reserved(worker_idle_ttl_ms));
                    state
                        .groups
                        .entry(key.clone())
                        .or_default()
                        .push(Arc::clone(&worker));
                    selected = Some((worker, false));
                } else if total >= self.max_sessions {
                    evicted_worker = remove_oldest_idle_worker(&mut state);
                    if evicted_worker.is_some() {
                        evicted_capacity_count = evicted_capacity_count.saturating_add(1);
                    }
                }
            }

            if let Some((worker, hit)) = selected {
                drop(state);
                drop(evicted_worker);
                if !hit {
                    let mut initialization_guard = WorkerInitializationGuard {
                        pool: self,
                        key: &key,
                        worker: Arc::clone(&worker),
                        armed: true,
                    };
                    let remaining =
                        deadline
                            .checked_duration_since(Instant::now())
                            .ok_or_else(|| {
                                UpstreamSessionPoolError::state(
                                    "upstream session pool timed out before worker initialization",
                                )
                            })?;
                    let session = PooledUpstreamSession::new(
                        invocation.root_path,
                        invocation.server,
                        remaining,
                        invocation.lease_lost,
                    )?;
                    match worker.session.lock() {
                        Ok(mut guard) => *guard = Some(session),
                        Err(poisoned) => {
                            drop(poisoned.into_inner());
                            return Err(UpstreamSessionPoolError::state(
                                "upstream worker session state was poisoned during initialization",
                            ));
                        }
                    }
                    worker.initialized.store(true, Ordering::Release);
                    initialization_guard.armed = false;
                }
                return Ok(UpstreamSessionCheckout {
                    pool: self,
                    key,
                    worker,
                    hit,
                    evicted_idle_count,
                    evicted_capacity_count,
                    failed: false,
                });
            }

            if let Some(worker) = evicted_worker {
                drop(state);
                drop(worker);
                continue;
            }

            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or_else(|| {
                    UpstreamSessionPoolError::state(format!(
                        "upstream session pool timed out waiting for one of {} worker(s)",
                        worker_limit.min(self.max_sessions)
                    ))
                })?;
            let wait_for = remaining.min(Duration::from_millis(50));
            let (next_state, _) = self
                .available
                .wait_timeout(state, wait_for)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            drop(next_state);
            evicted_idle_count = evicted_idle_count.saturating_add(self.purge_idle_and_exited());
        }
    }

    fn remove_worker(
        &self,
        key: &UpstreamSessionKey,
        worker: &Arc<PooledUpstreamWorker>,
    ) -> Option<Arc<PooledUpstreamWorker>> {
        let mut state = self.lock_state();
        let mut removed = None;
        let mut remove_group = false;
        if let Some(workers) = state.groups.get_mut(key) {
            if let Some(index) = workers
                .iter()
                .position(|candidate| Arc::ptr_eq(candidate, worker))
            {
                removed = Some(workers.remove(index));
            }
            remove_group = workers.is_empty();
        }
        if remove_group {
            state.groups.remove(key);
        }
        removed
    }

    fn remove_workers(
        &self,
        candidates: &[Arc<PooledUpstreamWorker>],
    ) -> Vec<Arc<PooledUpstreamWorker>> {
        if candidates.is_empty() {
            return Vec::new();
        }
        let mut state = self.lock_state();
        let mut removed = Vec::new();
        state.groups.retain(|_, workers| {
            let mut index = 0usize;
            while index < workers.len() {
                let candidate = candidates
                    .iter()
                    .any(|candidate| Arc::ptr_eq(candidate, &workers[index]));
                if candidate && !workers[index].busy.load(Ordering::Acquire) {
                    removed.push(workers.remove(index));
                } else {
                    index = index.saturating_add(1);
                }
            }
            !workers.is_empty()
        });
        removed
    }
}

fn registered_session_count(state: &UpstreamSessionPoolState) -> usize {
    state.groups.values().map(Vec::len).sum()
}

fn remove_oldest_idle_worker(
    state: &mut UpstreamSessionPoolState,
) -> Option<Arc<PooledUpstreamWorker>> {
    let candidate = state
        .groups
        .iter()
        .flat_map(|(key, workers)| {
            workers
                .iter()
                .enumerate()
                .filter_map(move |(index, worker)| {
                    (!worker.busy.load(Ordering::Acquire)).then_some((
                        key.clone(),
                        index,
                        worker.last_used_ms.load(Ordering::Acquire),
                    ))
                })
        })
        .min_by_key(|(_, _, last_used_ms)| *last_used_ms);
    let (key, index, _) = candidate?;
    let workers = state.groups.get_mut(&key)?;
    let worker = workers.remove(index);
    if workers.is_empty() {
        state.groups.remove(&key);
    }
    Some(worker)
}

fn now_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

#[cfg(test)]
mod tests;
