use crate::json::JsonValue;
use crate::json_helpers;
use crate::runtimepaths;
use std::collections::BTreeMap;
use std::fmt;
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};

mod import;
mod paths;
mod write;
mod write_helpers;

pub use self::import::{import_mcp_server_entries, McpServerImportOptions, McpServerImportResult};
use self::paths::collect_source_paths;
pub use self::write::{
    default_mcp_server_fragment_path, remove_mcp_server_entry, set_mcp_server_enabled,
    write_mcp_server_entry, McpServerRemoveOptions, McpServerRemoveResult, McpServerToggleOptions,
    McpServerToggleResult, McpServerWriteOptions, McpServerWriteResult,
};

const DEFAULT_SETTINGS_FILE: &str = "mcp_settings.json";
const DEFAULT_SETTINGS_DIR: &str = "mcp_settings.d";
const ENV_MCP_SETTINGS: &str = "MCPACE_MCP_SETTINGS";
const ENV_MCP_SETTINGS_DIRS: &str = "MCPACE_MCP_SETTINGS_DIRS";
const MAX_MCP_SETTINGS_FINGERPRINT_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct McpServerEntry {
    pub name: String,
    pub normalized_name: String,
    pub value: JsonValue,
    pub source: String,
}

#[derive(Clone, Debug, Default)]
pub struct McpServerRegistry {
    pub servers: BTreeMap<String, McpServerEntry>,
    pub sources: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct McpSourceStatus {
    pub path: String,
    pub origin: String,
    pub exists: bool,
    pub server_count: usize,
}

#[derive(Clone, Debug)]
pub struct McpSourceReport {
    pub registry: McpServerRegistry,
    pub source_statuses: Vec<McpSourceStatus>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum McpSourceError {
    UnsafeSource { path: PathBuf, reason: &'static str },
    InspectSource { path: PathBuf, reason: String },
    RuntimePath(runtimepaths::RuntimePathError),
    Upstream(String),
}

pub type McpSourceResult<T> = Result<T, McpSourceError>;

impl fmt::Display for McpSourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsafeSource { path, reason } => {
                write!(
                    formatter,
                    "MCP settings source '{}' {}; skipping",
                    path.display(),
                    reason
                )
            }
            Self::InspectSource { path, reason } => {
                write!(
                    formatter,
                    "failed to inspect MCP settings source '{}': {}; skipping",
                    path.display(),
                    reason
                )
            }
            Self::RuntimePath(error) => write!(formatter, "{}", error),
            Self::Upstream(error) => write!(formatter, "{}", error),
        }
    }
}

impl std::error::Error for McpSourceError {}

impl From<runtimepaths::RuntimePathError> for McpSourceError {
    fn from(error: runtimepaths::RuntimePathError) -> Self {
        Self::RuntimePath(error)
    }
}

impl From<String> for McpSourceError {
    fn from(error: String) -> Self {
        Self::Upstream(error)
    }
}

impl From<McpSourceError> for String {
    fn from(error: McpSourceError) -> Self {
        error.to_string()
    }
}

pub fn load_mcp_server_registry(root_path: &Path) -> McpSourceResult<McpServerRegistry> {
    let mut warnings = Vec::new();
    let sources = collect_source_paths(root_path, &mut warnings);
    let mut registry = McpServerRegistry {
        servers: BTreeMap::new(),
        sources: Vec::new(),
        warnings,
    };

    for source in sources {
        match settings_source_is_regular_file(&source.path) {
            Ok(true) => {}
            Ok(false) => {
                registry.warnings.push(format!(
                    "MCP settings source '{}' does not exist; skipping",
                    source.path.display()
                ));
                continue;
            }
            Err(error) => {
                registry.warnings.push(error.to_string());
                continue;
            }
        }
        let value = match json_helpers::read_json_file(&source.path) {
            Ok(value) => value,
            Err(error) => {
                registry.warnings.push(format!(
                    "failed to read MCP settings source '{}': {}; skipping",
                    source.path.display(),
                    error
                ));
                continue;
            }
        };
        registry.sources.push(source.path.display().to_string());
        let Some(servers) = json_helpers::mcp_servers_object(&value) else {
            registry.warnings.push(format!(
                "MCP settings source '{}' has no mcpServers or servers object; skipping",
                source.path.display()
            ));
            continue;
        };
        for (name, server) in servers {
            let normalized_name = normalize_server_name(name);
            if normalized_name.is_empty() {
                registry.warnings.push(format!(
                    "MCP settings source '{}' contains an empty server name; skipping",
                    source.path.display()
                ));
                continue;
            }
            if let Some(previous) = registry.servers.insert(
                normalized_name.clone(),
                McpServerEntry {
                    name: name.clone(),
                    normalized_name: normalized_name.clone(),
                    value: server.clone(),
                    source: source.path.display().to_string(),
                },
            ) {
                registry.warnings.push(format!(
                    "duplicate MCP server '{}' from '{}' overrides earlier source '{}'",
                    name,
                    source.path.display(),
                    previous.source
                ));
            }
        }
    }

    registry.sources.sort();
    registry.sources.dedup();
    registry.warnings.sort();
    registry.warnings.dedup();
    Ok(registry)
}

