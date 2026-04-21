mod common;

use common::*;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

fn write_minimal_config(root: &std::path::Path) {
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.0",
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
