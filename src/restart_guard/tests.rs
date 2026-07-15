use super::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_path() -> PathBuf {
    std::env::temp_dir().join(format!(
        "mcpace-restart-guard-test-{}-{}-{}.log",
        std::process::id(),
        runtimepaths::unix_time_ms(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}

#[test]
fn restart_guard_blocks_tight_restart_loops_and_clear_recovers() {
    let path = temp_path();
    for _ in 0..DEFAULT_RESTART_MAX_ATTEMPTS {
        check_and_record(&path, "serve").unwrap();
    }
    let error = check_and_record(&path, "serve").unwrap_err();
    assert!(error.contains("restart guard blocked launch"));
    clear(&path);
    check_and_record(&path, "serve").unwrap();
    clear(&path);
}
