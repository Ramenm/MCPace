use std::env;
use std::fs;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const DEFAULT_COMMAND_TIMEOUT_MS: u64 = 30_000;
const DROP_CLEANUP_TIMEOUT_MS: u64 = 5_000;

pub(crate) struct TempDir {
    path: PathBuf,
}

impl TempDir {
    pub(crate) fn new() -> Self {
        let unique = format!("mcpace-test-{}-{}", std::process::id(), now_nanos());
        let path = env::temp_dir().join(unique);
        fs::create_dir_all(&path).expect("create temp dir");
        TempDir { path }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        stop_serve_runtime_if_present(&self.path);
        stop_hub_runtime_if_present(&self.path);
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn now_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos()
}

fn command_timeout() -> Duration {
    let millis = env::var("MCPACE_TEST_COMMAND_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_COMMAND_TIMEOUT_MS);
    Duration::from_millis(millis)
}

pub(crate) fn bin_path() -> PathBuf {
    if let Ok(value) = env::var("CARGO_BIN_EXE_mcpace") {
        return PathBuf::from(value);
    }

    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("target");
    path.push("debug");
    path.push(if cfg!(windows) {
        "mcpace.exe"
    } else {
        "mcpace"
    });
    path
}

#[allow(dead_code)]
pub(crate) fn run(args: &[&str]) -> Output {
    run_with_env_pairs(args, &[])
}

#[allow(dead_code)]
pub(crate) fn run_with_env(args: &[&str], env_key: &str, env_value: &Path) -> Output {
    run_with_env_pairs(args, &[(env_key, env_value)])
}

#[allow(dead_code)]
pub(crate) fn run_with_envs(args: &[&str], envs: &[(&str, &Path)]) -> Output {
    run_with_env_pairs(args, envs)
}

fn run_with_env_pairs(args: &[&str], envs: &[(&str, &Path)]) -> Output {
    let stdout_path = temp_output_path("stdout");
    let stderr_path = temp_output_path("stderr");
    let stdout_file = File::create(&stdout_path).expect("create command stdout capture");
    let stderr_file = File::create(&stderr_path).expect("create command stderr capture");

    let mut command = Command::new(bin_path());
    for (key, value) in envs {
        command.env(key, value);
    }
    let mut child = command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file))
        .spawn()
        .expect("spawn mcpace");

    let status = match wait_for_child(&mut child, command_timeout()) {
        Some(status) => status,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            let stdout = read_capture(&stdout_path);
            let stderr = read_capture(&stderr_path);
            cleanup_capture(&stdout_path, &stderr_path);
            panic!(
                "mcpace command timed out after {:?}: {:?}\nstdout:\n{}\nstderr:\n{}",
                command_timeout(),
                args,
                String::from_utf8_lossy(&stdout),
                String::from_utf8_lossy(&stderr)
            );
        }
    };

    let stdout = read_capture(&stdout_path);
    let stderr = read_capture(&stderr_path);
    cleanup_capture(&stdout_path, &stderr_path);
    Output {
        status,
        stdout,
        stderr,
    }
}

fn wait_for_child(
    child: &mut std::process::Child,
    timeout: Duration,
) -> Option<std::process::ExitStatus> {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) => {
                if Instant::now() >= deadline {
                    return None;
                }
                thread::sleep(Duration::from_millis(25));
            }
            Err(error) => panic!("wait for mcpace child: {}", error),
        }
    }
}

fn temp_output_path(kind: &str) -> PathBuf {
    env::temp_dir().join(format!(
        "mcpace-output-{}-{}-{}.log",
        std::process::id(),
        now_nanos(),
        kind
    ))
}

fn read_capture(path: &Path) -> Vec<u8> {
    fs::read(path).unwrap_or_default()
}

fn cleanup_capture(stdout_path: &Path, stderr_path: &Path) {
    let _ = fs::remove_file(stdout_path);
    let _ = fs::remove_file(stderr_path);
}

fn stop_hub_runtime_if_present(root: &Path) {
    let hub_dir = root.join("data").join("runtime").join("hub");
    if !hub_dir.exists() {
        return;
    }
    let Some(root_text) = root.to_str() else {
        return;
    };
    let binary = bin_path();
    if !binary.is_file() {
        return;
    }

    let mut child = match Command::new(binary)
        .args(["hub", "down", "--json", "--root", root_text])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return,
    };

    if wait_for_child(&mut child, Duration::from_millis(DROP_CLEANUP_TIMEOUT_MS)).is_none() {
        let _ = child.kill();
        let _ = child.wait();
    }
}

fn stop_serve_runtime_if_present(root: &Path) {
    let serve_dir = root.join("data").join("runtime").join("serve");
    if !serve_dir.exists() {
        return;
    }
    let Some(root_text) = root.to_str() else {
        return;
    };
    let binary = bin_path();
    if !binary.is_file() {
        return;
    }

    let mut child = match Command::new(binary)
        .args(["serve", "stop", "--json", "--root", root_text])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return,
    };

    if wait_for_child(&mut child, Duration::from_millis(DROP_CLEANUP_TIMEOUT_MS)).is_none() {
        let _ = child.kill();
        let _ = child.wait();
    }
}

pub(crate) fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout utf8")
}

#[allow(dead_code)]
pub(crate) fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr utf8")
}
