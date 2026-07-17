use super::cleanup_report;
use crate::json::JsonValue;
use crate::{process_identity, runtimepaths};
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_root() -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!(
        "mcpace-cleanup-test-{}-{}",
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&root).unwrap();
    root
}

fn write_serve_state(root: &std::path::Path, pid: u32, identity: &str) -> std::path::PathBuf {
    let state_root = runtimepaths::resolve_state_root(root);
    runtimepaths::ensure_runtime_dir(&state_root).unwrap();
    runtimepaths::ensure_serve_dir(&state_root).unwrap();
    let state_path = runtimepaths::serve_state_path(&state_root);
    let current_exe = std::env::current_exe().unwrap();
    runtimepaths::write_text_atomic(
        &state_path,
        &JsonValue::object([
            ("rootPath", JsonValue::string(root.display().to_string())),
            (
                "stateRoot",
                JsonValue::string(state_root.display().to_string()),
            ),
            ("host", JsonValue::string("127.0.0.1")),
            ("port", JsonValue::number(39022)),
            ("pid", JsonValue::number(pid)),
            ("processIdentity", JsonValue::string(identity)),
            (
                "startedAtMs",
                JsonValue::number(runtimepaths::unix_time_ms()),
            ),
            (
                "runnerPath",
                JsonValue::string(current_exe.display().to_string()),
            ),
        ])
        .to_pretty_string(),
    )
    .unwrap();
    state_path
}

#[test]
fn runtime_cleanup_refuses_to_remove_live_serve_ownership() {
    let root = temp_root();
    let identity = process_identity::capture(std::process::id())
        .unwrap()
        .unwrap();
    let state_path = write_serve_state(&root, std::process::id(), &identity.start_token);

    let report = cleanup_report(&root, "runtime", false);
    assert_eq!(report.get("ok").and_then(JsonValue::as_bool), Some(false));
    assert!(state_path.is_file());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn runtime_cleanup_removes_stale_serve_ownership() {
    let root = temp_root();
    let state_path = write_serve_state(&root, 999_999, "stale-process-identity");

    let report = cleanup_report(&root, "runtime", false);
    assert_eq!(report.get("ok").and_then(JsonValue::as_bool), Some(true));
    assert!(!state_path.exists());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn runtime_cleanup_serializes_with_serve_startup() {
    let root = temp_root();
    let state_path = write_serve_state(&root, 999_999, "stale-process-identity");
    let state_root = runtimepaths::resolve_state_root(&root);
    let startup_guard = crate::serve::acquire_lifecycle_coordination(&state_root).unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let cleanup_root = root.clone();
    let worker = std::thread::spawn(move || {
        let report = cleanup_report(&cleanup_root, "runtime", false);
        tx.send(report).unwrap();
    });

    assert!(rx
        .recv_timeout(std::time::Duration::from_millis(100))
        .is_err());
    assert!(state_path.is_file());
    drop(startup_guard);

    let report = rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
    assert_eq!(report.get("ok").and_then(JsonValue::as_bool), Some(true));
    assert!(!state_path.exists());
    worker.join().unwrap();
    let _ = fs::remove_dir_all(root);
}
