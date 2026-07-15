use super::*;
use crate::json::parse_str;

#[test]
fn shared_mode_uses_one_global_affinity_key() {
    let policy = ExecutionPolicy::for_mode(ExecutionMode::Shared);
    let first = policy
        .affinity_key(&ExecutionAffinityContext {
            client_id: Some("cursor".to_string()),
            session_id: Some("chat-a".to_string()),
            project_root: Some("/a".to_string()),
            transport: Some("stdio".to_string()),
            ..ExecutionAffinityContext::default()
        })
        .unwrap();
    let second = policy
        .affinity_key(&ExecutionAffinityContext {
            client_id: Some("codex".to_string()),
            session_id: Some("chat-b".to_string()),
            project_root: Some("/b".to_string()),
            transport: Some("stdio".to_string()),
            ..ExecutionAffinityContext::default()
        })
        .unwrap();
    assert_eq!(first.fingerprint, second.fingerprint);
    assert_eq!(first.client_id, "shared");
}

#[test]
fn project_isolated_fails_closed_without_project() {
    let policy = ExecutionPolicy::for_mode(ExecutionMode::ProjectIsolated);
    let missing = policy.affinity_key(&ExecutionAffinityContext {
        client_id: Some("cursor".to_string()),
        session_id: Some("chat".to_string()),
        transport: Some("stdio".to_string()),
        ..ExecutionAffinityContext::default()
    });
    assert!(matches!(
        missing,
        Err(ExecutionPolicyError::MissingAffinity { dimension, .. }) if dimension == "project"
    ));
}

#[test]
fn application_session_metadata_wins_over_transport_session() {
    let policy = ExecutionPolicy::for_mode(ExecutionMode::SessionIsolated);
    let key = policy
        .affinity_key(&ExecutionAffinityContext {
            client_id: Some("cursor".to_string()),
            session_id: Some("transport-1".to_string()),
            metadata: Some(parse_str(r#"{"mcpace":{"applicationSessionId":"app-9"}}"#).unwrap()),
            ..ExecutionAffinityContext::default()
        })
        .unwrap();
    assert_eq!(key.session_id, "app-9");
    assert!(!key.fingerprint.contains("app-9"));
}

#[test]
fn explicit_server_mode_overrides_inferred_canonical_mode() {
    let defaults = parse_str(r#"{"mode":"serialized","maxQueueDepth":512}"#).unwrap();
    let canonical =
        parse_str(r#"{"scopeClass":"project-local","concurrencyPolicy":"isolated-per-project"}"#)
            .unwrap();
    let explicit = parse_str(r#"{"mode":"pool","minWorkers":2,"maxWorkers":6}"#).unwrap();
    let policy = ExecutionPolicy::resolve_with_canonical(
        Some(&defaults),
        Some(&canonical),
        Some(&explicit),
        ExecutionMode::ProjectIsolated,
    );
    assert_eq!(policy.mode, ExecutionMode::Pool);
    assert_eq!(policy.min_workers, 2);
    assert_eq!(policy.max_workers, 6);
    assert_eq!(policy.max_queue_depth, 512);
}

#[test]
fn matching_defaults_are_applied_as_runtime_values() {
    let defaults = parse_str(
        r#"{"mode":"serialized","affinity":["client"],"queueTimeoutMs":0,"maxQueueDepth":512,"idleTtlMs":120000}"#,
    )
    .unwrap();
    let policy = ExecutionPolicy::resolve(Some(&defaults), None, ExecutionMode::Serialized);
    assert_eq!(policy.affinity, vec!["client"]);
    assert_eq!(policy.queue_timeout_ms, 0);
    assert_eq!(policy.max_queue_depth, 512);
    assert_eq!(policy.idle_ttl_ms, 120000);
}

#[test]
fn canonical_disabled_cannot_be_reenabled_by_execution_block() {
    let canonical = parse_str(r#"{"concurrencyPolicy":"plan-only"}"#).unwrap();
    let explicit = parse_str(r#"{"mode":"pool","maxWorkers":8}"#).unwrap();
    let policy = ExecutionPolicy::resolve_with_canonical(
        None,
        Some(&canonical),
        Some(&explicit),
        ExecutionMode::Pool,
    );
    assert_eq!(policy.mode, ExecutionMode::Disabled);
    assert_eq!(policy.max_workers, 0);
}

#[test]
fn stdio_capacity_is_one_in_flight_per_worker() {
    let mut policy = ExecutionPolicy::for_mode(ExecutionMode::Pool);
    policy.max_workers = 7;
    policy.max_in_flight_per_worker = 9;
    policy.normalize();
    assert_eq!(policy.effective_max_in_flight_per_worker("stdio"), 1);
    assert_eq!(policy.effective_capacity("stdio"), 7);
    assert_eq!(policy.effective_capacity("streamable-http"), 63);
}

#[test]
fn zero_queue_timeout_is_preserved() {
    let mut policy = ExecutionPolicy::for_mode(ExecutionMode::Serialized);
    policy.queue_timeout_ms = 0;
    policy.normalize();
    assert_eq!(policy.queue_timeout_ms, 0);
}

#[test]
fn unsupported_affinity_is_removed_with_warning() {
    let mut policy = ExecutionPolicy::for_mode(ExecutionMode::Pool);
    policy.affinity = vec!["client".to_string(), "invented".to_string()];
    policy.normalize();
    assert_eq!(policy.affinity, vec!["client"]);
    assert!(policy
        .warnings
        .iter()
        .any(|warning| warning.contains("invented")));
}
