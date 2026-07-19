use super::*;

fn temp_root() -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "mcpace-client-removal-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    root
}

fn json_plan(root: &Path) -> ClientInstallPlan {
    ClientInstallPlan {
        client_target_id: "test-client".to_string(),
        display_name: "Test Client".to_string(),
        adapter_key_name: "MCPace".to_string(),
        config_path: root.join("client.json"),
        backup_root: root.join("backups"),
        config_scope: "test".to_string(),
        server_url: "http://127.0.0.1:39022/mcp".to_string(),
        config: ClientInstallConfig::JsonMcpServers {
            servers_object_key: "mcpServers".to_string(),
            server_config: JsonValue::object([(
                "url",
                JsonValue::string("http://127.0.0.1:39022/mcp"),
            )]),
        },
        warnings: Vec::new(),
    }
}

#[test]
fn owned_json_removal_is_previewable_backed_up_and_idempotent() {
    let root = temp_root();
    let plan = json_plan(&root);
    let original = r#"{
  "mcpServers": {
    "MCPace": { "url": "http://127.0.0.1:39022/mcp" },
    "foreign": { "url": "https://example.test/mcp" }
  },
  "theme": "dark"
}"#;
    fs::write(&plan.config_path, original).unwrap();

    let preview = plan.remove_owned_entry(true).unwrap();
    assert_eq!(
        preview.get("wouldRemove").and_then(JsonValue::as_bool),
        Some(true)
    );
    assert_eq!(
        preview.get("removed").and_then(JsonValue::as_bool),
        Some(false)
    );
    assert_eq!(fs::read_to_string(&plan.config_path).unwrap(), original);

    let applied = plan.remove_owned_entry(false).unwrap();
    assert_eq!(
        applied.get("removed").and_then(JsonValue::as_bool),
        Some(true)
    );
    let updated = fs::read_to_string(&plan.config_path).unwrap();
    assert!(!updated.contains("127.0.0.1:39022"));
    assert!(updated.contains("example.test"));
    assert!(updated.contains("dark"));
    let backup_path = applied
        .get("backupPath")
        .and_then(JsonValue::as_str)
        .map(PathBuf::from)
        .expect("removal backup path");
    assert!(backup_path.join("manifest.json").is_file());
    assert!(backup_path.join("config.before").is_file());

    let second = plan.remove_owned_entry(false).unwrap();
    assert_eq!(
        second.get("wouldRemove").and_then(JsonValue::as_bool),
        Some(false)
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn same_named_foreign_json_entry_is_not_removed() {
    let root = temp_root();
    let plan = json_plan(&root);
    let original = r#"{"mcpServers":{"MCPace":{"url":"https://foreign.example/mcp"}}}"#;
    fs::write(&plan.config_path, original).unwrap();

    let result = plan.remove_owned_entry(false).unwrap();
    assert_eq!(
        result.get("wouldRemove").and_then(JsonValue::as_bool),
        Some(false)
    );
    assert_eq!(fs::read_to_string(&plan.config_path).unwrap(), original);
    assert!(!root.join("backups").exists());

    let _ = fs::remove_dir_all(root);
}
