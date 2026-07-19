use super::*;
use crate::client::ClientIntegrationResult;
use crate::config_edit::{
    remove_json_mcp_server_entry, remove_toml_mcp_server_block, remove_yaml_mcp_server_entry,
};

pub(super) fn remove_owned_client_integrations(
    root_path: &Path,
    dry_run: bool,
) -> ClientIntegrationResult<JsonValue> {
    let registry =
        client_catalog::load_registry(Some(root_path)).map_err(|error| error.to_string())?;
    let mut removed = Vec::new();
    let mut skipped = Vec::new();
    let mut failed = Vec::new();

    for target in registry
        .targets
        .iter()
        .filter(|target| client_catalog::normalize(&target.surface_class) == "local")
    {
        if !target.supports_client_install() {
            continue;
        }
        let plan = match ClientInstallPlan::for_removal(root_path, target) {
            Ok(value) => value,
            Err(error) => {
                failed.push((target.id.clone(), error));
                continue;
            }
        };
        match plan.remove_owned_entry(dry_run) {
            Ok(result) => {
                let would_remove = result
                    .get("wouldRemove")
                    .and_then(JsonValue::as_bool)
                    .unwrap_or(false);
                if would_remove {
                    removed.push(result);
                } else {
                    skipped.push(result);
                }
            }
            Err(error) => failed.push((target.id.clone(), error)),
        }
    }

    Ok(JsonValue::object([
        ("schema", JsonValue::string("mcpace.clientRemoval.v1")),
        ("ok", JsonValue::bool(failed.is_empty())),
        ("dryRun", JsonValue::bool(dry_run)),
        ("removed", JsonValue::array(removed)),
        ("skipped", JsonValue::array(skipped)),
        (
            "failed",
            JsonValue::array(failed.into_iter().map(|(target, error)| {
                JsonValue::object([
                    ("clientTargetId", JsonValue::string(target)),
                    ("error", JsonValue::string(error.to_string())),
                ])
            })),
        ),
    ]))
}

impl ClientInstallPlan {
    fn for_removal(root_path: &Path, target: &ClientTarget) -> ClientIntegrationResult<Self> {
        let adapter_key_name =
            read_client_key_name(root_path).unwrap_or_else(|| "MCPace".to_string());
        let Some(install_support) = target.install_support() else {
            return Err(format!(
                "client '{}' does not have an MCPace-owned install surface",
                target.id
            )
            .into());
        };
        let config_path = resolve_install_path(
            platform_install_config_path(target)
                .unwrap_or(install_support.preferred_config_path.as_str()),
        )?;
        let server_url = local_mcp_url(root_path);
        Ok(Self {
            client_target_id: target.id.clone(),
            display_name: target.display_name.clone(),
            adapter_key_name,
            config_path,
            backup_root: install_backup_root(root_path),
            config_scope: install_support.preferred_scope.clone(),
            server_url: server_url.clone(),
            config: install_config_for_target(target, &server_url)?,
            warnings: Vec::new(),
        })
    }

    pub(super) fn remove_owned_entry(&self, dry_run: bool) -> ClientIntegrationResult<JsonValue> {
        if !self.config_path.is_file() {
            return Ok(self.removal_result(false, false, dry_run, None));
        }
        let _config_lock = if dry_run {
            None
        } else {
            Some(
                runtimepaths::acquire_exclusive_file_lock(
                    &self.config_path,
                    "client config removal",
                )
                .map_err(|error| error.to_string())?,
            )
        };
        let existing = fs::read_to_string(&self.config_path).map_err(|error| {
            format!(
                "failed to read client config '{}': {}",
                self.config_path.display(),
                error
            )
        })?;
        let update = match &self.config {
            ClientInstallConfig::TomlManagedTable => {
                remove_toml_mcp_server_block(&existing, &self.adapter_key_name, &self.config_path)
            }
            ClientInstallConfig::JsonMcpServers {
                servers_object_key, ..
            } => remove_json_mcp_server_entry(
                &existing,
                &self.adapter_key_name,
                servers_object_key,
                &self.server_url,
                &self.config_path,
            ),
            ClientInstallConfig::YamlMcpServers => {
                remove_yaml_mcp_server_entry(&existing, &self.adapter_key_name, &self.config_path)
            }
        }
        .map_err(|error| error.to_string())?;

        let would_remove = update.replaced_existing_block && update.contents != existing;
        if !would_remove {
            return Ok(self.removal_result(false, false, dry_run, None));
        }
        let backup = if dry_run {
            None
        } else {
            Some(self.create_backup(&existing, true)?)
        };
        if !dry_run {
            runtimepaths::write_text_atomic(&self.config_path, &update.contents)
                .map_err(|error| error.to_string())?;
        }
        Ok(self.removal_result(would_remove, !dry_run, dry_run, backup.as_ref()))
    }

    fn removal_result(
        &self,
        would_remove: bool,
        removed: bool,
        dry_run: bool,
        backup: Option<&ClientInstallBackup>,
    ) -> JsonValue {
        JsonValue::object([
            (
                "clientTargetId",
                JsonValue::string(self.client_target_id.clone()),
            ),
            ("displayName", JsonValue::string(self.display_name.clone())),
            (
                "configPath",
                JsonValue::string(sanitize_path_for_display(&self.config_path)),
            ),
            (
                "adapterKeyName",
                JsonValue::string(self.adapter_key_name.clone()),
            ),
            ("wouldRemove", JsonValue::bool(would_remove)),
            ("removed", JsonValue::bool(removed)),
            ("dryRun", JsonValue::bool(dry_run)),
            (
                "backupId",
                backup
                    .map(|value| JsonValue::string(value.id.clone()))
                    .unwrap_or(JsonValue::Null),
            ),
            (
                "backupPath",
                backup
                    .map(|value| JsonValue::string(sanitize_path_for_display(&value.path)))
                    .unwrap_or(JsonValue::Null),
            ),
        ])
    }
}
