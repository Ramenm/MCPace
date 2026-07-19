use super::model::{HubStatus, RepairReport};
use super::{launcher, runtime, status};
use crate::diagnostics;
use crate::json::JsonValue;
use crate::runtimepaths;
use crate::text_utils::join_or_none;
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::thread;
use std::time::Duration;

pub(super) fn run_up(
    root_path: &Path,
    foreground: bool,
    json_output: bool,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    if let Err(error) = runtime::ensure_runtime_layout(root_path) {
        diagnostics::stderr_line(stderr, format_args!("{}", error));
        return 1;
    }

    let current_status = match status::collect_status(root_path) {
        Ok(value) => value,
        Err(error) => {
            diagnostics::stderr_line(stderr, format_args!("{}", error));
            return 1;
        }
    };
    if status::is_live_status(&current_status.status) {
        return status::write_status_response(&current_status, json_output, stdout);
    }
    if current_status.status == "stale" {
        if let Err(error) = runtime::mark_stopped(root_path) {
            diagnostics::stderr_line(stderr, format_args!("{}", error));
            return 1;
        }
    }
    if current_status.status == "corrupt" {
        diagnostics::stderr_line(stderr, format_args!("hub runtime state is corrupt; run 'mcpace advanced runtime repair' to archive bad files and reseed a clean baseline"));
        return 1;
    }

    let state_root = runtimepaths::resolve_state_root(root_path);
    let stop_path = runtimepaths::hub_stop_path(&state_root);
    let _ = fs::remove_file(stop_path);

    if foreground {
        return run_loop(root_path, stderr);
    }

    let exe = match std::env::current_exe() {
        Ok(value) => value,
        Err(error) => {
            diagnostics::stderr_line(
                stderr,
                format_args!("failed to resolve mcpace binary path: {}", error),
            );
            return 1;
        }
    };
    let log_path = runtimepaths::hub_log_path(&state_root);
    if let Err(error) = runtime::rotate_logs_if_needed(&log_path) {
        diagnostics::stderr_line(stderr, format_args!("{}", error));
        return 1;
    }

    if let Err(error) = launcher::spawn_background(&exe, root_path) {
        diagnostics::stderr_line(stderr, format_args!("{}", error));
        return 1;
    }

    for _ in 0..60 {
        thread::sleep(Duration::from_millis(50));
        match status::collect_status(root_path) {
            Ok(status_value) if status_value.status == "running" => {
                return status::write_status_response(&status_value, json_output, stdout)
            }
            Ok(_) => {}
            Err(_) => {}
        }
    }

    let fallback_status = match status::collect_status(root_path) {
        Ok(value) => value,
        Err(error) => {
            diagnostics::stderr_line(stderr, format_args!("{}", error));
            return 1;
        }
    };
    if fallback_status.status == "running" {
        return status::write_status_response(&fallback_status, json_output, stdout);
    }

    diagnostics::stderr_line(
        stderr,
        format_args!("hub runtime did not become healthy in time"),
    );
    1
}

pub(super) fn run_down(
    root_path: &Path,
    json_output: bool,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    if let Err(error) = runtime::ensure_runtime_layout(root_path) {
        diagnostics::stderr_line(stderr, format_args!("{}", error));
        return 1;
    }

    let current_status = match status::collect_status(root_path) {
        Ok(value) => value,
        Err(error) => {
            diagnostics::stderr_line(stderr, format_args!("{}", error));
            return 1;
        }
    };

    if current_status.status == "stale" {
        if let Err(error) = runtime::mark_stopped(root_path) {
            diagnostics::stderr_line(stderr, format_args!("{}", error));
            return 1;
        }
        let final_status = match status::collect_status(root_path) {
            Ok(value) => value,
            Err(error) => {
                diagnostics::stderr_line(stderr, format_args!("{}", error));
                return 1;
            }
        };
        return status::write_status_response(&final_status, json_output, stdout);
    }

    if current_status.status == "corrupt" {
        diagnostics::stderr_line(stderr, format_args!("hub runtime state is corrupt; run 'mcpace advanced runtime repair' to archive bad files and reseed a clean baseline"));
        return 1;
    }

    if !status::is_live_status(&current_status.status) {
        return status::write_status_response(&current_status, json_output, stdout);
    }

    let state_root = runtimepaths::resolve_state_root(root_path);
    let stop_path = runtimepaths::hub_stop_path(&state_root);
    if let Err(error) = runtime::write_atomic(&stop_path, runtime::now_ms().to_string()) {
        diagnostics::stderr_line(stderr, format_args!("{}", error));
        return 1;
    }

    for _ in 0..80 {
        thread::sleep(Duration::from_millis(50));
        match status::collect_status(root_path) {
            Ok(status_value) if !status::is_live_status(&status_value.status) => {
                return status::write_status_response(&status_value, json_output, stdout)
            }
            Ok(_) => {}
            Err(_) => {}
        }
    }

    let final_status = match status::collect_status(root_path) {
        Ok(value) => value,
        Err(error) => {
            diagnostics::stderr_line(stderr, format_args!("{}", error));
            return 1;
        }
    };
    if !status::is_live_status(&final_status.status) {
        return status::write_status_response(&final_status, json_output, stdout);
    }

    diagnostics::stderr_line(stderr, format_args!("hub runtime did not stop in time"));
    1
}

