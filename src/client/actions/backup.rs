use super::render_models::ClientRestoreResult;
use super::sanitize_path_for_display;
use crate::json::JsonValue;
use crate::json_helpers;
use crate::runtimepaths;
use crate::text_utils;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use super::super::pathing::stable_hash_hex;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ClientBackupError {
    InvalidSelector {
        selector: String,
    },
    BackupNotFound {
        selector: String,
        client_id: String,
    },
    BackupRead {
        client_id: String,
        reason: String,
    },
    BackupInspect {
        client_id: String,
        reason: String,
    },
    NoBackups {
        client_id: String,
    },
    ManifestRead {
        path: PathBuf,
        reason: String,
    },
    UnsupportedSchema {
        path: PathBuf,
        schema: String,
    },
    MissingStringField {
        path: PathBuf,
        key: String,
    },
    MissingBoolField {
        path: PathBuf,
        key: String,
    },
    WrongClient {
        path: PathBuf,
        expected: String,
        actual: String,
    },
    IntegrityCheckFailed {
        path: PathBuf,
    },
    ContentRead {
        path: PathBuf,
        reason: String,
    },
    DirectoryCreate {
        path: PathBuf,
        reason: String,
    },
    ConfigWrite {
        path: PathBuf,
        reason: String,
    },
    ConfigRemove {
        path: PathBuf,
        reason: String,
    },
}

pub(super) type ClientBackupResult<T> = Result<T, ClientBackupError>;

impl fmt::Display for ClientBackupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSelector { selector } => write!(
                formatter,
                "client restore backup selector '{}' is invalid; use a backup id or 'latest'",
                selector
            ),
            Self::BackupNotFound {
                selector,
                client_id,
            } => write!(
                formatter,
                "client restore backup '{}' was not found for '{}'",
                selector, client_id
            ),
            Self::BackupRead { client_id, reason } => write!(
                formatter,
                "failed to read client install backups for '{}': {}",
                client_id, reason
            ),
            Self::BackupInspect { client_id, reason } => write!(
                formatter,
                "failed to inspect client install backups for '{}': {}",
                client_id, reason
            ),
            Self::NoBackups { client_id } => write!(
                formatter,
                "no client install backups found for '{}'; run client install first",
                client_id
            ),
            Self::ManifestRead { path, reason } => write!(
                formatter,
                "failed to read client install backup manifest '{}': {}",
                path.display(),
                reason
            ),
            Self::UnsupportedSchema { path, schema } => write!(
                formatter,
                "client install backup '{}' has unsupported schema '{}'",
                path.display(),
                schema
            ),
            Self::MissingStringField { path, key } => write!(
                formatter,
                "client install backup '{}' is missing string field {}",
                path.display(),
                key
            ),
            Self::MissingBoolField { path, key } => write!(
                formatter,
                "client install backup '{}' is missing boolean field {}",
                path.display(),
                key
            ),
            Self::WrongClient {
                path,
                expected,
                actual,
            } => write!(
                formatter,
                "client install backup '{}' belongs to '{}' not '{}'",
                path.display(),
                actual,
                expected
            ),
            Self::IntegrityCheckFailed { path } => write!(
                formatter,
                "client install backup '{}' failed config path integrity check",
                path.display()
            ),
            Self::ContentRead { path, reason } => write!(
                formatter,
                "failed to read client install backup content '{}': {}",
                path.display(),
                reason
            ),
            Self::DirectoryCreate { path, reason } => write!(
                formatter,
                "failed to create restored client config directory '{}': {}",
                path.display(),
                reason
            ),
            Self::ConfigWrite { path, reason } => write!(
                formatter,
                "failed to write restored client config '{}': {}",
                path.display(),
                reason
            ),
            Self::ConfigRemove { path, reason } => write!(
                formatter,
                "failed to remove client config '{}' during restore: {}",
                path.display(),
                reason
            ),
        }
    }
}

impl std::error::Error for ClientBackupError {}

