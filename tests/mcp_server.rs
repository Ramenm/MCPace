mod common;

use common::*;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

fn write_minimal_config(root: &std::path::Path) {
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.5",
  "client": {
    "keyName": "MCPace"
  },
  "profiles": {
    "runtime": {
      "default": "safe",
      "profiles": {
        "safe": { "description": "Safe", "serverOverrides": {} }
      }
    }
  },
  "servers": {}
}"#,
    )
    .unwrap();
}

fn write_fake_upstream(root: &std::path::Path) -> std::path::PathBuf {
    let script = root.join("fake-upstream.js");
    fs::write(
        &script,
        r#"
const fs = require('fs');
const readline = require('readline');

if (process.env.FAKE_STARTED_PATH) {
  fs.appendFileSync(process.env.FAKE_STARTED_PATH, 'started\n');
}

const rl = readline.createInterface({ input: process.stdin });

function send(id, result) {
  process.stdout.write(JSON.stringify({ jsonrpc: '2.0', id, result }) + '\n');
}

rl.on('line', (line) => {
  const trimmed = line.trim();
  if (!trimmed) return;
  let message;
  try {
    message = JSON.parse(trimmed);
  } catch {
    return;
  }
  if (message.method === 'initialize') {
    send(message.id, {
      protocolVersion: '2025-11-25',
      capabilities: { tools: {} },
      serverInfo: { name: 'fake-upstream', version: '0.1.0' }
    });
    return;
  }
  if (message.method === 'tools/list') {
    send(message.id, {
      tools: [
        {
          name: 'echo',
          description: 'Echo test tool',
          inputSchema: { type: 'object', additionalProperties: true }
        }
      ]
    });
    return;
  }
  if (message.method === 'tools/call') {
    const callArguments = (message.params && message.params.arguments) || {};
    const result = {
      content: [
        {
          type: 'text',
          text: JSON.stringify({
            tool: message.params && message.params.name,
            arguments: callArguments
          })
        }
      ],
      isError: false
    };
    if (callArguments.emitStale) {
      send(Number(message.id) + 1000, {
        content: [{ type: 'text', text: 'stale response must be ignored' }],
        isError: false
      });
    }
    const delayMs = Number(callArguments.delayMs || 0);
    if (delayMs > 0) {
      setTimeout(() => send(message.id, result), delayMs);
    } else {
      send(message.id, result);
    }
    return;
  }
});
"#,
    )
    .unwrap();
    script
}

fn write_fake_upstream_config(root: &std::path::Path) -> std::path::PathBuf {
    let script = write_fake_upstream(root);
    let started_path = root.join("fake-started.log");
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.5",
  "client": {
    "keyName": "MCPace"
  },
  "profiles": {
    "runtime": {
      "default": "safe",
      "profiles": {
        "safe": { "description": "Safe", "serverOverrides": {} }
      }
    }
  },
  "servers": {
    "fake": {
      "kind": "host-stdio",
      "required": true,
      "transportPreference": "stdio",
      "policy": {
        "scopeClass": "shared-global",
        "concurrencyPolicy": "single-writer",
        "stateBinding": "none",
        "credentialBinding": "none",
        "parallelismLimit": 1,
        "conflictDomain": "fake-shared"
      },
      "installer": {
        "installTarget": "none",
        "installMethod": "none",
        "installPackage": "",
        "verifyCommand": ""
      }
    }
  }
}"#,
    )
    .unwrap();
    fs::write(
        root.join("mcp_settings.json"),
        format!(
            r#"{{
  "mcpServers": {{
    "fake": {{
      "enabled": true,
      "type": "stdio",
      "command": "node",
      "args": ["{}"],
      "env": {{
        "FAKE_STARTED_PATH": "{}"
      }}
    }}
  }}
}}"#,
            json_escape(&script.display().to_string()),
            json_escape(&started_path.display().to_string())
        ),
    )
    .unwrap();
    started_path
}

fn write_fake_upstream_settings_only_config(root: &std::path::Path) -> std::path::PathBuf {
    let script = write_fake_upstream(root);
    let started_path = root.join("fake-started.log");
    write_minimal_config(root);
    fs::write(
        root.join("mcp_settings.json"),
        format!(
            r#"{{
  "mcpServers": {{
    "fake": {{
      "enabled": true,
      "type": "stdio",
      "command": "node",
      "args": ["{}"],
      "env": {{
        "FAKE_STARTED_PATH": "{}"
      }}
    }}
  }}
}}"#,
            json_escape(&script.display().to_string()),
            json_escape(&started_path.display().to_string())
        ),
    )
    .unwrap();
    started_path
}

