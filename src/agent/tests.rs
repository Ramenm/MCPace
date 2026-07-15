use super::*;

#[test]
fn defaults_to_run_action_and_forwards_runtime_flags() {
    let args = vec![
        "--autostart".to_string(),
        "--root".to_string(),
        "/tmp/mcpace".to_string(),
        "--port".to_string(),
        "39022".to_string(),
    ];
    let cli = parse_cli(&args).expect("parse default run args");
    match cli.command.expect("default command") {
        AgentCommand::Run(runtime) => {
            assert!(runtime.autostart);
            assert_eq!(runtime.root, Some(PathBuf::from("/tmp/mcpace")));
            let mut forwarded = Vec::new();
            runtime.append_forwarded_args(&mut forwarded);
            assert_eq!(forwarded, vec!["--port".to_string(), "39022".to_string()]);
        }
        AgentCommand::Status(_) => panic!("expected run command"),
    }
}

#[test]
fn rejects_unknown_arguments_instead_of_passing_shell_text() {
    let args = vec!["--shell".to_string()];
    let error = parse_cli(&args).expect_err("unknown arguments must be rejected");
    assert_eq!(error.kind(), clap::error::ErrorKind::UnknownArgument);
}

#[test]
fn status_command_uses_same_forwarding_contract() {
    let args = vec![
        "status".to_string(),
        "--json".to_string(),
        "--host".to_string(),
        "127.0.0.1".to_string(),
    ];
    let cli = parse_cli(&args).expect("parse status args");
    match cli.command.expect("status command") {
        AgentCommand::Status(runtime) => {
            let mut forwarded = Vec::new();
            runtime.append_forwarded_args(&mut forwarded);
            assert_eq!(
                forwarded,
                vec![
                    "--host".to_string(),
                    "127.0.0.1".to_string(),
                    "--json".to_string()
                ]
            );
        }
        AgentCommand::Run(_) => panic!("expected status command"),
    }
}
