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

struct EnvVarGuard {
    key: &'static str,
    previous: Option<std::ffi::OsString>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let previous = std::env::var_os(key);
        std::env::set_var(key, value);
        Self { key, previous }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(value) => std::env::set_var(self.key, value),
            None => std::env::remove_var(self.key),
        }
    }
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

fn write_parallel_gate_upstreams(root: &std::path::Path) {
    let script = root.join("parallel-gate-upstream.js");
    let marker_dir = root.join("parallel-markers");
    fs::create_dir_all(&marker_dir).unwrap();
    fs::write(
        &script,
        r#"
const fs = require('fs');
const path = require('path');
const readline = require('readline');

const name = process.env.PARALLEL_SERVER_NAME;
const peers = process.env.PARALLEL_PEERS.split(',');
const markerDir = process.env.PARALLEL_MARKER_DIR;
fs.writeFileSync(path.join(markerDir, `${name}.started`), 'started\n');

const rl = readline.createInterface({ input: process.stdin });
function send(id, result) {
  process.stdout.write(JSON.stringify({ jsonrpc: '2.0', id, result }) + '\n');
}
function allPeersStarted() {
  return peers.every((peer) => fs.existsSync(path.join(markerDir, `${peer}.started`)));
}
async function waitForPeers(deadlineMs) {
  while (Date.now() < deadlineMs) {
    if (allPeersStarted()) return true;
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
  return false;
}
rl.on('line', async (line) => {
  const message = JSON.parse(line);
  if (message.method === 'initialize') {
    send(message.id, { protocolVersion: '2025-11-25', capabilities: { tools: {} }, serverInfo: { name, version: '0' } });
  } else if (message.method === 'tools/list') {
    if (!(await waitForPeers(Date.now() + 5000))) {
      return;
    }
    send(message.id, { tools: [{ name: `get_${name.replace(/-/g, '_')}`, description: `Read from ${name}`, inputSchema: { type: 'object' } }] });
  }
});
"#,
    )
    .unwrap();
    let script = json_escape(&script.display().to_string());
    let marker_dir = json_escape(&marker_dir.display().to_string());
    fs::write(
        root.join("mcp_settings.json"),
        format!(
            r#"{{
  "mcpServers": {{
    "parallel-a": {{
      "enabled": true,
      "type": "stdio",
      "command": "node",
      "args": ["{}"],
      "env": {{
        "PARALLEL_SERVER_NAME": "parallel-a",
        "PARALLEL_PEERS": "parallel-a,parallel-b",
        "PARALLEL_MARKER_DIR": "{}"
      }}
    }},
    "parallel-b": {{
      "enabled": true,
      "type": "stdio",
      "command": "node",
      "args": ["{}"],
      "env": {{
        "PARALLEL_SERVER_NAME": "parallel-b",
        "PARALLEL_PEERS": "parallel-a,parallel-b",
        "PARALLEL_MARKER_DIR": "{}"
      }}
    }}
  }}
}}"#,
            script, marker_dir, script, marker_dir
        ),
    )
    .unwrap();
}

#[test]
fn upstream_search_finds_tool_names_when_server_metadata_does_not_match() {
    let root = temp_root();
    let started = write_probe_marker_upstream(&root);

    let result = upstream_search(&root, None, Some("echo"), 10, false, Some(2_500), true)
        .expect("upstream_search should succeed");
    let results = json_helpers::array_at_path(&result, &["results"]).unwrap_or(&[]);

    assert_eq!(
        json_helpers::value_at_path(&result, &["resultCount"]).and_then(JsonValue::as_i64),
        Some(1)
    );
    assert!(results.iter().any(|tool| {
        json_helpers::string_at_path(tool, &["server"]) == Some("probe")
            && json_helpers::string_at_path(tool, &["name"]) == Some("echo")
    }));
    assert!(
        started.exists(),
        "tool-name search must probe the bounded fallback catalog when server metadata misses"
    );
    let _ = fs::remove_dir_all(root);
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
fn auto_projection_uses_cache_only_on_cold_tools_list() {
    let root = temp_root();
    let started = write_probe_marker_upstream(&root);
    let options = ToolExposureOptions {
        mode: ToolExposureMode::Auto,
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
        "auto startup tools/list must not spawn upstream probes without a warm cache"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn projection_catalog_probes_callable_servers_in_parallel() {
    let root = temp_root();
    write_parallel_gate_upstreams(&root);
    let _workers = EnvVarGuard::set(crate::resources::ENV_UPSTREAM_WORKERS, "2");
    let options = ToolExposureOptions {
        mode: ToolExposureMode::Hybrid,
        budget: DEFAULT_TOOL_BUDGET,
        token_budget: DEFAULT_TOOL_TOKEN_BUDGET,
        timeout_ms: Some(7_500),
        refresh: true,
        projection_safety: DEFAULT_PROJECTED_TOOL_SAFETY,
    };

    let projected = projected_tool_set(&root, &BTreeSet::new(), &options).unwrap();

    assert!(projected.raw_upstream_tool_count >= 2);
    assert!(projected.total_upstream_tool_count >= 2);
    assert!(projected.projected_tool_count >= 2);
    let projected_names = projected
        .tools
        .iter()
        .filter_map(|tool| json_helpers::string_at_path(tool, &["name"]))
        .collect::<Vec<_>>();
    assert!(projected_names
        .iter()
        .any(|name| name.contains("parallel_a")));
    assert!(projected_names
        .iter()
        .any(|name| name.contains("parallel_b")));
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
