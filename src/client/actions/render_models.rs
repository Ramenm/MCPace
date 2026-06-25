use super::{build_adapter_contract, resolve_export_mode, yes_no};
use crate::client::model::ClientPlan;
use crate::client::render::join_semicolon_or_none;
use crate::client_catalog::ClientTargetRecord as ClientTarget;
use crate::json::JsonValue;
use crate::runtimepaths;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

pub(super) struct ClientExportPreview {
    pub(super) client_target_id: String,
    pub(super) display_name: String,
    pub(super) adapter_key_name: String,
    pub(super) config_format: String,
    pub(super) config_paths: Vec<String>,
    pub(super) config_precedence: Vec<String>,
    pub(super) native_scopes: Vec<String>,
    pub(super) preferred_ingress: String,
    pub(super) export_mode: String,
    pub(super) entrypoint_mode: String,
    pub(super) launcher_command: String,
    pub(super) root_path: String,
    pub(super) mode: String,
    pub(super) can_connect_today: bool,
    pub(super) writes_config: bool,
    pub(super) recommended_install_scope: Option<String>,
    pub(super) recommended_install_path: Option<String>,
    pub(super) adapter_contract: AdapterContractPreview,
    pub(super) blockers: Vec<String>,
    pub(super) warnings: Vec<String>,
    pub(super) next_actions: Vec<String>,
}

pub(super) struct AdapterContractPreview {
    pub(super) kind: String,
    pub(super) command: Option<String>,
    pub(super) args: Vec<String>,
    pub(super) url_template: Option<String>,
    pub(super) metadata_carrier: String,
    pub(super) session_model: String,
    pub(super) notes: Vec<String>,
}

pub(super) struct ClientInstallResult {
    pub(super) client_target_id: String,
    pub(super) display_name: String,
    pub(super) adapter_key_name: String,
    pub(super) config_path: String,
    pub(super) config_scope: String,
    pub(super) transport: String,
    pub(super) server_url: String,
    pub(super) changed: bool,
    pub(super) would_change: bool,
    pub(super) dry_run: bool,
    pub(super) diff_requested: bool,
    pub(super) diff: Option<String>,
    pub(super) persisted: bool,
    pub(super) backup_created: bool,
    pub(super) backup_id: Option<String>,
    pub(super) backup_path: Option<String>,
    pub(super) backup_manifest_path: Option<String>,
    pub(super) restore_command: Option<String>,
    pub(super) replaced_existing_block: bool,
    pub(super) created_config_dir: bool,
    pub(super) created_config_file: bool,
    pub(super) would_create_config_dir: bool,
    pub(super) would_create_config_file: bool,
    pub(super) warnings: Vec<String>,
}

pub(super) struct ClientRestoreResult {
    pub(super) client_target_id: String,
    pub(super) backup_id: String,
    pub(super) backup_path: String,
    pub(super) config_path: String,
    pub(super) restored_existing_config: bool,
    pub(super) removed_config_file: bool,
    pub(super) wrote_config_file: bool,
}

