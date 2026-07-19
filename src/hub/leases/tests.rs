use super::{acquire_lease_store_lock, retryable_windows_lock_contention};
use crate::runtimepaths;
use std::fs::{self, OpenOptions};
use std::os::windows::fs::OpenOptionsExt;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[test]
fn windows_lock_contention_error_codes_are_retryable() {
    for raw_os_error in [5, 32, 33] {
        assert!(retryable_windows_lock_contention(
            &std::io::Error::from_raw_os_error(raw_os_error)
        ));
    }
    assert!(!retryable_windows_lock_contention(
        &std::io::Error::from_raw_os_error(2)
    ));
}

#[test]
fn windows_exclusive_lock_handle_is_retried_as_contention() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let state_root = std::env::temp_dir().join(format!(
        "mcpace-lease-lock-test-{}-{nonce}",
        std::process::id()
    ));
    runtimepaths::ensure_hub_dir(&state_root).unwrap();
    let lock_path = runtimepaths::hub_lease_lock_path(&state_root);
    let blocker = OpenOptions::new()
        .create_new(true)
        .write(true)
        .share_mode(0)
        .open(&lock_path)
        .unwrap();

    let (started_tx, started_rx) = mpsc::channel();
    let (result_tx, result_rx) = mpsc::channel();
    let waiter_root = state_root.clone();
    let waiter = thread::spawn(move || {
        started_tx.send(()).unwrap();
        result_tx
            .send(acquire_lease_store_lock(&waiter_root))
            .unwrap();
    });

    started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    match result_rx.recv_timeout(Duration::from_millis(100)) {
        Err(RecvTimeoutError::Timeout) => {}
        Err(error) => panic!("lease lock waiter channel failed: {error}"),
        Ok(Err(error)) => panic!("lease lock waiter returned during contention: {error}"),
        Ok(Ok(_guard)) => panic!("lease lock waiter acquired an exclusive held lock"),
    }

    drop(blocker);
    for _ in 0..20 {
        match fs::remove_file(&lock_path) {
            Ok(()) => break,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) if retryable_windows_lock_contention(&error) => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("remove exclusive lease test lock: {error}"),
        }
    }
    assert!(
        !lock_path.exists(),
        "exclusive test lock should be removable"
    );

    let guard = result_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("lease lock waiter should finish after contention clears")
        .expect("lease lock waiter should acquire the released lock");
    drop(guard);
    waiter.join().unwrap();
    let _ = fs::remove_dir_all(state_root);
}
