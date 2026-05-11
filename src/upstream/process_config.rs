use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub(super) fn redact_command(command: &str) -> String {
    Path::new(command)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(command)
        .to_string()
}

pub(super) fn expand_template(input: &str, root_path: &Path) -> String {
    let mut output = String::new();
    let mut rest = input;
    while let Some(start) = rest.find("${") {
        output.push_str(&rest[..start]);
        let after_start = &rest[start + 2..];
        let Some(end) = after_start.find('}') else {
            output.push_str(&rest[start..]);
            return output;
        };
        let expression = &after_start[..end];
        output.push_str(&resolve_placeholder(expression, root_path));
        rest = &after_start[end + 1..];
    }
    output.push_str(rest);
    output
}

fn resolve_placeholder(expression: &str, root_path: &Path) -> String {
    let (name, fallback) = expression
        .split_once(":-")
        .map(|(name, fallback)| (name, Some(fallback)))
        .unwrap_or((expression, None));
    match name {
        "MCPACE_PRIMARY_WORKSPACE" => child_process_path(root_path),
        "MCPACE_MANAGER_DATA" => child_process_path(&manager_data_path(root_path)),
        other => env::var(other)
            .ok()
            .or_else(|| fallback.map(str::to_string))
            .unwrap_or_default(),
    }
}

pub(super) fn manager_data_path(root_path: &Path) -> PathBuf {
    root_path.join("data").join("runtime")
}

pub(super) fn child_process_path(path: &Path) -> String {
    let value = path.display().to_string();
    if let Some(rest) = value.strip_prefix("\\\\?\\UNC\\") {
        return format!("\\\\{}", rest);
    }
    value.strip_prefix("\\\\?\\").unwrap_or(&value).to_string()
}

pub(super) fn validate_stdio_cwd(cwd: &Path, server_name: &str) -> Option<String> {
    match fs::metadata(cwd) {
        Ok(metadata) if metadata.is_dir() => None,
        Ok(_) => Some(format!(
            "configured cwd '{}' for upstream server '{}' is not a directory",
            cwd.display(),
            server_name
        )),
        Err(error) => Some(format!(
            "configured cwd '{}' for upstream server '{}' is not accessible: {}",
            cwd.display(),
            server_name,
            error
        )),
    }
}

pub(super) fn resolve_command_for_cwd(command: &str, cwd: &Path) -> Result<PathBuf, String> {
    let command = command.trim();
    if command.is_empty() {
        return Err("empty command".to_string());
    }

    let raw = PathBuf::from(command);
    if raw.is_absolute() {
        return if raw.exists() {
            Ok(raw.canonicalize().unwrap_or(raw))
        } else {
            Err(format!(
                "absolute command path '{}' does not exist",
                raw.display()
            ))
        };
    }

    let looks_path_like = raw.components().count() > 1 || raw.extension().is_some();
    if looks_path_like {
        let cwd_candidate = cwd.join(&raw);
        if cwd_candidate.exists() {
            return Ok(cwd_candidate.canonicalize().unwrap_or(cwd_candidate));
        }
        if raw.exists() {
            return Ok(raw.canonicalize().unwrap_or(raw));
        }
    }

    which::which(command).map_err(|error| error.to_string())
}
