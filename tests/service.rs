mod common;

use common::*;
use std::path::Path;

#[test]
fn service_install_dry_run_uses_auto_launch_without_mutating_user_startup() {
    let home = TempDir::new();
    let appdata = TempDir::new();
    let xdg = TempDir::new();
    let repo = env!("CARGO_MANIFEST_DIR");
    let envs = service_envs(home.path(), appdata.path(), xdg.path());

    let install = run_with_envs(
        &[
            "service",
            "install",
            "--json",
            "--root",
            repo,
            "--host",
            "127.0.0.1",
            "--port",
            "39123",
            "--dry-run",
        ],
        &envs,
    );
    assert!(
        install.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        stdout(&install),
        stderr(&install)
    );
    let text = stdout(&install);
    assert!(
        text.contains(r#""backend": "auto-launch/"#),
        "stdout:\n{}",
        text
    );
    assert!(text.contains(r#""enabled": false"#), "stdout:\n{}", text);
    assert!(text.contains("39123"), "stdout:\n{}", text);
}

#[test]
fn service_print_is_non_mutating_and_shows_current_binary_contract() {
    let home = TempDir::new();
    let appdata = TempDir::new();
    let xdg = TempDir::new();
    let repo = env!("CARGO_MANIFEST_DIR");
    let envs = service_envs(home.path(), appdata.path(), xdg.path());

    let output = run_with_envs(
        &[
            "service", "print", "--json", "--root", repo, "--port", "39124",
        ],
        &envs,
    );
    assert!(output.status.success(), "stderr:\n{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains(r#""ok": true"#), "stdout:\n{}", text);
    assert!(text.contains(r#""enabled": false"#), "stdout:\n{}", text);
    assert!(
        text.contains(r#""appPath":"#) || text.contains(r#""appPath": "#),
        "stdout:\n{}",
        text
    );
    assert!(text.contains("39124"), "stdout:\n{}", text);
    #[cfg(windows)]
    {
        assert!(
            text.contains("wscript.exe"),
            "Windows autostart must use a GUI host, stdout:\n{}",
            text
        );
        assert!(
            !text.to_ascii_lowercase().contains("powershell.exe"),
            "Windows autostart must not use PowerShell because it can flash a console, stdout:\n{}",
            text
        );
        assert!(
            !text.contains("\\\\?\\"),
            "Windows autostart command paths must avoid extended prefixes that wscript cannot reliably open, stdout:\n{}",
            text
        );
    }
}

fn service_envs<'a>(home: &'a Path, appdata: &'a Path, xdg: &'a Path) -> [(&'a str, &'a Path); 4] {
    [
        ("HOME", home),
        ("USERPROFILE", home),
        ("APPDATA", appdata),
        ("XDG_CONFIG_HOME", xdg),
    ]
}
