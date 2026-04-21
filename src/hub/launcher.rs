use std::path::Path;

#[cfg(windows)]
use std::path::PathBuf;

pub(super) fn spawn_background(exe: &Path, root_path: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        return windows_spawn_background(exe, root_path);
    }

    #[cfg(unix)]
    {
        return unix_spawn_background(exe, root_path);
    }

    #[allow(unreachable_code)]
    Err("background hub launch is not implemented for this platform".to_string())
}

#[cfg(unix)]
fn unix_spawn_background(exe: &Path, root_path: &Path) -> Result<(), String> {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};

    unsafe extern "C" {
        fn setsid() -> i32;
    }

    let mut command = Command::new(exe);
    command
        .arg("hub")
        .arg("run")
        .arg("--root")
        .arg(root_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    unsafe {
        command.pre_exec(|| {
            if setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }

    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("failed to start hub runtime: {}", error))
}

#[cfg(windows)]
fn windows_spawn_background(exe: &Path, root_path: &Path) -> Result<(), String> {
    use std::process::Command;

    let powershell = resolve_powershell().unwrap_or_else(|| PathBuf::from("powershell.exe"));
    let script = format!(
        "Start-Process -FilePath '{}' -ArgumentList @('hub','run','--root','{}') -WindowStyle Hidden",
        powershell_quote(exe),
        powershell_quote(root_path)
    );

    Command::new(powershell)
        .arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-WindowStyle")
        .arg("Hidden")
        .arg("-Command")
        .arg(script)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("failed to start hub runtime: {}", error))
}

#[cfg(windows)]
fn resolve_powershell() -> Option<PathBuf> {
    std::env::var_os("SystemRoot").map(|root| {
        PathBuf::from(root)
            .join("System32")
            .join("WindowsPowerShell")
            .join("v1.0")
            .join("powershell.exe")
    })
}

#[cfg(windows)]
fn powershell_quote(path: &Path) -> String {
    path.display().to_string().replace('\'', "''")
}
