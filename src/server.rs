mod args;
mod loader;
mod model;
mod render;

use self::args::{parse_args, write_help};
pub use self::loader::load_server_records;
pub use self::model::ServerRecord;
use self::render::{render_capabilities, render_list};
use crate::candidates;
use std::io::Write;
use std::path::PathBuf;

pub fn run(
    args: &[String],
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let parsed = parse_args(args);
    if let Some(error) = parsed.error {
        let _ = writeln!(stderr, "{}", error);
        return 2;
    }
    if parsed.help || parsed.action.is_none() {
        write_help(stdout);
        return 0;
    }

    let action = parsed.action.clone().unwrap_or_default();
    if action == "candidates" {
        let mut forwarded = Vec::new();
        if parsed.json_output {
            forwarded.push("--json".to_string());
        }
        if let Some(root) = &parsed.root_override {
            forwarded.push("--root".to_string());
            forwarded.push(root.display().to_string());
        }
        return candidates::run(&forwarded, default_root, stdout, stderr);
    }

    let root_path = parsed.root_override.or(default_root);
    let Some(root_path) = root_path else {
        let _ = writeln!(stderr, "mcpace root not found; expected mcpace.config.json");
        return 1;
    };

    let records = match load_server_records(&root_path) {
        Ok(records) => records,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 1;
        }
    };

    match action.as_str() {
        "list" => render_list(&records, parsed.json_output, stdout),
        "capabilities" => render_capabilities(
            &records,
            parsed.name_filter.as_deref(),
            parsed.json_output,
            stdout,
            stderr,
        ),
        other => {
            let _ = writeln!(
                stderr,
                "unsupported server action in the Rust-only repo: {}",
                other
            );
            2
        }
    }
}
