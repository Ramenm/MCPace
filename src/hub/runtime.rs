use super::model::{JsonFileDiagnostic, RepairReport, RuntimeLockGuard};
use crate::json::JsonValue;
use crate::json_helpers;
use crate::runtimepaths;
use std::collections::BTreeMap;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const DEFAULT_MAX_LOG_BYTES: u64 = 1_048_576;

pub(super) fn ensure_runtime_layout(root_path: &Path) -> Result<(), String> {
    let state_root = runtimepaths::resolve_state_root(root_path);
    runtimepaths::ensure_runtime_dir(&state_root)?;
    runtimepaths::ensure_hub_dir(&state_root)?;
    let _ = seed_json_if_missing(
        &runtimepaths::project_registry_path(&state_root),
        default_project_registry(),
    )?;
    let _ = seed_json_if_missing(
        &runtimepaths::hub_leases_path(&state_root),
        default_lease_store(),
    )?;
    Ok(())
}

pub(super) fn mark_stopped(root_path: &Path) -> Result<(), String> {
    let state_root = runtimepaths::resolve_state_root(root_path);
    remove_if_present(&runtimepaths::hub_stop_path(&state_root))?;
    remove_if_present(&runtimepaths::hub_health_path(&state_root))?;
    remove_if_present(&runtimepaths::hub_lock_path(&state_root))?;
    let stop_ms = now_ms();
    let _ = append_log(
        root_path,
        "warn",
        "hub_marked_stopped",
        &[(
            "reason",
            JsonValue::string("stale-state-cleanup".to_string()),
        )],
    );
    write_state_metadata(root_path, "stopped", None, None, Some(stop_ms))
}

pub(super) fn repair_runtime_files(root_path: &Path) -> Result<RepairReport, String> {
    ensure_runtime_layout(root_path)?;
    let state_root = runtimepaths::resolve_state_root(root_path);
    let registry_path = runtimepaths::project_registry_path(&state_root);
    let lease_path = runtimepaths::hub_leases_path(&state_root);
    let state_path = runtimepaths::hub_state_path(&state_root);
    let health_path = runtimepaths::hub_health_path(&state_root);
    let lock_path = runtimepaths::hub_lock_path(&state_root);
    let stop_path = runtimepaths::hub_stop_path(&state_root);

    let diagnostics = vec![
        read_json_diagnostic(&registry_path),
        read_json_diagnostic(&lease_path),
        read_json_diagnostic(&state_path),
        read_json_diagnostic(&health_path),
        read_json_diagnostic(&lock_path),
    ];

    let mut archived_paths = Vec::new();
    let mut recreated_paths = Vec::new();
    let mut warnings = Vec::new();

    for diagnostic in diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.error.is_some())
    {
        let archived_path = archive_corrupted_file(&diagnostic.path)?;
        archived_paths.push(archived_path.display().to_string());
    }

    if stop_path.is_file() {
        remove_if_present(&stop_path)?;
    }
    if health_path.is_file() {
        remove_if_present(&health_path)?;
    }
    if lock_path.is_file() {
        remove_if_present(&lock_path)?;
    }

    if seed_json_if_missing(&registry_path, default_project_registry())? {
        recreated_paths.push(registry_path.display().to_string());
    }
    if seed_json_if_missing(&lease_path, default_lease_store())? {
        recreated_paths.push(lease_path.display().to_string());
    }

    write_state_metadata(root_path, "stopped", None, None, Some(now_ms()))?;
    recreated_paths.push(state_path.display().to_string());

    if archived_paths.is_empty() {
        warnings.push(
            "hub repair completed without archived corrupt files; state files were normalized to a clean stopped baseline".to_string(),
        );
    }

    let _ = append_log(
        root_path,
        "warn",
        "hub_repaired",
        &[
            ("archivedCount", JsonValue::number(archived_paths.len())),
            ("recreatedCount", JsonValue::number(recreated_paths.len())),
        ],
    );

    Ok(RepairReport {
        root_path: root_path.display().to_string(),
        state_root: state_root.display().to_string(),
        archived_paths,
        recreated_paths,
        warnings,
    })
}