pub(super) fn run_repair(
    root_path: &Path,
    json_output: bool,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    if let Err(error) = runtime::ensure_runtime_layout(root_path) {
        diagnostics::stderr_line(stderr, format_args!("{}", error));
        return 1;
    }

    let current_status = match status::collect_status(root_path) {
        Ok(value) => value,
        Err(error) => {
            diagnostics::stderr_line(stderr, format_args!("{}", error));
            return 1;
        }
    };
    if status::is_live_status(&current_status.status) {
        diagnostics::stderr_line(stderr, format_args!("hub repair refuses to run while the runtime is active; stop MCPace first with 'mcpace stop'"));
        return 1;
    }

    let repair_report = match runtime::repair_runtime_files(root_path) {
        Ok(value) => value,
        Err(error) => {
            diagnostics::stderr_line(stderr, format_args!("{}", error));
            return 1;
        }
    };
    let final_status = match status::collect_status(root_path) {
        Ok(value) => value,
        Err(error) => {
            diagnostics::stderr_line(stderr, format_args!("{}", error));
            return 1;
        }
    };
    write_repair_response(&repair_report, &final_status, json_output, stdout)
}

pub(super) fn run_status(
    root_path: &Path,
    json_output: bool,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let status_value = match status::collect_status(root_path) {
        Ok(value) => value,
        Err(error) => {
            diagnostics::stderr_line(stderr, format_args!("{}", error));
            return 1;
        }
    };
    status::write_status_response(&status_value, json_output, stdout)
}

pub(super) fn run_logs(
    root_path: &Path,
    tail: usize,
    json_output: bool,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    if let Err(error) = runtime::ensure_runtime_layout(root_path) {
        diagnostics::stderr_line(stderr, format_args!("{}", error));
        return 1;
    }

    let state_root = runtimepaths::resolve_state_root(root_path);
    let log_path = runtimepaths::hub_log_path(&state_root);
    if !log_path.is_file() {
        if json_output {
            let _ = writeln!(stdout, "[]");
        } else {
            let _ = writeln!(stdout, "No hub logs yet.");
        }
        return 0;
    }

    let raw = match fs::read_to_string(&log_path) {
        Ok(value) => value,
        Err(error) => {
            diagnostics::stderr_line(
                stderr,
                format_args!("failed to read {}: {}", log_path.display(), error),
            );
            return 1;
        }
    };
    let lines = raw
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    let start = lines.len().saturating_sub(tail);
    let selected = &lines[start..];

    if json_output {
        let json = JsonValue::array(selected.iter().map(|line| {
            crate::json::parse_str(line).unwrap_or_else(|_| JsonValue::string(line.clone()))
        }));
        let _ = writeln!(stdout, "{}", json.to_pretty_string());
        return 0;
    }

    for line in selected {
        let _ = writeln!(stdout, "{}", line);
    }
    0
}

pub(super) fn run_loop_command(root_path: &Path, stderr: &mut dyn Write) -> i32 {
    if let Err(error) = runtime::ensure_runtime_layout(root_path) {
        diagnostics::stderr_line(stderr, format_args!("{}", error));
        return 1;
    }
    run_loop(root_path, stderr)
}

