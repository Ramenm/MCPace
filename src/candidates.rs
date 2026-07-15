use crate::diagnostics;
use crate::json::JsonValue;
use crate::json_helpers;
use clap::{error::ErrorKind, Parser};
use std::ffi::OsString;
use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Default)]
struct ParsedArgs {
    json_output: bool,
    help: bool,
    root_override: Option<PathBuf>,
    error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CandidateSummary {
    pub name: String,
    pub status: String,
    pub priority: String,
    pub upstream_type: String,
    pub integration_source: String,
    pub scope_class: String,
    pub concurrency_policy: String,
    pub state_binding: String,
    pub credential_binding: String,
    pub why: String,
    pub evaluation_notes: String,
}

#[derive(Debug)]
pub enum CandidateCatalogError {
    MissingCatalog { path: PathBuf },
    ReadCatalog { path: PathBuf, source: String },
    InvalidShape { path: PathBuf },
}

impl fmt::Display for CandidateCatalogError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CandidateCatalogError::MissingCatalog { path } => {
                write!(formatter, "missing candidate catalog: {}", path.display())
            }
            CandidateCatalogError::ReadCatalog { path, source } => {
                write!(
                    formatter,
                    "failed to read candidate catalog {}: {}",
                    path.display(),
                    source
                )
            }
            CandidateCatalogError::InvalidShape { path } => {
                write!(
                    formatter,
                    "candidate catalog must be a JSON array: {}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for CandidateCatalogError {}

pub fn run(
    args: &[String],
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let parsed = parse_cli(args);
    if let Some(error) = parsed.error {
        diagnostics::stderr_line(stderr, format_args!("{}", error));
        return 2;
    }

    if parsed.help {
        write_help(stdout);
        return 0;
    }

    let root_path = parsed.root_override.or(default_root);
    let Some(root_path) = root_path else {
        diagnostics::stderr_line(
            stderr,
            format_args!("mcpace root not found; expected server-candidates.json"),
        );
        return 1;
    };

    let catalog_path = root_path.join("server-candidates.json");
    let candidates = match read_candidates(&catalog_path) {
        Ok(items) => items,
        Err(error) => {
            diagnostics::stderr_line(stderr, format_args!("{}", error));
            return 1;
        }
    };

    if parsed.json_output {
        let json = JsonValue::array(candidates.iter().map(CandidateSummary::to_json_value));
        let _ = writeln!(stdout, "{}", json.to_pretty_string());
        0
    } else {
        let _ = writeln!(stdout, "Candidate servers: {}", candidates.len());
        for candidate in &candidates {
            let _ = writeln!(stdout, "- {}", candidate.name);
            let _ = writeln!(
                stdout,
                "    priority={}; upstream={}; scope={}; concurrency={}",
                candidate.priority,
                candidate.upstream_type,
                candidate.scope_class,
                candidate.concurrency_policy
            );
            let _ = writeln!(
                stdout,
                "    credential={}; state={}",
                candidate.credential_binding, candidate.state_binding
            );
            let _ = writeln!(stdout, "    source: {}", candidate.integration_source);
            let _ = writeln!(stdout, "    why: {}", candidate.why);
        }
        0
    }
}

pub fn read_candidates(path: &Path) -> Result<Vec<CandidateSummary>, CandidateCatalogError> {
    if !path.is_file() {
        return Err(CandidateCatalogError::MissingCatalog {
            path: path.to_path_buf(),
        });
    }

    let json = json_helpers::read_json_file(path).map_err(|source| {
        CandidateCatalogError::ReadCatalog {
            path: path.to_path_buf(),
            source: source.to_string(),
        }
    })?;
    let Some(items) = json.as_array() else {
        return Err(CandidateCatalogError::InvalidShape {
            path: path.to_path_buf(),
        });
    };

    let mut candidates = items
        .iter()
        .filter_map(normalize_candidate)
        .collect::<Vec<_>>();

    candidates.sort_by(|left, right| {
        left.priority
            .to_ascii_lowercase()
            .cmp(&right.priority.to_ascii_lowercase())
            .then_with(|| {
                left.name
                    .to_ascii_lowercase()
                    .cmp(&right.name.to_ascii_lowercase())
            })
    });

    Ok(candidates)
}

fn write_help(stdout: &mut dyn Write) {
    let _ = writeln!(stdout, "Usage: mcpace candidates [--json] [--root <path>]");
}

#[derive(Debug, Parser)]
#[command(
    name = "mcpace candidates",
    disable_version_flag = true,
    about = "Inspect candidate MCP servers"
)]
struct CandidatesCli {
    /// Emit machine-readable JSON.
    #[arg(long = "json", short = 'j')]
    json_output: bool,

