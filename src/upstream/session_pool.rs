use super::stdio_runtime::{
    initialize_running_server, read_response, spawn_stdio_server, write_jsonrpc, RunningServer,
};
use super::{
    max_pooled_upstream_sessions, UpstreamServerConfig, UpstreamToolCall, INITIALIZE_ID, METHOD_ID,
    UPSTREAM_SESSION_IDLE_TTL,
};
use crate::json::JsonValue;
use crate::json_helpers;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

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
    pub(super) metadata_fingerprint: String,
}

struct PooledUpstreamSession {
    running: RunningServer,
    created_at: Instant,
    last_used: Instant,
    next_request_id: i64,
    call_count: usize,
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

pub(super) struct UpstreamPoolInvocation<'a> {
    pub(super) root_path: &'a Path,
    pub(super) server: &'a UpstreamServerConfig,
    pub(super) key: UpstreamSessionKey,
    pub(super) timeout: Duration,
    pub(super) lease_lost: Option<&'a AtomicBool>,
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

    pub(super) fn list_tools(
        &mut self,
        server: &UpstreamServerConfig,
        deadline: Instant,
        lease_lost: Option<&AtomicBool>,
    ) -> Result<JsonValue, String> {
        let request_id = self.next_request_id();
        write_jsonrpc(
            &mut self.running.stdin,
            JsonValue::object([
                ("jsonrpc", JsonValue::string("2.0")),
                ("id", JsonValue::number(request_id)),
                ("method", JsonValue::string("tools/list")),
            ]),
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
        self.last_used = Instant::now();
        Ok(result)
    }

    pub(super) fn call_tool(
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

    pub(super) fn call_tools(
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

    pub(crate) fn purge_idle_and_exited(&mut self) -> usize {
        self.remove_idle_and_exited_sessions()
    }

    pub(super) fn session_exists(&self, key: &UpstreamSessionKey) -> bool {
        self.sessions.contains_key(key)
    }

    pub(super) fn list_tools(
        &mut self,
        invocation: UpstreamPoolInvocation<'_>,
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

        let list_result = self
            .sessions
            .get_mut(&key)
            .ok_or_else(|| "upstream session pool lost its session entry".to_string())?
            .list_tools(invocation.server, deadline, invocation.lease_lost);

        match list_result {
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

    pub(super) fn call_tool(
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

    pub(super) fn call_tools(
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
        let evicted_idle_count = self.remove_idle_and_exited_sessions();

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

    fn remove_idle_and_exited_sessions(&mut self) -> usize {
        let now = Instant::now();
        let mut evicted_count = 0usize;
        self.sessions.retain(|_, session| {
            let idle = now.duration_since(session.last_used) > UPSTREAM_SESSION_IDLE_TTL;
            let exited = session.running.has_exited();
            if idle || exited {
                evicted_count = evicted_count.saturating_add(1);
                false
            } else {
                true
            }
        });
        evicted_count
    }
}
