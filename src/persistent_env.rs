#[cfg(windows)]
use std::env;

pub(crate) const LOGIN_ENV_KEYS: &[&str] = &[
    "MCPACE_MCP_SETTINGS",
    "MCPACE_MCP_SETTINGS_DIRS",
    "MCPACE_STATE_ROOT",
];

#[derive(Clone, Debug, Default)]
pub(crate) struct LoginEnvHydration {
    pub hydrated_keys: Vec<String>,
}

pub(crate) fn hydrate_login_environment() -> LoginEnvHydration {
    hydrate_login_environment_impl()
}

#[cfg(windows)]
pub(crate) fn persistent_env_value(key: &str) -> Option<String> {
    persistent_env_value_impl(key)
}

#[cfg(windows)]
pub(crate) fn current_env_registry_mismatches() -> Vec<String> {
    let mut mismatches = Vec::new();
    for key in LOGIN_ENV_KEYS {
        let Some(current) = nonempty_env(key) else {
            continue;
        };
        match persistent_env_value(key) {
            Some(persistent) if persistent == current => {}
            Some(_) => mismatches.push(format!(
                "{} differs from persistent user/machine environment",
                key
            )),
            None => mismatches.push(format!(
                "{} is set only in the current process environment",
                key
            )),
        }
    }
    mismatches
}

#[cfg(not(windows))]
pub(crate) fn current_env_registry_mismatches() -> Vec<String> {
    Vec::new()
}

#[cfg(windows)]
fn nonempty_env(key: &str) -> Option<String> {
    let value = env::var(key).ok()?;
    if value.trim().is_empty() {
        None
    } else {
        Some(value)
    }
}

#[cfg(windows)]
fn hydrate_login_environment_impl() -> LoginEnvHydration {
    let mut report = LoginEnvHydration::default();
    for key in LOGIN_ENV_KEYS {
        if nonempty_env(key).is_some() {
            continue;
        }
        let Some(value) = persistent_env_value(key) else {
            continue;
        };
        env::set_var(key, &value);
        report.hydrated_keys.push((*key).to_string());
    }
    report
}

#[cfg(not(windows))]
fn hydrate_login_environment_impl() -> LoginEnvHydration {
    LoginEnvHydration::default()
}

#[cfg(windows)]
fn persistent_env_value_impl(key: &str) -> Option<String> {
    for registry_key in [
        r"HKCU\Environment",
        r"HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Environment",
    ] {
        if let Some(value) = read_registry_env_value(registry_key, key) {
            return Some(expand_percent_env_vars(&value));
        }
    }
    None
}

#[cfg(windows)]
fn read_registry_env_value(registry_key: &str, value_name: &str) -> Option<String> {
    let mut command = std::process::Command::new("reg");
    command.args(["query", registry_key, "/v", value_name]);
    crate::windows_process::configure_no_window(&mut command);
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    registry_query_value(&stdout, value_name)
}

#[cfg(windows)]
fn registry_query_value(stdout: &str, value_name: &str) -> Option<String> {
    for line in stdout.lines() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix(value_name) else {
            continue;
        };
        let rest = rest.trim_start();
        let mut parts = rest.splitn(2, char::is_whitespace);
        let value_type = parts.next().unwrap_or_default();
        if !value_type.starts_with("REG_") {
            continue;
        }
        let value = parts.next().unwrap_or_default().trim_start();
        return Some(value.to_string());
    }
    None
}

#[cfg(windows)]
fn expand_percent_env_vars(value: &str) -> String {
    let mut expanded = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find('%') {
        expanded.push_str(&rest[..start]);
        let after_start = &rest[start + 1..];
        let Some(end) = after_start.find('%') else {
            expanded.push_str(&rest[start..]);
            return expanded;
        };
        let name = &after_start[..end];
        if name.is_empty() {
            expanded.push_str("%%");
        } else if let Ok(replacement) = env::var(name) {
            expanded.push_str(&replacement);
        } else {
            expanded.push('%');
            expanded.push_str(name);
            expanded.push('%');
        }
        rest = &after_start[end + 1..];
    }
    expanded.push_str(rest);
    expanded
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn registry_query_value_preserves_path_lists_with_spaces() {
        let stdout = r#"
HKEY_CURRENT_USER\Environment
    MCPACE_MCP_SETTINGS    REG_SZ    C:\Users\Me\MCP Settings\one.json;D:\two.json
"#;
        assert_eq!(
            registry_query_value(stdout, "MCPACE_MCP_SETTINGS").as_deref(),
            Some(r"C:\Users\Me\MCP Settings\one.json;D:\two.json")
        );
    }

    #[test]
    fn percent_expansion_uses_current_process_environment() {
        env::set_var("MCPACE_TEST_ENV_HOME", r"C:\Users\Me");
        assert_eq!(
            expand_percent_env_vars(r"%MCPACE_TEST_ENV_HOME%\mcp.json"),
            r"C:\Users\Me\mcp.json"
        );
        env::remove_var("MCPACE_TEST_ENV_HOME");
    }
}
