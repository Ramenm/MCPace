use super::model::{CorruptedRuntimeFile, HubStatus};
use super::runtime;
use crate::json::JsonValue;
use crate::json_helpers;
use crate::runtimepaths;
use crate::verify;
use std::io::Write;
use std::path::Path;

pub(super) fn collect_status(root_path: &Path) -> Result<HubStatus, String> {
    let readiness = verify::collect_readiness(root_path)?;
    let state_root = runtimepaths::resolve_state_root(root_path);
    let runtime_dir = runtimepaths::runtime_dir(&state_root);
    let hub_dir = runtimepaths::hub_dir(&state_root);
    let log_path = runtimepaths::hub_log_path(&state_root);
    let lease_store_path = runtimepaths::hub_leases_path(&state_root);
    let lock_path = runtimepaths::hub_lock_path(&state_root);

    let registry_file = runtime::read_json_diagnostic(&runtimepaths::project_registry_path(&state_root));
    let lease_file = runtime::read_json_diagnostic(&lease_store_path);
    let state_file = runtime::read_json_diagnostic(&runtimepaths::hub_state_path(&state_root));
    let health_file = runtime::read_json_diagnostic(&runtimepaths::hub_health_path(&state_root));
    let lock_file = runtime::read_json_diagnostic(&lock_path);

    let corrupted_files = vec![&registry_file, &lease_file, &state_file, &health_file, &lock_file]
        .into_iter()
        .filter_map(|diagnostic| diagnostic.corruption())
        .collect::<Vec<CorruptedRuntimeFile>>();

    let state_json = state_file.value.as_ref();
    let health_json = health_file.value.as_ref();
    let lock_json = lock_file.value.as_ref();

    let pid = first_u32(&[health_json, state_json], &["pid"]);
    let started_at_ms = first_u128(&[health_json, state_json], &["startedAtMs"]);
    let last_heartbeat_at_ms = first_u128(&[health_json, state_json], &["lastHeartbeatAtMs"]);
    let last_exit_at_ms = first_u128(&[state_json], &["lastExitAtMs"]);
    let lock_pid = first_u32(&[lock_json], &["pid"]);
    let mut status = first_string(&[health_json, state_json], &["status"])
        .unwrap_or_else(|| "stopped".to_string());
    let now = runtime::now_ms();
    let heartbeat_fresh = last_heartbeat_at_ms
        .map(|value| now.saturating_sub(value) <= 2_000)
        .unwrap_or(false);

    let mut warnings = Vec::new();
    if !corrupted_files.is_empty() {
        status = "corrupt".to_string();
        warnings.push(format!(
            "hub runtime metadata is corrupted in {} file(s); run 'mcpace hub repair' to archive and reseed them",
            corrupted_files.len()
        ));
    }
    if corrupted_files.is_empty()
        && matches!(status.as_str(), "running" | "starting" | "stopping")
        && !heartbeat_fresh
    {
        status = "stale".to_string();
        warnings.push(
            "hub heartbeat is stale; the runtime likely exited without a clean stop".to_string(),
        );
    }
    if corrupted_files.is_empty() && !is_live_status(&status) && lock_file.exists {
        status = "stale".to_string();
        warnings.push(match lock_pid {
            Some(owner_pid) => format!(
                "hub runtime lock is still present for pid {}; cleanup is required before a new start",
                owner_pid
            ),
            None => "hub runtime lock is still present without readable owner metadata; cleanup is required before a new start".to_string(),
        });
    }

    let health = match status.as_str() {
        "running" | "starting" | "stopping" => {
            if readiness.ready_for_runtime_ops {
                "healthy".to_string()
            } else {
                "degraded".to_string()
            }
        }
        "stale" => "stale".to_string(),
        "corrupt" => "corrupt".to_string(),
        _ => {
            if readiness.ready_for_runtime_ops {
                "stopped-ready".to_string()
            } else {
                "stopped-degraded".to_string()
            }
        }
    };

    let uptime_ms = started_at_ms.and_then(|value| {
        if is_live_status(&status) {
            Some(now.saturating_sub(value))
        } else {
            None
        }
    });

    if status == "stopped" && !readiness.ready_for_runtime_ops {
        warnings.push("hub is stopped and runtime readiness is currently degraded".to_string());
    }

    Ok(HubStatus {
        root_path: root_path.display().to_string(),
        state_root: state_root.display().to_string(),
        runtime_dir: runtime_dir.display().to_string(),
        hub_dir: hub_dir.display().to_string(),
        log_path: log_path.display().to_string(),
        lease_store_path: lease_store_path.display().to_string(),
        config_version: readiness.config_version,
        active_profile: readiness.active_profile,
        profile_selection_source: readiness.profile_selection_source,
        status: status.clone(),
        health,
        pid,
        started_at_ms,
        last_heartbeat_at_ms,
        last_exit_at_ms,
        uptime_ms,
        ready_for_read_only_ops: readiness.ready_for_read_only_ops,
        ready_for_runtime_ops: readiness.ready_for_runtime_ops,
        server_count: readiness.server_count,
        required_server_count: readiness.required_server_count,
        profile_enabled_server_count: readiness.profile_enabled_server_count,
        source_enabled_server_count: readiness.source_enabled_server_count,
        effective_enabled_server_count: readiness.effective_enabled_server_count,
        missing_required_source_enablement: readiness.missing_required_source_enablement,
        missing_profile_source_enablement: readiness.missing_profile_source_enablement,
        missing_required_commands: readiness.missing_required_commands,
        missing_profile_commands: readiness.missing_profile_commands,
        warnings,
        corrupted_files,
        repair_recommended: status == "stale" || status == "corrupt",
    })
}