impl ClientExportPreview {
    pub(super) fn from_plan(target: &ClientTarget, plan: &ClientPlan) -> Self {
        let adapter_key_name = plan
            .configured_client_key_name
            .clone()
            .unwrap_or_else(|| format!("{}-adapter", target.family_id));
        let export_mode = resolve_export_mode(target, plan);
        let recommended_install_scope = target
            .preferred_install_scope()
            .map(|value| value.to_string());
        let recommended_install_path = target
            .preferred_install_config_path()
            .map(|value| value.to_string());
        let configured_local_mcp_url = runtimepaths::configured_mcp_url(Path::new(&plan.root_path));

        let blockers = match export_mode.as_str() {
            "local-stdio-launcher" | "local-streamable-http" => Vec::new(),
            "public-http-connector" => vec![
                "This client surface needs a public HTTP MCP endpoint or relay, and MCPace does not ship that lane yet in this repo.".to_string(),
            ],
            _ => vec![
                "MCPace does not yet have a verified ingress lane for this client surface, so export stays preview-only.".to_string(),
            ],
        };

        let next_actions = match export_mode.as_str() {
            "local-streamable-http" if target.supports_client_install() => {
                let scope = recommended_install_scope
                    .as_deref()
                    .unwrap_or("recommended");
                let path = recommended_install_path
                    .as_deref()
                    .unwrap_or("documented install path");
                vec![
                format!(
                    "Run 'mcpace client install {} --root <path>' to patch the MCPace entry in {} automatically using the shared {} scope.",
                    target.id,
                    path,
                    scope
                ),
                format!(
                    "MCPace defaults to the shared {} scope for {} so one localhost MCPace server can be reused across projects.",
                    scope,
                    target.display_name
                ),
                format!(
                    "Keep one MCPace server running at {} so {} always points at the same resolved MCPace URL.",
                    configured_local_mcp_url,
                    target.display_name
                ),
            ]
            }
            "local-streamable-http" => vec![
                format!(
                    "Run 'mcpace serve' and point this client at the resolved MCPace URL: {}.",
                    configured_local_mcp_url
                ),
                "Keep export as the source of truth for the HTTP MCPace contract until that client gets a dedicated config patcher.".to_string(),
            ],
            "local-stdio-launcher" => vec![
                "This client surface still needs the stdio MCPace fallback today.".to_string(),
                "Keep stdio surfaces as internal compatibility lanes while HTTP-first clients move to the one-port MCPace server.".to_string(),
            ],
            "public-http-connector" => vec![
                "Ship a public HTTP / relay lane for cloud connectors before claiming this surface works through MCPace.".to_string(),
                "Add credential bootstrap and real hosted compatibility traces for this connector class.".to_string(),
            ],
            _ => vec![
                "Add a verified ingress lane for this client surface before turning preview output into a real config patch.".to_string(),
            ],
        };

        ClientExportPreview {
            client_target_id: target.id.clone(),
            display_name: target.display_name.clone(),
            adapter_key_name,
            config_format: target.config_format.clone(),
            config_paths: target.config_paths.clone(),
            config_precedence: target.config_precedence.clone(),
            native_scopes: target.native_scopes.clone(),
            preferred_ingress: plan.preferred_ingress.clone(),
            export_mode: export_mode.clone(),
            entrypoint_mode: plan.entrypoint_mode.clone(),
            launcher_command: plan.launcher_command.clone(),
            root_path: plan.root_path.clone(),
            mode: if matches!(
                export_mode.as_str(),
                "local-stdio-launcher" | "local-streamable-http"
            ) {
                "connectable-preview".to_string()
            } else {
                "preview-only".to_string()
            },
            can_connect_today: matches!(
                export_mode.as_str(),
                "local-stdio-launcher" | "local-streamable-http"
            ),
            writes_config: false,
            recommended_install_scope,
            recommended_install_path,
            adapter_contract: build_adapter_contract(target, plan, &export_mode),
            blockers,
            warnings: export_warnings_from_plan(plan),
            next_actions,
        }
    }

    pub(super) fn to_json_value(&self) -> JsonValue {
        let mut map = BTreeMap::new();
        map.insert("mode".to_string(), JsonValue::string(self.mode.clone()));
        map.insert(
            "clientTargetId".to_string(),
            JsonValue::string(self.client_target_id.clone()),
        );
        map.insert(
            "displayName".to_string(),
            JsonValue::string(self.display_name.clone()),
        );
        map.insert(
            "adapterKeyName".to_string(),
            JsonValue::string(self.adapter_key_name.clone()),
        );
        map.insert(
            "configFormat".to_string(),
            JsonValue::string(self.config_format.clone()),
        );
        map.insert(
            "configPaths".to_string(),
            JsonValue::array(self.config_paths.iter().cloned().map(JsonValue::string)),
        );
        map.insert(
            "configPrecedence".to_string(),
            JsonValue::array(
                self.config_precedence
                    .iter()
                    .cloned()
                    .map(JsonValue::string),
            ),
        );
        map.insert(
            "nativeScopes".to_string(),
            JsonValue::array(self.native_scopes.iter().cloned().map(JsonValue::string)),
        );
        map.insert(
            "preferredIngress".to_string(),
            JsonValue::string(self.preferred_ingress.clone()),
        );
        map.insert(
            "exportMode".to_string(),
            JsonValue::string(self.export_mode.clone()),
        );
        map.insert(
            "entrypointMode".to_string(),
            JsonValue::string(self.entrypoint_mode.clone()),
        );
        map.insert(
            "launcherCommand".to_string(),
            JsonValue::string(self.launcher_command.clone()),
        );
        map.insert(
            "rootPath".to_string(),
            JsonValue::string(self.root_path.clone()),
        );
        map.insert(
            "canConnectToday".to_string(),
            JsonValue::bool(self.can_connect_today),
        );
        map.insert(
            "writesConfig".to_string(),
            JsonValue::bool(self.writes_config),
        );
        match &self.recommended_install_scope {
            Some(value) => {
                map.insert(
                    "recommendedInstallScope".to_string(),
                    JsonValue::string(value.clone()),
                );
            }
            None => {
                map.insert("recommendedInstallScope".to_string(), JsonValue::Null);
            }
        }
        match &self.recommended_install_path {
            Some(value) => {
                map.insert(
                    "recommendedInstallPath".to_string(),
                    JsonValue::string(value.clone()),
                );
            }
            None => {
                map.insert("recommendedInstallPath".to_string(), JsonValue::Null);
            }
        }
        map.insert(
            "adapterContract".to_string(),
            self.adapter_contract.to_json_value(),
        );
        map.insert(
            "blockers".to_string(),
            JsonValue::array(self.blockers.iter().cloned().map(JsonValue::string)),
        );
        map.insert(
            "warnings".to_string(),
            JsonValue::array(self.warnings.iter().cloned().map(JsonValue::string)),
        );
        map.insert(
            "nextActions".to_string(),
            JsonValue::array(self.next_actions.iter().cloned().map(JsonValue::string)),
        );
        JsonValue::Object(map)
    }