    /// MCPace project/root directory.
    #[arg(long = "root", value_name = "PATH")]
    root_override: Option<PathBuf>,
}

fn parse_cli(args: &[String]) -> ParsedArgs {
    match CandidatesCli::try_parse_from(candidates_argv(args)) {
        Ok(cli) => ParsedArgs {
            json_output: cli.json_output,
            help: false,
            root_override: cli.root_override,
            error: None,
        },
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) =>
        {
            ParsedArgs {
                help: true,
                ..ParsedArgs::default()
            }
        }
        Err(error) => ParsedArgs {
            error: Some(error.to_string()),
            ..ParsedArgs::default()
        },
    }
}

fn candidates_argv(args: &[String]) -> Vec<OsString> {
    let mut argv = Vec::with_capacity(args.len() + 1);
    argv.push(OsString::from("mcpace candidates"));
    argv.extend(
        args.iter()
            .map(|arg| OsString::from(normalize_candidates_flag(arg))),
    );
    argv
}

fn normalize_candidates_flag(arg: &str) -> &str {
    match arg {
        "-json" => "--json",
        "-root" => "--root",
        "-?" => "--help",
        other => other,
    }
}

fn normalize_candidate(value: &JsonValue) -> Option<CandidateSummary> {
    let name = value.get("name")?.as_str()?.trim();
    if name.is_empty() {
        return None;
    }
    Some(CandidateSummary {
        name: name.to_string(),
        status: string_field(value, "status"),
        priority: string_field(value, "priority"),
        upstream_type: string_field(value, "upstreamType"),
        integration_source: string_field(value, "integrationSource"),
        scope_class: string_field(value, "scopeClass"),
        concurrency_policy: string_field(value, "concurrencyPolicy"),
        state_binding: string_field(value, "stateBinding"),
        credential_binding: string_field(value, "credentialBinding"),
        why: string_field(value, "why"),
        evaluation_notes: string_field(value, "evaluationNotes"),
    })
}

fn string_field(value: &JsonValue, key: &str) -> String {
    value
        .get(key)
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .unwrap_or("")
        .to_string()
}

impl CandidateSummary {
    pub fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("name", JsonValue::string(self.name.clone())),
            ("status", JsonValue::string(self.status.clone())),
            ("priority", JsonValue::string(self.priority.clone())),
            (
                "upstreamType",
                JsonValue::string(self.upstream_type.clone()),
            ),
            (
                "integrationSource",
                JsonValue::string(self.integration_source.clone()),
            ),
            ("scopeClass", JsonValue::string(self.scope_class.clone())),
            (
                "concurrencyPolicy",
                JsonValue::string(self.concurrency_policy.clone()),
            ),
            (
                "stateBinding",
                JsonValue::string(self.state_binding.clone()),
            ),
            (
                "credentialBinding",
                JsonValue::string(self.credential_binding.clone()),
            ),
            ("why", JsonValue::string(self.why.clone())),
            (
                "evaluationNotes",
                JsonValue::string(self.evaluation_notes.clone()),
            ),
        ])
    }
}
