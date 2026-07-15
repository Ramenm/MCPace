use super::enable_kill_on_exit_job;

#[test]
fn kill_on_exit_job_can_be_enabled_idempotently() {
    enable_kill_on_exit_job().expect("enable Windows kill-on-exit job");
    enable_kill_on_exit_job().expect("reuse Windows kill-on-exit job");
}
