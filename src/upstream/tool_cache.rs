use super::{
    cache_root_path, run_http_request, run_stdio_request, server_fingerprint, UpstreamServerConfig,
    TOOL_LIST_CACHE_MAX_ENTRIES, TOOL_LIST_CACHE_TTL,
};
use crate::json::JsonValue;
use crate::json_helpers;
use crate::mcp_sources;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(super) struct ToolListCacheKey {
    pub(super) root_path: String,
    pub(super) server_name: String,
    pub(super) settings_modified_ms: u128,
    pub(super) settings_len: u64,
    pub(super) server_fingerprint: String,
}

#[derive(Clone, Debug)]
pub(super) struct CachedToolList {
    pub(super) stored_at: Instant,
    pub(super) tools: JsonValue,
}

pub(super) static TOOL_LIST_CACHE: OnceLock<Mutex<BTreeMap<ToolListCacheKey, CachedToolList>>> =
    OnceLock::new();
static TOOL_LIST_INFLIGHT: OnceLock<(Mutex<BTreeSet<ToolListCacheKey>>, Condvar)> = OnceLock::new();

pub(super) fn cached_tools_list(
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
                let result = run_tool_list_request(root_path, server, timeout)?;
                let tools = json_helpers::value_at_path(&result, &["tools"])
                    .cloned()
                    .unwrap_or_else(|| JsonValue::array([]));
                write_cached_tools(key, tools.clone());
                drop(permit);
                return Ok((tools, false));
            }
        }
    }

    let result = run_tool_list_request(root_path, server, timeout)?;
    let tools = json_helpers::value_at_path(&result, &["tools"])
        .cloned()
        .unwrap_or_else(|| JsonValue::array([]));
    write_cached_tools(key, tools.clone());
    Ok((tools, false))
}

fn run_tool_list_request(
    root_path: &Path,
    server: &UpstreamServerConfig,
    timeout: Duration,
) -> Result<JsonValue, String> {
    if server.source_type == "http" {
        return run_http_request(server, "tools/list", None, timeout);
    }
    run_stdio_request(root_path, server, "tools/list", None, timeout, None)
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

pub(super) fn read_cached_tools(key: &ToolListCacheKey) -> Option<JsonValue> {
    let cache = TOOL_LIST_CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut guard = cache.lock().ok()?;
    let entry = guard.get(key)?;
    if entry.stored_at.elapsed() <= TOOL_LIST_CACHE_TTL {
        return Some(entry.tools.clone());
    }
    guard.remove(key);
    None
}

pub(super) fn write_cached_tools(key: ToolListCacheKey, tools: JsonValue) {
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

pub(super) fn prune_tool_list_cache(cache: &mut BTreeMap<ToolListCacheKey, CachedToolList>) {
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

pub(super) fn tool_list_cache_key(
    root_path: &Path,
    server: &UpstreamServerConfig,
) -> ToolListCacheKey {
    let (settings_modified_ms, settings_len) = mcp_sources::mcp_settings_fingerprint(root_path);
    ToolListCacheKey {
        root_path: cache_root_path(root_path),
        server_name: server.name.clone(),
        settings_modified_ms,
        settings_len,
        server_fingerprint: server_fingerprint(server),
    }
}
