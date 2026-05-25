use super::model::ServerRecord;
use crate::json::JsonValue;
use crate::mcp_sources::{McpServerRemoveResult, McpServerToggleResult};
use crate::text_utils::join_or_none;
use crate::text_utils::yes_no;
use std::io::Write;

pub(super) fn render_list(
    records: &[ServerRecord],
    json_output: bool,
    stdout: &mut dyn Write,
) -> i32 {
    if json_output {
        let json = JsonValue::array(records.iter().map(ServerRecord::summary_json_value));
        let _ = writeln!(stdout, "{}", json.to_pretty_string());
        return 0;
    }

    let _ = writeln!(stdout, "Configured servers: {}", records.len());
    for record in records {
        let _ = writeln!(
            stdout,
            "- {} [{}] required={} source-enabled={} profile-enabled={} platform-supported={} effective-enabled={} default-enabled={}",
            record.name,
            record.kind,
            yes_no(record.required),
            yes_no(record.source_enabled),
            yes_no(record.profile_enabled),
            yes_no(record.platform_supported),
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
            "    platform supported: {}",
            yes_no(record.platform_supported)
        );
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

fn blank_to_none(value: &str) -> String {
    if value.trim().is_empty() {
        "none".to_string()
    } else {
        value.to_string()
    }
}

pub(super) fn render_test_result(
    result: &JsonValue,
    json_output: bool,
    stdout: &mut dyn Write,
) -> i32 {
    if json_output {
        let _ = writeln!(stdout, "{}", result.to_pretty_string());
        return if result
            .get("ok")
            .and_then(JsonValue::as_bool)
            .unwrap_or(false)
        {
            0
        } else {
            1
        };
    }

    let ok = result
        .get("ok")
        .and_then(JsonValue::as_bool)
        .unwrap_or(false);
    let server_count = result
        .get("serverCount")
        .and_then(JsonValue::as_i64)
        .unwrap_or(0);
    let ok_count = result
        .get("okCount")
        .and_then(JsonValue::as_i64)
        .unwrap_or(0);
    let failed_count = result
        .get("failedCount")
        .and_then(JsonValue::as_i64)
        .unwrap_or(0);
    let skipped_count = result
        .get("skippedCount")
        .and_then(JsonValue::as_i64)
        .unwrap_or(0);
    let _ = writeln!(
        stdout,
        "Server test tools/list smoke: {} (servers={} ok={} failed={} skipped={})",
        if ok { "PASS" } else { "FAIL" },
        server_count,
        ok_count,
        failed_count,
        skipped_count
    );
    if let Some(results) = result.get("results").and_then(JsonValue::as_array) {
        for item in results {
            let server = item
                .get("server")
                .and_then(JsonValue::as_str)
                .unwrap_or("unknown");
            let status = item
                .get("status")
                .and_then(JsonValue::as_str)
                .unwrap_or("unknown");
            let item_ok = item.get("ok").and_then(JsonValue::as_bool).unwrap_or(false);
            let tool_count = item
                .get("toolCount")
                .and_then(JsonValue::as_i64)
                .map(|value| value.to_string())
                .unwrap_or_else(|| "n/a".to_string());
            let _ = writeln!(
                stdout,
                "- {} status={} ok={} tools={}",
                server,
                status,
                yes_no(item_ok),
                tool_count
            );
            if !item_ok {
                if let Some(error) = item.get("error").and_then(JsonValue::as_str) {
                    let _ = writeln!(stdout, "    error: {}", error);
                }
            }
        }
    }
    if ok {
        0
    } else {
        1
    }
}

pub(super) fn render_remove_result(
    result: &McpServerRemoveResult,
    json_output: bool,
    stdout: &mut dyn Write,
) -> i32 {
    if json_output {
        let _ = writeln!(stdout, "{}", result.to_json_value().to_pretty_string());
        return 0;
    }
    let dry_run_prefix = if result.dry_run { "dry-run " } else { "" };
    let _ = writeln!(
        stdout,
        "{}removed MCP server '{}' from {} (remaining servers: {})",
        dry_run_prefix, result.name, result.path, result.remaining_server_count
    );
    0
}

pub(super) fn render_import_result(
    result: &crate::mcp_sources::McpServerImportResult,
    json_output: bool,
    stdout: &mut dyn Write,
) -> i32 {
    if json_output {
        let _ = writeln!(stdout, "{}", result.to_json_value().to_pretty_string());
        return 0;
    }

    let dry_run_prefix = if result.dry_run { "dry-run " } else { "" };
    let _ = writeln!(
        stdout,
        "{}imported {} MCP server(s) from {} into {} target file(s)",
        dry_run_prefix, result.imported_count, result.source_path, result.target_file_count
    );
    for entry in &result.entries {
        let _ = writeln!(
            stdout,
            "- {}: {} -> {} (existedBefore={})",
            entry.name,
            entry.action,
            entry.path,
            yes_no(entry.existed_before)
        );
    }
    if !result.warnings.is_empty() {
        let _ = writeln!(stdout, "Warnings: {}", result.warnings.join(", "));
    }
    if result.dry_run {
        let _ = writeln!(stdout, "no files written; rerun without --dry-run to apply");
    }
    0
}

pub(super) fn render_sources(
    report: &crate::mcp_sources::McpSourceReport,
    json_output: bool,
    stdout: &mut dyn Write,
) -> i32 {
    if json_output {
        let _ = writeln!(stdout, "{}", report.to_json_value().to_pretty_string());
        return 0;
    }

    let _ = writeln!(
        stdout,
        "MCP settings sources: {} active file(s), {} server(s)",
        report.registry.sources.len(),
        report.registry.servers.len()
    );
    for source in &report.source_statuses {
        let _ = writeln!(
            stdout,
            "- {} [{}] exists={} servers={}",
            source.path,
            source.origin,
            yes_no(source.exists),
            source.server_count
        );
    }
    if !report.registry.servers.is_empty() {
        let _ = writeln!(stdout, "Servers:");
        for entry in report.registry.servers.values() {
            let _ = writeln!(
                stdout,
                "- {} (normalized={}, source={})",
                entry.name, entry.normalized_name, entry.source
            );
        }
    }
    let _ = writeln!(
        stdout,
        "Warnings: {}",
        join_or_none(&report.registry.warnings)
    );
    0
}

pub(super) fn render_add_result(
    result: &crate::mcp_sources::McpServerWriteResult,
    json_output: bool,
    stdout: &mut dyn Write,
) -> i32 {
    if json_output {
        let _ = writeln!(stdout, "{}", result.to_json_value().to_pretty_string());
        return 0;
    }

    let _ = writeln!(
        stdout,
        "MCP server {}: {} ({})",
        result.action, result.name, result.server_type
    );
    let _ = writeln!(stdout, "  source: {}", result.path);
    if let Some(command) = &result.command {
        let _ = writeln!(stdout, "  command: {}", command);
    }
    if let Some(url) = &result.url {
        let _ = writeln!(stdout, "  url: {}", url);
    }
    let _ = writeln!(
        stdout,
        "  args={} env={} headers={} existedBefore={} dryRun={}",
        result.args_count,
        result.env_count,
        result.header_count,
        yes_no(result.existed_before),
        yes_no(result.dry_run)
    );
    if result.dry_run {
        let _ = writeln!(
            stdout,
            "  no files written; rerun without --dry-run to apply"
        );
    } else {
        let _ = writeln!(
            stdout,
            "  next: mcpace server test {} --refresh",
            result.normalized_name
        );
        let _ = writeln!(
            stdout,
            "  then: mcpace client install <client|all> --dry-run --diff"
        );
    }
    0
}

pub(super) fn render_install_result(
    result: &crate::mcp_autoinstall::McpAutoInstallResult,
    json_output: bool,
    stdout: &mut dyn Write,
) -> i32 {
    if json_output {
        let _ = writeln!(stdout, "{}", result.to_json_value().to_pretty_string());
        return 0;
    }

    let _ = writeln!(
        stdout,
        "MCP server auto-install {}: {} ({}, launcher={})",
        result.write.action, result.write.name, result.plan.method, result.plan.launcher
    );
    let _ = writeln!(stdout, "  source: {}", result.write.path);
    if let Some(package) = &result.plan.package {
        let _ = writeln!(stdout, "  package: {}", package);
    }
    if let Some(command) = &result.plan.command {
        let _ = writeln!(stdout, "  command: {}", command);
    }
    if let Some(url) = &result.plan.url {
        let _ = writeln!(stdout, "  url: {}", url);
    }
    let _ = writeln!(
        stdout,
        "  args={} env={} headers={} existedBefore={} dryRun={}",
        result.write.args_count,
        result.write.env_count,
        result.write.header_count,
        yes_no(result.write.existed_before),
        yes_no(result.write.dry_run)
    );
    if !result.plan.assumptions.is_empty() {
        let _ = writeln!(
            stdout,
            "  assumptions: {}",
            result.plan.assumptions.join(" | ")
        );
    }
    if result.write.dry_run {
        let _ = writeln!(
            stdout,
            "  no files written; rerun without --dry-run to apply"
        );
    } else if result.write.server_type == "stdio" {
        let _ = writeln!(
            stdout,
            "  next: mcpace server test {} --refresh",
            result.write.normalized_name
        );
        let _ = writeln!(
            stdout,
            "  then: mcpace server capabilities {} --json",
            result.write.normalized_name
        );
    } else {
        let _ = writeln!(stdout, "  next: mcpace server sources --json");
    }
    0
}

pub(super) fn render_toggle_result(
    result: &McpServerToggleResult,
    json_output: bool,
    stdout: &mut dyn Write,
) -> i32 {
    if json_output {
        let _ = writeln!(stdout, "{}", result.to_json_value().to_pretty_string());
        return 0;
    }

    let dry_run_prefix = if result.dry_run { "dry-run " } else { "" };
    let previous = result
        .previous_enabled
        .map(yes_no)
        .unwrap_or("implicit-yes");
    let _ = writeln!(
        stdout,
        "{}{}d MCP server '{}' in {} (previous enabled={}, now enabled={})",
        dry_run_prefix,
        result.action,
        result.name,
        result.path,
        previous,
        yes_no(result.enabled)
    );
    if result.dry_run {
        let _ = writeln!(
            stdout,
            "  no files written; rerun without --dry-run to apply"
        );
    } else {
        let _ = writeln!(
            stdout,
            "  next: mcpace server test {} --refresh",
            result.normalized_name
        );
        let _ = writeln!(stdout, "  then: mcpace verify readiness --json");
    }
    0
}
