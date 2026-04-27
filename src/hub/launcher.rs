use std::path::Path;

#[cfg(windows)]
use std::ffi::OsString;

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

    extern "C" {
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
    let args = vec![
        OsString::from("hub"),
        OsString::from("run"),
        OsString::from("--root"),
        root_path.as_os_str().to_os_string(),
    ];
    crate::windows_process::spawn_detached_no_window(exe, &args, Some(root_path))
        .map(|_| ())
        .map_err(|error| format!("failed to start hub runtime: {}", error))
}
