use super::*;

#[test]
fn service_config_launches_agent_not_serve_or_wscript() {
    let root = PathBuf::from("/tmp/mcpace");
    let config = service_config(&root, "127.0.0.1", 39022, None, None, None, None).unwrap();
    if cfg!(windows) && config.native_background {
        assert_eq!(config.launch_args, vec!["--from-login".to_string()]);
        assert_eq!(config.target_args[0], "agent");
        assert_eq!(config.target_args[1], "run");
    } else if cfg!(target_os = "linux") {
        assert!(config
            .launch_args
            .iter()
            .any(|value| value == &config.target_app_path));
        assert!(config.launch_args.iter().any(|value| value == "agent"));
        assert!(config.launch_args.iter().any(|value| value == "run"));
    } else {
        assert_eq!(config.launch_args[0], "agent");
        assert_eq!(config.launch_args[1], "run");
    }
    assert_eq!(
        config.target_args,
        vec![
            "agent".to_string(),
            "run".to_string(),
            "--autostart".to_string(),
            "--root".to_string(),
            root.display().to_string(),
            "--host".to_string(),
            "127.0.0.1".to_string(),
            "--port".to_string(),
            "39022".to_string(),
        ]
    );
    assert!(!config.app_path.to_ascii_lowercase().contains("wscript"));
    assert!(launches_mcpace_agent(&config));
}

#[test]
fn parse_action_aliases_keep_existing_service_surface_compatible() {
    let args = vec![
        "enable".to_string(),
        "--dry-run".to_string(),
        "--json".to_string(),
    ];
    let parsed = parse_cli(&args);
    assert_eq!(parsed.action, "enable");
    assert!(parsed.dry_run);
    assert!(parsed.json_output);
    assert_eq!(parsed.host, None);
    assert_eq!(parsed.port, None);
}

#[test]
fn prove_action_and_dry_run_are_parsed_without_aliasing() {
    let parsed = parse_cli(&[
        "prove".to_string(),
        "--dry-run".to_string(),
        "--json".to_string(),
    ]);
    assert_eq!(parsed.action, "prove");
    assert!(parsed.dry_run);
    assert!(parsed.json_output);
}

#[test]
fn proof_report_records_activation_and_state_restoration_evidence() {
    let root = PathBuf::from("/tmp/mcpace-proof");
    let config = service_config(&root, "127.0.0.1", 39022, None, None, None, None).unwrap();
    let report = report_with_supervisor_proof(
        &config,
        true,
        SupervisorProof {
            dry_run: false,
            initial_runtime_active: true,
            activation_attempted: true,
            endpoint_verified: true,
            supervisor_verified: true,
            restored_initial_state: true,
        },
    );
    let proof = report.get("proof").expect("proof object");
    assert_eq!(
        proof.get("schema").and_then(JsonValue::as_str),
        Some("mcpace.autostartProof.v1")
    );
    assert_eq!(
        proof.get("endpointVerified").and_then(JsonValue::as_bool),
        Some(true)
    );
    assert_eq!(
        proof.get("supervisorVerified").and_then(JsonValue::as_bool),
        Some(true)
    );
    assert_eq!(
        proof
            .get("restoredInitialState")
            .and_then(JsonValue::as_bool),
        Some(true)
    );
}

#[test]
fn endpoint_flags_remain_optional_so_configured_values_are_not_shadowed() {
    let args = vec![
        "print".to_string(),
        "--host".to_string(),
        "::1".to_string(),
        "--port".to_string(),
        "43123".to_string(),
    ];
    let parsed = parse_cli(&args);

    assert_eq!(parsed.host.as_deref(), Some("::1"));
    assert_eq!(parsed.port, Some(43123));
}

#[test]
fn target_arg_value_finds_forwarded_root() {
    let root = PathBuf::from("/tmp/mcpace");
    let config = service_config(&root, "127.0.0.1", 39022, None, None, None, None).unwrap();
    assert_eq!(
        target_arg_value(&config.target_args, "--root"),
        Some("/tmp/mcpace")
    );
}