fn json_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn write_json_line(writer: &mut impl Write, line: &str) {
    writer.write_all(line.as_bytes()).unwrap();
    writer.write_all(b"\n").unwrap();
    writer.flush().unwrap();
}

fn mcp_structured_content(line: &str) -> serde_json::Value {
    let response: serde_json::Value = serde_json::from_str(line).expect("valid JSON-RPC response");
    response["result"]["structuredContent"].clone()
}

fn active_lease_ids(root: &std::path::Path) -> Vec<String> {
    let output = run(&[
        "hub",
        "lease",
        "list",
        "--json",
        "--root",
        root.to_str().unwrap(),
    ]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let json: serde_json::Value =
        serde_json::from_str(&stdout(&output)).expect("valid lease list JSON");
    match json["leases"].as_array() {
        Some(items) => items
            .iter()
            .filter_map(|item| item["leaseId"].as_str().map(str::to_string))
            .collect(),
        None => Vec::new(),
    }
}

#[test]
fn mcp_server_completes_initialize_and_lists_tools() {
    let temp = TempDir::new();
    let root = temp.path();
    write_minimal_config(root);

    let mut child = Command::new(bin_path())
        .args([
            "mcp-server",
            "--root",
            root.to_str().unwrap(),
            "--client-id",
            "codex",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mcpace mcp-server");

    let mut stdin = child.stdin.take().expect("child stdin");
    let child_stdout = child.stdout.take().expect("child stdout");
    let mut reader = BufReader::new(child_stdout);
    let mut line = String::new();

    writeln!(
        stdin,
        "{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{{\"protocolVersion\":\"2025-11-25\",\"capabilities\":{{}},\"clientInfo\":{{\"name\":\"codex-cli\",\"version\":\"0.122.0\"}}}}}}"
    )
    .unwrap();
    line.clear();
    reader
        .read_line(&mut line)
        .expect("read initialize response");
    assert!(line.contains(r#""jsonrpc":"2.0""#), "line: {}", line);
    assert!(
        line.contains(r#""protocolVersion":"2025-11-25""#),
        "line: {}",
        line
    );
    assert!(
        line.contains(r#""serverInfo":{"name":"mcpace""#),
        "line: {}",
        line
    );
    assert!(
        line.contains(r#"hub was started automatically"#),
        "line: {}",
        line
    );

    let hub_status = run(&["hub", "status", "--json", "--root", root.to_str().unwrap()]);
    assert!(
        hub_status.status.success(),
        "stderr: {}",
        stderr(&hub_status)
    );
    let hub_text = stdout(&hub_status);
    assert!(
        hub_text.contains(r#""status": "running""#),
        "stdout: {}",
        hub_text
    );

    writeln!(
        stdin,
        "{{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}}"
    )
    .unwrap();
    writeln!(
        stdin,
        "{{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}}"
    )
    .unwrap();
    line.clear();
    reader
        .read_line(&mut line)
        .expect("read tools/list response");
    assert!(line.contains(r#""name":"doctor""#), "line: {}", line);
    assert!(line.contains(r#""name":"client_plan""#), "line: {}", line);
    assert!(line.contains(r#""name":"client_export""#), "line: {}", line);
    assert!(
        line.contains(r#""name":"runtime_leases""#),
        "line: {}",
        line
    );
    assert!(
        line.contains(r#""name":"runtime_acquire""#),
        "line: {}",
        line
    );
    assert!(line.contains(r#""name":"runtime_renew""#), "line: {}", line);
    assert!(
        line.contains(r#""name":"runtime_release""#),
        "line: {}",
        line
    );
    assert!(
        line.contains(r#""name":"surface_manifest""#),
        "line: {}",
        line
    );
    assert!(
        line.contains(r#""name":"upstream_tools""#),
        "line: {}",
        line
    );
    assert!(
        line.contains(r#""name":"upstream_catalog""#),
        "line: {}",
        line
    );
    assert!(
        line.contains(r#""name":"upstream_probe""#),
        "line: {}",
        line
    );
    assert!(
        line.contains(r#""name":"upstream_policy_audit""#),
        "line: {}",
        line
    );
    assert!(
        line.contains(r#""name":"upstream_policy_suggest""#),
        "line: {}",
        line
    );
    assert!(line.contains(r#""name":"upstream_call""#), "line: {}", line);
    assert!(
        line.contains(r#""name":"upstream_batch""#),
        "line: {}",
        line
    );
    assert!(
        line.contains(r#""name":"browser_status""#),
        "line: {}",
        line
    );

    writeln!(
        stdin,
        "{{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"tools/call\",\"params\":{{\"name\":\"client_export\",\"arguments\":{{}}}}}}"
    )
    .unwrap();
    line.clear();
    reader
        .read_line(&mut line)
        .expect("read tools/call client_export response");
    assert!(line.contains(r#""isError":false"#), "line: {}", line);
    assert!(
        line.contains(r#""clientTargetId":"codex""#),
        "line: {}",
        line
    );
    assert!(line.contains(r#""canConnectToday":true"#), "line: {}", line);
    assert!(
        line.contains(r#""exportMode":"local-streamable-http""#),
        "line: {}",
        line
    );
    assert!(
        line.contains(r#""urlTemplate":"http://127.0.0.1:39022/mcp""#),
        "line: {}",
        line
    );

    drop(stdin);
    let status = child.wait().expect("wait for child");
    assert!(status.success(), "status: {:?}", status);

    let down = run(&["hub", "down", "--json", "--root", root.to_str().unwrap()]);
    assert!(down.status.success(), "stderr: {}", stderr(&down));
}

#[test]
fn mcp_server_upstream_call_attaches_and_releases_runtime_lease() {
    let temp = TempDir::new();
    let root = temp.path();
    let started_path = write_fake_upstream_config(root);

    let mut child = Command::new(bin_path())
        .args([
            "mcp-server",
            "--root",
            root.to_str().unwrap(),
            "--client-id",
            "codex",
            "--session-id",
            "lease-forwarding",
            "--project-root",
            root.to_str().unwrap(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mcpace mcp-server");

    let mut stdin = child.stdin.take().expect("child stdin");
    let child_stdout = child.stdout.take().expect("child stdout");
    let mut reader = BufReader::new(child_stdout);
    let mut line = String::new();

    writeln!(
        stdin,
        "{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{{\"protocolVersion\":\"2025-11-25\",\"capabilities\":{{}},\"clientInfo\":{{\"name\":\"codex-cli\",\"version\":\"0.122.0\"}}}}}}"
    )
    .unwrap();
    reader
        .read_line(&mut line)
        .expect("read initialize response");
    assert!(
        line.contains(r#""serverInfo":{"name":"mcpace""#),
        "line: {}",
        line
    );

    writeln!(
        stdin,
        "{{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}}"
    )
    .unwrap();
    write_json_line(
        &mut stdin,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"upstream_call","arguments":{"server":"fake","tool":"echo","arguments":{"message":"hello"},"timeoutMs":5000}}}"#,
    );
    line.clear();
    reader
        .read_line(&mut line)
        .expect("read upstream_call response");
    assert!(line.contains(r#""isError":false"#), "line: {}", line);
    assert!(line.contains(r#""upstreamOk":true"#), "line: {}", line);
    assert!(line.contains(r#""leaseAttached":true"#), "line: {}", line);
    assert!(line.contains(r#""leaseReleased":true"#), "line: {}", line);
    assert!(line.contains(r#""leaseId":"lease:fake"#), "line: {}", line);
    assert!(
        started_path.is_file(),
        "fake upstream should have been launched"
    );

    write_json_line(
        &mut stdin,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"runtime_leases","arguments":{}}}"#,
    );
    line.clear();
    reader
        .read_line(&mut line)
        .expect("read runtime_leases response");
    assert!(line.contains(r#""isError":false"#), "line: {}", line);
    assert!(line.contains(r#""activeLeaseCount":0"#), "line: {}", line);

    drop(stdin);
    let status = child.wait().expect("wait for child");
    assert!(status.success(), "status: {:?}", status);

    let down = run(&["hub", "down", "--json", "--root", root.to_str().unwrap()]);
    assert!(down.status.success(), "stderr: {}", stderr(&down));
}

#[test]
fn mcp_server_reuses_upstream_session_pool_for_same_session_calls() {
    let temp = TempDir::new();
    let root = temp.path();
    let started_path = write_fake_upstream_config(root);

    let mut child = Command::new(bin_path())
        .args([
            "mcp-server",
            "--root",
            root.to_str().unwrap(),
            "--client-id",
            "codex",
            "--session-id",
            "pooled-forwarding",
            "--project-root",
            root.to_str().unwrap(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mcpace mcp-server");

    let mut stdin = child.stdin.take().expect("child stdin");
    let child_stdout = child.stdout.take().expect("child stdout");
    let mut reader = BufReader::new(child_stdout);
    let mut line = String::new();

    writeln!(
        stdin,
        "{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{{\"protocolVersion\":\"2025-11-25\",\"capabilities\":{{}},\"clientInfo\":{{\"name\":\"codex-cli\",\"version\":\"0.122.0\"}}}}}}"
    )
    .unwrap();
    reader
        .read_line(&mut line)
        .expect("read initialize response");
    assert!(
        line.contains(r#""serverInfo":{"name":"mcpace""#),
        "line: {}",
        line
    );

    writeln!(
        stdin,
        "{{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}}"
    )
    .unwrap();
    write_json_line(
        &mut stdin,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"upstream_call","arguments":{"server":"fake","tool":"echo","arguments":{"message":"first"},"timeoutMs":5000}}}"#,
    );
    line.clear();
    reader
        .read_line(&mut line)
        .expect("read first pooled upstream_call response");
    let first = mcp_structured_content(&line);
    assert_eq!(first["upstreamOk"].as_bool(), Some(true), "line: {line}");
    assert_eq!(
        first["sessionPoolEnabled"].as_bool(),
        Some(true),
        "line: {line}"
    );
    assert_eq!(
        first["sessionPoolHit"].as_bool(),
        Some(false),
        "line: {line}"
    );
    assert_eq!(
        fs::read_to_string(&started_path)
            .expect("started log")
            .lines()
            .count(),
        1,
        "first call should start one upstream process"
    );

    write_json_line(
        &mut stdin,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"upstream_call","arguments":{"server":"fake","tool":"echo","arguments":{"message":"second"},"timeoutMs":5000}}}"#,
    );
    line.clear();
    reader
        .read_line(&mut line)
        .expect("read second pooled upstream_call response");
    let second = mcp_structured_content(&line);
    assert_eq!(second["upstreamOk"].as_bool(), Some(true), "line: {line}");
    assert_eq!(
        second["sessionPoolHit"].as_bool(),
        Some(true),
        "line: {line}"
    );
    assert_eq!(
        second["sessionPoolSessionCallCount"].as_u64(),
        Some(2),
        "line: {line}"
    );
    assert_eq!(
        fs::read_to_string(&started_path)
            .expect("started log")
            .lines()
            .count(),
        1,
        "second call should reuse the initialized upstream process"
    );

    write_json_line(
        &mut stdin,
        r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"runtime_leases","arguments":{}}}"#,
    );
    line.clear();
    reader
        .read_line(&mut line)
        .expect("read runtime_leases response");
    let leases = mcp_structured_content(&line);
    assert_eq!(leases["activeLeaseCount"].as_u64(), Some(0), "line: {line}");

    drop(stdin);
    let status = child.wait().expect("wait for child");
    assert!(status.success(), "status: {:?}", status);

    let down = run(&["hub", "down", "--json", "--root", root.to_str().unwrap()]);
    assert!(down.status.success(), "stderr: {}", stderr(&down));
}

#[test]
fn mcp_server_upstream_call_conservatively_leases_settings_only_server() {
    let temp = TempDir::new();
    let root = temp.path();
    let started_path = write_fake_upstream_settings_only_config(root);

    let mut child = Command::new(bin_path())
        .args([
            "mcp-server",
            "--root",
            root.to_str().unwrap(),
            "--client-id",
            "codex",
            "--session-id",
            "settings-only",
            "--project-root",
            root.to_str().unwrap(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mcpace mcp-server");

    let mut stdin = child.stdin.take().expect("child stdin");
    let child_stdout = child.stdout.take().expect("child stdout");
    let mut reader = BufReader::new(child_stdout);
    let mut line = String::new();

    writeln!(
        stdin,
        "{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{{\"protocolVersion\":\"2025-11-25\",\"capabilities\":{{}},\"clientInfo\":{{\"name\":\"codex-cli\",\"version\":\"0.122.0\"}}}}}}"
    )
    .unwrap();
    reader
        .read_line(&mut line)
        .expect("read initialize response");
    assert!(
        line.contains(r#""serverInfo":{"name":"mcpace""#),
        "line: {}",
        line
    );

    writeln!(
        stdin,
        "{{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}}"
    )
    .unwrap();
    write_json_line(
        &mut stdin,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"upstream_call","arguments":{"server":"fake","tool":"echo","arguments":{"message":"settings-only"},"timeoutMs":5000}}}"#,
    );
    line.clear();
    reader
        .read_line(&mut line)
        .expect("read settings-only upstream_call response");
    let structured = mcp_structured_content(&line);
    assert_eq!(
        structured["upstreamOk"].as_bool(),
        Some(true),
        "line: {line}"
    );
    assert_eq!(
        structured["leaseAttached"].as_bool(),
        Some(true),
        "line: {line}"
    );
    assert!(
        structured["leaseBypassReason"].is_null(),
        "settings-only servers should get a conservative lease instead of bypassing scheduling: {line}"
    );
    assert_eq!(
        structured["lease"]["route"]["schedulerLane"].as_str(),
        Some("settings-only-conservative"),
        "line: {line}"
    );
    assert!(
        started_path.is_file(),
        "fake upstream should have been launched"
    );

    write_json_line(
        &mut stdin,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"runtime_leases","arguments":{}}}"#,
    );
    line.clear();
    reader
        .read_line(&mut line)
        .expect("read runtime_leases response");
    let leases = mcp_structured_content(&line);
    assert_eq!(leases["activeLeaseCount"].as_u64(), Some(0), "line: {line}");

    drop(stdin);
    let status = child.wait().expect("wait for child");
    assert!(status.success(), "status: {:?}", status);

    let down = run(&["hub", "down", "--json", "--root", root.to_str().unwrap()]);
    assert!(down.status.success(), "stderr: {}", stderr(&down));
}

#[test]
fn mcp_server_upstream_call_renews_short_lease_and_ignores_stale_ids() {
    let temp = TempDir::new();
    let root = temp.path();
    write_fake_upstream_config(root);

    let mut child = Command::new(bin_path())
        .args([
            "mcp-server",
            "--root",
            root.to_str().unwrap(),
            "--client-id",
            "codex",
            "--session-id",
            "lease-heartbeat",
            "--project-root",
            root.to_str().unwrap(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mcpace mcp-server");

    let mut stdin = child.stdin.take().expect("child stdin");
    let child_stdout = child.stdout.take().expect("child stdout");
    let mut reader = BufReader::new(child_stdout);
    let mut line = String::new();

    writeln!(
        stdin,
        "{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{{\"protocolVersion\":\"2025-11-25\",\"capabilities\":{{}},\"clientInfo\":{{\"name\":\"codex-cli\",\"version\":\"0.122.0\"}}}}}}"
    )
    .unwrap();
    reader
        .read_line(&mut line)
        .expect("read initialize response");
    assert!(
        line.contains(r#""serverInfo":{"name":"mcpace""#),
        "line: {}",
        line
    );

    writeln!(
        stdin,
        "{{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}}"
    )
    .unwrap();
    write_json_line(
        &mut stdin,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"upstream_call","arguments":{"server":"fake","tool":"echo","arguments":{"delayMs":900,"emitStale":true},"timeoutMs":5000,"ttlMs":300}}}"#,
    );
    line.clear();
    reader
        .read_line(&mut line)
        .expect("read upstream_call response");
    let structured = mcp_structured_content(&line);
    assert_eq!(
        structured["upstreamOk"].as_bool(),
        Some(true),
        "line: {line}"
    );
    assert_eq!(
        structured["leaseAttached"].as_bool(),
        Some(true),
        "line: {line}"
    );
    assert_eq!(
        structured["leaseReleased"].as_bool(),
        Some(true),
        "line: {line}"
    );
    assert_eq!(
        structured["leaseHeartbeatStarted"].as_bool(),
        Some(true),
        "line: {line}"
    );
    assert!(
        structured["leaseHeartbeatRenewalCount"]
            .as_u64()
            .unwrap_or(0)
            > 0,
        "line: {line}"
    );
    let upstream_text = structured["upstreamResult"]["content"][0]["text"]
        .as_str()
        .unwrap_or_default();
    assert!(
        !upstream_text.contains("stale response must be ignored"),
        "line: {line}"
    );

    write_json_line(
        &mut stdin,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"runtime_leases","arguments":{}}}"#,
    );
    line.clear();
    reader
        .read_line(&mut line)
        .expect("read runtime_leases response");
    let leases = mcp_structured_content(&line);
    assert_eq!(leases["activeLeaseCount"].as_u64(), Some(0), "line: {line}");

    drop(stdin);
    let status = child.wait().expect("wait for child");
    assert!(status.success(), "status: {:?}", status);

    let down = run(&["hub", "down", "--json", "--root", root.to_str().unwrap()]);
    assert!(down.status.success(), "stderr: {}", stderr(&down));
}

#[test]
fn mcp_server_upstream_call_cancels_when_heartbeat_loses_lease() {
    let temp = TempDir::new();
    let root = temp.path();
    write_fake_upstream_config(root);

    let mut child = Command::new(bin_path())
        .args([
            "mcp-server",
            "--root",
            root.to_str().unwrap(),
            "--client-id",
            "codex",
            "--session-id",
            "lease-loss",
            "--project-root",
            root.to_str().unwrap(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mcpace mcp-server");

    let mut stdin = child.stdin.take().expect("child stdin");
    let child_stdout = child.stdout.take().expect("child stdout");
    let mut reader = BufReader::new(child_stdout);
    let mut line = String::new();

    writeln!(
        stdin,
        "{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{{\"protocolVersion\":\"2025-11-25\",\"capabilities\":{{}},\"clientInfo\":{{\"name\":\"codex-cli\",\"version\":\"0.122.0\"}}}}}}"
    )
    .unwrap();
    reader
        .read_line(&mut line)
        .expect("read initialize response");
    assert!(
        line.contains(r#""serverInfo":{"name":"mcpace""#),
        "line: {}",
        line
    );

    writeln!(
        stdin,
        "{{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}}"
    )
    .unwrap();
    write_json_line(
        &mut stdin,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"upstream_call","arguments":{"server":"fake","tool":"echo","arguments":{"delayMs":1200},"timeoutMs":5000,"ttlMs":1000}}}"#,
    );

    thread::sleep(Duration::from_millis(350));
    let lease_ids = active_lease_ids(root);
    assert_eq!(
        lease_ids.len(),
        1,
        "expected one active lease before forced loss"
    );
    let released = run(&[
        "hub",
        "lease",
        "release",
        "--json",
        "--root",
        root.to_str().unwrap(),
        "--lease-id",
        lease_ids[0].as_str(),
    ]);
    assert!(released.status.success(), "stderr: {}", stderr(&released));

    line.clear();
    reader
        .read_line(&mut line)
        .expect("read lease-loss upstream_call response");
    assert!(line.contains(r#""isError":true"#), "line: {}", line);
    assert!(line.contains("runtime lease lost"), "line: {}", line);
    assert!(
        !line.contains(r#""upstreamOk":true"#),
        "lost lease must not return a stale successful upstream result: {line}"
    );

    let lease_ids_after = active_lease_ids(root);
    assert!(
        lease_ids_after.is_empty(),
        "lease store should be empty after forced loss, got {lease_ids_after:?}"
    );

    drop(stdin);
    let status = child.wait().expect("wait for child");
    assert!(status.success(), "status: {:?}", status);

    let down = run(&["hub", "down", "--json", "--root", root.to_str().unwrap()]);
    assert!(down.status.success(), "stderr: {}", stderr(&down));
}

#[test]
fn mcp_server_upstream_call_blocks_when_runtime_lease_conflicts() {
    let temp = TempDir::new();
    let root = temp.path();
    let started_path = write_fake_upstream_config(root);

    let held = run(&[
        "hub",
        "lease",
        "acquire",
        "--json",
        "--root",
        root.to_str().unwrap(),
        "--server",
        "fake",
        "--client-id",
        "other-client",
        "--session-id",
        "held-open",
        "--project-root",
        root.to_str().unwrap(),
    ]);
    assert!(held.status.success(), "stderr: {}", stderr(&held));
    assert!(
        stdout(&held).contains(r#""status": "acquired""#),
        "stdout: {}",
        stdout(&held)
    );

    let mut child = Command::new(bin_path())
        .args([
            "mcp-server",
            "--root",
            root.to_str().unwrap(),
            "--client-id",
            "codex",
            "--session-id",
            "blocked-forwarding",
            "--project-root",
            root.to_str().unwrap(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mcpace mcp-server");

    let mut stdin = child.stdin.take().expect("child stdin");
    let child_stdout = child.stdout.take().expect("child stdout");
    let mut reader = BufReader::new(child_stdout);
    let mut line = String::new();

    writeln!(
        stdin,
        "{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{{\"protocolVersion\":\"2025-11-25\",\"capabilities\":{{}},\"clientInfo\":{{\"name\":\"codex-cli\",\"version\":\"0.122.0\"}}}}}}"
    )
    .unwrap();
    reader
        .read_line(&mut line)
        .expect("read initialize response");
    assert!(
        line.contains(r#""serverInfo":{"name":"mcpace""#),
        "line: {}",
        line
    );

    writeln!(
        stdin,
        "{{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}}"
    )
    .unwrap();
    write_json_line(
        &mut stdin,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"upstream_call","arguments":{"server":"fake","tool":"echo","arguments":{},"timeoutMs":5000}}}"#,
    );
    line.clear();
    reader
        .read_line(&mut line)
        .expect("read blocked upstream_call response");
    assert!(line.contains(r#""isError":true"#), "line: {}", line);
    assert!(line.contains("runtime lease blocked"), "line: {}", line);
    assert!(line.contains("already held"), "line: {}", line);
    assert!(
        !started_path.exists(),
        "fake upstream should not launch when lease acquisition is blocked"
    );

    drop(stdin);
    let status = child.wait().expect("wait for child");
    assert!(status.success(), "status: {:?}", status);

    let down = run(&["hub", "down", "--json", "--root", root.to_str().unwrap()]);
    assert!(down.status.success(), "stderr: {}", stderr(&down));
}

#[test]
fn mcp_server_repairs_corrupt_hub_state_before_initialize() {
    let temp = TempDir::new();
    let root = temp.path();
    write_minimal_config(root);
    let hub_dir = root.join("data").join("runtime").join("hub");
    fs::create_dir_all(&hub_dir).unwrap();
    fs::write(hub_dir.join("state.json"), "{ not-valid-json").unwrap();

    let mut child = Command::new(bin_path())
        .args([
            "mcp-server",
            "--root",
            root.to_str().unwrap(),
            "--client-id",
            "codex",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mcpace mcp-server");

    let mut stdin = child.stdin.take().expect("child stdin");
    let child_stdout = child.stdout.take().expect("child stdout");
    let mut reader = BufReader::new(child_stdout);
    let mut line = String::new();

    writeln!(
        stdin,
        "{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{{\"protocolVersion\":\"2025-11-25\",\"capabilities\":{{}},\"clientInfo\":{{\"name\":\"codex-cli\",\"version\":\"0.122.0\"}}}}}}"
    )
    .unwrap();
    line.clear();
    reader
        .read_line(&mut line)
        .expect("read initialize response");
    assert!(
        line.contains("corrupt hub state was repaired automatically"),
        "line: {}",
        line
    );
    assert!(
        line.contains("hub was started automatically"),
        "line: {}",
        line
    );

    drop(stdin);
    let status = child.wait().expect("wait for child");
    assert!(status.success(), "status: {:?}", status);

    let hub_status = run(&["hub", "status", "--json", "--root", root.to_str().unwrap()]);
    assert!(
        hub_status.status.success(),
        "stderr: {}",
        stderr(&hub_status)
    );
    let hub_text = stdout(&hub_status);
    assert!(
        hub_text.contains(r#""status": "running""#),
        "stdout: {}",
        hub_text
    );

    let down = run(&["hub", "down", "--json", "--root", root.to_str().unwrap()]);
    assert!(down.status.success(), "stderr: {}", stderr(&down));
}
