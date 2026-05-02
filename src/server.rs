mod add;
mod args;
mod import;
mod loader;
mod model;
mod preset_render;
mod presets;
mod query;
mod remove;
mod render;
mod sources;
mod test;
mod toggle;

use self::args::{parse_args, write_help};
pub use self::loader::load_server_records;
pub use self::model::ServerRecord;
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
    if action == "sources" {
        return sources::run(&parsed, default_root, stdout, stderr);
    }
    if action == "add" {
        return add::run(&parsed, default_root, stdout, stderr);
    }
    if action == "presets" {
        return presets::run(&parsed, default_root, stdout, stderr);
    }
    if action == "install" {
        return presets::install(&parsed, default_root, stdout, stderr);
    }
    if action == "starter" {
        return presets::starter(&parsed, default_root, stdout, stderr);
    }
    if action == "remove" {
        return remove::run(&parsed, default_root, stdout, stderr);
    }
    if action == "import" {
        return import::run(&parsed, default_root, stdout, stderr);
    }
    if action == "enable" || action == "disable" {
        return toggle::run(&parsed, default_root, stdout, stderr);
    }
    if action == "test" {
        return test::run(&parsed, default_root, stdout, stderr);
    }

    query::run(&action, &parsed, default_root, stdout, stderr)
}
