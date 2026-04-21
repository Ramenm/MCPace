use super::model::ServerRecord;
use crate::json::JsonValue;
use std::io::Write;

pub(super) fn render_list(records: &[ServerRecord], json_output: bool, stdout: &mut dyn Write) -> i32 {
    if json_output {
        let json = JsonValue::array(records.iter().map(ServerRecord::summary_json_value));
        let _ = writeln!(stdout, "{}", json.to_pretty_string());
        return 0;
    }

    let _ = writeln!(stdout, "Configured servers: {}", records.len());
    for record in records {
        let _ = writeln!(
            stdout,
            "- {} [{}] required={} source-enabled={} profile-enabled={} effective-enabled={} default-enabled={}",
            record.name,
            record.kind,
            yes_no(record.required),
            yes_no(record.source_enabled),
            yes_no(record.profile_enabled),
            yes_no(record.effective_enabled),
            yes_no(record.default_enabled)
        );
        let _ = writeln!(
            stdout,
            "    scope={}; concurrency={}; state={}; credential={}",
            record.scope_class,
            record.concurrency_policy,
            record.state_binding,
            record.credential_binding
        );
    }
    0
}

pub(super) fn render_capabilities(
    records: &[ServerRecord],
    name_filter: Option<&str>,
    json_output: bool,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let filtered = filter_records(records, name_filter);
    if filtered.is_empty() {
        if let Some(name) = name_filter {
            let _ = writeln!(stderr, "no configured server named '{}'", name);
            return 1;
        }
    }

    if json_output {
        let json = JsonValue::array(
            filtered
                .iter()
                .map(|record| record.capabilities_json_value()),
        );
        let _ = writeln!(stdout, "{}", json.to_pretty_string());
        return 0;
    }

    for record in filtered {
        let _ = writeln!(stdout, "- {}", record.name);
        let _ = writeln!(stdout, "    kind: {}", record.kind);
        let _ = writeln!(
            stdout,
            "    supported transports: {}",
            join_or_none(&record.supported_transports)
        );
        let _ = writeln!(stdout, "    platforms: {}", join_or_none(&record.platforms));
        let _ = writeln!(
            stdout,
            "    required commands: {}",
            join_or_none(&record.required_commands)
        );
        let _ = writeln!(
            stdout,
            "    profile enabled: {}",
            yes_no(record.profile_enabled)
        );
        let _ = writeln!(
            stdout,
            "    effective enabled: {}",
            yes_no(record.effective_enabled)
        );
        let _ = writeln!(
            stdout,
            "    source type: {}",
            blank_to_none(&record.source_type)
        );
        let _ = writeln!(
            stdout,
            "    source command: {}",
            blank_to_none(&record.source_command)
        );
        let _ = writeln!(
            stdout,
            "    source url: {}",
            blank_to_none(&record.source_url)
        );
        let _ = writeln!(
            stdout,
            "    health url: {}",
            blank_to_none(&record.health_url)
        );
        let _ = writeln!(
            stdout,
            "    installer: target={}, method={}, package={}, verify={}",
            blank_to_none(&record.installer_target),
            blank_to_none(&record.installer_method),
            blank_to_none(&record.installer_package),
            blank_to_none(&record.installer_verify_command)
        );
    }
    0
}

fn filter_records<'a>(
    records: &'a [ServerRecord],
    name_filter: Option<&str>,
) -> Vec<&'a ServerRecord> {
    match name_filter {
        Some(name) => {
            let normalized = name.trim().to_ascii_lowercase();
            records
                .iter()
                .filter(|record| record.name.to_ascii_lowercase() == normalized)
                .collect()
        }
        None => records.iter().collect(),
    }
}

fn join_or_none(values: &[String]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values.join(", ")
    }
}

fn blank_to_none(value: &str) -> String {
    if value.trim().is_empty() {
        "none".to_string()
    } else {
        value.to_string()
    }
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}
