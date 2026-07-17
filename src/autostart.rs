use crate::diagnostics;
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
            "status" | "verify" | "prove" | "print" | "install" | "uninstall" => {
                action.to_ascii_lowercase()
            }
            other => {
                diagnostics::stderr_line(
                    stderr,
                    format_args!("unsupported autostart action: {}", other),
                );
                return 2;
            }
        };
    }
    service::run(&forwarded, default_root, stdout, stderr)
}

fn write_help(stdout: &mut dyn Write) {
    let _ = writeln!(stdout, "Usage: mcpace advanced autostart <enable|disable|status|verify|prove|repair|print> [--json] [serve options]");
    let _ = writeln!(stdout);
    let _ = writeln!(stdout, "Autostart is installed and repaired automatically by `mcpace up` unless --no-autostart is used. It is user-level: systemd --user on Linux, a supervised hidden login launcher on Windows, and a LaunchAgent on macOS.");
    let _ = writeln!(stdout, "On Windows and Linux, enable/repair activates the supervisor immediately and disable stops its current runtime.");
    let _ = writeln!(stdout, "`prove` activates the exact registered login target without rebooting, verifies endpoint/process identity, and restores the initial running state.");
}
