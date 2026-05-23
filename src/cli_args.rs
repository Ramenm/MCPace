use clap::{Arg, ArgAction, Command};

/// Build a clap argv while preserving MCPace's historical single-dash long flags
/// such as `-json` and `-root` until the public CLI can drop them deliberately.
pub(crate) fn argv(
    command_name: &'static str,
    args: &[String],
    legacy_long_flags: &[&str],
) -> Vec<String> {
    let mut out = Vec::with_capacity(args.len() + 1);
    out.push(command_name.to_string());
    for arg in args {
        out.push(normalize_legacy_arg(arg, legacy_long_flags));
    }
    out
}

fn normalize_legacy_arg(arg: &str, legacy_long_flags: &[&str]) -> String {
    if arg == "-?" {
        return "--help".to_string();
    }
    if let Some(name) = arg.strip_prefix('-') {
        if !name.starts_with('-') && legacy_long_flags.contains(&name) {
            return format!("--{}", name);
        }
    }
    arg.to_string()
}

pub(crate) fn help_arg() -> Arg {
    Arg::new("help")
        .short('h')
        .long("help")
        .action(ArgAction::SetTrue)
}

pub(crate) fn json_arg() -> Arg {
    Arg::new("json").long("json").action(ArgAction::SetTrue)
}

pub(crate) fn dry_run_arg() -> Arg {
    Arg::new("dry-run")
        .long("dry-run")
        .action(ArgAction::SetTrue)
}

pub(crate) fn root_arg(help: &'static str) -> Arg {
    Arg::new("root")
        .long("root")
        .value_name("path")
        .help(help)
        .num_args(1)
}

pub(crate) fn value_arg(id: &'static str, long: &'static str, value_name: &'static str) -> Arg {
    Arg::new(id).long(long).value_name(value_name).num_args(1)
}

pub(crate) fn command(name: &'static str) -> Command {
    Command::new(name)
        .disable_help_flag(true)
        .disable_version_flag(true)
        .no_binary_name(false)
        .ignore_errors(false)
}

pub(crate) fn clap_error(error: clap::Error) -> String {
    error.to_string().trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::argv;

    #[test]
    fn normalizes_legacy_single_dash_long_flags() {
        let args = vec![
            "-json".to_string(),
            "-root".to_string(),
            "./tmp".to_string(),
            "-x".to_string(),
            "-?".to_string(),
        ];
        assert_eq!(
            argv("mcpace test", &args, &["json", "root"]),
            vec!["mcpace test", "--json", "--root", "./tmp", "-x", "--help"]
        );
    }
}
