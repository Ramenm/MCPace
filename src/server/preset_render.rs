use std::io::Write;

pub(super) fn render_preset_catalog(
    catalog: &crate::mcp_presets::McpPresetCatalog,
    json_output: bool,
    stdout: &mut dyn Write,
) -> i32 {
    if json_output {
        let _ = writeln!(stdout, "{}", catalog.to_json_value().to_pretty_string());
        return 0;
    }

    let _ = writeln!(stdout, "Useful MCP presets: {}", catalog.presets.len());
    if !catalog.sources.is_empty() {
        let _ = writeln!(stdout, "Sources: {}", catalog.sources.join(", "));
    }
    if !catalog.warnings.is_empty() {
        let _ = writeln!(stdout, "Warnings: {}", catalog.warnings.join(" | "));
    }
    if !catalog.starter_presets.is_empty() {
        let _ = writeln!(
            stdout,
            "Starter pack '{}': {}",
            catalog.starter_name,
            catalog.starter_presets.join(", ")
        );
    }
    for preset in &catalog.presets {
        let _ = writeln!(stdout, "- {} ({})", preset.id, preset.title);
        let _ = writeln!(stdout, "    {}", preset.description);
        let _ = writeln!(
            stdout,
            "    command: {} {}",
            preset.command,
            preset.args.join(" ")
        );
        let _ = writeln!(
            stdout,
            "    trust: {}; pathMode: {}; defaultName: {}",
            preset.trust_level, preset.path_mode, preset.default_name
        );
        if !preset.notes.is_empty() {
            let _ = writeln!(stdout, "    notes: {}", preset.notes.join(" | "));
        }
        let _ = writeln!(
            stdout,
            "    install: mcpace server install {}{}",
            preset.id,
            if matches!(preset.path_mode.as_str(), "append" | "repository-flag") {
                " --path ."
            } else {
                ""
            }
        );
    }
    0
}

pub(super) fn render_preset_install_result(
    result: &crate::mcp_presets::McpPresetInstallResult,
    json_output: bool,
    stdout: &mut dyn Write,
) -> i32 {
    if json_output {
        let _ = writeln!(stdout, "{}", result.to_json_value().to_pretty_string());
        return 0;
    }

    let _ = writeln!(
        stdout,
        "MCP preset installed: {} -> {}",
        result.preset.id, result.write.name
    );
    let _ = writeln!(stdout, "  source: {}", result.write.path);
    let _ = writeln!(
        stdout,
        "  command: {} {}",
        result.preset.command,
        result.preset.args.join(" ")
    );
    if !result.paths.is_empty() {
        let _ = writeln!(stdout, "  allowed paths: {}", result.paths.join(", "));
    }
    let _ = writeln!(stdout, "  trust: {}", result.preset.trust_level);
    if result.write.dry_run {
        let _ = writeln!(
            stdout,
            "  no files written; rerun without --dry-run to apply"
        );
    } else {
        let _ = writeln!(
            stdout,
            "  next: mcpace server test {} --refresh",
            result.write.normalized_name
        );
        let _ = writeln!(
            stdout,
            "  then: mcpace connect --server {}",
            result.write.normalized_name
        );
    }
    0
}

pub(super) fn render_starter_result(
    result: &crate::mcp_presets::McpPresetStarterResult,
    json_output: bool,
    stdout: &mut dyn Write,
) -> i32 {
    if json_output {
        let _ = writeln!(stdout, "{}", result.to_json_value().to_pretty_string());
        return 0;
    }

    let _ = writeln!(
        stdout,
        "MCP starter pack '{}': {} preset(s)",
        result.name,
        result.installed.len()
    );
    if !result.description.is_empty() {
        let _ = writeln!(stdout, "  {}", result.description);
    }
    for install in &result.installed {
        let _ = writeln!(
            stdout,
            "- {} -> {} ({})",
            install.preset.id, install.write.name, install.write.path
        );
        if !install.paths.is_empty() {
            let _ = writeln!(stdout, "    allowed paths: {}", install.paths.join(", "));
        }
    }
    if result.dry_run {
        let _ = writeln!(
            stdout,
            "  no files written; rerun without --dry-run to apply"
        );
    } else {
        let _ = writeln!(stdout, "  next: mcpace server test --refresh");
        let _ = writeln!(stdout, "  then: mcpace connect");
    }
    0
}
