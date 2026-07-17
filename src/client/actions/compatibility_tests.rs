use super::*;

#[test]
fn generated_stdio_launcher_keeps_the_installed_config_contract() {
    assert_eq!(
        stdio_launcher_args(r"C:\Users\example\MCPace", "cursor-local"),
        vec![
            "stdio",
            "--root",
            r"C:\Users\example\MCPace",
            "--client-id",
            "cursor-local",
        ]
    );
}

#[test]
fn generated_stdio_launcher_strips_windows_extended_path_prefix() {
    assert_eq!(
        stdio_launcher_args(r"\\?\C:\Users\example\MCPace", "codex"),
        vec![
            "stdio",
            "--root",
            r"C:\Users\example\MCPace",
            "--client-id",
            "codex",
        ]
    );
}
