use super::{
    cache_root_path, run_http_request, run_stdio_request, server_fingerprint, UpstreamServerConfig,
    TOOL_LIST_CACHE_MAX_ENTRIES, TOOL_LIST_CACHE_TTL,
};
use crate::json::JsonValue;
use crate::json_helpers;
use crate::mcp_sources;
use crate::runtimepaths;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const TOOL_LIST_DISK_CACHE_SCHEMA_VERSION: i64 = 2;
const TOOL_LIST_DISK_CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

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
    if let Some(tools) = read_memory_cached_tools(key) {
        return Some(tools);
    }
    let tools = read_disk_cached_tools(key)?;
    write_memory_cached_tools(key.clone(), tools.clone());
    Some(tools)
}

fn read_memory_cached_tools(key: &ToolListCacheKey) -> Option<JsonValue> {
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
    let _ = write_disk_cached_tools(&key, &tools);
    write_memory_cached_tools(key, tools);
}

fn write_memory_cached_tools(key: ToolListCacheKey, tools: JsonValue) {
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

fn read_disk_cached_tools(key: &ToolListCacheKey) -> Option<JsonValue> {
    let path = read_disk_cache_path(key)?;
    let value = json_helpers::read_json_file(&path).ok()?;
    if json_helpers::value_at_path(&value, &["schemaVersion"]).and_then(JsonValue::as_i64)
        != Some(TOOL_LIST_DISK_CACHE_SCHEMA_VERSION)
    {
        return None;
    }
    let expected_hash = disk_cache_key_hash(key);
    if json_helpers::string_at_path(&value, &["keyHash"]) != Some(expected_hash.as_str()) {
        return None;
    }
    if json_helpers::string_at_path(&value, &["serverName"]) != Some(key.server_name.as_str()) {
        return None;
    }
    let stored_at = u128_at_path(&value, &["storedAtUnixMs"])?;
    let now = unix_time_ms()?;
    if stored_at.saturating_add(TOOL_LIST_DISK_CACHE_TTL.as_millis()) < now {
        let _ = fs::remove_file(path);
        return None;
    }
    let tools = json_helpers::value_at_path(&value, &["tools"])?.clone();
    tools.as_array()?;
    Some(tools)
}

fn write_disk_cached_tools(key: &ToolListCacheKey, tools: &JsonValue) -> Result<(), String> {
    let Some(items) = tools.as_array() else {
        return Ok(());
    };
    let path = write_disk_cache_path(key)?;
    let stored_at =
        unix_time_ms().ok_or_else(|| "system clock is before UNIX epoch".to_string())?;
    let envelope = JsonValue::object([
        (
            "schemaVersion",
            JsonValue::number(TOOL_LIST_DISK_CACHE_SCHEMA_VERSION),
        ),
        ("storedAtUnixMs", JsonValue::string(stored_at.to_string())),
        ("keyHash", JsonValue::string(disk_cache_key_hash(key))),
        ("serverName", JsonValue::string(&key.server_name)),
        (
            "mcpaceVersion",
            JsonValue::string(env!("CARGO_PKG_VERSION")),
        ),
        (
            "mcpProtocolVersion",
            JsonValue::string(crate::mcp_protocol::CURRENT_PROTOCOL_VERSION),
        ),
        ("toolCount", JsonValue::number(items.len())),
        ("tools", tools.clone()),
    ]);
    runtimepaths::write_text_atomic(&path, &envelope.to_compact_string())
}

fn read_disk_cache_path(key: &ToolListCacheKey) -> Option<PathBuf> {
    let root_path = Path::new(&key.root_path);
    let state_root = runtimepaths::resolve_state_root(root_path);
    let dir = runtimepaths::tool_list_cache_dir(&state_root);
    if !dir.is_dir() {
        return None;
    }
    Some(dir.join(disk_cache_file_name(key)))
}

fn write_disk_cache_path(key: &ToolListCacheKey) -> Result<PathBuf, String> {
    let root_path = Path::new(&key.root_path);
    let state_root = runtimepaths::resolve_state_root(root_path);
    let dir = runtimepaths::ensure_tool_list_cache_dir(&state_root)?;
    Ok(dir.join(disk_cache_file_name(key)))
}

fn disk_cache_file_name(key: &ToolListCacheKey) -> String {
    format!(
        "{}-{}.json",
        safe_cache_file_stem(&key.server_name),
        disk_cache_key_hash(key)
    )
}

fn safe_cache_file_stem(value: &str) -> String {
    let stem = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
                ch
            } else {
                '_'
            }
        })
        .take(48)
        .collect::<String>()
        .trim_matches(|ch| matches!(ch, '.' | '_' | '-'))
        .to_string();
    if stem.is_empty() {
        "server".to_string()
    } else {
        stem
    }
}

fn disk_cache_key_hash(key: &ToolListCacheKey) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    feed_stable_hash(&mut hash, key.root_path.as_bytes());
    feed_stable_hash(&mut hash, key.server_name.as_bytes());
    feed_stable_hash(&mut hash, key.settings_modified_ms.to_string().as_bytes());
    feed_stable_hash(&mut hash, key.settings_len.to_string().as_bytes());
    feed_stable_hash(&mut hash, key.server_fingerprint.as_bytes());
    feed_stable_hash(&mut hash, env!("CARGO_PKG_VERSION").as_bytes());
    feed_stable_hash(
        &mut hash,
        crate::mcp_protocol::CURRENT_PROTOCOL_VERSION.as_bytes(),
    );
    format!("{hash:016x}")
}

fn feed_stable_hash(hash: &mut u64, bytes: &[u8]) {
    for byte in bytes {
        *hash ^= u64::from(*byte);
        *hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    *hash ^= 0xff;
    *hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
}

fn unix_time_ms() -> Option<u128> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis())
}

fn u128_at_path(value: &JsonValue, path: &[&str]) -> Option<u128> {
    match json_helpers::value_at_path(value, path)? {
        JsonValue::Number(value) | JsonValue::String(value) => value.parse::<u128>().ok(),
        _ => None,
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
