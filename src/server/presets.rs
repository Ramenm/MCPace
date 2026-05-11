use super::args::ParsedArgs;
use super::preset_render;
use crate::mcp_presets;
use std::io::Write;
use std::path::PathBuf;

pub(super) fn run(
    parsed: &ParsedArgs,
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let root_path = parsed.root_override.clone().or(default_root);
    let Some(root_path) = root_path else {
        let _ = writeln!(stderr, "mcpace root not found; expected mcpace.config.json");
        return 1;
    };
    let catalog = match mcp_presets::load_preset_catalog(&root_path) {
        Ok(catalog) => catalog,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 1;
        }
    };
    preset_render::render_preset_catalog(&catalog, parsed.json_output, stdout)
}

pub(super) fn install(
    parsed: &ParsedArgs,
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let root_path = parsed.root_override.clone().or(default_root);
    let Some(root_path) = root_path else {
        let _ = writeln!(stderr, "mcpace root not found; expected mcpace.config.json");
        return 1;
    };
    let Some(preset_id) = parsed.name_filter.clone() else {
        let _ = writeln!(stderr, "server install requires a preset id, for example: mcpace server install filesystem --path .");
        return 2;
    };
    let result = match mcp_presets::install_preset(
        &root_path,
        mcp_presets::McpPresetInstallOptions {
            preset_id,
            name_override: parsed.install_name_override.clone(),
            paths: parsed.paths.clone(),
            extra_args: parsed.args.clone(),
            env: parsed.env.clone(),
            settings_path: parsed.settings_path.clone(),
            dry_run: parsed.dry_run,
            force: parsed.force,
        },
    ) {
        Ok(result) => result,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 1;
        }
    };
    preset_render::render_preset_install_result(&result, parsed.json_output, stdout)
}

pub(super) fn starter(
    parsed: &ParsedArgs,
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let root_path = parsed.root_override.clone().or(default_root);
    let Some(root_path) = root_path else {
        let _ = writeln!(stderr, "mcpace root not found; expected mcpace.config.json");
        return 1;
    };
    let result = match mcp_presets::install_starter(
        &root_path,
        &parsed.paths,
        parsed.settings_path.clone(),
        parsed.dry_run,
        parsed.force,
    ) {
        Ok(result) => result,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 1;
        }
    };
    preset_render::render_starter_result(&result, parsed.json_output, stdout)
}
