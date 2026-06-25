//! Small restart-loop guard for local background runtimes.
//!
//! This is intentionally file-backed and std-only. Autostart managers can retry
//! quickly after a bad reboot/session restore; the guard prevents MCPace from
//! spawning an unbounded sequence of local runtimes and burning CPU, memory, or
//! disk while the operator-visible logs explain the underlying failure.

use crate::resources;
use crate::runtimepaths;
use std::fs;
use std::path::Path;

pub const ENV_RESTART_WINDOW_MS: &str = "MCPACE_SERVE_RESTART_WINDOW_MS";
pub const ENV_RESTART_MAX_ATTEMPTS: &str = "MCPACE_SERVE_RESTART_MAX_ATTEMPTS";
pub const DEFAULT_RESTART_WINDOW_MS: u64 = 120_000;
pub const DEFAULT_RESTART_MAX_ATTEMPTS: usize = 5;
pub const RESTART_WINDOW_MS_MAX: u64 = 3_600_000;
pub const RESTART_MAX_ATTEMPTS_MAX: usize = 100;

pub fn check_and_record(path: &Path, label: &str) -> Result<(), String> {
    let now = runtimepaths::unix_time_ms();
    let window_ms = restart_window_ms();
    let max_attempts = restart_max_attempts();
    let cutoff = now.saturating_sub(window_ms as u128);
    let mut attempts = read_attempts(path)
        .into_iter()
        .filter(|value| *value >= cutoff && *value <= now)
        .collect::<Vec<_>>();

    if attempts.len() >= max_attempts {
        return Err(format!(
            "{} restart guard blocked launch after {} starts within {} ms; inspect serve logs/status, then run 'mcpace serve stop' to clear the guard after fixing the cause",
            label, attempts.len(), window_ms
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

fn write_attempts(path: &Path, attempts: &[u128]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {}", parent.display(), error))?;
    }
    let mut payload = attempts
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    payload.push('\n');
    runtimepaths::write_text_atomic(path, &payload).map_err(|error| {
        format!(
            "failed to write restart guard '{}': {}",
            path.display(),
            error
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "mcpace-restart-guard-test-{}-{}-{}.log",
            std::process::id(),
            runtimepaths::unix_time_ms(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn restart_guard_blocks_tight_restart_loops_and_clear_recovers() {
        let path = temp_path();
        for _ in 0..DEFAULT_RESTART_MAX_ATTEMPTS {
            check_and_record(&path, "serve").unwrap();
        }
        let error = check_and_record(&path, "serve").unwrap_err();
        assert!(error.contains("restart guard blocked launch"));
        clear(&path);
        check_and_record(&path, "serve").unwrap();
        clear(&path);
    }
}
