use super::*;

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

#[test]
fn install_command_preserves_launcher_flags_and_mcpace_options() {
    let parsed = parse_cli(&args(&[
        "install",
        "npx",
        "-y",
        "@modelcontextprotocol/server-filesystem",
        ".",
        "--as",
        "filesystem",
        "--dry-run",
        "--json",
    ]));

    assert_eq!(parsed.error, None);
    assert_eq!(parsed.action.as_deref(), Some("install"));
    assert_eq!(
        parsed.name_filter.as_deref(),
        Some("npx -y @modelcontextprotocol/server-filesystem .")
    );
    assert_eq!(parsed.install_name_override.as_deref(), Some("filesystem"));
    assert!(parsed.dry_run);
    assert!(parsed.json_output);
}

#[test]
fn unknown_hyphenated_install_values_do_not_hide_known_options() {
    let parsed = parse_cli(&args(&[
        "install",
        "uvx",
        "-p",
        "mcp-server-time",
        "mcp-server-time",
        "--as",
        "time",
        "--disabled",
    ]));

    assert_eq!(parsed.error, None);
    assert_eq!(
        parsed.name_filter.as_deref(),
        Some("uvx -p mcp-server-time mcp-server-time")
    );
    assert_eq!(parsed.install_name_override.as_deref(), Some("time"));
    assert!(parsed.disabled);
}
