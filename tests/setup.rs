mod common;

use common::*;
use mcpace::{json, json_helpers};
use std::net::TcpListener;
use std::path::Path;

#[test]
fn setup_json_starts_server_installs_clients_and_smokes_mcp() {
    let state = TempDir::new();
    let home = TempDir::new();
    let repo = env!("CARGO_MANIFEST_DIR");
    let port = free_local_port().to_string();
    let envs = isolated_envs(state.path(), home.path());

    let args = [
        "setup",
        "--json",
        "--root",
        repo,
        "--host",
        "127.0.0.1",
        "--port",
        port.as_str(),
    ];
    let output = run_with_envs(&args, &envs);
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        stdout(&output),
        stderr(&output)
    );

    let report = json::parse_str(stdout(&output).trim()).expect("setup JSON");
    assert_eq!(
        json_helpers::string_at_path(&report, &["status"]),
        Some("ready")
    );
    assert_eq!(
        json_helpers::bool_at_path(&report, &["checks", "serveRunning"]),
        Some(true)
    );
    assert_eq!(
        json_helpers::bool_at_path(&report, &["checks", "healthOk"]),
        Some(true)
    );
    assert_eq!(
        json_helpers::bool_at_path(&report, &["checks", "mcpInitializeOk"]),
        Some(true)
    );
    assert_eq!(
        json_helpers::bool_at_path(&report, &["checks", "mcpToolsOk"]),
        Some(true)
    );
    assert!(
        json_helpers::array_at_path(&report, &["warnings"])
            .unwrap_or(&[])
            .iter()
            .any(|warning| warning
                .as_str()
                .unwrap_or_default()
                .contains("Cloud/public connector surfaces")),
        "cloud/public warning missing from {}",
        stdout(&output)
    );
    assert!(
        home.path().join(".codex").join("config.toml").is_file(),
        "codex config was not installed into isolated HOME"
    );
    assert!(
        home.path().join(".cursor").join("mcp.json").is_file(),
        "cursor config was not installed into isolated HOME"
    );

    stop_serve(repo, &envs);
}

#[test]
fn setup_skip_client_install_does_not_write_home_configs() {
    let state = TempDir::new();
    let home = TempDir::new();
    let repo = env!("CARGO_MANIFEST_DIR");
    let port = free_local_port().to_string();
    let envs = isolated_envs(state.path(), home.path());

    let args = [
        "setup",
        "--json",
        "--root",
        repo,
        "--host",
        "127.0.0.1",
        "--port",
        port.as_str(),
        "--skip-client-install",
    ];
    let output = run_with_envs(&args, &envs);
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        stdout(&output),
        stderr(&output)
    );

    let report = json::parse_str(stdout(&output).trim()).expect("setup JSON");
    assert_eq!(
        json_helpers::string_at_path(&report, &["status"]),
        Some("ready")
    );
    assert_eq!(
        json_helpers::bool_at_path(&report, &["clientInstall", "skipped"]),
        Some(true)
    );
    assert!(
        !home.path().join(".codex").exists(),
        "skip-client-install unexpectedly wrote Codex config"
    );
    assert!(
        !home.path().join(".cursor").exists(),
        "skip-client-install unexpectedly wrote Cursor config"
    );

    stop_serve(repo, &envs);
}

#[test]
fn setup_can_install_user_autostart_when_explicitly_requested() {
    let state = TempDir::new();
    let home = TempDir::new();
    let repo = env!("CARGO_MANIFEST_DIR");
    let port = free_local_port().to_string();
    let envs = isolated_envs(state.path(), home.path());

    let args = [
        "setup",
        "--json",
        "--root",
        repo,
        "--host",
        "127.0.0.1",
        "--port",
        port.as_str(),
        "--skip-client-install",
        "--install-service",
        "--no-enable",
    ];
    let output = run_with_envs(&args, &envs);
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        stdout(&output),
        stderr(&output)
    );

    let report = json::parse_str(stdout(&output).trim()).expect("setup JSON");
    assert_eq!(
        json_helpers::bool_at_path(&report, &["checks", "serviceInstallReady"]),
        Some(true)
    );
    assert_eq!(
        json_helpers::bool_at_path(&report, &["serviceInstall", "json", "enabled"]),
        Some(false)
    );
    let text = stdout(&output);
    assert!(text.contains("auto-launch/"), "stdout:\n{}", text);
    assert!(text.contains(port.as_str()), "stdout:\n{}", text);

    stop_serve(repo, &envs);
}

fn free_local_port() -> u16 {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind free local port");
    listener.local_addr().expect("local addr").port()
}

fn isolated_envs<'a>(state: &'a Path, home: &'a Path) -> [(&'a str, &'a Path); 5] {
    [
        ("MCPACE_STATE_ROOT", state),
        ("HOME", home),
        ("USERPROFILE", home),
        ("APPDATA", home),
        ("XDG_CONFIG_HOME", home),
    ]
}

fn stop_serve(repo: &str, envs: &[(&str, &Path)]) {
    let output = run_with_envs(&["serve", "stop", "--json", "--root", repo], envs);
    assert!(
        output.status.success(),
        "serve stop failed:\nstdout:\n{}\nstderr:\n{}",
        stdout(&output),
        stderr(&output)
    );
}
