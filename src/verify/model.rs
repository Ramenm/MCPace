use crate::doctor;
use crate::json::JsonValue;
use crate::profile;
use crate::server;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct ReadinessReport {
    pub root_path: Option<String>,
    pub config_version: Option<String>,
    pub active_profile: String,
    pub profile_selection_source: String,
    pub rust_source_ready: bool,
    pub npm_surface_ready: bool,
    pub runtime_prerequisites_ready: bool,
    pub container_tooling_ready: bool,
    pub server_count: usize,
    pub required_server_count: usize,
    pub profile_enabled_server_count: usize,
    pub required_source_enabled_count: usize,
    pub source_enabled_server_count: usize,
    pub effective_enabled_server_count: usize,
    pub missing_required_source_enablement: Vec<String>,
    pub missing_profile_source_enablement: Vec<String>,
    pub missing_required_commands: Vec<String>,
    pub missing_profile_commands: Vec<String>,
    pub ready_for_read_only_ops: bool,
    pub ready_for_runtime_ops: bool,
}

pub fn collect_readiness(root_path: &Path) -> Result<ReadinessReport, String> {
    let doctor_report = doctor::run(Some(root_path.to_path_buf()));
    let server_records = server::load_server_records(root_path)?;
    let runtime_profile = profile::load_runtime_profile_selection(root_path)?;
    Ok(build_readiness_report(
        &doctor_report,
        &runtime_profile,
        &server_records,
    ))
}

fn build_readiness_report(
    doctor_report: &doctor::Report,
    runtime_profile: &profile::RuntimeProfileSelection,
    server_records: &[server::ServerRecord],
) -> ReadinessReport {
    let required_servers = server_records
        .iter()
        .filter(|record| record.required)
        .collect::<Vec<_>>();
    let profile_enabled_servers = server_records
        .iter()
        .filter(|record| record.profile_enabled)
        .collect::<Vec<_>>();

    let required_source_enabled_count = required_servers
        .iter()
        .filter(|record| record.source_enabled)
        .count();
    let source_enabled_server_count = server_records
        .iter()
        .filter(|record| record.source_enabled)
        .count();
    let effective_enabled_server_count = server_records
        .iter()
        .filter(|record| record.effective_enabled)
        .count();

    let missing_required_source_enablement = sorted_unique(
        required_servers
            .iter()
            .filter(|record| !record.source_enabled)
            .map(|record| record.name.clone())
            .collect(),
    );
    let missing_profile_source_enablement = sorted_unique(
        profile_enabled_servers
            .iter()
            .filter(|record| !record.source_enabled)
            .map(|record| record.name.clone())
            .collect(),
    );

    let missing_required_commands = sorted_unique(
        required_servers
            .iter()
            .flat_map(|record| record.required_commands.iter())
            .filter(|command| !doctor::command_available(command))
            .cloned()
            .collect(),
    );
    let missing_profile_commands = sorted_unique(
        profile_enabled_servers
            .iter()
            .flat_map(|record| record.required_commands.iter())
            .filter(|command| !doctor::command_available(command))
            .cloned()
            .collect(),
    );

    let ready_for_read_only_ops = doctor_report.project.config_found;
    let ready_for_runtime_ops = ready_for_read_only_ops
        && doctor_report.project.runtime_prerequisites_ready
        && missing_required_source_enablement.is_empty()
        && missing_profile_source_enablement.is_empty()
        && missing_required_commands.is_empty()
        && missing_profile_commands.is_empty();

    ReadinessReport {
        root_path: doctor_report.project.root_path.clone(),
        config_version: doctor_report.project.config_version.clone(),
        active_profile: runtime_profile.active_profile.clone(),
        profile_selection_source: runtime_profile.selection_source.clone(),
        rust_source_ready: doctor_report.project.rust_source_ready,
        npm_surface_ready: doctor_report.project.npm_surface_ready,
        runtime_prerequisites_ready: doctor_report.project.runtime_prerequisites_ready,
        container_tooling_ready: doctor_report.project.container_tooling_ready,
        server_count: server_records.len(),
        required_server_count: required_servers.len(),
        profile_enabled_server_count: profile_enabled_servers.len(),
        required_source_enabled_count,
        source_enabled_server_count,
        effective_enabled_server_count,
        missing_required_source_enablement,
        missing_profile_source_enablement,
        missing_required_commands,
        missing_profile_commands,
        ready_for_read_only_ops,
        ready_for_runtime_ops,
    }
}

impl ReadinessReport {
    pub fn to_json_value(&self) -> JsonValue {
        let mut map = BTreeMap::new();
        match &self.root_path {
            Some(value) => {
                map.insert("rootPath".to_string(), JsonValue::string(value.clone()));
            }
            None => {
                map.insert("rootPath".to_string(), JsonValue::Null);
            }
        }
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
        map.insert(
            "rustSourceReady".to_string(),
            JsonValue::bool(self.rust_source_ready),
        );
        map.insert(
            "npmSurfaceReady".to_string(),
            JsonValue::bool(self.npm_surface_ready),
        );
        map.insert(
            "runtimePrerequisitesReady".to_string(),
            JsonValue::bool(self.runtime_prerequisites_ready),
        );
        map.insert(
            "containerToolingReady".to_string(),
            JsonValue::bool(self.container_tooling_ready),
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
            "requiredSourceEnabledCount".to_string(),
            JsonValue::number(self.required_source_enabled_count),
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
            "readyForReadOnlyOps".to_string(),
            JsonValue::bool(self.ready_for_read_only_ops),
        );
        map.insert(
            "readyForRuntimeOps".to_string(),
            JsonValue::bool(self.ready_for_runtime_ops),
        );
        JsonValue::Object(map)
    }
}

fn sorted_unique(mut values: Vec<String>) -> Vec<String> {
    values.sort();
    values.dedup();
    values
}