fn run_loop(root_path: &Path, stderr: &mut dyn Write) -> i32 {
    let state_root = runtimepaths::resolve_state_root(root_path);
    let stop_path = runtimepaths::hub_stop_path(&state_root);

    let existing_status = match status::collect_status(root_path) {
        Ok(value) => value,
        Err(error) => {
            diagnostics::stderr_line(stderr, format_args!("{}", error));
            return 1;
        }
    };
    if status::is_live_status(&existing_status.status)
        && existing_status.pid != Some(std::process::id())
    {
        diagnostics::stderr_line(
            stderr,
            format_args!("hub runtime is already active for this root"),
        );
        return 1;
    }
    if existing_status.status == "stale" {
        diagnostics::stderr_line(stderr, format_args!("hub runtime state is stale; run 'mcpace advanced runtime repair' before starting again"));
        return 1;
    }
    if existing_status.status == "corrupt" {
        diagnostics::stderr_line(stderr, format_args!("hub runtime state is corrupt; run 'mcpace advanced runtime repair' to archive bad files and reseed a clean baseline"));
        return 1;
    }

    let start_ms = runtime::now_ms();
    let pid = std::process::id();
    let runtime_lock = match runtime::acquire_runtime_lock(root_path, pid, start_ms) {
        Ok(value) => value,
        Err(error) => {
            diagnostics::stderr_line(stderr, format_args!("{}", error));
            return 1;
        }
    };

    if let Err(error) =
        runtime::write_state_metadata(root_path, "starting", Some(pid), Some(start_ms), None)
    {
        diagnostics::stderr_line(stderr, format_args!("{}", error));
        return 1;
    }
    if let Err(error) =
        runtime::write_health_metadata(root_path, "starting", pid, start_ms, start_ms)
    {
        diagnostics::stderr_line(stderr, format_args!("{}", error));
        return 1;
    }
    let _ = runtime::append_log(
        root_path,
        "info",
        "hub_starting",
        &[("pid", JsonValue::number(pid))],
    );

    let _ = fs::remove_file(&stop_path);

    let mut started_logged = false;
    loop {
        let now = runtime::now_ms();
        let lifecycle_status = if started_logged {
            "running"
        } else {
            "starting"
        };
        if let Err(error) =
            runtime::write_health_metadata(root_path, lifecycle_status, pid, start_ms, now)
        {
            diagnostics::stderr_line(stderr, format_args!("{}", error));
            return 1;
        }
        if !started_logged {
            if let Err(error) =
                runtime::write_state_metadata(root_path, "running", Some(pid), Some(start_ms), None)
            {
                diagnostics::stderr_line(stderr, format_args!("{}", error));
                return 1;
            }
            let _ = runtime::append_log(
                root_path,
                "info",
                "hub_started",
                &[("pid", JsonValue::number(pid))],
            );
            started_logged = true;
        }

        if stop_path.is_file() {
            let _ = runtime::append_log(
                root_path,
                "info",
                "hub_stop_requested",
                &[("pid", JsonValue::number(pid))],
            );
            break;
        }

        thread::sleep(Duration::from_millis(250));
    }

    let stop_ms = runtime::now_ms();
    let _ = fs::remove_file(stop_path);
    let _ = fs::remove_file(runtimepaths::hub_health_path(&state_root));
    if let Err(error) =
        runtime::write_state_metadata(root_path, "stopped", None, None, Some(stop_ms))
    {
        diagnostics::stderr_line(stderr, format_args!("{}", error));
        return 1;
    }
    drop(runtime_lock);
    let _ = runtime::append_log(
        root_path,
        "info",
        "hub_stopped",
        &[("pid", JsonValue::number(pid))],
    );
    0
}

fn write_repair_response(
    repair_report: &RepairReport,
    final_status: &HubStatus,
    json_output: bool,
    stdout: &mut dyn Write,
) -> i32 {
    if json_output {
        let mut map = BTreeMap::new();
        map.insert("repair".to_string(), repair_report.to_json_value());
        map.insert("hubStatus".to_string(), final_status.to_json_value());
        let _ = writeln!(stdout, "{}", JsonValue::Object(map).to_pretty_string());
        return 0;
    }

    let _ = writeln!(stdout, "Hub repair completed.");
    let _ = writeln!(stdout, "Root path: {}", repair_report.root_path);
    let _ = writeln!(stdout, "State root: {}", repair_report.state_root);
    let _ = writeln!(
        stdout,
        "Archived runtime files: {}",
        join_or_none(&repair_report.archived_paths)
    );
    let _ = writeln!(
        stdout,
        "Recreated runtime files: {}",
        join_or_none(&repair_report.recreated_paths)
    );
    if !repair_report.warnings.is_empty() {
        let _ = writeln!(
            stdout,
            "Repair notes: {}",
            repair_report.warnings.join(" | ")
        );
    }
    let _ = writeln!(
        stdout,
        "Final hub status: {} ({})",
        final_status.status, final_status.health
    );
    0
}