pub(super) fn read_json_diagnostic(path: &Path) -> JsonFileDiagnostic {
    if !path.is_file() {
        return JsonFileDiagnostic {
            path: path.to_path_buf(),
            exists: false,
            value: None,
            error: None,
        };
    }

    let raw = match fs::read_to_string(path) {
        Ok(value) => value,
        Err(error) => {
            return JsonFileDiagnostic {
                path: path.to_path_buf(),
                exists: true,
                value: None,
                error: Some(format!("failed to read {}: {}", path.display(), error)),
            };
        }
    };

    match crate::json::parse_str(&raw) {
        Ok(value) => JsonFileDiagnostic {
            path: path.to_path_buf(),
            exists: true,
            value: Some(value),
            error: None,
        },
        Err(error) => JsonFileDiagnostic {
            path: path.to_path_buf(),
            exists: true,
            value: None,
            error: Some(format!("failed to parse {}: {}", path.display(), error)),
        },
    }
}

pub(super) fn write_state_metadata(
    root_path: &Path,
    status: &str,
    pid: Option<u32>,
    started_at_ms: Option<u128>,
    last_exit_at_ms: Option<u128>,
) -> Result<(), String> {
    let state_root = runtimepaths::resolve_state_root(root_path);
    let mut map = BTreeMap::new();
    map.insert("status".to_string(), JsonValue::string(status.to_string()));
    map.insert(
        "rootPath".to_string(),
        JsonValue::string(root_path.display().to_string()),
    );
    map.insert(
        "stateRoot".to_string(),
        JsonValue::string(state_root.display().to_string()),
    );
    map.insert(
        "runtimeDir".to_string(),
        JsonValue::string(runtimepaths::runtime_dir(&state_root).display().to_string()),
    );
    map.insert(
        "hubDir".to_string(),
        JsonValue::string(runtimepaths::hub_dir(&state_root).display().to_string()),
    );
    map.insert(
        "logPath".to_string(),
        JsonValue::string(
            runtimepaths::hub_log_path(&state_root)
                .display()
                .to_string(),
        ),
    );
    map.insert(
        "leaseStorePath".to_string(),
        JsonValue::string(
            runtimepaths::hub_leases_path(&state_root)
                .display()
                .to_string(),
        ),
    );
    match pid {
        Some(value) => {
            map.insert("pid".to_string(), JsonValue::number(value));
        }
        None => {
            map.insert("pid".to_string(), JsonValue::Null);
        }
    }
    match started_at_ms {
        Some(value) => {
            map.insert("startedAtMs".to_string(), JsonValue::number(value));
        }
        None => {
            map.insert("startedAtMs".to_string(), JsonValue::Null);
        }
    }
    match last_exit_at_ms {
        Some(value) => {
            map.insert("lastExitAtMs".to_string(), JsonValue::number(value));
        }
        None => {
            map.insert("lastExitAtMs".to_string(), JsonValue::Null);
        }
    }
    write_atomic(
        &runtimepaths::hub_state_path(&state_root),
        JsonValue::Object(map).to_pretty_string(),
    )
}

pub(super) fn write_health_metadata(
    root_path: &Path,
    status: &str,
    pid: u32,
    started_at_ms: u128,
    last_heartbeat_at_ms: u128,
) -> Result<(), String> {
    let state_root = runtimepaths::resolve_state_root(root_path);
    let value = JsonValue::object([
        ("status", JsonValue::string(status.to_string())),
        ("pid", JsonValue::number(pid)),
        ("startedAtMs", JsonValue::number(started_at_ms)),
        ("lastHeartbeatAtMs", JsonValue::number(last_heartbeat_at_ms)),
    ]);
    write_atomic(
        &runtimepaths::hub_health_path(&state_root),
        value.to_pretty_string(),
    )
}

