use crate::json::JsonValue;
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CorruptedRuntimeFile {
    pub(super) path: String,
    pub(super) reason: String,
}

impl CorruptedRuntimeFile {
    fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("path", JsonValue::string(self.path.clone())),
            ("reason", JsonValue::string(self.reason.clone())),
        ])
    }
}

#[derive(Debug, Clone)]
pub(super) struct JsonFileDiagnostic {
    pub(super) path: PathBuf,
    pub(super) exists: bool,
    pub(super) value: Option<JsonValue>,
    pub(super) error: Option<String>,
}

impl JsonFileDiagnostic {
    pub(super) fn corruption(&self) -> Option<CorruptedRuntimeFile> {
        self.error.as_ref().map(|reason| CorruptedRuntimeFile {
            path: self.path.display().to_string(),
            reason: reason.clone(),
        })
    }
}

#[derive(Debug, Clone)]
pub(super) struct HubStatus {
    pub(super) root_path: String,
    pub(super) state_root: String,
    pub(super) runtime_dir: String,
    pub(super) hub_dir: String,
    pub(super) log_path: String,
    pub(super) lease_store_path: String,
    pub(super) config_version: Option<String>,
    pub(super) active_profile: String,
    pub(super) profile_selection_source: String,
    pub(super) status: String,
    pub(super) health: String,
    pub(super) pid: Option<u32>,
    pub(super) started_at_ms: Option<u128>,
    pub(super) last_heartbeat_at_ms: Option<u128>,
    pub(super) last_exit_at_ms: Option<u128>,
    pub(super) uptime_ms: Option<u128>,
    pub(super) ready_for_read_only_ops: bool,
    pub(super) ready_for_runtime_ops: bool,
    pub(super) server_count: usize,
    pub(super) required_server_count: usize,
    pub(super) profile_enabled_server_count: usize,
    pub(super) source_enabled_server_count: usize,
    pub(super) effective_enabled_server_count: usize,
    pub(super) missing_required_source_enablement: Vec<String>,
    pub(super) missing_profile_source_enablement: Vec<String>,
    pub(super) missing_required_commands: Vec<String>,
    pub(super) missing_profile_commands: Vec<String>,
    pub(super) warnings: Vec<String>,
    pub(super) corrupted_files: Vec<CorruptedRuntimeFile>,
    pub(super) repair_recommended: bool,
}