    pub(super) fn write_text(&self, stdout: &mut dyn Write) {
        let _ = writeln!(
            stdout,
            "Client export {}",
            if self.can_connect_today {
                "connectable preview"
            } else {
                "preview only"
            }
        );
        let _ = writeln!(
            stdout,
            "Client target: {} ({})",
            self.client_target_id, self.display_name
        );
        let _ = writeln!(stdout, "Adapter key: {}", self.adapter_key_name);
        let _ = writeln!(stdout, "Config format: {}", self.config_format);
        let _ = writeln!(
            stdout,
            "Config paths: {}",
            join_semicolon_or_none(&self.config_paths)
        );
        let _ = writeln!(
            stdout,
            "Config precedence: {}",
            join_semicolon_or_none(&self.config_precedence)
        );
        let _ = writeln!(
            stdout,
            "Native scopes: {}",
            join_semicolon_or_none(&self.native_scopes)
        );
        let _ = writeln!(stdout, "Preferred ingress: {}", self.preferred_ingress);
        let _ = writeln!(stdout, "Export mode: {}", self.export_mode);
        let _ = writeln!(stdout, "Entrypoint mode: {}", self.entrypoint_mode);
        let _ = writeln!(stdout, "Launcher command: {}", self.launcher_command);
        let _ = writeln!(
            stdout,
            "Can connect today: {}",
            yes_no(self.can_connect_today)
        );
        let _ = writeln!(stdout, "Writes config: {}", yes_no(self.writes_config));
        let _ = writeln!(
            stdout,
            "Recommended install scope: {}",
            self.recommended_install_scope.as_deref().unwrap_or("none")
        );
        let _ = writeln!(
            stdout,
            "Recommended install path: {}",
            self.recommended_install_path.as_deref().unwrap_or("none")
        );
        self.adapter_contract.write_text(stdout);
        let _ = writeln!(
            stdout,
            "Blockers: {}",
            join_semicolon_or_none(&self.blockers)
        );
        let _ = writeln!(
            stdout,
            "Warnings: {}",
            join_semicolon_or_none(&self.warnings)
        );
        let _ = writeln!(
            stdout,
            "Next actions: {}",
            join_semicolon_or_none(&self.next_actions)
        );
    }
}

fn export_warnings_from_plan(plan: &ClientPlan) -> Vec<String> {
    let mut warnings = plan
        .warnings
        .iter()
        .filter(|warning| export_warning_is_user_actionable(warning))
        .cloned()
        .collect::<Vec<_>>();
    warnings.sort();
    warnings.dedup();
    warnings
}

fn export_warning_is_user_actionable(warning: &str) -> bool {
    warning.contains("At least one routed server uses stdio")
        || warning.contains("At least one server is single-session")
        || warning.contains("Client surface")
        || warning.contains("No external session")
        || warning.contains("Streamable HTTP is available")
        || warning.contains("public HTTP")
        || warning.contains("cannot consume MCPace")
}

