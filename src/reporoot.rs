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
    find_from(exe_dir.as_path()).or_else(find_from_installed_service)
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

fn find_from_installed_service() -> Option<PathBuf> {
    find_from_windows_autostart()
}

#[cfg(not(windows))]
fn find_from_windows_autostart() -> Option<PathBuf> {
    None
}

#[cfg(windows)]
fn find_from_windows_autostart() -> Option<PathBuf> {
    for key in [
        "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run",
        "HKLM\\Software\\Microsoft\\Windows\\CurrentVersion\\Run",
    ] {
        for value_name in [crate::service::APP_NAME, crate::service::LEGACY_APP_NAME] {
            let Some(command) = read_windows_autostart_command(key, value_name) else {
                continue;
            };
            if let Some(root) = root_from_launcher_text(&command).or_else(|| {
                let script_path = windows_vbs_path_from_command(&command)?;
                let script = std::fs::read_to_string(script_path).ok()?;
                root_from_launcher_text(&script)
            }) {
                return Some(root);
            }
        }
    }
    root_from_windows_autostart_plan(&windows_autostart_plan_path())
}

#[cfg(windows)]
fn windows_autostart_plan_path() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("TEMP").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("MCPace")
        .join("agent")
        .join("autostart-plan.json")
}

#[cfg(windows)]
fn root_from_windows_autostart_plan(path: &Path) -> Option<PathBuf> {
    let value: serde_json::Value = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    let root = value
        .get("rootPath")
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string)
        .or_else(|| {
            let args = value.get("targetArgs")?.as_array()?;
            args.windows(2).find_map(|items| {
                (items[0].as_str()? == "--root")
                    .then(|| items[1].as_str().map(ToString::to_string))
                    .flatten()
            })
        })?;
    let candidate = PathBuf::from(root);
    if has_root_markers(&candidate) {
        Some(candidate)
    } else {
        find_from(&candidate)
    }
}

#[cfg(windows)]
fn read_windows_autostart_command(key: &str, value_name: &str) -> Option<String> {
    let mut command = std::process::Command::new("reg");
    command.args(["query", key, "/v", value_name]);
    crate::windows_process::configure_no_window(&mut command);
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let line = line.trim_start();
        if !line.starts_with(value_name) {
            continue;
        }
        for marker in ["REG_EXPAND_SZ", "REG_SZ"] {
            if let Some(index) = line.find(marker) {
                let value = line[index + marker.len()..].trim();
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
    }
    None
}

#[cfg(windows)]
fn windows_vbs_path_from_command(command: &str) -> Option<PathBuf> {
    let lower = command.to_ascii_lowercase();
    let extension = ".vbs";
    let end = lower.rfind(extension)? + extension.len();
    let prefix = &command[..end];
    let start = prefix
        .rfind('"')
        .map(|index| index + 1)
        .or_else(|| {
            prefix
                .char_indices()
                .rev()
                .find(|(_, ch)| ch.is_whitespace())
                .map(|(index, ch)| index + ch.len_utf8())
        })
        .unwrap_or(0);
    let value = command[start..end].trim();
    if value.is_empty() {
        None
    } else {
        Some(PathBuf::from(value))
    }
}

#[cfg(windows)]
fn root_from_launcher_text(text: &str) -> Option<PathBuf> {
    let value = root_arg_from_text(text)?;
    let candidate = PathBuf::from(value);
    if has_root_markers(&candidate) {
        Some(candidate)
    } else {
        find_from(candidate.as_path())
    }
}

#[cfg(windows)]
fn root_arg_from_text(text: &str) -> Option<String> {
    let mut search_from = 0usize;
    while let Some(relative) = text[search_from..].find("--root") {
        let start = search_from + relative;
        let before = text[..start].chars().next_back();
        let after = text[start + "--root".len()..].chars().next();
        if before.is_some_and(|ch| !ch.is_whitespace())
            || after.is_some_and(|ch| !ch.is_whitespace())
        {
            search_from = start + "--root".len();
            continue;
        }

        let rest = text[start + "--root".len()..].trim_start();
        if let Some(value) = quoted_arg(rest, "\"\"") {
            return Some(value);
        }
        if let Some(value) = quoted_arg(rest, "\"") {
            return Some(value);
        }
        let value = rest
            .chars()
            .take_while(|ch| !ch.is_whitespace())
            .collect::<String>();
        if !value.is_empty() {
            return Some(value);
        }
        search_from = start + "--root".len();
    }
    None
}

#[cfg(windows)]
fn quoted_arg(rest: &str, delimiter: &str) -> Option<String> {
    let value = rest.strip_prefix(delimiter)?;
    let end = value.find(delimiter)?;
    Some(value[..end].to_string())
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn root_arg_parser_accepts_vbscript_escaped_quotes() {
        let text = r#"shell.Run """C:\bin\mcpace.exe"" serve start --root ""C:\work\mcpace"" --host 127.0.0.1", 0, False"#;
        assert_eq!(root_arg_from_text(text).as_deref(), Some(r"C:\work\mcpace"));
    }

    #[test]
    fn root_arg_parser_accepts_regular_windows_quotes() {
        let text = r#""C:\bin\mcpace.exe" serve restart --root "C:\work\mcpace""#;
        assert_eq!(root_arg_from_text(text).as_deref(), Some(r"C:\work\mcpace"));
    }

    #[test]
    fn vbs_path_parser_uses_the_launcher_script_not_wscript() {
        let command = r#"C:\WINDOWS\System32\wscript.exe //B //Nologo "C:\MCPace\runtime\service\mcpace-autostart.vbs""#;
        assert_eq!(
            windows_vbs_path_from_command(command).as_deref(),
            Some(Path::new(r"C:\MCPace\runtime\service\mcpace-autostart.vbs"))
        );
    }

    #[test]
    fn current_hidden_launcher_plan_recovers_the_installed_root() {
        let base =
            std::env::temp_dir().join(format!("mcpace-reporoot-plan-test-{}", std::process::id()));
        let root = base.join("root");
        let plan = base.join("autostart-plan.json");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("mcpace.config.json"), "{}\n").unwrap();
        std::fs::write(
            &plan,
            serde_json::json!({
                "schema": "mcpace.windowsAutostartPlan.v1",
                "targetAppPath": "C:\\MCPace\\mcpace.exe",
                "targetArgs": ["agent", "run", "--autostart", "--root", root],
                "rootPath": root,
            })
            .to_string(),
        )
        .unwrap();

        assert_eq!(root_from_windows_autostart_plan(&plan), Some(root));
        let _ = std::fs::remove_dir_all(base);
    }
}
