use crate::json::JsonValue;
use std::io::Write;

#[derive(Debug, Clone)]
struct ProcessFinding {
    pid: u32,
    command: String,
    reason: String,
}

impl ProcessFinding {
    fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("pid", JsonValue::number(self.pid)),
            ("command", JsonValue::string(self.command.clone())),
            ("reason", JsonValue::string(self.reason.clone())),
        ])
    }
}

#[derive(Debug, Clone)]
pub struct ProcessDoctorReport {
    status: String,
    platform: String,
    scanned_count: usize,
    direct_candidate_count: usize,
    direct_candidates: Vec<ProcessFinding>,
    warnings: Vec<String>,
}

impl ProcessDoctorReport {
    pub fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("schema", JsonValue::string("mcpace.processDoctor.v1")),
            ("status", JsonValue::string(self.status.clone())),
            ("platform", JsonValue::string(self.platform.clone())),
            ("scannedCount", JsonValue::number(self.scanned_count)),
            (
                "directCandidateCount",
                JsonValue::number(self.direct_candidate_count),
            ),
            (
                "directCandidates",
                JsonValue::array(
                    self.direct_candidates
                        .iter()
                        .map(ProcessFinding::to_json_value),
                ),
            ),
            (
                "warnings",
                JsonValue::array(self.warnings.iter().cloned().map(JsonValue::string)),
            ),
        ])
    }
}

pub fn collect() -> ProcessDoctorReport {
    collect_platform()
}

#[cfg(target_os = "linux")]
fn collect_platform() -> ProcessDoctorReport {
    use std::fs;

    let mut scanned_count = 0usize;
    let mut direct_candidates = Vec::new();
    let entries = match fs::read_dir("/proc") {
        Ok(value) => value,
        Err(error) => {
            return ProcessDoctorReport {
                status: "unsupported".to_string(),
                platform: std::env::consts::OS.to_string(),
                scanned_count: 0,
                direct_candidate_count: 0,
                direct_candidates: Vec::new(),
                warnings: vec![format!("failed to read /proc: {error}")],
            }
        }
    };

    for entry in entries.flatten() {
        let file_name = entry.file_name().to_string_lossy().to_string();
        let Ok(pid) = file_name.parse::<u32>() else {
            continue;
        };
        let cmdline_path = entry.path().join("cmdline");
        let Ok(raw) = fs::read(&cmdline_path) else {
            continue;
        };
        if raw.is_empty() {
            continue;
        }
        scanned_count += 1;
        let args = raw
            .split(|byte| *byte == 0)
            .filter(|part| !part.is_empty())
            .map(|part| String::from_utf8_lossy(part).to_string())
            .collect::<Vec<_>>();
        let command = args.join(" ");
        if let Some(reason) = direct_mcp_process_reason(&args, &command) {
            direct_candidates.push(ProcessFinding {
                pid,
                command,
                reason,
            });
        }
    }
    direct_candidates.sort_by_key(|finding| finding.pid);
    let direct_candidate_count = direct_candidates.len();
    let mut warnings = Vec::new();
    if direct_candidate_count > 0 {
        warnings.push(
            "Potential direct MCP subprocesses are running outside MCPace; run `mcpace doctor clients`, repair configs, then fully restart the client.".to_string(),
        );
    }
    ProcessDoctorReport {
        status: if direct_candidate_count > 0 {
            "direct-processes-detected".to_string()
        } else {
            "ok".to_string()
        },
        platform: std::env::consts::OS.to_string(),
        scanned_count,
        direct_candidate_count,
        direct_candidates,
        warnings,
    }
}

#[cfg(not(target_os = "linux"))]
fn collect_platform() -> ProcessDoctorReport {
    ProcessDoctorReport {
        status: "unsupported".to_string(),
        platform: std::env::consts::OS.to_string(),
        scanned_count: 0,
        direct_candidate_count: 0,
        direct_candidates: Vec::new(),
        warnings: vec![
            "process doctor currently scans Linux /proc; use OS-native task manager commands on this platform until native support is added".to_string(),
        ],
    }
}

#[cfg(target_os = "linux")]
fn direct_mcp_process_reason(args: &[String], command: &str) -> Option<String> {
    let lower = command.to_ascii_lowercase();
    if lower.contains("mcpace") {
        return None;
    }
    let program = args
        .first()
        .and_then(|value| value.rsplit(['/', '\\']).next())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let program_is_launcher = matches!(
        program.as_str(),
        "npx" | "uvx" | "node" | "python" | "python3" | "docker" | "bun" | "deno"
    );
    if lower.contains("modelcontextprotocol") {
        return Some("command references modelcontextprotocol".to_string());
    }
    if program_is_launcher
        && (lower.contains(" mcp")
            || lower.contains("/mcp")
            || lower.contains("-mcp")
            || lower.contains("_mcp"))
    {
        return Some(format!(
            "{} launcher command appears to reference MCP",
            program
        ));
    }
    None
}

pub fn write_text(report: &ProcessDoctorReport, stdout: &mut dyn Write) {
    let _ = writeln!(stdout, "Process doctor: {}", report.status);
    let _ = writeln!(
        stdout,
        "Scanned {} process(es); {} direct MCP candidate(s).",
        report.scanned_count, report.direct_candidate_count
    );
    for finding in &report.direct_candidates {
        let _ = writeln!(
            stdout,
            "- pid={} reason={} command={}",
            finding.pid, finding.reason, finding.command
        );
    }
    for warning in &report.warnings {
        let _ = writeln!(stdout, "warning: {}", warning);
    }
}

pub fn run(json_output: bool, stdout: &mut dyn Write) -> i32 {
    let report = collect();
    if json_output {
        let _ = writeln!(stdout, "{}", report.to_json_value().to_pretty_string());
    } else {
        write_text(&report, stdout);
    }
    if report.direct_candidate_count > 0 {
        1
    } else {
        0
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::direct_mcp_process_reason;

    #[test]
    fn detects_direct_modelcontextprotocol_processes_but_not_mcpace() {
        assert!(direct_mcp_process_reason(
            &[
                "npx".to_string(),
                "@modelcontextprotocol/server-filesystem".to_string()
            ],
            "npx @modelcontextprotocol/server-filesystem"
        )
        .is_some());
        assert!(direct_mcp_process_reason(&["mcpace".to_string()], "mcpace hub run").is_none());
    }
}