pub fn load_mcp_source_report(root_path: &Path) -> McpSourceResult<McpSourceReport> {
    let mut warnings = Vec::new();
    let sources = collect_source_paths(root_path, &mut warnings);
    let mut registry = McpServerRegistry {
        servers: BTreeMap::new(),
        sources: Vec::new(),
        warnings,
    };
    let mut source_statuses = Vec::new();

    for source in sources {
        match settings_source_is_regular_file(&source.path) {
            Ok(true) => {}
            Ok(false) => {
                registry.warnings.push(format!(
                    "MCP settings source '{}' does not exist; skipping",
                    source.path.display()
                ));
                source_statuses.push(McpSourceStatus {
                    path: source.path.display().to_string(),
                    origin: source.origin,
                    exists: false,
                    server_count: 0,
                });
                continue;
            }
            Err(error) => {
                registry.warnings.push(error.to_string());
                source_statuses.push(McpSourceStatus {
                    path: source.path.display().to_string(),
                    origin: source.origin,
                    exists: true,
                    server_count: 0,
                });
                continue;
            }
        }
        let value = match json_helpers::read_json_file(&source.path) {
            Ok(value) => value,
            Err(error) => {
                registry.warnings.push(format!(
                    "failed to read MCP settings source '{}': {}; skipping",
                    source.path.display(),
                    error
                ));
                source_statuses.push(McpSourceStatus {
                    path: source.path.display().to_string(),
                    origin: source.origin,
                    exists: true,
                    server_count: 0,
                });
                continue;
            }
        };
        registry.sources.push(source.path.display().to_string());
        let mut source_server_count = 0usize;
        if let Some(servers) = json_helpers::mcp_servers_object(&value) {
            source_server_count = servers.len();
            for (name, server) in servers {
                let normalized_name = normalize_server_name(name);
                if normalized_name.is_empty() {
                    registry.warnings.push(format!(
                        "MCP settings source '{}' contains an empty server name; skipping",
                        source.path.display()
                    ));
                    continue;
                }
                if let Some(previous) = registry.servers.insert(
                    normalized_name.clone(),
                    McpServerEntry {
                        name: name.clone(),
                        normalized_name: normalized_name.clone(),
                        value: server.clone(),
                        source: source.path.display().to_string(),
                    },
                ) {
                    registry.warnings.push(format!(
                        "duplicate MCP server '{}' from '{}' overrides earlier source '{}'",
                        name,
                        source.path.display(),
                        previous.source
                    ));
                }
            }
        } else {
            registry.warnings.push(format!(
                "MCP settings source '{}' has no mcpServers or servers object; skipping",
                source.path.display()
            ));
        }
        source_statuses.push(McpSourceStatus {
            path: source.path.display().to_string(),
            origin: source.origin,
            exists: true,
            server_count: source_server_count,
        });
    }

    registry.sources.sort();
    registry.sources.dedup();
    registry.warnings.sort();
    registry.warnings.dedup();
    source_statuses.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(McpSourceReport {
        registry,
        source_statuses,
    })
}

fn settings_source_is_regular_file(path: &Path) -> McpSourceResult<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(McpSourceError::UnsafeSource {
            path: path.to_path_buf(),
            reason: "is a symlink",
        }),
        Ok(metadata) if !metadata.is_file() => Err(McpSourceError::UnsafeSource {
            path: path.to_path_buf(),
            reason: "is not a regular file",
        }),
        Ok(_) => Ok(true),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(McpSourceError::InspectSource {
            path: path.to_path_buf(),
            reason: error.to_string(),
        }),
    }
}

pub(crate) fn acquire_mcp_settings_namespace_lock(
    root_path: &Path,
) -> McpSourceResult<runtimepaths::ExclusiveFileLockGuard> {
    let runtime_dir = runtimepaths::ensure_runtime_dir(root_path)?;
    Ok(runtimepaths::acquire_exclusive_file_lock(
        &runtime_dir.join("mcp-settings.namespace"),
        "MCP settings namespace update",
    )?)
}