impl HubStatus {
    pub(super) fn to_json_value(&self) -> JsonValue {
        let mut map = BTreeMap::new();
        map.insert(
            "rootPath".to_string(),
            JsonValue::string(self.root_path.clone()),
        );
        map.insert(
            "stateRoot".to_string(),
            JsonValue::string(self.state_root.clone()),
        );
        map.insert(
            "runtimeDir".to_string(),
            JsonValue::string(self.runtime_dir.clone()),
        );
        map.insert(
            "hubDir".to_string(),
            JsonValue::string(self.hub_dir.clone()),
        );
        map.insert(
            "logPath".to_string(),
            JsonValue::string(self.log_path.clone()),
        );
        map.insert(
            "leaseStorePath".to_string(),
            JsonValue::string(self.lease_store_path.clone()),
        );
        match &self.config_version {
            Some(value) => {
                map.insert(
                    "configVersion".to_string(),
                    JsonValue::string(value.clone()),
                );
            }
            None => {
                map.insert("configVersion".to_string(), JsonValue::Null);
            }
        }
        map.insert(
            "activeProfile".to_string(),
            JsonValue::string(self.active_profile.clone()),
        );
        map.insert(
            "profileSelectionSource".to_string(),
            JsonValue::string(self.profile_selection_source.clone()),
        );
        map.insert("status".to_string(), JsonValue::string(self.status.clone()));
        map.insert("health".to_string(), JsonValue::string(self.health.clone()));
        match self.pid {
            Some(value) => {
                map.insert("pid".to_string(), JsonValue::number(value));
            }
            None => {
                map.insert("pid".to_string(), JsonValue::Null);
            }
        }
        insert_optional_number(&mut map, "startedAtMs", self.started_at_ms);
        insert_optional_number(&mut map, "lastHeartbeatAtMs", self.last_heartbeat_at_ms);
        insert_optional_number(&mut map, "lastExitAtMs", self.last_exit_at_ms);
        insert_optional_number(&mut map, "uptimeMs", self.uptime_ms);
        map.insert(
            "readyForReadOnlyOps".to_string(),
            JsonValue::bool(self.ready_for_read_only_ops),
        );
        map.insert(
            "readyForRuntimeOps".to_string(),
            JsonValue::bool(self.ready_for_runtime_ops),
        );
        map.insert(
            "serverCount".to_string(),
            JsonValue::number(self.server_count),
        );
        map.insert(
            "requiredServerCount".to_string(),
            JsonValue::number(self.required_server_count),
        );
        map.insert(
            "profileEnabledServerCount".to_string(),
            JsonValue::number(self.profile_enabled_server_count),
        );
        map.insert(
            "sourceEnabledServerCount".to_string(),
            JsonValue::number(self.source_enabled_server_count),
        );
        map.insert(
            "effectiveEnabledServerCount".to_string(),
            JsonValue::number(self.effective_enabled_server_count),
        );
        map.insert(
            "missingRequiredSourceEnablement".to_string(),
            JsonValue::array(
                self.missing_required_source_enablement
                    .iter()
                    .cloned()
                    .map(JsonValue::string),
            ),
        );
        map.insert(
            "missingProfileSourceEnablement".to_string(),
            JsonValue::array(
                self.missing_profile_source_enablement
                    .iter()
                    .cloned()
                    .map(JsonValue::string),
            ),
        );
        map.insert(
            "missingRequiredCommands".to_string(),
            JsonValue::array(
                self.missing_required_commands
                    .iter()
                    .cloned()
                    .map(JsonValue::string),
            ),
        );
        map.insert(
            "missingProfileCommands".to_string(),
            JsonValue::array(
                self.missing_profile_commands
                    .iter()
                    .cloned()
                    .map(JsonValue::string),
            ),
        );
        map.insert(
            "warnings".to_string(),
            JsonValue::array(self.warnings.iter().cloned().map(JsonValue::string)),
        );
        map.insert(
            "corruptedFiles".to_string(),
            JsonValue::array(
                self.corrupted_files
                    .iter()
                    .map(CorruptedRuntimeFile::to_json_value),
            ),
        );
        map.insert(
            "repairRecommended".to_string(),
            JsonValue::bool(self.repair_recommended),
        );
        JsonValue::Object(map)
    }
}

#[derive(Debug, Clone)]
pub(super) struct RepairReport {
    pub(super) root_path: String,
    pub(super) state_root: String,
    pub(super) archived_paths: Vec<String>,
    pub(super) recreated_paths: Vec<String>,
    pub(super) warnings: Vec<String>,
}

impl RepairReport {
    pub(super) fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("rootPath", JsonValue::string(self.root_path.clone())),
            ("stateRoot", JsonValue::string(self.state_root.clone())),
            (
                "archivedPaths",
                JsonValue::array(self.archived_paths.iter().cloned().map(JsonValue::string)),
            ),
            (
                "recreatedPaths",
                JsonValue::array(self.recreated_paths.iter().cloned().map(JsonValue::string)),
            ),
            (
                "warnings",
                JsonValue::array(self.warnings.iter().cloned().map(JsonValue::string)),
            ),
        ])
    }
}

pub(super) struct RuntimeLockGuard {
    pub(super) path: PathBuf,
}

impl Drop for RuntimeLockGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

pub(super) fn insert_optional_number(
    map: &mut BTreeMap<String, JsonValue>,
    key: &str,
    value: Option<u128>,
) {
    match value {
        Some(number) => {
            map.insert(key.to_string(), JsonValue::number(number));
        }
        None => {
            map.insert(key.to_string(), JsonValue::Null);
        }
    }
}
