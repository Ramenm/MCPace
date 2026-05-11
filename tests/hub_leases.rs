mod common;

use common::*;
use std::fs;

fn write_config(root: &std::path::Path, servers: &str, settings: &str) {
    fs::write(
        root.join("mcpace.config.json"),
        format!(
            r#"{{
  "version": "0.3.5",
  "profiles": {{
    "runtime": {{
      "default": "safe",
      "profiles": {{ "safe": {{ "description": "Safe", "serverOverrides": {{}} }} }}
    }}
  }},
  "servers": {}
}}"#,
            servers
        ),
    )
    .unwrap();
    fs::write(root.join("mcp_settings.json"), settings).unwrap();
}

fn json_string_field(text: &str, key: &str) -> String {
    let needle = format!("\"{}\": \"", key);
    let start = text.find(&needle).expect("field exists") + needle.len();
    let rest = &text[start..];
    let end = rest.find('"').expect("field terminates");
    rest[..end].to_string()
}

#[test]
fn hub_lease_blocks_conflicting_host_lock_until_release() {
    let temp = TempDir::new();
    let root = temp.path();
    write_config(
        root,
        r#"{
    "windows": {
      "kind": "host-stdio",
      "required": true,
      "policy": {
        "scopeClass": "shared-exclusive",
        "concurrencyPolicy": "single-session",
        "stateBinding": "host-desktop",
        "credentialBinding": "none",
        "parallelismLimit": 1,
        "conflictDomain": "windows-desktop",
        "hostLock": "desktop-session"
      },
      "installer": { "installTarget": "none", "installMethod": "none", "installPackage": "", "verifyCommand": "" }
    }
  }"#,
        r#"{"mcpServers":{"windows":{"enabled":true,"type":"stdio","command":"node"}}}"#,
    );

    let first = run(&[
        "hub",
        "lease",
        "acquire",
        "--json",
        "--root",
        root.to_str().unwrap(),
        "--server",
        "windows",
        "--client-id",
        "codex",
        "--session-id",
        "first",
    ]);
    assert!(first.status.success(), "stderr: {}", stderr(&first));
    let first_text = stdout(&first);
    assert!(
        first_text.contains(r#""status": "acquired""#),
        "stdout: {}",
        first_text
    );
    assert!(first_text.contains(r#""requestStrategy": "exclusive-host-lock""#));
    let lease_id = json_string_field(&first_text, "leaseId");

    let second = run(&[
        "hub",
        "lease",
        "acquire",
        "--json",
        "--root",
        root.to_str().unwrap(),
        "--server",
        "windows",
        "--client-id",
        "codex",
        "--session-id",
        "second",
    ]);
    assert!(!second.status.success(), "second acquire should be blocked");
    let second_text = stdout(&second);
    assert!(
        second_text.contains(r#""status": "blocked""#),
        "stdout: {}",
        second_text
    );
    assert!(
        second_text.contains("windows-desktop"),
        "stdout: {}",
        second_text
    );
    assert!(
        second_text.contains(r#""activeLeaseCount": 1"#),
        "stdout: {}",
        second_text
    );

    let release = run(&[
        "hub",
        "lease",
        "release",
        "--json",
        "--root",
        root.to_str().unwrap(),
        "--lease-id",
        &lease_id,
    ]);
    assert!(release.status.success(), "stderr: {}", stderr(&release));
    assert!(stdout(&release).contains(r#""status": "released""#));

    let third = run(&[
        "hub",
        "lease",
        "acquire",
        "--json",
        "--root",
        root.to_str().unwrap(),
        "--server",
        "windows",
        "--client-id",
        "codex",
        "--session-id",
        "third",
    ]);
    assert!(third.status.success(), "stderr: {}", stderr(&third));
    assert!(stdout(&third).contains(r#""status": "acquired""#));
}

#[test]
fn hub_lease_takeover_replaces_conflict_only_for_same_session() {
    let temp = TempDir::new();
    let root = temp.path();
    write_config(
        root,
        r#"{
    "windows": {
      "kind": "host-stdio",
      "required": true,
      "policy": {
        "scopeClass": "shared-exclusive",
        "concurrencyPolicy": "single-session",
        "stateBinding": "host-desktop",
        "credentialBinding": "none",
        "parallelismLimit": 1,
        "conflictDomain": "windows-desktop",
        "hostLock": "desktop-session"
      },
      "installer": { "installTarget": "none", "installMethod": "none", "installPackage": "", "verifyCommand": "" }
    }
  }"#,
        r#"{"mcpServers":{"windows":{"enabled":true,"type":"stdio","command":"node"}}}"#,
    );

    let first = run(&[
        "hub",
        "lease",
        "acquire",
        "--json",
        "--root",
        root.to_str().unwrap(),
        "--server",
        "windows",
        "--client-id",
        "codex",
        "--session-id",
        "same-session",
    ]);
    assert!(first.status.success(), "stderr: {}", stderr(&first));
    let first_text = stdout(&first);
    let first_lease_id = json_string_field(&first_text, "leaseId");

    let other_session_takeover = run(&[
        "hub",
        "lease",
        "acquire",
        "--json",
        "--root",
        root.to_str().unwrap(),
        "--server",
        "windows",
        "--client-id",
        "codex",
        "--session-id",
        "other-session",
        "--takeover",
    ]);
    assert!(
        !other_session_takeover.status.success(),
        "takeover must stay scoped to the same sessionLeaseId"
    );
    assert!(stdout(&other_session_takeover).contains(r#""status": "blocked""#));

    let same_session_takeover = run(&[
        "hub",
        "lease",
        "acquire",
        "--json",
        "--root",
        root.to_str().unwrap(),
        "--server",
        "windows",
        "--client-id",
        "codex",
        "--session-id",
        "same-session",
        "--takeover",
    ]);
    assert!(
        same_session_takeover.status.success(),
        "stderr: {}",
        stderr(&same_session_takeover)
    );
    let takeover_text = stdout(&same_session_takeover);
    assert!(takeover_text.contains(r#""status": "acquired""#));
    assert!(takeover_text.contains(r#""takeover": true"#));
    assert!(takeover_text.contains(r#""takenOverLease""#));
    assert!(takeover_text.contains(&first_lease_id));
    let replacement_lease_id = json_string_field(&takeover_text, "leaseId");
    assert_ne!(first_lease_id, replacement_lease_id);

    let leases = run(&[
        "hub",
        "lease",
        "list",
        "--json",
        "--root",
        root.to_str().unwrap(),
    ]);
    assert!(leases.status.success(), "stderr: {}", stderr(&leases));
    let leases_text = stdout(&leases);
    assert!(leases_text.contains(r#""activeLeaseCount": 1"#));
    assert!(leases_text.contains(&replacement_lease_id));
    assert!(!leases_text.contains(&first_lease_id));
}

#[test]
fn hub_lease_requires_project_root_but_allows_distinct_project_partitions() {
    let temp = TempDir::new();
    let root = temp.path();
    write_config(
        root,
        r#"{
    "filesystem": {
      "kind": "container-stdio",
      "required": true,
      "policy": {
        "scopeClass": "project-local",
        "concurrencyPolicy": "isolated-per-project",
        "stateBinding": "workspace-roots",
        "credentialBinding": "none"
      },
      "installer": { "installTarget": "none", "installMethod": "none", "installPackage": "", "verifyCommand": "" }
    }
  }"#,
        r#"{"mcpServers":{"filesystem":{"enabled":true,"type":"stdio","command":"node"}}}"#,
    );

    let missing_project = run(&[
        "hub",
        "lease",
        "acquire",
        "--json",
        "--root",
        root.to_str().unwrap(),
        "--server",
        "filesystem",
        "--client-id",
        "codex",
        "--session-id",
        "one",
    ]);
    assert!(!missing_project.status.success());
    let missing_text = stdout(&missing_project);
    assert!(
        missing_text.contains(r#""status": "blocked""#),
        "stdout: {}",
        missing_text
    );
    assert!(
        missing_text.contains("needs an explicit project root"),
        "stdout: {}",
        missing_text
    );

    let project_a = run(&[
        "hub",
        "lease",
        "acquire",
        "--json",
        "--root",
        root.to_str().unwrap(),
        "--server",
        "filesystem",
        "--client-id",
        "codex",
        "--session-id",
        "one",
        "--project-root",
        "/repo/a",
    ]);
    assert!(project_a.status.success(), "stderr: {}", stderr(&project_a));
    let text_a = stdout(&project_a);
    assert!(
        text_a.contains(r#""status": "acquired""#),
        "stdout: {}",
        text_a
    );
    assert!(text_a.contains(r#""projectBindingKey": "project:/repo/a""#));

    let project_b = run(&[
        "hub",
        "lease",
        "acquire",
        "--json",
        "--root",
        root.to_str().unwrap(),
        "--server",
        "filesystem",
        "--client-id",
        "codex",
        "--session-id",
        "two",
        "--project-root",
        "/repo/b",
    ]);
    assert!(project_b.status.success(), "stderr: {}", stderr(&project_b));
    let text_b = stdout(&project_b);
    assert!(
        text_b.contains(r#""status": "acquired""#),
        "stdout: {}",
        text_b
    );
    assert!(text_b.contains(r#""projectBindingKey": "project:/repo/b""#));

    let leases = run(&[
        "hub",
        "lease",
        "list",
        "--json",
        "--root",
        root.to_str().unwrap(),
    ]);
    assert!(leases.status.success(), "stderr: {}", stderr(&leases));
    assert!(stdout(&leases).contains(r#""activeLeaseCount": 2"#));
}

#[test]
fn hub_lease_renew_extends_existing_lease_and_stale_lock_recovers() {
    let temp = TempDir::new();
    let root = temp.path();
    write_config(
        root,
        r#"{
    "windows": {
      "kind": "host-stdio",
      "required": true,
      "policy": {
        "scopeClass": "shared-exclusive",
        "concurrencyPolicy": "single-session",
        "stateBinding": "host-desktop",
        "credentialBinding": "none",
        "parallelismLimit": 1,
        "conflictDomain": "windows-desktop",
        "hostLock": "desktop-session"
      },
      "installer": { "installTarget": "none", "installMethod": "none", "installPackage": "", "verifyCommand": "" }
    }
  }"#,
        r#"{"mcpServers":{"windows":{"enabled":true,"type":"stdio","command":"node"}}}"#,
    );

    let hub_dir = root.join("data").join("runtime").join("hub");
    fs::create_dir_all(&hub_dir).unwrap();
    fs::write(hub_dir.join("leases.lock"), r#"{"pid":0,"createdAtMs":0}"#).unwrap();

    let acquire = run(&[
        "hub",
        "lease",
        "acquire",
        "--json",
        "--root",
        root.to_str().unwrap(),
        "--server",
        "windows",
        "--client-id",
        "codex",
        "--session-id",
        "renew-me",
        "--ttl-ms",
        "1000",
    ]);
    assert!(acquire.status.success(), "stderr: {}", stderr(&acquire));
    let acquire_text = stdout(&acquire);
    assert!(
        acquire_text.contains(r#""status": "acquired""#),
        "stdout: {}",
        acquire_text
    );
    let lease_id = json_string_field(&acquire_text, "leaseId");

    let renew = run(&[
        "hub",
        "lease",
        "renew",
        "--json",
        "--root",
        root.to_str().unwrap(),
        "--lease-id",
        &lease_id,
        "--ttl-ms",
        "5000",
    ]);
    assert!(renew.status.success(), "stderr: {}", stderr(&renew));
    let renew_text = stdout(&renew);
    assert!(
        renew_text.contains(r#""status": "renewed""#),
        "stdout: {}",
        renew_text
    );
    assert!(
        renew_text.contains(r#""ttlMs": 5000"#),
        "stdout: {}",
        renew_text
    );
    assert!(
        renew_text.contains(r#""renewedAtMs"#),
        "stdout: {}",
        renew_text
    );
}

#[test]
fn hub_lease_enforces_bounded_parallel_capacity() {
    let temp = TempDir::new();
    let root = temp.path();
    write_config(
        root,
        r#"{
    "search": {
      "kind": "host-stdio",
      "required": true,
      "policy": {
        "scopeClass": "shared-global",
        "concurrencyPolicy": "multi-reader",
        "stateBinding": "none",
        "credentialBinding": "none",
        "parallelismLimit": 2,
        "conflictDomain": "search-shared"
      },
      "installer": { "installTarget": "none", "installMethod": "none", "installPackage": "", "verifyCommand": "" }
    }
  }"#,
        r#"{"mcpServers":{"search":{"enabled":true,"type":"stdio","command":"node"}}}"#,
    );

    let first = run(&[
        "hub",
        "lease",
        "acquire",
        "--json",
        "--root",
        root.to_str().unwrap(),
        "--server",
        "search",
        "--client-id",
        "codex",
        "--session-id",
        "first",
    ]);
    assert!(first.status.success(), "stderr: {}", stderr(&first));

    let second = run(&[
        "hub",
        "lease",
        "acquire",
        "--json",
        "--root",
        root.to_str().unwrap(),
        "--server",
        "search",
        "--client-id",
        "codex",
        "--session-id",
        "second",
    ]);
    assert!(second.status.success(), "stderr: {}", stderr(&second));

    let third = run(&[
        "hub",
        "lease",
        "acquire",
        "--json",
        "--root",
        root.to_str().unwrap(),
        "--server",
        "search",
        "--client-id",
        "codex",
        "--session-id",
        "third",
    ]);
    assert!(!third.status.success(), "third acquire should be blocked");
    let third_text = stdout(&third);
    assert!(
        third_text.contains(r#""status": "blocked""#),
        "stdout: {}",
        third_text
    );
    assert!(
        third_text.contains("parallelism limit 2"),
        "stdout: {}",
        third_text
    );
}

#[test]
fn hub_lease_list_tracks_active_session_registry() {
    let temp = TempDir::new();
    let root = temp.path();
    write_config(
        root,
        r#"{
    "search": {
      "kind": "host-stdio",
      "required": true,
      "policy": {
        "scopeClass": "shared-global",
        "concurrencyPolicy": "multi-reader",
        "stateBinding": "none",
        "credentialBinding": "none",
        "parallelismLimit": 2,
        "conflictDomain": "search-shared"
      },
      "installer": { "installTarget": "none", "installMethod": "none", "installPackage": "", "verifyCommand": "" }
    }
  }"#,
        r#"{"mcpServers":{"search":{"enabled":true,"type":"stdio","command":"node"}}}"#,
    );

    let first = run(&[
        "hub",
        "lease",
        "acquire",
        "--json",
        "--root",
        root.to_str().unwrap(),
        "--server",
        "search",
        "--client-id",
        "codex",
        "--session-id",
        "shared",
    ]);
    assert!(first.status.success(), "stderr: {}", stderr(&first));
    let first_text = stdout(&first);
    let first_lease_id = json_string_field(&first_text, "leaseId");

    let second = run(&[
        "hub",
        "lease",
        "acquire",
        "--json",
        "--root",
        root.to_str().unwrap(),
        "--server",
        "search",
        "--client-id",
        "codex",
        "--session-id",
        "shared",
    ]);
    assert!(second.status.success(), "stderr: {}", stderr(&second));
    let second_text = stdout(&second);
    let second_lease_id = json_string_field(&second_text, "leaseId");

    let listed = run(&[
        "hub",
        "lease",
        "list",
        "--json",
        "--root",
        root.to_str().unwrap(),
    ]);
    assert!(listed.status.success(), "stderr: {}", stderr(&listed));
    let listed_text = stdout(&listed);
    assert!(listed_text.contains(r#""activeLeaseCount": 2"#));
    assert!(listed_text.contains(r#""activeSessionCount": 1"#));
    assert!(listed_text.contains(r#""sessionLeaseId": "external:shared""#));
    assert!(listed_text.contains(r#""activeLeaseIds": ["#));
    assert!(listed_text.contains(&first_lease_id));
    assert!(listed_text.contains(&second_lease_id));

    let hub_status = run(&["hub", "status", "--json", "--root", root.to_str().unwrap()]);
    assert!(
        hub_status.status.success(),
        "stderr: {}",
        stderr(&hub_status)
    );
    let hub_status_text = stdout(&hub_status);
    assert!(hub_status_text.contains(r#""activeLeaseCount": 2"#));
    assert!(hub_status_text.contains(r#""activeSessionCount": 1"#));

    let release_first = run(&[
        "hub",
        "lease",
        "release",
        "--json",
        "--root",
        root.to_str().unwrap(),
        "--lease-id",
        &first_lease_id,
    ]);
    assert!(
        release_first.status.success(),
        "stderr: {}",
        stderr(&release_first)
    );

    let listed_after_one_release = run(&[
        "hub",
        "lease",
        "list",
        "--json",
        "--root",
        root.to_str().unwrap(),
    ]);
    assert!(listed_after_one_release.status.success());
    let listed_after_one_release_text = stdout(&listed_after_one_release);
    assert!(listed_after_one_release_text.contains(r#""activeLeaseCount": 1"#));
    assert!(listed_after_one_release_text.contains(r#""activeSessionCount": 1"#));
    assert!(!listed_after_one_release_text.contains(&first_lease_id));
    assert!(listed_after_one_release_text.contains(&second_lease_id));

    let release_second = run(&[
        "hub",
        "lease",
        "release",
        "--json",
        "--root",
        root.to_str().unwrap(),
        "--lease-id",
        &second_lease_id,
    ]);
    assert!(
        release_second.status.success(),
        "stderr: {}",
        stderr(&release_second)
    );

    let empty = run(&[
        "hub",
        "lease",
        "list",
        "--json",
        "--root",
        root.to_str().unwrap(),
    ]);
    assert!(empty.status.success(), "stderr: {}", stderr(&empty));
    let empty_text = stdout(&empty);
    assert!(empty_text.contains(r#""activeLeaseCount": 0"#));
    assert!(empty_text.contains(r#""activeSessionCount": 0"#));
}
