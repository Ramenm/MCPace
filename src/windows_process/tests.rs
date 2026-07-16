use super::{configure_no_window, enable_kill_on_exit_job, process_image_is};

#[test]
fn kill_on_exit_job_can_be_enabled_idempotently() {
    enable_kill_on_exit_job().expect("enable Windows kill-on-exit job");
    enable_kill_on_exit_job().expect("reuse Windows kill-on-exit job");
}

#[test]
fn process_image_check_rejects_stale_or_reused_pid_files() {
    let mut command = std::process::Command::new("cmd.exe");
    command.args(["/D", "/S", "/C", "ping -n 4 127.0.0.1 >NUL"]);
    configure_no_window(&mut command);
    let mut child = command.spawn().expect("spawn cmd process");
    assert!(process_image_is(child.id(), "cmd.exe"));
    assert!(!process_image_is(child.id(), "mcpace-agent-launcher.exe"));
    let _ = child.kill();
    let _ = child.wait();
}
