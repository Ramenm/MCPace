pub mod adapter;
pub mod agent;
pub mod app;
pub mod autostart;
pub mod candidates;
pub mod catalog;
pub mod cleanup;
pub mod client;
pub mod client_catalog;
pub(crate) mod codex_config;
pub(crate) mod config_edit;
pub mod dashboard;
pub(crate) mod diagnostics;
pub mod doctor;
mod execution;
pub(crate) mod http_client;
pub(crate) mod http_probe;
pub mod hub;
pub mod init;
pub mod json;
pub mod json_helpers;
pub mod lab;
#[cfg(target_os = "macos")]
pub(crate) mod macos_launch_agent;
pub(crate) mod mcp_autoinstall;
pub mod mcp_protocol;
pub mod mcp_server;
pub(crate) mod mcp_sources;
pub(crate) mod persistent_env;
pub(crate) mod platform_utils;
pub(crate) mod process_detach;
pub(crate) mod process_identity;
pub mod profile;
pub mod projects;
pub mod release;
pub mod reporoot;
pub(crate) mod resources;
pub(crate) mod restart_guard;
pub mod runtimepaths;
pub mod serve;
pub mod server;
pub mod service;
pub mod setup;
pub(crate) mod source_type;
pub mod status;
pub(crate) mod stdio_shim;
pub(crate) mod text_utils;
pub mod tool_result;
pub(crate) mod tool_schemas;
pub mod uninstall;
pub mod update;
pub mod upstream;
pub mod verify;
#[cfg(windows)]
pub(crate) mod windows_process;

pub use app::run;

#[doc(hidden)]
pub fn write_startup_diagnostic(stderr: &mut dyn std::io::Write, message: &str) {
    diagnostics::stderr_line(stderr, format_args!("{}", message));
}

#[doc(hidden)]
#[derive(Debug)]
pub struct ProcessContainmentError(String);

impl std::fmt::Display for ProcessContainmentError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ProcessContainmentError {}

#[doc(hidden)]
pub fn enable_kill_on_exit_process_tree() -> Result<(), ProcessContainmentError> {
    #[cfg(windows)]
    {
        windows_process::enable_kill_on_exit_job()
            .map_err(|error| ProcessContainmentError(error.to_string()))
    }
    #[cfg(not(windows))]
    {
        Ok(())
    }
}

#[cfg(test)]
pub(crate) static LOCAL_SERVER_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod test_support_tests;
#[cfg(test)]
pub(crate) use test_support_tests::bind_loopback_test_listener;
