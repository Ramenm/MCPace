use super::args::ParsedArgs;
use super::render;
use crate::mcp_autoinstall;
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
    let spec = parsed.name_filter.clone().unwrap_or_default();
    let result = match mcp_autoinstall::install_auto(
        &root_path,
        mcp_autoinstall::McpAutoInstallOptions {
            spec,
            name_override: parsed.install_name_override.clone(),
            server_type: parsed.server_type.clone(),
            command: parsed.command.clone(),
            url: parsed.url.clone(),
            paths: parsed.paths.clone(),
            extra_args: parsed.args.clone(),
            env: parsed.env.clone(),
            headers: parsed.headers.clone(),
            settings_path: parsed.settings_path.clone(),
            dry_run: parsed.dry_run,
            force: parsed.force,
            disabled: parsed.disabled,
        },
    ) {
        Ok(result) => result,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 1;
        }
    };
    render::render_install_result(&result, parsed.json_output, stdout)
}