pub(super) fn write_status_response(
    status: &HubStatus,
    json_output: bool,
    stdout: &mut dyn Write,
) -> i32 {
    if json_output {
        let _ = writeln!(stdout, "{}", status.to_json_value().to_pretty_string());
        return 0;
    }

    let _ = writeln!(stdout, "Hub status: {}", status.status);
    let _ = writeln!(stdout, "Health: {}", status.health);
    let _ = writeln!(stdout, "Root path: {}", status.root_path);
    let _ = writeln!(stdout, "State root: {}", status.state_root);
    let _ = writeln!(stdout, "Runtime dir: {}", status.runtime_dir);
    let _ = writeln!(stdout, "Hub dir: {}", status.hub_dir);
    let _ = writeln!(stdout, "Log path: {}", status.log_path);
    let _ = writeln!(stdout, "Lease store: {}", status.lease_store_path);
    let _ = writeln!(stdout, "Active profile: {}", status.active_profile);
    let _ = writeln!(
        stdout,
        "Profile selection source: {}",
        status.profile_selection_source
    );
    let _ = writeln!(
        stdout,
        "Runtime readiness: {}",
        yes_no(status.ready_for_runtime_ops)
    );
    let _ = writeln!(
        stdout,
        "Repair recommended: {}",
        yes_no(status.repair_recommended)
    );
    let _ = writeln!(
        stdout,
        "Configured servers: {} (required={}, profile-enabled={}, source-enabled={}, effective-enabled={})",
        status.server_count,
        status.required_server_count,
        status.profile_enabled_server_count,
        status.source_enabled_server_count,
        status.effective_enabled_server_count
    );
    if let Some(pid) = status.pid {
        let _ = writeln!(stdout, "PID: {}", pid);
    }
    if let Some(started_at_ms) = status.started_at_ms {
        let _ = writeln!(stdout, "Started at ms: {}", started_at_ms);
    }
    if let Some(last_heartbeat_at_ms) = status.last_heartbeat_at_ms {
        let _ = writeln!(stdout, "Last heartbeat at ms: {}", last_heartbeat_at_ms);
    }
    if let Some(uptime_ms) = status.uptime_ms {
        let _ = writeln!(stdout, "Uptime ms: {}", uptime_ms);
    }
    if let Some(last_exit_at_ms) = status.last_exit_at_ms {
        let _ = writeln!(stdout, "Last exit at ms: {}", last_exit_at_ms);
    }
    if !status.missing_required_source_enablement.is_empty() {
        let _ = writeln!(
            stdout,
            "Missing required source enablement: {}",
            status.missing_required_source_enablement.join(", ")
        );
    }
    if !status.missing_profile_source_enablement.is_empty() {
        let _ = writeln!(
            stdout,
            "Missing profile source enablement: {}",
            status.missing_profile_source_enablement.join(", ")
        );
    }
    if !status.missing_required_commands.is_empty() {
        let _ = writeln!(
            stdout,
            "Missing required commands: {}",
            status.missing_required_commands.join(", ")
        );
    }
    if !status.missing_profile_commands.is_empty() {
        let _ = writeln!(
            stdout,
            "Missing profile commands: {}",
            status.missing_profile_commands.join(", ")
        );
    }
    if !status.corrupted_files.is_empty() {
        let _ = writeln!(stdout, "Corrupted runtime files:");
        for file in &status.corrupted_files {
            let _ = writeln!(stdout, "- {} :: {}", file.path, file.reason);
        }
    }
    if !status.warnings.is_empty() {
        let _ = writeln!(stdout, "Warnings: {}", status.warnings.join(" | "));
    }
    0
}

pub(super) fn is_live_status(status: &str) -> bool {
    matches!(status, "running" | "starting" | "stopping")
}

fn first_string(documents: &[Option<&JsonValue>], path: &[&str]) -> Option<String> {
    for document in documents {
        if let Some(value) = document.and_then(|doc| json_helpers::string_at_path(doc, path)) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

fn first_u128(documents: &[Option<&JsonValue>], path: &[&str]) -> Option<u128> {
    for document in documents {
        if let Some(value) = document.and_then(|doc| json_helpers::value_at_path(doc, path)) {
            match value {
                JsonValue::Number(number) => {
                    if let Ok(parsed) = number.parse::<u128>() {
                        return Some(parsed);
                    }
                }
                _ => {}
            }
        }
    }
    None
}

fn first_u32(documents: &[Option<&JsonValue>], path: &[&str]) -> Option<u32> {
    first_u128(documents, path).and_then(|value| u32::try_from(value).ok())
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}