impl ClientInstallResult {
    pub(super) fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            (
                "mode",
                JsonValue::string(if self.dry_run {
                    "install-preview"
                } else {
                    "installed"
                }),
            ),
            (
                "clientTargetId",
                JsonValue::string(self.client_target_id.clone()),
            ),
            ("displayName", JsonValue::string(self.display_name.clone())),
            (
                "adapterKeyName",
                JsonValue::string(self.adapter_key_name.clone()),
            ),
            ("configPath", JsonValue::string(self.config_path.clone())),
            ("configScope", JsonValue::string(self.config_scope.clone())),
            ("transport", JsonValue::string(self.transport.clone())),
            ("url", JsonValue::string(self.server_url.clone())),
            ("writesConfig", JsonValue::bool(!self.dry_run)),
            ("dryRun", JsonValue::bool(self.dry_run)),
            ("persisted", JsonValue::bool(self.persisted)),
            ("backupCreated", JsonValue::bool(self.backup_created)),
            (
                "backupId",
                self.backup_id
                    .as_ref()
                    .map(|value| JsonValue::string(value.clone()))
                    .unwrap_or(JsonValue::Null),
            ),
            (
                "backupPath",
                self.backup_path
                    .as_ref()
                    .map(|value| JsonValue::string(value.clone()))
                    .unwrap_or(JsonValue::Null),
            ),
            (
                "backupManifestPath",
                self.backup_manifest_path
                    .as_ref()
                    .map(|value| JsonValue::string(value.clone()))
                    .unwrap_or(JsonValue::Null),
            ),
            (
                "restoreCommand",
                self.restore_command
                    .as_ref()
                    .map(|value| JsonValue::string(value.clone()))
                    .unwrap_or(JsonValue::Null),
            ),
            ("changed", JsonValue::bool(self.changed)),
            ("wouldChange", JsonValue::bool(self.would_change)),
            ("diffRequested", JsonValue::bool(self.diff_requested)),
            (
                "diff",
                self.diff
                    .as_ref()
                    .map(|value| JsonValue::string(value.clone()))
                    .unwrap_or(JsonValue::Null),
            ),
            (
                "replacedExistingBlock",
                JsonValue::bool(self.replaced_existing_block),
            ),
            ("createdConfigDir", JsonValue::bool(self.created_config_dir)),
            (
                "createdConfigFile",
                JsonValue::bool(self.created_config_file),
            ),
            (
                "wouldCreateConfigDir",
                JsonValue::bool(self.would_create_config_dir),
            ),
            (
                "wouldCreateConfigFile",
                JsonValue::bool(self.would_create_config_file),
            ),
            (
                "warnings",
                JsonValue::array(self.warnings.iter().cloned().map(JsonValue::string)),
            ),
        ])
    }

    pub(super) fn write_text(&self, stdout: &mut dyn Write) {
        let _ = writeln!(
            stdout,
            "{}",
            if self.dry_run {
                "Client install dry-run complete"
            } else {
                "Client install complete"
            }
        );
        let _ = writeln!(
            stdout,
            "Client target: {} ({})",
            self.client_target_id, self.display_name
        );
        let _ = writeln!(stdout, "Adapter key: {}", self.adapter_key_name);
        let _ = writeln!(stdout, "Config path: {}", self.config_path);
        let _ = writeln!(stdout, "Config scope: {}", self.config_scope);
        let _ = writeln!(stdout, "Transport: {}", self.transport);
        let _ = writeln!(stdout, "URL: {}", self.server_url);
        let _ = writeln!(stdout, "Changed config: {}", yes_no(self.changed));
        let _ = writeln!(stdout, "Would change config: {}", yes_no(self.would_change));
        let _ = writeln!(stdout, "Persisted: {}", yes_no(self.persisted));
        let _ = writeln!(stdout, "Backup created: {}", yes_no(self.backup_created));
        if let Some(backup_path) = &self.backup_path {
            let _ = writeln!(stdout, "Backup path: {}", backup_path);
        }
        if let Some(restore_command) = &self.restore_command {
            let _ = writeln!(stdout, "Restore command: {}", restore_command);
        }
        let _ = writeln!(
            stdout,
            "Replaced existing block: {}",
            yes_no(self.replaced_existing_block)
        );
        let _ = writeln!(
            stdout,
            "Created config directory: {}",
            yes_no(self.created_config_dir)
        );
        let _ = writeln!(
            stdout,
            "Created config file: {}",
            yes_no(self.created_config_file)
        );
        let _ = writeln!(
            stdout,
            "Would create config directory: {}",
            yes_no(self.would_create_config_dir)
        );
        let _ = writeln!(
            stdout,
            "Would create config file: {}",
            yes_no(self.would_create_config_file)
        );
        if let Some(diff) = &self.diff {
            if !diff.is_empty() {
                let _ = writeln!(stdout, "Diff:\n{}", diff);
            }
        }
        let _ = writeln!(
            stdout,
            "Warnings: {}",
            join_semicolon_or_none(&self.warnings)
        );
    }
}

