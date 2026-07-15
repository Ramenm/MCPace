use super::*;

#[test]
fn mcp_server_commands_parse_quoted_names_and_inline_comments() {
    let commands = mcp_server_commands(
        r#"
[mcp_servers."name.with#hash"] # user comment
command = "tool#name" # another user comment
args = ["serve"]

[mcp_servers.other]
command = 'single-quoted-command' # comment
"#,
    );

    assert_eq!(
        commands,
        vec![
            CodexMcpCommand {
                server_name: "name.with#hash".to_string(),
                command: "tool#name".to_string(),
            },
            CodexMcpCommand {
                server_name: "other".to_string(),
                command: "single-quoted-command".to_string(),
            },
        ]
    );
}
