use super::run;
use crate::json::{parse_str, JsonValue};

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| item.to_string()).collect()
}

#[test]
fn client_export_json_missing_target_is_machine_readable() {
    let args = strings(&["export", "--json"]);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let status = run(&args, None, &mut stdout, &mut stderr);

    assert_eq!(status, 2);
    assert!(stderr.is_empty());
    let output = String::from_utf8(stdout).expect("json output is utf-8");
    let payload = parse_str(&output).expect("missing target error must be valid JSON");
    assert_eq!(
        payload.get("schema").and_then(JsonValue::as_str),
        Some("mcpace.clientError.v1")
    );
    assert_eq!(payload.get("ok").and_then(JsonValue::as_bool), Some(false));
    assert_eq!(
        payload.get("action").and_then(JsonValue::as_str),
        Some("export")
    );
    assert!(payload
        .get("error")
        .and_then(JsonValue::as_str)
        .unwrap_or_default()
        .contains("requires a client target"));
}

#[test]
fn client_export_text_missing_target_points_to_catalog() {
    let args = strings(&["export"]);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let status = run(&args, None, &mut stdout, &mut stderr);

    assert_eq!(status, 2);
    assert!(stdout.is_empty());
    let error = String::from_utf8(stderr).expect("stderr is utf-8");
    assert!(error.contains("requires a client target"));
    assert!(error.contains("mcpace client list"));
}

#[test]
fn client_install_selects_the_current_platform_user_config_path() {
    let registry = crate::client_catalog::load_registry(None).expect("load client catalog");
    let vscode = registry
        .targets
        .iter()
        .find(|target| target.id == "vscode-workspace")
        .expect("VS Code client target");
    let selected = super::actions::platform_install_config_path(vscode)
        .expect("platform-specific VS Code install path");

    if cfg!(windows) {
        assert_eq!(selected, "~/AppData/Roaming/Code/User/mcp.json");
    } else if cfg!(target_os = "macos") {
        assert_eq!(selected, "~/Library/Application Support/Code/User/mcp.json");
    } else {
        assert_eq!(selected, "~/.config/Code/User/mcp.json");
    }
}