impl ClientRestoreResult {
    pub(super) fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("mode", JsonValue::string("restored")),
            (
                "clientTargetId",
                JsonValue::string(self.client_target_id.clone()),
            ),
            ("backupId", JsonValue::string(self.backup_id.clone())),
            ("backupPath", JsonValue::string(self.backup_path.clone())),
            ("configPath", JsonValue::string(self.config_path.clone())),
            (
                "restoredExistingConfig",
                JsonValue::bool(self.restored_existing_config),
            ),
            (
                "removedConfigFile",
                JsonValue::bool(self.removed_config_file),
            ),
            ("wroteConfigFile", JsonValue::bool(self.wrote_config_file)),
        ])
    }

    pub(super) fn write_text(&self, stdout: &mut dyn Write) {
        let _ = writeln!(stdout, "Client config restore complete");
        let _ = writeln!(stdout, "Client target: {}", self.client_target_id);
        let _ = writeln!(stdout, "Backup id: {}", self.backup_id);
        let _ = writeln!(stdout, "Backup path: {}", self.backup_path);
        let _ = writeln!(stdout, "Config path: {}", self.config_path);
        let _ = writeln!(
            stdout,
            "Restored existing config: {}",
            yes_no(self.restored_existing_config)
        );
        let _ = writeln!(
            stdout,
            "Removed config file: {}",
            yes_no(self.removed_config_file)
        );
        let _ = writeln!(
            stdout,
            "Wrote config file: {}",
            yes_no(self.wrote_config_file)
        );
    }
}

impl AdapterContractPreview {
    pub(super) fn to_json_value(&self) -> JsonValue {
        let mut map = BTreeMap::new();
        map.insert("type".to_string(), JsonValue::string(self.kind.clone()));
        match &self.command {
            Some(value) => {
                map.insert("command".to_string(), JsonValue::string(value.clone()));
            }
            None => {
                map.insert("command".to_string(), JsonValue::Null);
            }
        }
        map.insert(
            "args".to_string(),
            JsonValue::array(self.args.iter().cloned().map(JsonValue::string)),
        );
        match &self.url_template {
            Some(value) => {
                map.insert("urlTemplate".to_string(), JsonValue::string(value.clone()));
            }
            None => {
                map.insert("urlTemplate".to_string(), JsonValue::Null);
            }
        }
        map.insert(
            "metadataCarrier".to_string(),
            JsonValue::string(self.metadata_carrier.clone()),
        );
        map.insert(
            "sessionModel".to_string(),
            JsonValue::string(self.session_model.clone()),
        );
        map.insert(
            "notes".to_string(),
            JsonValue::array(self.notes.iter().cloned().map(JsonValue::string)),
        );
        JsonValue::Object(map)
    }

    pub(super) fn write_text(&self, stdout: &mut dyn Write) {
        let _ = writeln!(stdout, "Adapter contract type: {}", self.kind);
        let _ = writeln!(
            stdout,
            "Adapter command: {}",
            self.command.as_deref().unwrap_or("none")
        );
        let _ = writeln!(
            stdout,
            "Adapter args: {}",
            join_semicolon_or_none(&self.args)
        );
        let _ = writeln!(
            stdout,
            "Adapter URL template: {}",
            self.url_template.as_deref().unwrap_or("none")
        );
        let _ = writeln!(stdout, "Metadata carrier: {}", self.metadata_carrier);
        let _ = writeln!(stdout, "Session model: {}", self.session_model);
        let _ = writeln!(
            stdout,
            "Adapter notes: {}",
            join_semicolon_or_none(&self.notes)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::export_warning_is_user_actionable;

    #[test]
    fn export_warnings_keep_client_actions_and_hide_per_server_plan_noise() {
        assert!(export_warning_is_user_actionable(
            "At least one routed server uses stdio; the hub must own the child process."
        ));
        assert!(export_warning_is_user_actionable(
            "No external session id was resolved; the plan derived an internal session lease."
        ));
        assert!(export_warning_is_user_actionable(
            "Client surface 'windsurf' has a documented enabled-tool budget of 100."
        ));
        assert!(export_warning_is_user_actionable(
            "Streamable HTTP is available through the one-port local MCPace server."
        ));

        assert!(!export_warning_is_user_actionable(
            "filesystem is disabled or plan-only; MCPace must not route tool calls to it."
        ));
        assert!(!export_warning_is_user_actionable(
            "browser has unknown scopeClass 'configured-source'; treating it as lease-local."
        ));
        assert!(!export_warning_is_user_actionable(
            "fetch is credential-scoped but no credential profile id was resolved."
        ));
    }
}
