use super::render_models::ClientRestoreResult;
use super::sanitize_path_for_display;
use crate::json::JsonValue;
use crate::json_helpers;
use crate::runtimepaths;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::super::pathing::stable_hash_hex;

pub(super) struct ClientInstallBackup {
    pub(super) id: String,
    pub(super) path: PathBuf,
    pub(super) manifest_path: PathBuf,
    pub(super) restore_command: String,
}

pub(super) fn restore_client_install_backup(
    root_path: &Path,
    client_id: &str,
    selector: &str,
) -> Result<ClientRestoreResult, String> {
    let backup_path = resolve_backup_path(root_path, client_id, selector)?;
    let manifest_path = backup_path.join("manifest.json");
    let manifest = json_helpers::read_json_file(&manifest_path)?;
    let schema = json_helpers::string_at_path(&manifest, &["schema"]).unwrap_or("");
    if schema != "mcpace.clientInstallBackup.v1" {
        return Err(format!(
            "client install backup '{}' has unsupported schema '{}'",
            manifest_path.display(),
            schema
        ));
    }

    let backup_id = required_manifest_string(&manifest, &manifest_path, "backupId")?;
    let manifest_client_id = required_manifest_string(&manifest, &manifest_path, "clientTargetId")?;
    if manifest_client_id != client_id {
        return Err(format!(
            "client install backup '{}' belongs to '{}' not '{}'",
            manifest_path.display(),
            manifest_client_id,
            client_id
        ));
    }
    let config_path_text = required_manifest_string(&manifest, &manifest_path, "configPath")?;
    let expected_hash = required_manifest_string(&manifest, &manifest_path, "configPathHash")?;
    let actual_hash = stable_hash_hex(&config_path_text);
    if actual_hash != expected_hash {
        return Err(format!(
            "client install backup '{}' failed config path integrity check",
            manifest_path.display()
        ));
    }
    let config_existed =
        json_helpers::bool_at_path(&manifest, &["configExisted"]).ok_or_else(|| {
            format!(
                "client install backup '{}' is missing boolean field configExisted",
                manifest_path.display()
            )
        })?;
    let config_path = PathBuf::from(&config_path_text);

    let mut removed_config_file = false;
    let mut wrote_config_file = false;
    if config_existed {
        let content_path = backup_path.join("config.before");
        let contents = fs::read_to_string(&content_path).map_err(|error| {
            format!(
                "failed to read client install backup content '{}': {}",
                content_path.display(),
                error
            )
        })?;
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "failed to create restored client config directory '{}': {}",
                    parent.display(),
                    error
                )
            })?;
        }
        runtimepaths::write_text_atomic(&config_path, &contents)?;
        wrote_config_file = true;
    } else if config_path.is_file() {
        fs::remove_file(&config_path).map_err(|error| {
            format!(
                "failed to remove client config '{}' during restore: {}",
                config_path.display(),
                error
            )
        })?;
        removed_config_file = true;
    }

    Ok(ClientRestoreResult {
        client_target_id: client_id.to_string(),
        backup_id,
        backup_path: sanitize_path_for_display(&backup_path),
        config_path: sanitize_path_for_display(&config_path),
        restored_existing_config: config_existed,
        removed_config_file,
        wrote_config_file,
    })
}

fn resolve_backup_path(
    root_path: &Path,
    client_id: &str,
    selector: &str,
) -> Result<PathBuf, String> {
    let client_backup_root = install_backup_root(root_path).join(safe_file_segment(client_id));
    let normalized_selector = selector.trim();
    if normalized_selector.is_empty() || normalized_selector.eq_ignore_ascii_case("latest") {
        return latest_backup_path(&client_backup_root, client_id);
    }
    if !is_safe_backup_id(normalized_selector) {
        return Err(format!(
            "client restore backup selector '{}' is invalid; use a backup id or 'latest'",
            selector
        ));
    }
    let path = client_backup_root.join(normalized_selector);
    if !path.join("manifest.json").is_file() {
        return Err(format!(
            "client restore backup '{}' was not found for '{}'",
            normalized_selector, client_id
        ));
    }
    Ok(path)
}

fn latest_backup_path(client_backup_root: &Path, client_id: &str) -> Result<PathBuf, String> {
    let entries = fs::read_dir(client_backup_root).map_err(|error| {
        format!(
            "failed to read client install backups for '{}': {}",
            client_id, error
        )
    })?;
    let mut candidates = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            format!(
                "failed to inspect client install backups for '{}': {}",
                client_id, error
            )
        })?;
        let path = entry.path();
        if path.join("manifest.json").is_file() {
            candidates.push(path);
        }
    }
    candidates.sort_by(|left, right| left.file_name().cmp(&right.file_name()));
    candidates.pop().ok_or_else(|| {
        format!(
            "no client install backups found for '{}'; run client install first",
            client_id
        )
    })
}

fn required_manifest_string(
    manifest: &JsonValue,
    manifest_path: &Path,
    key: &str,
) -> Result<String, String> {
    json_helpers::string_at_path(manifest, &[key])
        .map(|value| value.to_string())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            format!(
                "client install backup '{}' is missing string field {}",
                manifest_path.display(),
                key
            )
        })
}

fn is_safe_backup_id(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
}

pub(super) fn install_backup_root(root_path: &Path) -> PathBuf {
    runtimepaths::resolve_state_root(root_path)
        .join("data")
        .join("client-install-backups")
}

pub(super) fn safe_file_segment(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    if sanitized.trim_matches(['.', '_', '-']).is_empty() {
        "unknown".to_string()
    } else {
        sanitized
    }
}

pub(super) fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}
