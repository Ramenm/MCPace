use super::http_runtime::run_http_tools_list;
use super::stdio_runtime::run_stdio_tools_list;
use super::{
    cache_root_path, server_fingerprint, UpstreamServerConfig, TOOL_LIST_CACHE_MAX_ENTRIES,
    TOOL_LIST_CACHE_TTL,
};
use crate::json::JsonValue;
use crate::json_helpers;
use crate::mcp_sources;
use crate::runtimepaths;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

const TOOL_LIST_DISK_CACHE_SCHEMA_VERSION: i64 = 3;

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ToolListCacheError {
    RuntimePath(runtimepaths::RuntimePathError),
    ClockBeforeUnixEpoch,
    Upstream(String),
}

pub(super) type ToolListCacheResult<T> = Result<T, ToolListCacheError>;

impl fmt::Display for ToolListCacheError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RuntimePath(error) => write!(formatter, "{}", error),
            Self::ClockBeforeUnixEpoch => formatter.write_str("system clock is before UNIX epoch"),
            Self::Upstream(error) => write!(formatter, "{}", error),
        }
    }
}

impl std::error::Error for ToolListCacheError {}

impl From<runtimepaths::RuntimePathError> for ToolListCacheError {
    fn from(error: runtimepaths::RuntimePathError) -> Self {
        Self::RuntimePath(error)
    }
}

impl From<String> for ToolListCacheError {
    fn from(error: String) -> Self {
        Self::Upstream(error)
    }
}

impl From<ToolListCacheError> for String {
    fn from(error: ToolListCacheError) -> Self {
        error.to_string()
    }
}

pub(super) static TOOL_LIST_CACHE: OnceLock<Mutex<BTreeMap<ToolListCacheKey, CachedToolList>>> =
    OnceLock::new();
static TOOL_LIST_INFLIGHT: OnceLock<(Mutex<BTreeSet<ToolListCacheKey>>, Condvar)> = OnceLock::new();

pub(super) fn cached_tools_list(
    root_path: &Path,
    server: &UpstreamServerConfig,
    timeout: Duration,
    refresh: bool,
) -> ToolListCacheResult<(JsonValue, bool)> {
    let key = tool_list_cache_key(root_path, server);
    if !refresh {
        if let Some(tools) = read_cached_tools(&key) {
            return Ok((tools, true));
        }
    }
    match acquire_tool_list_load_permit_or_cached(&key) {
        ToolListCacheAcquire::Cached(tools) => Ok((tools, true)),
        ToolListCacheAcquire::Load(permit) => {
            let result = run_tool_list_request(root_path, server, timeout)?;
            let tools = validate_tools_list_result(server, &result)?;
            write_cached_tools(key, tools.clone());
            drop(permit);
            Ok((tools, false))
        }
    }
}

pub(super) fn validate_tools_list_result(
    server: &UpstreamServerConfig,
    result: &JsonValue,
) -> ToolListCacheResult<JsonValue> {
    let tools = json_helpers::value_at_path(result, &["tools"])
        .and_then(JsonValue::as_array)
        .ok_or_else(|| {
            ToolListCacheError::Upstream(format!(
                "upstream server '{}' tools/list result must contain a tools array",
                server.name
            ))
        })?;
    let mut names = BTreeSet::new();
    for (index, tool) in tools.iter().enumerate() {
        let name = json_helpers::string_at_path(tool, &["name"])
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                ToolListCacheError::Upstream(format!(
                    "upstream server '{}' tools/list item {} has no non-empty name",
                    server.name, index
                ))
            })?;
        if !names.insert(name.to_string()) {
            return Err(ToolListCacheError::Upstream(format!(
                "upstream server '{}' tools/list returned duplicate tool name '{}'",
                server.name, name
            )));
        }
        if !matches!(
            json_helpers::value_at_path(tool, &["inputSchema"]),
            Some(JsonValue::Object(_))
        ) {
            return Err(ToolListCacheError::Upstream(format!(
                "upstream server '{}' tool '{}' has no object inputSchema",
                server.name, name
            )));
        }
    }
    Ok(JsonValue::array(tools.to_vec()))
}

fn run_tool_list_request(
    root_path: &Path,
    server: &UpstreamServerConfig,
    timeout: Duration,
) -> ToolListCacheResult<JsonValue> {
    if server.source_type == "http" {
        return run_http_tools_list(server, timeout)
            .map_err(|error| ToolListCacheError::Upstream(error.to_string()));
    }
    run_stdio_tools_list(root_path, server, timeout, None)
        .map_err(|error| ToolListCacheError::Upstream(error.to_string()))
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
    let now = runtimepaths::unix_time_ms_checked()?;
    if stored_at > now.saturating_add(Duration::from_secs(5 * 60).as_millis())
        || stored_at.saturating_add(TOOL_LIST_CACHE_TTL.as_millis()) < now
    {
        let _ = fs::remove_file(path);
        return None;
    }
    let tools = json_helpers::value_at_path(&value, &["tools"])?.clone();
    tools.as_array()?;
    Some(tools)
}

fn write_disk_cached_tools(key: &ToolListCacheKey, tools: &JsonValue) -> ToolListCacheResult<()> {
    let Some(items) = tools.as_array() else {
        return Ok(());
    };
    let path = write_disk_cache_path(key)?;
    let stored_at =
        runtimepaths::unix_time_ms_checked().ok_or(ToolListCacheError::ClockBeforeUnixEpoch)?;
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
    runtimepaths::write_text_atomic(&path, &envelope.to_compact_string())?;
    if let Some(directory) = path.parent() {
        prune_disk_tool_cache(directory);
    }
    Ok(())
}

fn prune_disk_tool_cache(directory: &Path) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let mut files = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let file_type = entry.file_type().ok()?;
            if !file_type.is_file() || file_type.is_symlink() {
                return None;
            }
            let path = entry.path();
            if !path
                .extension()
                .and_then(|value| value.to_str())
                .map(|value| value.eq_ignore_ascii_case("json"))
                .unwrap_or(false)
            {
                return None;
            }
            let modified = entry.metadata().ok()?.modified().ok()?;
            Some((modified, path))
        })
        .collect::<Vec<_>>();
    files.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    for (_modified, path) in files.into_iter().skip(TOOL_LIST_CACHE_MAX_ENTRIES) {
        let _ = fs::remove_file(path);
    }
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

fn write_disk_cache_path(key: &ToolListCacheKey) -> ToolListCacheResult<PathBuf> {
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
