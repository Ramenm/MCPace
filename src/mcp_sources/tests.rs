use super::{load_mcp_server_registry, load_mcp_source_report, mcp_settings_fingerprint};
use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn write_validation_accepts_equivalent_safe_url_forms_and_rejects_ambiguous_keys() {
    use super::write_helpers::{
        parse_key_value_pairs, validate_env_name, validate_http_header_name,
        validate_remote_mcp_url,
    };

    assert!(validate_remote_mcp_url("HTTPS://mcp.example.test/mcp").is_ok());
    assert!(validate_remote_mcp_url("http://[::ffff:127.0.0.1]:39022/mcp").is_ok());
    assert!(validate_remote_mcp_url("http://example.test/mcp").is_err());
    assert!(validate_remote_mcp_url("http://127.0.0.1:0/mcp").is_err());

    assert!(parse_key_value_pairs(
        &[
            "Authorization=one".to_string(),
            "authorization=two".to_string()
        ],
        "--header",
        validate_http_header_name,
    )
    .is_err());
    assert!(parse_key_value_pairs(
        &["TOKEN=one".to_string(), "token=two".to_string()],
        "--env",
        validate_env_name,
    )
    .is_err());
}

struct EnvGuard {
    values: Vec<(&'static str, Option<OsString>)>,
}

impl EnvGuard {
    fn remove(keys: &[&'static str]) -> Self {
        let values = keys
            .iter()
            .map(|key| {
                let value = std::env::var_os(key);
                std::env::remove_var(key);
                (*key, value)
            })
            .collect();
        Self { values }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, value) in self.values.drain(..) {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }
}

fn temp_root() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "mcpace-source-discovery-{}-{}",
        std::process::id(),
        nonce
    ));
    fs::create_dir_all(root.join("mcp_settings.d")).expect("create source fixture");
    root
}

#[test]
fn new_fragment_is_detected_and_removed_without_process_restart() {
    let _local_server_lock = crate::LOCAL_SERVER_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _environment_lock = crate::resources::TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _environment = EnvGuard::remove(&["MCPACE_MCP_SETTINGS", "MCPACE_MCP_SETTINGS_DIRS"]);
    let root = temp_root();
    fs::write(
        root.join("mcp_settings.json"),
        r#"{"mcpServers":{"alpha":{"command":"alpha-server"}}}"#,
    )
    .expect("write root source");

    let first = load_mcp_server_registry(&root).expect("initial registry");
    assert_eq!(first.servers.len(), 1);
    assert!(first.servers.contains_key("alpha"));
    let first_fingerprint = mcp_settings_fingerprint(&root);

    let beta_path = root.join("mcp_settings.d").join("beta.JSON");
    fs::write(
        &beta_path,
        r#"{"servers":{"Beta Server":{"url":"https://mcp.example.test/mcp"}}}"#,
    )
    .expect("write newly discovered fragment");
    fs::write(root.join("mcp_settings.d").join("ignored.txt"), "not json")
        .expect("write ignored source");

    let second = load_mcp_server_registry(&root).expect("registry after add");
    assert_eq!(second.servers.len(), 2);
    assert!(second.servers.contains_key("beta-server"));
    assert_ne!(mcp_settings_fingerprint(&root), first_fingerprint);
    let report = load_mcp_source_report(&root).expect("source report after add");
    assert!(report.source_statuses.iter().any(|source| {
        source.origin == "default-dir"
            && source.server_count == 1
            && source.path.ends_with("beta.JSON")
    }));

    fs::remove_file(beta_path).expect("remove fragment");
    let third = load_mcp_server_registry(&root).expect("registry after remove");
    assert_eq!(third.servers.len(), 1);
    assert!(!third.servers.contains_key("beta-server"));

    let _ = fs::remove_dir_all(root);
}
