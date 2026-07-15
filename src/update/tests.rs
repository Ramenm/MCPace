use super::*;

#[test]
fn semver_compare_handles_current_and_outdated_cases() {
    assert_eq!(compare_semver("0.3.5", "0.3.5"), Some(Ordering::Equal));
    assert_eq!(compare_semver("0.3.5", "0.3.6"), Some(Ordering::Less));
    assert_eq!(compare_semver("0.4.1", "0.3.6"), Some(Ordering::Greater));
}

#[test]
fn update_check_with_explicit_latest_reports_outdated() {
    let parsed = ParsedArgs {
        action: "check".to_string(),
        json_output: true,
        latest_version: Some("99.0.0".to_string()),
        source: UpdateSource::Argument,
        package_name: DEFAULT_PACKAGE_NAME.to_string(),
        help: false,
        error: None,
    };
    let report = check_update(&parsed);
    assert_eq!(report.status, "outdated");
    assert!(report.update_available);
    assert_eq!(report.latest_version.as_deref(), Some("99.0.0"));
}

#[test]
fn update_report_json_keeps_dashboard_updates_package_manager_managed() {
    let parsed = ParsedArgs {
        action: "check".to_string(),
        json_output: true,
        latest_version: Some("99.0.0".to_string()),
        source: UpdateSource::Argument,
        package_name: DEFAULT_PACKAGE_NAME.to_string(),
        help: false,
        error: None,
    };
    let json = check_update(&parsed).to_json_value().to_pretty_string();
    assert!(json.contains("\"cached\": false"));
    assert!(json.contains("\"checkedAtMs\":"));
    assert!(json.contains("\"selfUpdateEnabled\": false"));
    assert!(json.contains("\"autoUpdateMode\": \"package-manager-managed\""));
}

#[test]
fn update_source_defaults_to_npm_when_no_offline_hint_is_set() {
    std::env::remove_var("MCPACE_UPDATE_SOURCE");
    std::env::remove_var("MCPACE_LATEST_VERSION");
    assert_eq!(derive_update_source(None, None), Ok(UpdateSource::Npm));
}

#[test]
fn update_source_env_rejects_unknown_values() {
    std::env::set_var("MCPACE_UPDATE_SOURCE", "surprise-network");
    std::env::remove_var("MCPACE_LATEST_VERSION");
    let parsed = parse_cli(&[]);
    std::env::remove_var("MCPACE_UPDATE_SOURCE");
    assert_eq!(
        parsed.error.as_deref(),
        Some("unsupported MCPACE_UPDATE_SOURCE 'surprise-network'; expected none, env, or npm")
    );
}