impl From<ClientBackupError> for String {
    fn from(error: ClientBackupError) -> Self {
        error.to_string()
    }
}

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
) -> ClientBackupResult<ClientRestoreResult> {
    let backup_path = resolve_backup_path(root_path, client_id, selector)?;
    let manifest_path = backup_path.join("manifest.json");
    let manifest = json_helpers::read_json_file(&manifest_path).map_err(|error| {
        ClientBackupError::ManifestRead {
            path: manifest_path.clone(),
            reason: error.to_string(),
        }
    })?;
    let schema = json_helpers::string_at_path(&manifest, &["schema"]).unwrap_or("");
    if schema != "mcpace.clientInstallBackup.v1" {
        return Err(ClientBackupError::UnsupportedSchema {
            path: manifest_path.clone(),
            schema: schema.to_string(),
        });
    }

    let backup_id = required_manifest_string(&manifest, &manifest_path, "backupId")?;
    let manifest_client_id = required_manifest_string(&manifest, &manifest_path, "clientTargetId")?;
    if manifest_client_id != client_id {
        return Err(ClientBackupError::WrongClient {
            path: manifest_path.clone(),
            expected: client_id.to_string(),
            actual: manifest_client_id,
        });
    }
    let config_path_text = required_manifest_string(&manifest, &manifest_path, "configPath")?;
    let expected_hash = required_manifest_string(&manifest, &manifest_path, "configPathHash")?;
    let actual_hash = stable_hash_hex(&config_path_text);
    if actual_hash != expected_hash {
        return Err(ClientBackupError::IntegrityCheckFailed {
            path: manifest_path.clone(),
        });
    }
    let config_existed =
        json_helpers::bool_at_path(&manifest, &["configExisted"]).ok_or_else(|| {
            ClientBackupError::MissingBoolField {
                path: manifest_path.clone(),
                key: "configExisted".to_string(),
            }
        })?;
    let config_path = PathBuf::from(&config_path_text);

    let mut removed_config_file = false;
    let mut wrote_config_file = false;
    if config_existed {
        let content_path = backup_path.join("config.before");
        let contents =
            fs::read_to_string(&content_path).map_err(|error| ClientBackupError::ContentRead {
                path: content_path.clone(),
                reason: error.to_string(),
            })?;
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent).map_err(|error| ClientBackupError::DirectoryCreate {
                path: parent.to_path_buf(),
                reason: error.to_string(),
            })?;
        }
        runtimepaths::write_text_atomic(&config_path, &contents).map_err(|error| {
            ClientBackupError::ConfigWrite {
                path: config_path.clone(),
                reason: error.to_string(),
            }
        })?;
        wrote_config_file = true;
    } else if config_path.is_file() {
        fs::remove_file(&config_path).map_err(|error| ClientBackupError::ConfigRemove {
            path: config_path.clone(),
            reason: error.to_string(),
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
) -> ClientBackupResult<PathBuf> {
    let client_backup_root = install_backup_root(root_path).join(safe_file_segment(client_id));
    let normalized_selector = selector.trim();
    if normalized_selector.is_empty() || normalized_selector.eq_ignore_ascii_case("latest") {
        return latest_backup_path(&client_backup_root, client_id);
    }
    if !text_utils::ascii_alnum_dash_underscore(normalized_selector) {
        return Err(ClientBackupError::InvalidSelector {
            selector: selector.to_string(),
        });
    }
    let path = client_backup_root.join(normalized_selector);
    if !path.join("manifest.json").is_file() {
        return Err(ClientBackupError::BackupNotFound {
            selector: normalized_selector.to_string(),
            client_id: client_id.to_string(),
        });
    }
    Ok(path)
}

fn latest_backup_path(client_backup_root: &Path, client_id: &str) -> ClientBackupResult<PathBuf> {
    let entries =
        fs::read_dir(client_backup_root).map_err(|error| ClientBackupError::BackupRead {
            client_id: client_id.to_string(),
            reason: error.to_string(),
        })?;
    let mut candidates = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| ClientBackupError::BackupInspect {
            client_id: client_id.to_string(),
            reason: error.to_string(),
        })?;
        let path = entry.path();
        if path.join("manifest.json").is_file() {
            candidates.push(path);
        }
    }
    candidates.sort_by(|left, right| left.file_name().cmp(&right.file_name()));
    candidates
        .pop()
        .ok_or_else(|| ClientBackupError::NoBackups {
            client_id: client_id.to_string(),
        })
}

fn required_manifest_string(
    manifest: &JsonValue,
    manifest_path: &Path,
    key: &str,
) -> ClientBackupResult<String> {
    json_helpers::string_at_path(manifest, &[key])
        .map(|value| value.to_string())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| ClientBackupError::MissingStringField {
            path: manifest_path.to_path_buf(),
            key: key.to_string(),
        })
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
    runtimepaths::unix_time_ms()
}
