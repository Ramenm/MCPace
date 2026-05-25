pub mod adapter;
pub mod app;
pub mod candidates;
pub mod catalog;
pub mod cleanup;
pub mod client;
pub mod client_catalog;
pub(crate) mod codex_config;
pub mod connect;
pub mod dashboard;
pub mod doctor;
pub mod hub;
pub mod init;
pub mod json;
pub mod json_helpers;
pub mod lab;
pub(crate) mod mcp_autoinstall;
pub mod mcp_protocol;
pub mod mcp_server;
pub(crate) mod mcp_sources;
pub(crate) mod platform_utils;
pub(crate) mod process_detach;
pub mod profile;
pub mod projects;
pub mod release;
pub mod repair;
pub mod reporoot;
pub(crate) mod resources;
pub mod runtimepaths;
pub mod serve;
pub mod server;
pub mod service;
pub mod setup;
pub(crate) mod source_type;
pub mod stdio_shim;
pub(crate) mod text_utils;
pub mod tool_result;
pub(crate) mod tool_schemas;
pub mod update;
pub mod upstream;
pub mod verify;
#[cfg(windows)]
pub(crate) mod windows_process;

pub use app::run;

#[cfg(test)]
pub(crate) static LOCAL_SERVER_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
