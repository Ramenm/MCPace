use std::fmt;
use std::path::Path;

#[cfg(windows)]
use std::ffi::OsString;

#[derive(Debug)]
pub(super) enum HubLaunchError {
    UnsupportedPlatform,
    Spawn { reason: String },
}

impl fmt::Display for HubLaunchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HubLaunchError::UnsupportedPlatform => {
                formatter.write_str("background hub launch is not implemented for this platform")
            }
            HubLaunchError::Spawn { reason } => {
                write!(formatter, "failed to start hub runtime: {}", reason)
            }
        }
    }
}

impl std::error::Error for HubLaunchError {}

impl From<HubLaunchError> for String {
    fn from(error: HubLaunchError) -> Self {
        error.to_string()
    }
}

type HubLaunchResult<T> = Result<T, HubLaunchError>;

pub(super) fn spawn_background(exe: &Path, root_path: &Path) -> HubLaunchResult<()> {
    #[cfg(windows)]
    {
        return windows_spawn_background(exe, root_path);
    }

    #[cfg(unix)]
    {
        return unix_spawn_background(exe, root_path);
    }

    #[allow(unreachable_code)]
    Err(HubLaunchError::UnsupportedPlatform)
}

#[cfg(unix)]
fn unix_spawn_background(exe: &Path, root_path: &Path) -> HubLaunchResult<()> {
    use std::process::{Command, Stdio};

    let mut command = Command::new(exe);
    command
        .arg("hub")
        .arg("run")
        .arg("--root")
        .arg(root_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    crate::process_detach::configure_unix_new_session(&mut command);

    command
        .spawn()
        .map(|_| ())
        .map_err(|error| HubLaunchError::Spawn {
            reason: error.to_string(),
        })
}

#[cfg(windows)]
fn windows_spawn_background(exe: &Path, root_path: &Path) -> HubLaunchResult<()> {
    let args = vec![
        OsString::from("hub"),
        OsString::from("run"),
        OsString::from("--root"),
        root_path.as_os_str().to_os_string(),
    ];
    crate::windows_process::spawn_detached_no_window(exe, &args, Some(root_path))
        .map(|_| ())
        .map_err(|error| HubLaunchError::Spawn {
            reason: error.to_string(),
        })
}
