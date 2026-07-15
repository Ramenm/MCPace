use super::initialize_layout;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempRoot {
    path: PathBuf,
}

impl TempRoot {
    fn new(label: &str) -> Self {
        let sequence = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "mcpace-init-{label}-{}-{sequence}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        Self { path }
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn assert_bootstrapped(root: &TempRoot) {
    assert!(root.path.join("mcpace.config.json").is_file());
    assert!(root.path.join("mcp_settings.json").is_file());
    assert!(root.path.join("mcp_settings.d").is_dir());
    assert!(root
        .path
        .join("data/runtime/project-registry.json")
        .is_file());
    assert!(root.path.join("data/runtime/hub/leases.json").is_file());
}

#[test]
fn init_bootstraps_a_missing_root_and_is_idempotent() {
    let root = TempRoot::new("missing");

    let first = initialize_layout(&root.path).expect("missing root should be initialized");
    assert_bootstrapped(&root);
    assert!(first.ready_for_read_only_ops);
    assert!(first.ready_for_runtime_ops);

    initialize_layout(&root.path).expect("repeated initialization should be safe");
    assert_bootstrapped(&root);
}

#[test]
fn init_bootstraps_a_preexisting_empty_root() {
    let root = TempRoot::new("empty");
    fs::create_dir_all(&root.path).expect("create empty root");

    let report = initialize_layout(&root.path).expect("empty root should be initialized");

    assert_bootstrapped(&root);
    assert!(report.ready_for_read_only_ops);
    assert!(report.ready_for_runtime_ops);
}
