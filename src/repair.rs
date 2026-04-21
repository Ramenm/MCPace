use crate::hub;
use std::io::Write;
use std::path::PathBuf;

pub fn run(
    args: &[String],
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let mut forwarded = Vec::with_capacity(args.len() + 1);
    forwarded.push("repair".to_string());

    for arg in args {
        match arg.as_str() {
            "--json" | "-json" | "--root" | "-root" => forwarded.push(arg.clone()),
            "-h" | "--help" | "-?" => {
                write_help(stdout);
                return 0;
            }
            _ => forwarded.push(arg.clone()),
        }
    }

    hub::run(&forwarded, default_root, stdout, stderr)
}

fn write_help(stdout: &mut dyn Write) {
    let _ = writeln!(stdout, "Usage: mcpace repair [--json] [--root <path>]");
    let _ = writeln!(stdout, "");
    let _ = writeln!(stdout, "Implemented now:");
    let _ = writeln!(stdout, "  mcpace repair [--json] [--root <path>]");
    let _ = writeln!(stdout, "");
    let _ = writeln!(
        stdout,
        "Current scope: this grouped maintenance command is a safe shorthand for 'mcpace hub repair'."
    );
}
