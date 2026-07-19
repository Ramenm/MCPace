//! Small restart-loop guard for local background runtimes.
//!
//! This is intentionally file-backed and std-only. Autostart managers can retry
//! quickly after a bad reboot/session restore; the guard prevents MCPace from
//! spawning an unbounded sequence of local runtimes and burning CPU, memory, or
//! disk while the operator-visible logs explain the underlying failure.

use crate::resources;
use crate::runtimepaths;
use std::fmt;
use std::fs;
use std::path::Path;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RestartGuardError {
    LaunchBlocked {
        label: String,
        attempts: usize,
        window_ms: u64,
    },
    WriteFailed {
        path: String,
        reason: String,
    },
}

pub type RestartGuardResult<T> = std::result::Result<T, RestartGuardError>;

impl RestartGuardError {
    fn launch_blocked(label: &str, attempts: usize, window_ms: u64) -> Self {
        Self::LaunchBlocked {
            label: label.to_string(),
            attempts,
            window_ms,
        }
    }

    fn write_failed(path: &Path, reason: impl Into<String>) -> Self {
        Self::WriteFailed {
            path: path.display().to_string(),
            reason: reason.into(),
        }
    }

    pub fn contains(&self, needle: &str) -> bool {
        self.to_string().contains(needle)
    }
}

impl fmt::Display for RestartGuardError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LaunchBlocked {
                label,
                attempts,
                window_ms,
            } => write!(
                formatter,
                "{} restart guard blocked launch after {} starts within {} ms; inspect `mcpace status` and `mcpace advanced runtime logs`, then run 'mcpace stop' after fixing the cause",
                label, attempts, window_ms
            ),
            Self::WriteFailed { path, reason } => {
                write!(formatter, "failed to write restart guard '{}': {}", path, reason)
            }
        }
    }
}

impl std::error::Error for RestartGuardError {}

impl From<RestartGuardError> for String {
    fn from(error: RestartGuardError) -> Self {
        error.to_string()
    }
}

pub const ENV_RESTART_WINDOW_MS: &str = "MCPACE_SERVE_RESTART_WINDOW_MS";
pub const ENV_RESTART_MAX_ATTEMPTS: &str = "MCPACE_SERVE_RESTART_MAX_ATTEMPTS";
pub const DEFAULT_RESTART_WINDOW_MS: u64 = 120_000;
pub const DEFAULT_RESTART_MAX_ATTEMPTS: usize = 5;
pub const RESTART_WINDOW_MS_MAX: u64 = 3_600_000;
pub const RESTART_MAX_ATTEMPTS_MAX: usize = 100;

pub fn check_and_record(path: &Path, label: &str) -> RestartGuardResult<()> {
    let now = runtimepaths::unix_time_ms();
    let window_ms = restart_window_ms();
    let max_attempts = restart_max_attempts();
    let cutoff = now.saturating_sub(window_ms as u128);
    let mut attempts = read_attempts(path)
        .into_iter()
        .filter(|value| *value >= cutoff && *value <= now)
        .collect::<Vec<_>>();

    if attempts.len() >= max_attempts {
        return Err(RestartGuardError::launch_blocked(
            label,
            attempts.len(),
            window_ms,
        ));
    }

    attempts.push(now);
    write_attempts(path, &attempts)?;
    Ok(())
}

pub fn clear(path: &Path) {
    let _ = fs::remove_file(path);
}

fn restart_window_ms() -> u64 {
    resources::env_u64(ENV_RESTART_WINDOW_MS)
        .map(|value| value.clamp(1, RESTART_WINDOW_MS_MAX))
        .unwrap_or(DEFAULT_RESTART_WINDOW_MS)
}

fn restart_max_attempts() -> usize {
    resources::env_usize(ENV_RESTART_MAX_ATTEMPTS)
        .map(|value| value.clamp(1, RESTART_MAX_ATTEMPTS_MAX))
        .unwrap_or(DEFAULT_RESTART_MAX_ATTEMPTS)
}

fn read_attempts(path: &Path) -> Vec<u128> {
    fs::read_to_string(path)
        .ok()
        .map(|raw| {
            raw.lines()
                .filter_map(|line| line.trim().parse::<u128>().ok())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn write_attempts(path: &Path, attempts: &[u128]) -> RestartGuardResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            RestartGuardError::write_failed(
                path,
                format!("failed to create {}: {}", parent.display(), error),
            )
        })?;
    }
    let mut payload = attempts
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    payload.push('\n');
    runtimepaths::write_text_atomic(path, &payload)
        .map_err(|error| RestartGuardError::write_failed(path, error))
}

#[cfg(test)]
mod tests;
