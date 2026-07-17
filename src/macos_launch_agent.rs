use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::{Duration, Instant};

const LAUNCHCTL: &str = "/bin/launchctl";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MacosLaunchAgentError {
    message: String,
}

impl MacosLaunchAgentError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for MacosLaunchAgentError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for MacosLaunchAgentError {}

type MacosLaunchAgentResult<T> = Result<T, MacosLaunchAgentError>;

pub(crate) fn is_loaded(label: &str) -> MacosLaunchAgentResult<bool> {
    let output = launchctl(&["print", &service_target(label)])?;
    if output.status.success() {
        return Ok(true);
    }
    if service_not_found(&output) {
        return Ok(false);
    }
    Err(command_error("inspect", label, &output))
}

pub(crate) fn stop(label: &str) -> MacosLaunchAgentResult<()> {
    if !is_loaded(label)? {
        return Ok(());
    }
    let output = launchctl(&["bootout", &service_target(label)])?;
    if !output.status.success() && !service_not_found(&output) {
        return Err(command_error("stop", label, &output));
    }
    wait_until_unloaded(label, Duration::from_secs(5))
}

pub(crate) fn start(label: &str) -> MacosLaunchAgentResult<()> {
    if !is_loaded(label)? {
        let plist = launch_agent_plist(label)?;
        if !plist.is_file() {
            return Err(MacosLaunchAgentError::new(format!(
                "macOS LaunchAgent plist is missing: {}",
                plist.display()
            )));
        }
        let output = launchctl(&[
            "bootstrap",
            &user_domain(),
            plist.to_str().ok_or_else(|| {
                MacosLaunchAgentError::new(format!(
                    "LaunchAgent plist path is not valid UTF-8: {}",
                    plist.display()
                ))
            })?,
        ])?;
        if !output.status.success() && !already_loaded(&output) {
            return Err(command_error("load", label, &output));
        }
    }

    let output = launchctl(&["kickstart", "-k", &service_target(label)])?;
    if output.status.success() {
        Ok(())
    } else {
        Err(command_error("start", label, &output))
    }
}

fn wait_until_unloaded(label: &str, timeout: Duration) -> MacosLaunchAgentResult<()> {
    let started = Instant::now();
    while started.elapsed() < timeout {
        if !is_loaded(label)? {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(MacosLaunchAgentError::new(format!(
        "timed out waiting for macOS LaunchAgent '{}' to unload",
        label
    )))
}

fn launch_agent_plist(label: &str) -> MacosLaunchAgentResult<PathBuf> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .ok_or_else(|| {
            MacosLaunchAgentError::new("HOME is unavailable for the macOS LaunchAgent")
        })?;
    Ok(home
        .join("Library")
        .join("LaunchAgents")
        .join(format!("{label}.plist")))
}

fn launchctl(args: &[&str]) -> MacosLaunchAgentResult<Output> {
    Command::new(LAUNCHCTL)
        .args(args)
        .output()
        .map_err(|error| {
            MacosLaunchAgentError::new(format!("failed to execute {}: {}", LAUNCHCTL, error))
        })
}

fn user_domain() -> String {
    extern "C" {
        fn getuid() -> u32;
    }

    // SAFETY: `getuid` takes no pointers and has no preconditions; it only
    // returns the real user id for the current process.
    let uid = unsafe { getuid() };
    format!("gui/{uid}")
}

fn service_target(label: &str) -> String {
    format!("{}/{}", user_domain(), label)
}

fn service_not_found(output: &Output) -> bool {
    let detail = output_detail(output).to_ascii_lowercase();
    detail.contains("could not find service")
        || detail.contains("service not found")
        || detail.contains("no such process")
}

fn already_loaded(output: &Output) -> bool {
    let detail = output_detail(output).to_ascii_lowercase();
    detail.contains("service already loaded") || detail.contains("already bootstrapped")
}

fn command_error(action: &str, label: &str, output: &Output) -> MacosLaunchAgentError {
    let detail = output_detail(output);
    MacosLaunchAgentError::new(format!(
        "failed to {} macOS LaunchAgent '{}': {}",
        action,
        label,
        if detail.is_empty() {
            format!("launchctl exited with {}", output.status)
        } else {
            detail
        }
    ))
}

fn output_detail(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !stderr.is_empty() {
        return stderr;
    }
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

#[cfg(test)]
mod tests;
