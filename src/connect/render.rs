use super::model::ConnectReport;
use std::io::Write;

pub(super) fn render(report: &ConnectReport, json_output: bool, stdout: &mut dyn Write) -> i32 {
    if json_output {
        let _ = writeln!(stdout, "{}", report.to_json_value().to_pretty_string());
        return if report.blockers.is_empty() { 0 } else { 1 };
    }

    let _ = writeln!(stdout, "MCPace connect guide");
    let _ = writeln!(stdout, "root: {}", report.root_path);
    let _ = writeln!(stdout);
    let _ = writeln!(stdout, "Endpoint:");
    let _ = writeln!(stdout, "  local MCP URL: {}", report.endpoint.local_mcp_url);
    let _ = writeln!(
        stdout,
        "  advertised MCP URL: {}",
        report.endpoint.advertised_mcp_url
    );
    let _ = writeln!(stdout, "  health URL: {}", report.endpoint.health_url);
    let _ = writeln!(
        stdout,
        "  public URL configured: {}",
        yes_no(report.endpoint.public_url_configured)
    );
    let _ = writeln!(stdout);

    let _ = writeln!(stdout, "Client:");
    if let Some(client) = &report.selected_client {
        let _ = writeln!(
            stdout,
            "  selected: {} ({})",
            client.id, client.display_name
        );
        let _ = writeln!(stdout, "  proof tier: {}", client.proof_tier);
        let _ = writeln!(
            stdout,
            "  local HTTP: {}",
            yes_no(client.supports_local_http)
        );
        let _ = writeln!(
            stdout,
            "  install patcher: {}",
            yes_no(client.supports_install)
        );
        if let Some(path) = &client.install_path {
            let _ = writeln!(stdout, "  install path: {}", path);
        }
        let _ = writeln!(stdout, "  selection: {}", client.selection_reason);
    } else {
        let _ = writeln!(stdout, "  selected: none");
    }
    let _ = writeln!(stdout);

    let _ = writeln!(stdout, "Upstream MCP servers:");
    let _ = writeln!(stdout, "  configured: {}", report.upstream.configured_count);
    let _ = writeln!(stdout, "  sources: {}", report.upstream.source_count);
    let _ = writeln!(
        stdout,
        "  enabled/effective: {}",
        report.upstream.effective_enabled_count
    );
    let _ = writeln!(stdout, "  stdio callable: {}", report.upstream.stdio_count);
    let _ = writeln!(
        stdout,
        "  HTTP inventory-only: {}",
        report.upstream.http_inventory_count
    );
    let _ = writeln!(
        stdout,
        "  selected server: {}",
        report.upstream.selected_server.as_deref().unwrap_or("none")
    );
    if !report.upstream.names.is_empty() {
        let _ = writeln!(stdout, "  names: {}", report.upstream.names.join(", "));
    }
    let _ = writeln!(stdout);

    let _ = writeln!(stdout, "Readiness:");
    let _ = writeln!(
        stdout,
        "  read-only: {}",
        yes_no(report.readiness.read_only_ready)
    );
    let _ = writeln!(
        stdout,
        "  runtime: {}",
        yes_no(report.readiness.runtime_ready)
    );
    let _ = writeln!(stdout);

    if !report.blockers.is_empty() {
        let _ = writeln!(stdout, "Blockers:");
        for blocker in &report.blockers {
            let _ = writeln!(stdout, "  - {}", blocker);
        }
        let _ = writeln!(stdout);
    }

    if !report.warnings.is_empty() {
        let _ = writeln!(stdout, "Warnings:");
        for warning in &report.warnings {
            let _ = writeln!(stdout, "  - {}", warning);
        }
        let _ = writeln!(stdout);
    }

    let _ = writeln!(stdout, "Next steps:");
    for (index, step) in report.next_steps.iter().enumerate() {
        let _ = writeln!(stdout, "  {}. {}", index + 1, step);
    }

    if report.blockers.is_empty() {
        0
    } else {
        1
    }
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}
