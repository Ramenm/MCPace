use super::actions::client_install_support_summary;
use super::pathing::normalize;
use crate::runtimepaths;
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Default, Clone)]
pub(super) struct ParsedArgs {
    pub(super) action: Option<String>,
    pub(super) json_output: bool,
    pub(super) help: bool,
    pub(super) root_override: Option<PathBuf>,
    pub(super) client_id: Option<String>,
    pub(super) session_id: Option<String>,
    pub(super) project_root: Option<String>,
    pub(super) transport: Option<String>,
    pub(super) metadata_json: Option<String>,
    pub(super) dry_run: bool,
    pub(super) diff: bool,
    pub(super) backup: Option<String>,
    pub(super) error: Option<String>,
}

pub(super) fn parse_args(args: &[String]) -> ParsedArgs {
    let mut parsed = ParsedArgs::default();
    let mut index = 0usize;

    while index < args.len() {
        let token = normalize(&args[index]);
        match token.as_str() {
            "plan" | "install" | "export" | "list" | "restore" => {
                if parsed.action.is_some() {
                    parsed.error = Some("client accepts only one action".to_string());
                    return parsed;
                }
                parsed.action = Some(token);
                index += 1;
            }
            "--json" | "-json" => {
                parsed.json_output = true;
                index += 1;
            }
            "--dry-run" => {
                parsed.dry_run = true;
                index += 1;
            }
            "--diff" => {
                parsed.diff = true;
                index += 1;
            }
            "--backup" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error =
                        Some("client restore requires a value after --backup".to_string());
                    return parsed;
                };
                parsed.backup = Some(value.to_string());
                index += 2;
            }
            "--latest" => {
                parsed.backup = Some("latest".to_string());
                index += 1;
            }
            "--root" | "-root" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some("client requires a path after --root".to_string());
                    return parsed;
                };
                parsed.root_override = Some(PathBuf::from(value));
                index += 2;
            }
            "--client-id" | "-client-id" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error =
                        Some("client plan requires a value after --client-id".to_string());
                    return parsed;
                };
                parsed.client_id = Some(value.to_string());
                index += 2;
            }
            "--session-id" | "-session-id" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error =
                        Some("client plan requires a value after --session-id".to_string());
                    return parsed;
                };
                parsed.session_id = Some(value.to_string());
                index += 2;
            }
            "--project-root" | "-project-root" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error =
                        Some("client plan requires a value after --project-root".to_string());
                    return parsed;
                };
                parsed.project_root = Some(value.to_string());
                index += 2;
            }
            "--transport" | "-transport" | "--ingress" | "-ingress" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error =
                        Some("client plan requires a value after --transport".to_string());
                    return parsed;
                };
                parsed.transport = Some(value.to_string());
                index += 2;
            }
            "--metadata-json" | "-metadata-json" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some(
                        "client plan requires a JSON string after --metadata-json".to_string(),
                    );
                    return parsed;
                };
                parsed.metadata_json = Some(value.to_string());
                index += 2;
            }
            "-h" | "--help" | "-?" => {
                parsed.help = true;
                return parsed;
            }
            _ => {
                if matches!(
                    parsed.action.as_deref(),
                    Some("export" | "install" | "restore")
                ) && parsed.client_id.is_none()
                {
                    parsed.client_id = Some(args[index].to_string());
                    index += 1;
                    continue;
                }

                parsed.error = Some(format!(
                    "unsupported client arguments in the Rust-only repo: {}",
                    args[index]
                ));
                return parsed;
            }
        }
    }

    parsed
}

pub(super) fn write_help(stdout: &mut dyn Write) {
    let _ = writeln!(
        stdout,
        "Usage: mcpace client <plan|list|install|restore|export> [options]"
    );
    let _ = writeln!(stdout);
    let _ = writeln!(stdout, "Implemented now:");
    let _ = writeln!(stdout, "  mcpace client list [--json] [--root <path>]");
    let _ = writeln!(stdout, "  mcpace client plan [--json] [--root <path>] [--client-id <id>] [--session-id <id>] [--project-root <path>] [--transport <stdio|streamable-http>] [--metadata-json <json>]");
    let _ = writeln!(
        stdout,
        "  mcpace client install <client|all> [--json] [--root <path>] [--dry-run] [--diff]"
    );
    let _ = writeln!(
        stdout,
        "  mcpace client restore <client|all> [--json] [--root <path>] [--backup <id|latest>]"
    );
    let _ = writeln!(stdout, "  mcpace client export <client> [--json] [--root <path>] [--transport <stdio|streamable-http>] [--session-id <id>] [--project-root <path>] [--metadata-json <json>]");
    let _ = writeln!(stdout);
    let _ = writeln!(
        stdout,
        "client list shows the currently verified/generic client target catalog."
    );
    let _ = writeln!(stdout, "client plan inspects routing context, derived session leases, and server arbitration without starting a hub runtime.");
    let _ = writeln!(
        stdout,
        "client install currently supports {}. Use client install all to patch every catalog-declared local client that has an install writer. It writes only the MCPace-owned config entry or block and defaults to the broadest documented shared scope for that client surface.",
        client_install_support_summary()
    );
    let _ = writeln!(
        stdout,
        "Use --dry-run to preview install patches without writing client config files; add --diff to inspect the exact candidate config change. Real writes create a rollback backup that can be applied with client restore."
    );
    let _ = writeln!(
        stdout,
        "client export is HTTP-first: for local clients that document Streamable HTTP, it emits the configured MCPace URL (default {}). Override it with mcpace.config.json serve.publicUrl or MCPACE_PUBLIC_MCP_URL when a cloud/public connector must reach MCPace.",
        runtimepaths::default_local_mcp_url()
    );
}
