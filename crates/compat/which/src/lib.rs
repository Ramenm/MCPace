use std::env;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Error(String);
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Error {}

pub fn which(command: &str) -> Result<PathBuf, Error> {
    if command.trim().is_empty() {
        return Err(Error("empty command".into()));
    }
    let direct = Path::new(command);
    if direct.components().count() > 1 {
        return executable(direct)
            .map(Path::to_path_buf)
            .ok_or_else(|| Error(format!("command is not executable: {command}")));
    }
    let path = env::var_os("PATH").ok_or_else(|| Error("PATH is not set".into()))?;
    for dir in env::split_paths(&path) {
        for name in candidate_names(command) {
            let candidate = dir.join(name);
            if executable(&candidate).is_some() {
                return Ok(candidate);
            }
        }
    }
    Err(Error(format!("command not found: {command}")))
}

fn candidate_names(command: &str) -> Vec<String> {
    #[cfg(windows)]
    {
        let pathext = env::var_os("PATHEXT")
            .map(|v| v.to_string_lossy().into_owned())
            .unwrap_or_else(|| ".COM;.EXE;.BAT;.CMD".into());
        let lower = command.to_ascii_lowercase();
        if pathext
            .split(';')
            .any(|ext| !ext.is_empty() && lower.ends_with(&ext.to_ascii_lowercase()))
        {
            return vec![command.into()];
        }
        // On Windows, try PATHEXT extensions first so that proper Windows
        // executables and .cmd wrappers (e.g. npx.cmd) are preferred over
        // extension-less shebang scripts that cannot be spawned directly.
        let mut names: Vec<String> = pathext
            .split(';')
            .filter(|ext| !ext.is_empty())
            .map(|ext| format!("{command}{ext}"))
            .collect();
        // Fall back to the bare name last (handles the case where a native
        // Windows binary has no extension, e.g. a custom compiled tool).
        names.push(command.into());
        names
    }
    #[cfg(not(windows))]
    {
        vec![command.into()]
    }
}
fn executable(path: &Path) -> Option<&Path> {
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return None;
        }
    }
    Some(path)
}