pub(super) fn append_log(
    root_path: &Path,
    level: &str,
    event: &str,
    fields: &[(&str, JsonValue)],
) -> Result<(), String> {
    let state_root = runtimepaths::resolve_state_root(root_path);
    runtimepaths::ensure_hub_dir(&state_root)?;
    let log_path = runtimepaths::hub_log_path(&state_root);
    let rotated = rotate_logs_if_needed(&log_path)?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|error| format!("failed to open {}: {}", log_path.display(), error))?;

    if rotated {
        let archive_path = rotated_log_path(&log_path);
        let rotation_line = JsonValue::object([
            ("tsMs", JsonValue::number(now_ms())),
            ("level", JsonValue::string("warn".to_string())),
            ("event", JsonValue::string("hub_log_rotated".to_string())),
            (
                "archivedPath",
                JsonValue::string(archive_path.display().to_string()),
            ),
            ("maxBytes", JsonValue::number(max_log_bytes())),
        ])
        .to_compact_string();
        writeln!(file, "{}", rotation_line)
            .map_err(|error| format!("failed to append {}: {}", log_path.display(), error))?;
    }

    let mut map = BTreeMap::new();
    map.insert("tsMs".to_string(), JsonValue::number(now_ms()));
    map.insert("level".to_string(), JsonValue::string(level.to_string()));
    map.insert("event".to_string(), JsonValue::string(event.to_string()));
    for (key, value) in fields {
        map.insert((*key).to_string(), value.clone());
    }
    let line = JsonValue::Object(map).to_compact_string();
    writeln!(file, "{}", line)
        .map_err(|error| format!("failed to append {}: {}", log_path.display(), error))?;
    Ok(())
}

pub(super) fn write_atomic(path: &Path, contents: String) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {}", parent.display(), error))?;
    }
    let temp_path = path.with_extension(format!("tmp-{}-{}", std::process::id(), now_ms()));
    fs::write(&temp_path, contents)
        .map_err(|error| format!("failed to write {}: {}", temp_path.display(), error))?;
    #[cfg(windows)]
    {
        let _ = fs::remove_file(path);
    }
    fs::rename(&temp_path, path).map_err(|error| {
        format!(
            "failed to move {} to {}: {}",
            temp_path.display(),
            path.display(),
            error
        )
    })?;
    Ok(())
}

pub(super) fn acquire_runtime_lock(
    root_path: &Path,
    pid: u32,
    started_at_ms: u128,
) -> Result<RuntimeLockGuard, String> {
    let state_root = runtimepaths::resolve_state_root(root_path);
    runtimepaths::ensure_hub_dir(&state_root)?;
    let lock_path = runtimepaths::hub_lock_path(&state_root);
    let payload = JsonValue::object([
        ("pid", JsonValue::number(pid)),
        ("startedAtMs", JsonValue::number(started_at_ms)),
    ])
    .to_pretty_string();

    match OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&lock_path)
    {
        Ok(mut file) => {
            file.write_all(payload.as_bytes())
                .map_err(|error| format!("failed to write {}: {}", lock_path.display(), error))?;
            Ok(RuntimeLockGuard { path: lock_path })
        }
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {
            let existing = read_json_diagnostic(&lock_path);
            let owner_pid = existing
                .value
                .as_ref()
                .and_then(|value| json_helpers::value_at_path(value, &["pid"]))
                .and_then(|value| value.as_i64())
                .and_then(|value| u32::try_from(value).ok());
            let owner_started_at_ms = existing
                .value
                .as_ref()
                .and_then(|value| json_helpers::value_at_path(value, &["startedAtMs"]))
                .and_then(|value| value.as_i64())
                .and_then(|value| u128::try_from(value).ok());
            if let Some(reason) = existing.error {
                return Err(format!(
                    "hub runtime lock exists but is unreadable: {}. Run 'mcpace hub repair' to archive and reseed it",
                    reason
                ));
            }
            Err(match (owner_pid, owner_started_at_ms) {
                (Some(owner_pid), Some(owner_started_at_ms)) => format!(
                    "hub runtime lock is already held by pid {} (startedAtMs={})",
                    owner_pid, owner_started_at_ms
                ),
                (Some(owner_pid), None) => {
                    format!("hub runtime lock is already held by pid {}", owner_pid)
                }
                _ => format!(
                    "hub runtime lock already exists at {}; run 'mcpace hub down' or 'mcpace hub repair' to clean it up before starting again",
                    lock_path.display()
                ),
            })
        }
        Err(error) => Err(format!(
            "failed to acquire hub runtime lock at {}: {}",
            lock_path.display(),
            error
        )),
    }
}

