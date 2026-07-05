use crate::service;
use std::io::Write;
use std::path::PathBuf;

pub fn run(
    args: &[String],
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let mut forwarded = args.to_vec();
    if forwarded.is_empty() {
        forwarded.push("status".to_string());
    }
    if matches!(
        forwarded.first().map(String::as_str),
        Some("-h" | "--help" | "-?")
    ) {
        write_help(stdout);
        return 0;
    }
    if let Some(action) = forwarded.first_mut() {
        *action = match action.to_ascii_lowercase().as_str() {
            "enable" => "install".to_string(),
            "disable" => "uninstall".to_string(),
            "repair" => "install".to_string(),
            "status" | "verify" | "print" | "install" | "uninstall" => action.to_ascii_lowercase(),
            other => {
                let _ = writeln!(stderr, "unsupported autostart action: {}", other);
                return 2;
            }
        };
    }
    service::run(&forwarded, default_root, stdout, stderr)
}

fn write_help(stdout: &mut dyn Write) {
    let _ = writeln!(stdout, "Usage: mcpace autostart <enable|disable|status|verify|repair|print> [--json] [serve options]");
    let _ = writeln!(stdout);
    let _ = writeln!(stdout, "Autostart is the normal user-level login item. It installs MCPace Agent via auto-launch and does not create a privileged system service.");
}
