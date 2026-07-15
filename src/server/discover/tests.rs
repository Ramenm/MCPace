use super::{
    candidate_from_record, deduplicate_discovery_candidates, install_launcher_error,
    missing_candidate_configuration, read_optional_json, registry_list_url_with_cursor,
    registry_query_cache_path, registry_static_arguments, search_terms, ParsedArgs,
};
use crate::json::parse_str;
use crate::mcp_autoinstall::McpAutoInstallOptions;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn registry_static_named_and_positional_arguments_preserve_order() {
    let value = parse_str(
        r#"{"packageArguments":[
          {"type":"named","name":"--transport","value":"stdio"},
          {"type":"positional","value":"serve"},
          {"type":"named","name":"port","value":"9000"}
        ]}"#,
    )
    .unwrap();
    assert_eq!(
        registry_static_arguments(&value, "packageArguments"),
        vec!["--transport", "stdio", "serve", "--port", "9000"]
    );
}

#[test]
fn automatic_discovery_reports_missing_launchers_before_writing_config() {
    let options = McpAutoInstallOptions {
        spec: "pypi:example-package".to_string(),
        command: Some("definitely-missing-mcpace-launcher".to_string()),
        ..McpAutoInstallOptions::default()
    };
    assert!(install_launcher_error(&options)
        .expect("missing launcher")
        .contains("not available on PATH"));
}

#[test]
fn malformed_discovery_config_fails_closed() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "mcpace-malformed-discovery-{}-{}.json",
        std::process::id(),
        nonce
    ));
    fs::write(&path, "{ broken").unwrap();
    assert!(read_optional_json(&path).is_err());
    let _ = fs::remove_file(path);
}

#[test]
fn official_registry_package_envelope_becomes_installable_candidate() {
    let value = parse_str(
        r#"{
          "server": {
            "name": "com.example/filesystem",
            "title": "Filesystem",
            "description": "Read project files",
            "trustLevel": "approved",
            "packages": [{
              "registryType": "npm",
              "registryBaseUrl": "https://registry.npmjs.org",
              "identifier": "@example/mcp-filesystem",
              "version": "1.2.3",
              "transport": {"type": "stdio"},
              "runtimeArguments": [{"value": "-y", "type": "positional"}],
              "environmentVariables": [{"name": "API_TOKEN", "isRequired": true}],
              "packageArguments": [
                {"value": "serve", "type": "positional", "isRequired": true},
                {"name": "workspace", "type": "positional", "isRequired": true}
              ]
            }]
          },
          "_meta": {"io.modelcontextprotocol.registry/official": {"status": "active"}}
        }"#,
    )
    .expect("official package record");

    let candidate = candidate_from_record(None, &value, "registry:custom-cache.json", "review")
        .expect("registry candidate");
    assert_eq!(candidate.name, "com.example/filesystem");
    assert_eq!(candidate.install_spec, "npm:@example/mcp-filesystem@1.2.3");
    assert_eq!(candidate.registry_type, "npm");
    assert_eq!(candidate.transport, "stdio");
    assert_eq!(candidate.trust_level, "review");
    assert_eq!(candidate.required_env, vec!["API_TOKEN"]);
    assert_eq!(candidate.required_args, vec!["workspace"]);
    assert_eq!(candidate.launcher_args, vec!["-y"]);
    assert_eq!(candidate.extra_args, vec!["serve"]);
    assert!(missing_candidate_configuration(&candidate, &ParsedArgs::default()).is_some());
    let configured = ParsedArgs {
        env: vec!["API_TOKEN=${API_TOKEN}".to_string()],
        args: vec!["C:/workspace".to_string()],
        ..ParsedArgs::default()
    };
    assert!(missing_candidate_configuration(&candidate, &configured).is_none());
    let plan = crate::mcp_autoinstall::plan_auto_install(&super::install_options_from_candidate(
        &candidate,
        &configured,
    ))
    .unwrap();
    assert_eq!(
        plan.args,
        vec![
            "-y",
            "@example/mcp-filesystem@1.2.3",
            "serve",
            "C:/workspace",
        ]
    );
}

#[test]
fn registry_remote_url_placeholders_are_required_even_without_variable_metadata() {
    let value = parse_str(
        r#"{
          "server": {
            "name": "com.example/tenant",
            "remotes": [{
              "type": "streamable-http",
              "url": "https://mcp.example.com/tenants/{tenant_id}/mcp"
            }]
          }
        }"#,
    )
    .unwrap();
    let candidate =
        candidate_from_record(None, &value, "registry:custom-cache.json", "review").unwrap();
    assert_eq!(candidate.required_variables, vec!["tenant_id"]);
    assert!(missing_candidate_configuration(&candidate, &ParsedArgs::default()).is_some());
}

