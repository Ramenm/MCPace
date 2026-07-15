use super::*;
use std::path::Path;

#[test]
fn toml_managed_block_repair_preserves_foreign_tables_when_marker_overreaches() {
    let existing = concat!(
        "model = \"gpt\"\n",
        "\n",
        "# BEGIN MCPACE MANAGED BLOCK: MCPace\n",
        "# This block is managed by `mcpace client install`.\n",
        "[mcp_servers.MCPace]\n",
        "url = \"http://127.0.0.1:1/mcp\"\n",
        "enabled = true\n",
        "[plugins]\n",
        "enabled = true\n",
        "\n",
        "[notice]\n",
        "text = \"keep me\"\n",
        "# END MCPACE MANAGED BLOCK: MCPace\n",
        "approval_policy = \"never\"\n",
    );
    let managed_block = build_toml_mcp_server_block("MCPace", "http://127.0.0.1:39022/mcp", "\n");

    let update =
        apply_toml_mcp_server_block(existing, "MCPace", &managed_block, Path::new("config.toml"))
            .expect("over-wide managed block should be recoverable");

    assert!(update.replaced_existing_block);
    assert!(update.contents.contains("[plugins]\nenabled = true"));
    assert!(update.contents.contains("[notice]\ntext = \"keep me\""));
    assert!(update
        .contents
        .contains("url = \"http://127.0.0.1:39022/mcp\""));
    assert!(!update.contents.contains("url = \"http://127.0.0.1:1/mcp\""));
    assert_eq!(
        update
            .contents
            .matches("# BEGIN MCPACE MANAGED BLOCK: MCPace")
            .count(),
        1
    );
    assert_eq!(
        update
            .contents
            .matches("# END MCPACE MANAGED BLOCK: MCPace")
            .count(),
        1
    );
    let new_end = update
        .contents
        .find("# END MCPACE MANAGED BLOCK: MCPace")
        .expect("new end marker should exist");
    let plugins = update
        .contents
        .find("[plugins]")
        .expect("foreign table should be preserved");
    assert!(new_end < plugins);
}

#[test]
fn toml_managed_block_rejects_unrecoverable_overwide_marker() {
    let existing = concat!(
        "# BEGIN MCPACE MANAGED BLOCK: MCPace\n",
        "[plugins]\n",
        "enabled = true\n",
        "# END MCPACE MANAGED BLOCK: MCPace\n",
    );
    let managed_block = build_toml_mcp_server_block("MCPace", "http://127.0.0.1:39022/mcp", "\n");

    let error = match apply_toml_mcp_server_block(
        existing,
        "MCPace",
        &managed_block,
        Path::new("config.toml"),
    ) {
        Ok(_) => panic!("foreign table without MCPace table should not be rewritten"),
        Err(error) => error,
    };

    assert!(error.to_string().contains("over-wide MCPace managed block"));
}

#[test]
fn json_server_object_error_is_typed_and_user_facing_message_matches_contract() {
    let error = apply_json_mcp_server_entry(
        r#"{"servers": false}"#,
        "MCPace",
        "servers",
        json_helpers::empty_object(),
        Path::new(".vscode/mcp.json"),
    )
    .expect_err("non-object servers field should be rejected");

    assert!(matches!(error, ConfigEditError::JsonServersObject { .. }));
    assert!(error
        .to_string()
        .contains("JSON client config '.vscode/mcp.json' has a non-object servers field"));
}
