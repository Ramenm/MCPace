use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) struct TempDir {
    path: PathBuf,
}

impl TempDir {
    pub(crate) fn new() -> Self {
        let unique = format!(
            "mcpace-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        );
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
        let _ = fs::remove_dir_all(&self.path);
    }
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

pub(crate) fn run(args: &[&str]) -> Output {
    Command::new(bin_path())
        .args(args)
        .output()
        .expect("run mcpace")
}

#[allow(dead_code)]
pub(crate) fn run_with_env(args: &[&str], env_key: &str, env_value: &Path) -> Output {
    Command::new(bin_path())
        .env(env_key, env_value)
        .args(args)
        .output()
        .expect("run mcpace with env")
}

#[allow(dead_code)]
pub(crate) fn run_with_envs(args: &[&str], envs: &[(&str, &Path)]) -> Output {
    let mut command = Command::new(bin_path());
    for (key, value) in envs {
        command.env(key, value);
    }
    command
        .args(args)
        .output()
        .expect("run mcpace with env overrides")
}

pub(crate) fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout utf8")
}

#[allow(dead_code)]
pub(crate) fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr utf8")
}