#[test]
fn official_registry_remote_envelope_becomes_https_candidate() {
    let value = parse_str(
        r#"{
          "server": {
            "name": "com.example/remote",
            "description": "Hosted MCP endpoint",
            "packages": [{
              "registryType": "npm",
              "identifier": "@example/http-launcher",
              "transport": {"type": "streamable-http"}
            }],
            "remotes": [{
              "type": "streamable-http",
              "url": "https://mcp.example.com/mcp",
              "headers": [{"name": "Authorization", "value": "Bearer {token}"}]
            }]
          },
          "_meta": {"io.modelcontextprotocol.registry/official": {"status": "active"}}
        }"#,
    )
    .expect("official remote record");

    let candidate = candidate_from_record(None, &value, "registry:custom-cache.json", "review")
        .expect("remote registry candidate");
    assert_eq!(
        candidate.url.as_deref(),
        Some("https://mcp.example.com/mcp")
    );
    assert_eq!(candidate.install_spec, "https://mcp.example.com/mcp");
    assert_eq!(candidate.server_type.as_deref(), Some("streamable-http"));
    assert_eq!(candidate.required_headers, vec!["Authorization"]);
    assert!(missing_candidate_configuration(&candidate, &ParsedArgs::default()).is_some());
    let configured = ParsedArgs {
        headers: vec!["authorization=Bearer test".to_string()],
        ..ParsedArgs::default()
    };
    assert!(missing_candidate_configuration(&candidate, &configured).is_none());

    let terms = search_terms("hosted endpoint");
    assert!(super::candidate_score(&candidate, &terms) > 0);
}

#[test]
fn custom_registry_bases_do_not_fall_back_to_public_package_names() {
    let value = parse_str(
        r#"{
          "server": {
            "name": "com.example/private",
            "packages": [{
              "registryType": "npm",
              "registryBaseUrl": "https://packages.internal.example/npm",
              "identifier": "private-name",
              "transport": {"type": "stdio"}
            }]
          }
        }"#,
    )
    .unwrap();
    assert!(candidate_from_record(None, &value, "registry:custom-cache.json", "review").is_none());
}

#[test]
fn unknown_registry_package_types_stay_plan_only_instead_of_becoming_npm() {
    let value = parse_str(
        r#"{
          "server": {
            "name": "com.example/unknown",
            "packages": [{
              "registryType": "unknown-registry",
              "identifier": "dangerous-name",
              "transport": {"type": "stdio"}
            }]
          },
          "_meta": {"io.modelcontextprotocol.registry/official": {"status": "active"}}
        }"#,
    )
    .unwrap();
    assert!(candidate_from_record(None, &value, "registry:custom-cache.json", "review").is_none());
}

#[test]
fn official_registry_deprecated_records_cannot_cross_the_review_override() {
    let value = parse_str(
        r#"{
          "server": {
            "name": "com.example/deprecated",
            "remotes": [{"type": "streamable-http", "url": "https://old.example/mcp"}]
          },
          "_meta": {"io.modelcontextprotocol.registry/official": {"status": "deprecated"}}
        }"#,
    )
    .unwrap();
    let candidate =
        candidate_from_record(None, &value, "registry:custom-cache.json", "review").unwrap();
    assert_eq!(candidate.trust_level, "deprecated");
    assert!(!super::install_allowed(&candidate, "review", true));
}

#[test]
fn official_registry_deleted_records_are_not_discovered() {
    let value = parse_str(
        r#"{
          "server": {
            "name": "com.example/deleted",
            "remotes": [{"type": "streamable-http", "url": "https://deleted.example/mcp"}]
          },
          "_meta": {"io.modelcontextprotocol.registry/official": {"status": "deleted"}}
        }"#,
    )
    .expect("deleted registry record");
    assert!(candidate_from_record(None, &value, "registry:custom-cache.json", "review").is_none());
}

#[test]
fn registry_trust_is_bound_to_source_kind_not_a_cache_file_name() {
    let value =
        parse_str(r#"{"name":"custom","installSpec":"npm:custom","trustLevel":"approved"}"#)
            .unwrap();
    let registry =
        candidate_from_record(None, &value, "registry:C:/cache/custom.json", "approved").unwrap();
    assert_eq!(registry.trust_level, "review");
    let local =
        candidate_from_record(None, &value, "C:/team/registry-cache.json", "review").unwrap();
    assert_eq!(local.trust_level, "approved");
}

#[test]
fn local_catalog_can_block_a_builtin_candidate() {
    let builtin_value = parse_str(
        r#"{"name":"filesystem","installSpec":"npm:filesystem","trustLevel":"approved"}"#,
    )
    .expect("builtin candidate");
    let local_value =
        parse_str(r#"{"name":"filesystem","installSpec":"npm:filesystem","trustLevel":"blocked"}"#)
            .expect("local candidate");
    let builtin = candidate_from_record(None, &builtin_value, "builtin:approved-servers", "review")
        .expect("builtin candidate");
    let local = candidate_from_record(
        None,
        &local_value,
        "C:/team/catalog/approved-servers.json",
        "review",
    )
    .expect("local candidate");
    let mut candidates = vec![builtin, local];
    assert_eq!(deduplicate_discovery_candidates(&mut candidates), 1);
    assert_eq!(candidates[0].trust_level, "blocked");
}

#[test]
fn registry_search_uses_latest_versions_and_query_specific_cache() {
    let url = registry_list_url_with_cursor(
        "https://registry.modelcontextprotocol.io",
        Some("next:value"),
        "file system",
    );
    assert!(url.contains("version=latest"));
    assert!(url.contains("search=file%20system"));
    assert!(url.contains("cursor=next%3Avalue"));

    let base = Path::new("catalog/registry-cache.json");
    let first = registry_query_cache_path(base, "filesystem");
    let second = registry_query_cache_path(base, "database");
    assert_ne!(first, second);
    assert_eq!(registry_query_cache_path(base, ""), base);
}