pub(super) fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

fn seed_json_if_missing(path: &Path, value: JsonValue) -> Result<bool, String> {
    if path.is_file() {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {}", parent.display(), error))?;
    }
    write_atomic(path, value.to_pretty_string())?;
    Ok(true)
}

fn default_project_registry() -> JsonValue {
    JsonValue::object([
        ("version", JsonValue::number(1)),
        ("projects", JsonValue::Object(BTreeMap::new())),
    ])
}

fn default_lease_store() -> JsonValue {
    JsonValue::object([
        ("version", JsonValue::number(2)),
        ("leases", JsonValue::Object(BTreeMap::new())),
        ("sessions", JsonValue::Object(BTreeMap::new())),
        ("updatedAtMs", JsonValue::number(now_ms())),
    ])
}

fn archive_corrupted_file(path: &Path) -> Result<PathBuf, String> {
    let file_name = path
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| "runtime-state.json".to_string());
    let archive_name = format!("{}.corrupt-{}", file_name, now_ms());
    let archive_path = path.with_file_name(archive_name);
    fs::rename(path, &archive_path).map_err(|error| {
        format!(
            "failed to archive corrupt runtime file {} to {}: {}",
            path.display(),
            archive_path.display(),
            error
        )
    })?;
    Ok(archive_path)
}

fn remove_if_present(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("failed to remove {}: {}", path.display(), error)),
    }
}

pub(super) fn rotate_logs_if_needed(log_path: &Path) -> Result<bool, String> {
    rotate_logs_if_needed_with_max(log_path, max_log_bytes())
}

fn rotate_logs_if_needed_with_max(log_path: &Path, max_bytes: u64) -> Result<bool, String> {
    let metadata = match fs::metadata(log_path) {
        Ok(value) => value,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(format!(
                "failed to inspect {}: {}",
                log_path.display(),
                error
            ))
        }
    };

    if metadata.len() < max_bytes {
        return Ok(false);
    }

    let archive_path = rotated_log_path(log_path);
    if archive_path.is_file() {
        remove_if_present(&archive_path)?;
    }
    fs::rename(log_path, &archive_path).map_err(|error| {
        format!(
            "failed to rotate {} to {}: {}",
            log_path.display(),
            archive_path.display(),
            error
        )
    })?;
    Ok(true)
}

fn rotated_log_path(log_path: &Path) -> PathBuf {
    let file_name = log_path
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| "events.log".to_string());
    log_path.with_file_name(format!("{}.1", file_name))
}

fn max_log_bytes() -> u64 {
    env::var("MCPACE_HUB_LOG_MAX_BYTES")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_MAX_LOG_BYTES)
}

#[cfg(test)]
mod tests {
    use super::rotate_logs_if_needed_with_max;
    use std::env;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new() -> Self {
            let unique = format!(
                "mcpace-runtime-test-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("system time")
                    .as_nanos()
            );
            let path = env::temp_dir().join(unique);
            fs::create_dir_all(&path).expect("create temp dir");
            TempDir { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn rotate_logs_archives_oversized_file_without_launching_runtime() {
        let temp = TempDir::new();
        let log_path = temp.path().join("events.log");
        fs::write(&log_path, "x".repeat(256)).expect("write log");

        let rotated = rotate_logs_if_needed_with_max(&log_path, 64).expect("rotate logs");

        assert!(rotated);
        assert!(temp.path().join("events.log.1").is_file());
        assert!(!temp.path().join("events.log").exists());
    }
}
