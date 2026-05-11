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

fn json_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn write_probe_marker_upstream(root: &std::path::Path) -> PathBuf {
    let script = root.join("probe-marker-upstream.js");
    let started = root.join("probe-started.log");
    fs::write(
        &script,
        r#"
const fs = require('fs');
const readline = require('readline');
if (process.env.PROBE_STARTED_PATH) {
  fs.writeFileSync(process.env.PROBE_STARTED_PATH, 'started\n');
}
const rl = readline.createInterface({ input: process.stdin });
function send(id, result) {
  process.stdout.write(JSON.stringify({ jsonrpc: '2.0', id, result }) + '\n');
}
rl.on('line', (line) => {
  const message = JSON.parse(line);
  if (message.method === 'initialize') {
    send(message.id, { protocolVersion: '2025-11-25', capabilities: { tools: {} }, serverInfo: { name: 'probe', version: '0' } });
  } else if (message.method === 'tools/list') {
    send(message.id, { tools: [{ name: 'echo', description: 'Echo', inputSchema: { type: 'object' } }] });
  }
});
"#,
    )
    .unwrap();
    fs::write(
        root.join("mcp_settings.json"),
        format!(
            r#"{{
  "mcpServers": {{
    "probe": {{
      "enabled": true,
      "type": "stdio",
      "command": "node",
      "args": ["{}"],
      "env": {{ "PROBE_STARTED_PATH": "{}" }}
    }}
  }}
}}"#,
            json_escape(&script.display().to_string()),
            json_escape(&started.display().to_string())
        ),
    )
    .unwrap();
    started
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
fn default_tool_exposure_is_broker_and_broker_mode_does_not_probe_on_tools_list() {
    let root = temp_root();
    let started = write_probe_marker_upstream(&root);
    assert_eq!(default_tool_exposure_mode(), ToolExposureMode::Broker);

    let options = ToolExposureOptions {
        mode: default_tool_exposure_mode(),
        budget: DEFAULT_TOOL_BUDGET,
        token_budget: DEFAULT_TOOL_TOKEN_BUDGET,
        timeout_ms: Some(DEFAULT_TOOLS_LIST_TIMEOUT_MS),
        refresh: false,
        projection_safety: DEFAULT_PROJECTED_TOOL_SAFETY,
    };
    let tools = augment_tool_definitions_with_options(
        &root,
        vec![JsonValue::object([
            ("name", JsonValue::string("upstream_search")),
            ("description", JsonValue::string("Search upstreams")),
        ])],
        &options,
    );

    assert_eq!(tools.len(), 1);
    assert!(
        !started.exists(),
        "default startup tools/list must not spawn upstream probes"
    );
    let _ = fs::remove_dir_all(root);
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
