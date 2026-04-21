use std::env;
use std::path::{Path, PathBuf};

pub fn find_from_current_or_executable() -> Option<PathBuf> {
    if let Some(root) = find_from_env("MCPACE_ROOT") {
        return Some(root);
    }

    if let Ok(cwd) = env::current_dir() {
        if let Some(root) = find_from(cwd.as_path()) {
            return Some(root);
        }
    }

    let exe_dir = env::current_exe().ok()?.parent()?.to_path_buf();
    find_from(exe_dir.as_path())
}

pub fn find_from_env(key: &str) -> Option<PathBuf> {
    let value = env::var_os(key)?;
    let candidate = PathBuf::from(value);
    if candidate.as_os_str().is_empty() {
        return None;
    }
    if has_root_markers(&candidate) {
        return Some(candidate);
    }
    find_from(candidate.as_path())
}

pub fn find_from(start: &Path) -> Option<PathBuf> {
    let mut current = std::fs::canonicalize(start)
        .ok()
        .unwrap_or_else(|| start.to_path_buf());

    loop {
        if has_root_markers(&current) {
            return Some(current);
        }
        let parent = current.parent()?.to_path_buf();
        if parent == current {
            return None;
        }
        current = parent;
    }
}

pub fn has_root_markers(dir: &Path) -> bool {
    dir.join("mcpace.config.json").is_file()
}
