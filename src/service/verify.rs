use super::*;
use crate::json::JsonValue;
use std::path::Path;

pub(super) fn launches_mcpace_agent(config: &ServiceConfig) -> bool {
    config
        .target_args
        .first()
        .is_some_and(|value| value == "agent")
        && config
            .target_args
            .get(1)
            .is_some_and(|value| value == "run")
        && config
            .target_args
            .iter()
            .any(|value| value == "--autostart")
}

pub(super) fn resources_forwarded(config: &ServiceConfig) -> bool {
    [
        "--max-connections",
        "--io-timeout-ms",
        "--max-body-bytes",
        "--overview-cache-ms",
    ]
    .iter()
    .all(|flag| {
        !config.target_args.iter().any(|value| value == *flag)
            || target_arg_value(&config.target_args, flag).is_some()
    })
}

pub(super) fn target_arg_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|items| items[0] == flag)
        .map(|items| items[1].as_str())
}

pub(super) fn target_root_path_valid(config: &ServiceConfig) -> bool {
    let Some(root) = target_arg_value(&config.target_args, "--root") else {
        return false;
    };
    crate::reporoot::has_root_markers(Path::new(root))
}

pub(super) fn persistent_env_alignment_detail(mismatches: &[String]) -> String {
    if mismatches.is_empty() {
        if cfg!(windows) {
            return format!(
                "Windows login agent hydrates persistent path environment keys from registry when present: {}",
                crate::persistent_env::LOGIN_ENV_KEYS.join(", ")
            );
        }
        return "persistent Windows registry environment hydration is not required on this platform"
            .to_string();
    }
    format!(
        "current process has MCPace path environment values that are not available to Windows login startup: {}",
        mismatches.join("; ")
    )
}

pub(super) fn verification_check(name: &str, ok: bool, detail: &str) -> JsonValue {
    JsonValue::object([
        ("name", JsonValue::string(name)),
        ("ok", JsonValue::bool(ok)),
        ("detail", JsonValue::string(detail)),
    ])
}

pub(super) fn service_applied_state_json(config: &ServiceConfig) -> JsonValue {
    let (manager, visible_in, supervised_by_os) = if cfg!(target_os = "linux") {
        (
            "systemd user service",
            "systemctl --user status mcpace-agent.service",
            true,
        )
    } else if cfg!(target_os = "macos") {
        ("launchd LaunchAgent", "Login Items / LaunchAgents", true)
    } else if cfg!(windows) {
        (
            "Windows current-user Run registry + supervised hidden MCPace launcher",
            "Settings > Apps > Startup / Task Manager Startup apps",
            true,
        )
    } else {
        ("unsupported", "unsupported", false)
    };
    JsonValue::object([
        ("schema", JsonValue::string(AUTOSTART_APPLIED_STATE_SCHEMA)),
        ("platform", JsonValue::string(config.platform.clone())),
        ("manager", JsonValue::string(manager)),
        ("visibleAs", JsonValue::string(APP_NAME)),
        ("visibleIn", JsonValue::string(visible_in)),
        ("supervisedByOs", JsonValue::bool(supervised_by_os)),
        (
            "activatedImmediately",
            JsonValue::bool(cfg!(any(windows, target_os = "linux"))),
        ),
        ("supervisedByMcpaceAgent", JsonValue::bool(true)),
        ("environment", service_environment_json()),
        ("command", command_json(config)),
    ])
}

fn service_environment_json() -> JsonValue {
    JsonValue::object([
        (
            "schema",
            JsonValue::string("mcpace.autostartEnvironment.v1"),
        ),
        ("windowsRegistryHydration", JsonValue::bool(cfg!(windows))),
        (
            "persistentPathKeys",
            JsonValue::array(
                crate::persistent_env::LOGIN_ENV_KEYS
                    .iter()
                    .map(|key| JsonValue::string(*key)),
            ),
        ),
        (
            "detail",
            JsonValue::string(if cfg!(windows) {
                "MCPace Agent reads persistent user/machine registry environment for path-like MCPace settings before starting serve"
            } else {
                "This platform inherits login-manager environment directly; Windows registry hydration is not used"
            }),
        ),
    ])
}
