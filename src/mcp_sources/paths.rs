use super::{DEFAULT_SETTINGS_DIR, DEFAULT_SETTINGS_FILE, ENV_MCP_SETTINGS, ENV_MCP_SETTINGS_DIRS};
use crate::json::JsonValue;
use crate::json_helpers;
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

const MAX_MCP_SETTINGS_FILES_PER_DIRECTORY: usize = 1_024;

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(super) struct SourcePath {
    pub(super) path: PathBuf,
    pub(super) origin: String,
}

pub(super) fn collect_source_paths(
    root_path: &Path,
    warnings: &mut Vec<String>,
) -> Vec<SourcePath> {
    let mut seen = BTreeSet::new();
    let mut sources = Vec::new();

    push_source(
        &mut sources,
        &mut seen,
        root_path.join(DEFAULT_SETTINGS_FILE),
        "root",
    );

    collect_directory_sources(
        root_path,
        &root_path.join(DEFAULT_SETTINGS_DIR),
        "default-dir",
        false,
        warnings,
        &mut sources,
        &mut seen,
    );
    collect_config_paths(root_path, warnings, &mut sources, &mut seen);
    collect_env_paths(root_path, warnings, &mut sources, &mut seen);

    sources
}

fn collect_config_paths(
    root_path: &Path,
    warnings: &mut Vec<String>,
    sources: &mut Vec<SourcePath>,
    seen: &mut BTreeSet<PathBuf>,
) {
    let config_path = root_path.join("mcpace.config.json");
    if !config_path.is_file() {
        return;
    }
    let Ok(config) = json_helpers::read_json_file(&config_path) else {
        warnings.push(format!(
            "failed to inspect MCP settings include paths from '{}'; falling back to {} only",
            config_path.display(),
            DEFAULT_SETTINGS_FILE
        ));
        return;
    };
    for path in string_array_at(&config, &["mcpSettings", "includePaths"])
        .into_iter()
        .chain(string_array_at(&config, &["upstreams", "includePaths"]))
        .chain(string_array_at(&config, &["mcpServers", "includePaths"]))
    {
        push_source(
            sources,
            seen,
            resolve_under_root(root_path, &path),
            "config-include-path",
        );
    }
    for directory in string_array_at(&config, &["mcpSettings", "includeDirs"])
        .into_iter()
        .chain(string_array_at(&config, &["upstreams", "includeDirs"]))
        .chain(string_array_at(&config, &["mcpServers", "includeDirs"]))
    {
        let directory = resolve_under_root(root_path, &directory);
        collect_directory_sources(
            root_path,
            &directory,
            "config-include-dir",
            true,
            warnings,
            sources,
            seen,
        );
    }
}

fn collect_env_paths(
    root_path: &Path,
    warnings: &mut Vec<String>,
    sources: &mut Vec<SourcePath>,
    seen: &mut BTreeSet<PathBuf>,
) {
    if let Some(raw) = env::var_os(ENV_MCP_SETTINGS) {
        for path in env::split_paths(&raw) {
            push_source(
                sources,
                seen,
                resolve_under_root(root_path, &path),
                "env:MCPACE_MCP_SETTINGS",
            );
        }
    }
    if let Some(raw) = env::var_os(ENV_MCP_SETTINGS_DIRS) {
        for directory in env::split_paths(&raw) {
            let directory = resolve_under_root(root_path, &directory);
            collect_directory_sources(
                root_path,
                &directory,
                "env:MCPACE_MCP_SETTINGS_DIRS",
                true,
                warnings,
                sources,
                seen,
            );
        }
    }
}

fn collect_directory_sources(
    _root_path: &Path,
    directory: &Path,
    origin: &str,
    warn_if_missing: bool,
    warnings: &mut Vec<String>,
    sources: &mut Vec<SourcePath>,
    seen: &mut BTreeSet<PathBuf>,
) {
    match fs::symlink_metadata(directory) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            warnings.push(format!(
                "MCP settings directory '{}' must be a real directory; skipping",
                directory.display()
            ));
            return;
        }
        Ok(_) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => {
            if warn_if_missing {
                warnings.push(format!(
                    "MCP settings directory '{}' does not exist; skipping",
                    directory.display()
                ));
            }
            return;
        }
        Err(error) => {
            warnings.push(format!(
                "failed to inspect MCP settings directory '{}': {}; skipping",
                directory.display(),
                error
            ));
            return;
        }
    }
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) => {
            warnings.push(format!(
                "failed to read MCP settings directory '{}': {}",
                directory.display(),
                error
            ));
            return;
        }
    };
    let mut json_files = Vec::new();
    for entry in entries {
        match entry {
            Ok(entry) => {
                let path = entry.path();
                let is_regular_file = entry
                    .file_type()
                    .map(|file_type| file_type.is_file() && !file_type.is_symlink())
                    .unwrap_or(false);
                if is_regular_file
                    && path
                        .extension()
                        .and_then(|extension| extension.to_str())
                        .map(|extension| extension.eq_ignore_ascii_case("json"))
                        .unwrap_or(false)
                {
                    if json_files.len() >= MAX_MCP_SETTINGS_FILES_PER_DIRECTORY {
                        warnings.push(format!(
                            "MCP settings directory '{}' exceeds the {}-file safety limit; remaining entries were skipped",
                            directory.display(),
                            MAX_MCP_SETTINGS_FILES_PER_DIRECTORY
                        ));
                        break;
                    }
                    json_files.push(path);
                }
            }
            Err(error) => warnings.push(format!(
                "failed to inspect MCP settings directory '{}': {}",
                directory.display(),
                error
            )),
        }
    }
    json_files.sort();
    for path in json_files {
        push_source(sources, seen, path, origin);
    }
}

fn push_source(
    sources: &mut Vec<SourcePath>,
    seen: &mut BTreeSet<PathBuf>,
    path: PathBuf,
    origin: impl Into<String>,
) {
    let path = normalize_path_for_dedupe(path);
    if seen.insert(path.clone()) {
        sources.push(SourcePath {
            path,
            origin: origin.into(),
        });
    }
}

pub(super) fn resolve_under_root(root_path: &Path, path: impl AsRef<Path>) -> PathBuf {
    let path = path.as_ref();
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root_path.join(path)
    }
}

fn normalize_path_for_dedupe(path: PathBuf) -> PathBuf {
    std::fs::canonicalize(&path).unwrap_or(path)
}

fn string_array_at(value: &JsonValue, path: &[&str]) -> Vec<String> {
    let Some(JsonValue::Array(values)) = json_helpers::value_at_path(value, path) else {
        return Vec::new();
    };
    values
        .iter()
        .filter_map(JsonValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect()
}
