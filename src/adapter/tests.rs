use super::*;
use crate::{json_helpers, tool_result};
use std::fs;
use std::path::PathBuf;

fn temp_root() -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "mcpace-adapter-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&path).unwrap();
    fs::write(path.join("mcpace.config.json"), r#"{"servers":{}}"#).unwrap();
    fs::write(path.join("mcp_settings.json"), r#"{"mcpServers":{}}"#).unwrap();
    path
}

#[test]
fn safe_projected_names_are_bounded_and_unique() {
    let mut used = BTreeSet::new();
    let first = unique_projected_name("u", "Example Server", "read/file", &mut used);
    let second = unique_projected_name("u", "Example Server", "read file", &mut used);
    assert!(first.len() <= PROJECTED_NAME_MAX);
    assert!(second.len() <= PROJECTED_NAME_MAX);
    assert_ne!(first, second);
}

#[test]
fn resource_uri_round_trips() {
    let uri = encode_resource_uri("filesystem", "file:///tmp/hello world.txt");
    let (server, upstream_uri) = decode_resource_uri(&uri).unwrap();
    assert_eq!(server, "filesystem");
    assert_eq!(upstream_uri, "file:///tmp/hello world.txt");
}

#[test]
fn adapter_profile_advertises_only_supported_token_reducer_plugins() {
    let root = temp_root();
    let profile = adapter_profile(&root, None, "stdio", &[], false, Some(1), false).unwrap();
    let advertised = json_helpers::array_at_path(&profile, &["pluginHooks", "tokenReducers"])
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    let supported = tool_result::supported_token_reducer_plugins()
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>();
    assert_eq!(advertised, supported);
    let _ = fs::remove_dir_all(root);
}
