use super::rotate_logs_if_needed_with_max;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> Self {
        let unique = format!(
            "mcpace-runtime-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        );
        let path = env::temp_dir().join(unique);
        fs::create_dir_all(&path).expect("create temp dir");
        TempDir { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn rotate_logs_archives_oversized_file_without_launching_runtime() {
    let temp = TempDir::new();
    let log_path = temp.path().join("events.log");
    fs::write(&log_path, "x".repeat(256)).expect("write log");

    let rotated = rotate_logs_if_needed_with_max(&log_path, 64).expect("rotate logs");

    assert!(rotated);
    assert!(temp.path().join("events.log.1").is_file());
    assert!(!temp.path().join("events.log").exists());
}
