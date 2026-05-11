use crate::doctor;
use crate::verify::model::collect_readiness;
use std::io::Write;
use std::path::PathBuf;

pub(super) fn run_grouped_doctor(
    root_override: Option<PathBuf>,
    json_output: bool,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let report = doctor::run(root_override);
    if json_output {
        let _ = writeln!(stdout, "{}", report.to_pretty_json());
        return 0;
    }
    doctor::write_text_report(&report, stdout);
    let _ = writeln!(
        stdout,
        "verify doctor completed against the grouped Rust surface."
    );
    let _ = stderr.flush();
    0
}

pub(super) fn run_readiness(
    root_override: Option<PathBuf>,
    json_output: bool,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let Some(root_path) = root_override else {
        let _ = writeln!(stderr, "mcpace root not found; expected mcpace.config.json");
        return 1;
    };

    let readiness = match collect_readiness(&root_path) {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 1;
        }
    };

    if json_output {
        let _ = writeln!(stdout, "{}", readiness.to_json_value().to_pretty_string());
        return 0;
    }

    let _ = writeln!(stdout, "Active profile: {}", readiness.active_profile);
    let _ = writeln!(
        stdout,
        "Profile selection source: {}",
        readiness.profile_selection_source
    );
    let _ = writeln!(
        stdout,
        "Read-only readiness: {}",
        yes_no(readiness.ready_for_read_only_ops)
    );
    let _ = writeln!(
        stdout,
        "Runtime readiness: {}",
        yes_no(readiness.ready_for_runtime_ops)
    );
    let _ = writeln!(stdout, "Configured servers: {}", readiness.server_count);
    let _ = writeln!(
        stdout,
        "Required servers: {}",
        readiness.required_server_count
    );
    let _ = writeln!(
        stdout,
        "Profile-enabled servers: {}",
        readiness.profile_enabled_server_count
    );
    let _ = writeln!(
        stdout,
        "Required servers source-enabled: {}",
        readiness.required_source_enabled_count
    );
    let _ = writeln!(
        stdout,
        "Source-enabled servers: {}",
        readiness.source_enabled_server_count
    );
    let _ = writeln!(
        stdout,
        "Effective enabled servers: {}",
        readiness.effective_enabled_server_count
    );
    let _ = writeln!(
        stdout,
        "Missing required source enablement: {}",
        join_or_none(&readiness.missing_required_source_enablement)
    );
    let _ = writeln!(
        stdout,
        "Missing profile source enablement: {}",
        join_or_none(&readiness.missing_profile_source_enablement)
    );
    let _ = writeln!(
        stdout,
        "Missing runtime prerequisites: {}",
        join_or_none(&readiness.missing_runtime_prerequisites)
    );
    let _ = writeln!(
        stdout,
        "Client config warnings: {}",
        join_or_none(&readiness.client_config_warnings)
    );
    let _ = writeln!(
        stdout,
        "Missing required commands: {}",
        join_or_none(&readiness.missing_required_commands)
    );
    let _ = writeln!(
        stdout,
        "Missing profile commands: {}",
        join_or_none(&readiness.missing_profile_commands)
    );
    let _ = writeln!(
        stdout,
        "Rust source readiness: {}",
        yes_no(readiness.rust_source_ready)
    );
    let _ = writeln!(
        stdout,
        "npm surface readiness: {}",
        yes_no(readiness.npm_surface_ready)
    );
    let _ = writeln!(
        stdout,
        "Runtime prerequisites readiness: {}",
        yes_no(readiness.runtime_prerequisites_ready)
    );
    let _ = writeln!(
        stdout,
        "Optional container tooling readiness: {}",
        yes_no(readiness.container_tooling_ready)
    );
    0
}

fn join_or_none(values: &[String]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values.join(", ")
    }
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}