#[test]
fn autolaunch_tokens_quote_space_and_shell_sensitive_paths() {
    assert_eq!(quote_shellish_token("/tmp/mcpace"), "/tmp/mcpace");
    assert_eq!(
        quote_shellish_token("/tmp/MCPace Demo"),
        "\"/tmp/MCPace Demo\""
    );
    assert_eq!(quote_shellish_token("/tmp/mc$pace"), "\"/tmp/mc\\$pace\"");
}

#[test]
fn service_config_keeps_plain_target_args_but_escapes_autolaunch_args() {
    let root = PathBuf::from("/tmp/MCPace Demo");
    let config = service_config(&root, "127.0.0.1", 39022, None, None, None, None).unwrap();
    assert_eq!(
        target_arg_value(&config.target_args, "--root"),
        Some("/tmp/MCPace Demo")
    );
    if cfg!(target_os = "linux") {
        assert!(config
            .args
            .iter()
            .any(|value| value == "\"/tmp/MCPace Demo\""));
    }
    if cfg!(windows) && config.native_background {
        assert_eq!(config.args, vec!["--from-login".to_string()]);
        assert!(config
            .target_args
            .iter()
            .any(|value| value == "/tmp/MCPace Demo"));
    } else {
        assert!(config
            .launch_args
            .iter()
            .any(|value| value == "/tmp/MCPace Demo"));
    }
}

#[cfg(target_os = "linux")]
#[test]
fn linux_systemd_tokens_escape_specifiers_and_environment_expansion() {
    assert_eq!(
        quote_systemd_token("/tmp/MCPace Demo"),
        "\"/tmp/MCPace Demo\""
    );
    assert_eq!(
        quote_systemd_token("PATH=/opt/100%/$USER/bin"),
        "\"PATH=/opt/100%%/$$USER/bin\""
    );
}

#[cfg(target_os = "linux")]
#[test]
fn linux_login_path_deduplicates_and_drops_wsl_windows_mounts() {
    let raw = std::ffi::OsString::from(
        "/home/user/.local/bin:/usr/bin:/mnt/c/Program Files/nodejs:/usr/bin:/bin",
    );
    assert_eq!(
        normalized_linux_path(&raw, true).as_deref(),
        Some("/home/user/.local/bin:/usr/bin:/bin")
    );
}

#[cfg(windows)]
#[test]
fn windows_autolaunch_token_uses_windows_quoting_without_doubling_path_separators() {
    assert_eq!(
        autolaunch_token(r"C:\Program Files\MCPace\mcpace.exe"),
        r#""C:\Program Files\MCPace\mcpace.exe""#
    );
}

#[test]
fn verification_checks_cover_command_viability_not_just_enabled_state() {
    let root =
        std::env::temp_dir().join(format!("mcpace-service-verify-test-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("mcpace.config.json"), "{}\n").unwrap();
    let config = service_config(&root, "127.0.0.1", 39022, None, None, None, None).unwrap();

    let checks = JsonValue::array(service_verification_checks(&config, false)).to_compact_string();

    assert!(checks.contains("target-executable-exists"), "{}", checks);
    assert!(checks.contains("launch-program-exists"), "{}", checks);
    assert!(checks.contains("native-background-launcher"), "{}", checks);
    assert!(
        checks.contains("autostart-command-matches-plan"),
        "{}",
        checks
    );
    assert!(
        checks.contains("windows-autostart-plan-written"),
        "{}",
        checks
    );
    assert!(checks.contains("windows-run-command-length"), "{}", checks);
    assert!(
        checks.contains("machine-wide-autostart-entry-absent"),
        "{}",
        checks
    );
    assert!(checks.contains("root-path-valid"), "{}", checks);
    assert!(
        checks.contains("legacy-autostart-entry-removed"),
        "{}",
        checks
    );
    assert!(
        checks.contains("windows-persistent-env-aligned"),
        "{}",
        checks
    );

    let _ = std::fs::remove_dir_all(root);
}