pub(crate) fn source_paths_for_normalized_server(
    root_path: &Path,
    normalized_name: &str,
) -> McpSourceResult<Vec<PathBuf>> {
    let mut warnings = Vec::new();
    let sources = collect_source_paths(root_path, &mut warnings);
    let mut matches = Vec::new();
    for source in sources {
        if settings_source_is_regular_file(&source.path).ok() != Some(true) {
            continue;
        }
        let value = match json_helpers::read_json_file(&source.path) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let Some(servers) = json_helpers::mcp_servers_object(&value) else {
            continue;
        };
        if servers
            .keys()
            .any(|name| normalize_server_name(name) == normalized_name)
        {
            matches.push(source.path.clone());
        }
    }
    matches.sort();
    matches.dedup();
    Ok(matches)
}

pub fn mcp_settings_fingerprint(root_path: &Path) -> (u128, u64) {
    let mut warnings = Vec::new();
    let sources = collect_source_paths(root_path, &mut warnings);
    let mut fingerprint = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58du128;
    let mut len = 0u64;
    for source in sources {
        feed_settings_fingerprint(
            &mut fingerprint,
            source.path.display().to_string().as_bytes(),
        );
        if settings_source_is_regular_file(&source.path).ok() != Some(true) {
            feed_settings_fingerprint(&mut fingerprint, b"<missing-or-unsafe>");
            continue;
        }
        match feed_settings_file_fingerprint(&source.path, &mut fingerprint) {
            Ok(file_len) => len = len.wrapping_add(file_len),
            Err(error) => {
                feed_settings_fingerprint(&mut fingerprint, b"<missing-or-unreadable>");
                feed_settings_fingerprint(&mut fingerprint, error.to_string().as_bytes());
            }
        }
    }
    for warning in warnings {
        feed_settings_fingerprint(&mut fingerprint, warning.as_bytes());
    }
    (fingerprint, len)
}

fn feed_settings_file_fingerprint(path: &Path, hash: &mut u128) -> std::io::Result<u64> {
    let file = std::fs::File::open(path)?;
    let file_len = file.metadata().map(|metadata| metadata.len()).unwrap_or(0);
    let mut reader = file.take(MAX_MCP_SETTINGS_FINGERPRINT_BYTES.saturating_add(1));
    let mut buffer = [0u8; 16 * 1024];
    let mut consumed = 0u64;
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        consumed = consumed.saturating_add(count as u64);
        if consumed > MAX_MCP_SETTINGS_FINGERPRINT_BYTES {
            feed_settings_fingerprint(hash, b"<fingerprint-input-truncated>");
            break;
        }
        feed_settings_fingerprint(hash, &buffer[..count]);
    }
    feed_settings_fingerprint(hash, file_len.to_string().as_bytes());
    Ok(file_len)
}

fn feed_settings_fingerprint(hash: &mut u128, bytes: &[u8]) {
    const FNV_PRIME_128: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;
    for byte in bytes {
        *hash ^= *byte as u128;
        *hash = hash.wrapping_mul(FNV_PRIME_128);
    }
    *hash ^= bytes.len() as u128;
    *hash = hash.wrapping_mul(FNV_PRIME_128);
}

pub fn normalize_server_name(value: &str) -> String {
    value
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_ascii_lowercase()
}

impl McpSourceReport {
    pub fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            (
                "serverCount",
                JsonValue::number(self.registry.servers.len()),
            ),
            (
                "sourceCount",
                JsonValue::number(self.registry.sources.len()),
            ),
            (
                "sources",
                JsonValue::array(
                    self.source_statuses
                        .iter()
                        .map(McpSourceStatus::to_json_value),
                ),
            ),
            (
                "servers",
                JsonValue::array(self.registry.servers.values().map(|entry| {
                    JsonValue::object([
                        ("name", JsonValue::string(entry.name.clone())),
                        (
                            "normalizedName",
                            JsonValue::string(entry.normalized_name.clone()),
                        ),
                        ("source", JsonValue::string(entry.source.clone())),
                    ])
                })),
            ),
            (
                "warnings",
                JsonValue::array(
                    self.registry
                        .warnings
                        .iter()
                        .cloned()
                        .map(JsonValue::string),
                ),
            ),
        ])
    }
}

impl McpSourceStatus {
    pub fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("path", JsonValue::string(self.path.clone())),
            ("origin", JsonValue::string(self.origin.clone())),
            ("exists", JsonValue::bool(self.exists)),
            ("serverCount", JsonValue::number(self.server_count)),
        ])
    }
}

#[cfg(test)]
mod tests;
