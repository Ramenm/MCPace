use super::*;
use std::panic::{catch_unwind, AssertUnwindSafe};

fn test_key() -> UpstreamSessionKey {
    UpstreamSessionKey {
        root_path: "test-root".to_string(),
        server_name: "test-server".to_string(),
        settings_modified_ms: 0,
        settings_len: 0,
        server_fingerprint: "fingerprint".to_string(),
        client_id: String::new(),
        session_id: String::new(),
        project_root: String::new(),
        transport: "stdio".to_string(),
        execution_mode: "pool".to_string(),
        affinity_fingerprint: String::new(),
    }
}

#[test]
fn poisoned_checked_out_session_is_removed_instead_of_reused() {
    let pool = UpstreamSessionPool::with_max_sessions(1);
    let key = test_key();
    let worker = Arc::new(PooledUpstreamWorker::reserved(1_000));
    worker.initialized.store(true, Ordering::Release);
    pool.lock_state()
        .groups
        .insert(key.clone(), vec![Arc::clone(&worker)]);

    let poisoned = catch_unwind(AssertUnwindSafe(|| {
        if let Ok(_guard) = worker.session.lock() {
            panic!("poison test worker session");
        }
    }));
    assert!(poisoned.is_err());

    let mut checkout = UpstreamSessionCheckout {
        pool: &pool,
        key,
        worker: Arc::clone(&worker),
        hit: true,
        evicted_idle_count: 0,
        evicted_capacity_count: 0,
        failed: false,
    };
    let error = checkout.outcome().err().map(|value| value.to_string());
    assert!(error.is_some_and(|value| value.contains("poisoned")));
    drop(checkout);

    assert!(!worker.initialized.load(Ordering::Acquire));
    assert_eq!(pool.session_count(), 0);
}
