use super::export_warning_is_user_actionable;

#[test]
fn export_warnings_keep_client_actions_and_hide_per_server_plan_noise() {
    assert!(export_warning_is_user_actionable(
        "At least one routed server uses stdio; the hub must own the child process."
    ));
    assert!(export_warning_is_user_actionable(
        "No external session id was resolved; the plan derived an internal session lease."
    ));
    assert!(export_warning_is_user_actionable(
        "Client surface 'windsurf' has a documented enabled-tool budget of 100."
    ));
    assert!(export_warning_is_user_actionable(
        "Streamable HTTP is available through the one-port local MCPace server."
    ));

    assert!(!export_warning_is_user_actionable(
        "filesystem is disabled or plan-only; MCPace must not route tool calls to it."
    ));
    assert!(!export_warning_is_user_actionable(
        "browser has unknown scopeClass 'configured-source'; treating it as lease-local."
    ));
    assert!(!export_warning_is_user_actionable(
        "fetch is credential-scoped but no credential profile id was resolved."
    ));
}
