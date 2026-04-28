use serde_json::json;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

const RELEASE_SCRIPT: &str = "scripts/build-release-artifacts.mjs";
const LOCAL_ONLY_CLAIM: &str =
    "local release artifact build only; npm/GitHub publication is not performed";

pub fn run(
    args: &[String],
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let mut json_output = false;
    let mut root_override = default_root;
    let mut index = 0usize;

    if let Some(action) = args.first() {
        match action.as_str() {
            "build" | "artifacts" => index = 1,
            "-h" | "--help" | "help" => {
                write_help(stdout);
                return 0;
            }
            value if value.starts_with('-') => {}
            other => {
                let _ = writeln!(stderr, "unsupported release command: {}", other);
                write_usage(stderr);
                return 2;
            }
        }
    }

    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                json_output = true;
                index += 1;
            }
            "--root" => {
                let Some(value) = args.get(index + 1) else {
                    let _ = writeln!(stderr, "release build requires a path after --root");
                    return 2;
                };
                root_override = Some(PathBuf::from(value));
                index += 2;
            }
            "-h" | "--help" => {
                write_help(stdout);
                return 0;
            }
            other => {
                let _ = writeln!(stderr, "unsupported release build argument: {}", other);
                write_usage(stderr);
                return 2;
            }
        }
    }

    let Some(root_path) = root_override else {
        return write_failure(
            json_output,
            stdout,
            stderr,
            "failed to resolve MCPace project root for release build",
            None,
            "",
            "",
        );
    };

    if !root_path.join(RELEASE_SCRIPT).is_file() {
        return write_failure(
            json_output,
            stdout,
            stderr,
            &format!(
                "release artifact script '{}' was not found under {}",
                RELEASE_SCRIPT,
                root_path.display()
            ),
            None,
            "",
            "",
        );
    }

    let output = match Command::new("node")
        .arg(RELEASE_SCRIPT)
        .arg("--json")
        .current_dir(&root_path)
        .output()
    {
        Ok(output) => output,
        Err(error) => {
            return write_failure(
                json_output,
                stdout,
                stderr,
                &format!(
                    "failed to launch node for release artifact build: {}",
                    error
                ),
                None,
                "",
                "",
            )
        }
    };

    let script_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let script_stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        return write_failure(
            json_output,
            stdout,
            stderr,
            "release artifact build failed",
            output.status.code(),
            &script_stdout,
            &script_stderr,
        );
    }

    let artifact_build = serde_json::from_str::<serde_json::Value>(&script_stdout)
        .unwrap_or_else(|_| json!({ "rawStdout": script_stdout.trim() }));

    if json_output {
        let payload = json!({
            "command": "release build",
            "status": "pass",
            "rootPath": root_path.display().to_string(),
            "script": RELEASE_SCRIPT,
            "claim": LOCAL_ONLY_CLAIM,
            "artifactBuild": artifact_build,
        });
        write_json(stdout, &payload);
        return 0;
    }

    write_text_summary(stdout, &artifact_build);
    0
}

fn write_help(stdout: &mut dyn Write) {
    let _ = writeln!(
        stdout,
        "Usage: mcpace release [build] [--json] [--root <path>]"
    );
    let _ = writeln!(stdout);
    let _ = writeln!(
        stdout,
        "Build local release artifacts by running {} --json.",
        RELEASE_SCRIPT
    );
    let _ = writeln!(
        stdout,
        "This is a local/source proof surface only; it does not publish to npm or GitHub."
    );
}

fn write_usage(stderr: &mut dyn Write) {
    let _ = writeln!(
        stderr,
        "Usage: mcpace release [build] [--json] [--root <path>]"
    );
}

fn write_failure(
    json_output: bool,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
    message: &str,
    exit_code: Option<i32>,
    script_stdout: &str,
    script_stderr: &str,
) -> i32 {
    if json_output {
        let payload = json!({
            "command": "release build",
            "status": "failed",
            "claim": LOCAL_ONLY_CLAIM,
            "error": message,
            "scriptExitCode": exit_code,
            "scriptStdout": script_stdout.trim(),
            "scriptStderr": script_stderr.trim(),
        });
        write_json(stdout, &payload);
    } else {
        let _ = writeln!(stderr, "{}", message);
        if !script_stderr.trim().is_empty() {
            let _ = writeln!(stderr, "{}", script_stderr.trim());
        }
        if !script_stdout.trim().is_empty() {
            let _ = writeln!(stderr, "stdout:\n{}", script_stdout.trim());
        }
    }
    exit_code.filter(|code| *code != 0).unwrap_or(1)
}

fn write_json(stdout: &mut dyn Write, payload: &serde_json::Value) {
    let _ = writeln!(
        stdout,
        "{}",
        serde_json::to_string_pretty(payload).unwrap_or_else(|_| "{}".to_string())
    );
}

fn write_text_summary(stdout: &mut dyn Write, artifact_build: &serde_json::Value) {
    let archive_path = artifact_build
        .pointer("/archive/path")
        .and_then(|value| value.as_str())
        .unwrap_or("(archive path unavailable)");
    let manifest_path = artifact_build
        .get("manifestPath")
        .and_then(|value| value.as_str())
        .unwrap_or("(manifest path unavailable)");
    let release_proof_status = artifact_build
        .get("releaseProofStatus")
        .and_then(|value| value.as_str())
        .unwrap_or("unknown");
    let source_proof_status = artifact_build
        .pointer("/verificationReport/sourceProofStatus")
        .and_then(|value| value.as_str())
        .unwrap_or("unknown");

    let _ = writeln!(stdout, "Release artifacts built locally.");
    let _ = writeln!(stdout, "Archive: {}", archive_path);
    let _ = writeln!(stdout, "Manifest: {}", manifest_path);
    let _ = writeln!(stdout, "Source proof: {}", source_proof_status);
    let _ = writeln!(stdout, "Release proof: {}", release_proof_status);
    let _ = writeln!(stdout, "Claim: {}", LOCAL_ONLY_CLAIM);
}
