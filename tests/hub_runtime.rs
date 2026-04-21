mod common;

use common::*;
use std::fs;
use std::process::Command;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

#[test]
fn init_json_creates_runtime_layout_and_seed_files() {
    let temp = TempDir::new();
    let root = temp.path();

    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.0",
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

    let output = run(&["init", "--json", "--root", root.to_str().unwrap()]);
    assert!(output.status.success());
    let text = stdout(&output);
    assert!(text.contains(r#""activeProfile": "safe""#));
    assert!(text.contains(r#""projectRegistryPath": "#));
    assert!(text.contains(r#""leaseStorePath": "#));
    assert!(root
        .join("data")
        .join("runtime")
        .join("project-registry.json")
        .is_file());
    assert!(root
        .join("data")
        .join("runtime")
        .join("hub")
        .join("leases.json")
        .is_file());
}

#[test]
fn hub_status_json_reports_stopped_state_before_start() {
    let temp = TempDir::new();
    let root = temp.path();

    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.0",
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

    let output = run(&["hub", "status", "--json", "--root", root.to_str().unwrap()]);
    assert!(output.status.success());
    let text = stdout(&output);
    assert!(text.contains(r#""status": "stopped""#));
    assert!(text.contains(r#""health": "stopped-ready""#));
    assert!(text.contains(r#""repairRecommended": false"#));
}

#[test]
fn hub_status_and_down_cleanup_orphan_lock_file() {
    let temp = TempDir::new();
    let root = temp.path();

    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.0",
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

    let hub_dir = root.join("data").join("runtime").join("hub");
    fs::create_dir_all(&hub_dir).unwrap();
    fs::write(
        hub_dir.join("lock.json"),
        r#"{
  "pid": 4242,
  "startedAtMs": 1
}"#,
    )
    .unwrap();

    let status = run(&["hub", "status", "--json", "--root", root.to_str().unwrap()]);
    assert!(status.status.success(), "stderr: {}", stderr(&status));
    let status_text = stdout(&status);
    assert!(
        status_text.contains(r#""status": "stale""#),
        "stdout: {}",
        status_text
    );
    assert!(
        status_text.contains("hub runtime lock is still present"),
        "stdout: {}",
        status_text
    );

    let down = run(&["hub", "down", "--json", "--root", root.to_str().unwrap()]);
    assert!(down.status.success(), "stderr: {}", stderr(&down));
    let down_text = stdout(&down);
    assert!(
        down_text.contains(r#""status": "stopped""#),
        "stdout: {}",
        down_text
    );
    assert!(!hub_dir.join("lock.json").exists());
}

#[test]
fn hub_status_and_repair_handle_corrupt_runtime_state() {
    let temp = TempDir::new();
    let root = temp.path();

    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.0",
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

    let hub_dir = root.join("data").join("runtime").join("hub");
    fs::create_dir_all(&hub_dir).unwrap();
    fs::write(hub_dir.join("state.json"), "{ not-valid-json").unwrap();

    let status = run(&["hub", "status", "--json", "--root", root.to_str().unwrap()]);
    assert!(status.status.success(), "stderr: {}", stderr(&status));
    let status_text = stdout(&status);
    assert!(
        status_text.contains(r#""status": "corrupt""#),
        "stdout: {}",
        status_text
    );
    assert!(
        status_text.contains(r#""repairRecommended": true"#),
        "stdout: {}",
        status_text
    );
    assert!(
        status_text.contains("state.json"),
        "stdout: {}",
        status_text
    );

    let repair = run(&["hub", "repair", "--json", "--root", root.to_str().unwrap()]);
    assert!(repair.status.success(), "stderr: {}", stderr(&repair));
    let repair_text = stdout(&repair);
    assert!(
        repair_text.contains(r#""hubStatus""#),
        "stdout: {}",
        repair_text
    );
    assert!(
        repair_text.contains(r#""status": "stopped""#),
        "stdout: {}",
        repair_text
    );

    let entries = fs::read_dir(&hub_dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
        .collect::<Vec<_>>();
    assert!(
        entries
            .iter()
            .any(|name| name.starts_with("state.json.corrupt-")),
        "entries: {:?}",
        entries
    );
}

#[test]
fn hub_up_down_round_trip_writes_event_logs() {
    let temp = TempDir::new();
    let root = temp.path();

    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.0",
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

    let up = run(&["hub", "up", "--json", "--root", root.to_str().unwrap()]);
    assert!(up.status.success(), "stderr: {}", stderr(&up));
    let up_text = stdout(&up);
    assert!(
        up_text.contains(r#""status": "running""#) || up_text.contains(r#""status": "starting""#),
        "stdout: {}",
        up_text
    );

    let logs = run(&[
        "hub",
        "logs",
        "--json",
        "--root",
        root.to_str().unwrap(),
        "--tail",
        "20",
    ]);
    assert!(logs.status.success(), "stderr: {}", stderr(&logs));
    let log_text = stdout(&logs);
    assert!(
        log_text.contains("hub_started") || log_text.contains("hub_starting"),
        "stdout: {}",
        log_text
    );

    let down = run(&["hub", "down", "--json", "--root", root.to_str().unwrap()]);
    assert!(down.status.success(), "stderr: {}", stderr(&down));
    let down_text = stdout(&down);
    if !down_text.contains(r#""status": "stopped""#) {
        let mut settled = false;
        for _ in 0..20 {
            thread::sleep(Duration::from_millis(100));
            let status = run(&["hub", "status", "--json", "--root", root.to_str().unwrap()]);
            assert!(status.status.success(), "stderr: {}", stderr(&status));
            let status_text = stdout(&status);
            if status_text.contains(r#""status": "stopped""#) {
                settled = true;
                break;
            }
        }
        assert!(settled, "stdout: {}", down_text);
    }
}

#[test]
fn hub_up_releases_captured_stdio_for_background_launcher() {
    let temp = TempDir::new();
    let root = temp.path();

    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.0",
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

    let root_string = root.to_str().unwrap().to_string();
    let root_for_thread = root_string.clone();
    let (tx, rx) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let output = run(&["hub", "up", "--json", "--root", root_for_thread.as_str()]);
        let _ = tx.send(output);
    });

    let up = match rx.recv_timeout(Duration::from_secs(15)) {
        Ok(output) => output,
        Err(_) => {
            let _ = run(&["hub", "down", "--json", "--root", root_string.as_str()]);
            kill_mcpace_processes();
            panic!("captured `mcpace hub up` did not exit within the timeout");
        }
    };

    assert!(up.status.success(), "stderr: {}", stderr(&up));
    let up_text = stdout(&up);
    assert!(
        up_text.contains(r#""status": "running""#) || up_text.contains(r#""status": "starting""#),
        "stdout: {}",
        up_text
    );

    let down = run(&["hub", "down", "--json", "--root", root_string.as_str()]);
    assert!(down.status.success(), "stderr: {}", stderr(&down));
}

fn kill_mcpace_processes() {
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/IM", "mcpace.exe", "/T", "/F"])
            .output();
    }

    #[cfg(unix)]
    {
        let _ = Command::new("pkill").args(["-f", "mcpace"]).output();
    }
}
