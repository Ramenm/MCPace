use std::io;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProcessIdentity {
    pub(crate) start_token: String,
    pub(crate) executable: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProcessMatch {
    Match,
    NotFound,
    Mismatch,
}

pub(crate) fn capture(pid: u32) -> io::Result<Option<ProcessIdentity>> {
    if pid == 0 {
        return Ok(None);
    }
    capture_platform(pid)
}

pub(crate) fn match_process(
    pid: u32,
    expected_start_token: Option<&str>,
    expected_executable: Option<&Path>,
) -> io::Result<ProcessMatch> {
    let Some(actual) = capture(pid)? else {
        return Ok(ProcessMatch::NotFound);
    };
    let expected_executable = expected_executable.filter(|path| !path.as_os_str().is_empty());
    if expected_start_token.is_none() && expected_executable.is_none() {
        return Ok(ProcessMatch::Mismatch);
    }

    if expected_start_token.is_some_and(|expected| expected != actual.start_token) {
        return Ok(ProcessMatch::Mismatch);
    }

    if let Some(expected) = expected_executable {
        let Some(actual_executable) = actual.executable.as_deref() else {
            return if expected_start_token.is_some() {
                Ok(ProcessMatch::Match)
            } else {
                Ok(ProcessMatch::Mismatch)
            };
        };
        if !paths_match(expected, actual_executable) {
            return Ok(ProcessMatch::Mismatch);
        }
    }

    Ok(ProcessMatch::Match)
}

fn paths_match(expected: &Path, actual: &Path) -> bool {
    let expected = std::fs::canonicalize(expected).unwrap_or_else(|_| expected.to_path_buf());
    let actual = std::fs::canonicalize(actual).unwrap_or_else(|_| actual.to_path_buf());
    if cfg!(windows) {
        expected
            .to_string_lossy()
            .eq_ignore_ascii_case(&actual.to_string_lossy())
    } else {
        expected == actual
    }
}

#[cfg(target_os = "linux")]
fn capture_platform(pid: u32) -> io::Result<Option<ProcessIdentity>> {
    let stat_path = PathBuf::from(format!("/proc/{pid}/stat"));
    let stat = match std::fs::read_to_string(&stat_path) {
        Ok(value) => value,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let close_paren = stat.rfind(')').ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{} has malformed process metadata", stat_path.display()),
        )
    })?;
    let fields = stat[close_paren + 1..]
        .split_whitespace()
        .collect::<Vec<_>>();
    let start_time = fields.get(19).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{} is missing process start time", stat_path.display()),
        )
    })?;
    let executable = match std::fs::read_link(format!("/proc/{pid}/exe")) {
        Ok(value) => Some(value),
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => None,
        Err(error) => return Err(error),
    };
    Ok(Some(ProcessIdentity {
        start_token: format!("linux:{start_time}"),
        executable,
    }))
}

#[cfg(target_os = "macos")]
fn capture_platform(pid: u32) -> io::Result<Option<ProcessIdentity>> {
    use std::ffi::c_void;
    use std::mem::{size_of, zeroed};

    const PROC_PIDTBSDINFO: i32 = 3;
    const ESRCH: i32 = 3;

    #[repr(C)]
    struct ProcBsdInfo {
        flags: u32,
        status: u32,
        xstatus: u32,
        pid: u32,
        ppid: u32,
        uid: u32,
        gid: u32,
        ruid: u32,
        rgid: u32,
        svuid: u32,
        svgid: u32,
        reserved: u32,
        command: [u8; 16],
        name: [u8; 32],
        nfiles: u32,
        process_group: u32,
        job_control_count: u32,
        controlling_tty: u32,
        tty_process_group: u32,
        nice: i32,
        start_seconds: u64,
        start_microseconds: u64,
    }

    #[link(name = "proc")]
    extern "C" {
        fn proc_pidinfo(
            pid: i32,
            flavor: i32,
            argument: u64,
            buffer: *mut c_void,
            buffer_size: i32,
        ) -> i32;
        fn proc_pidpath(pid: i32, buffer: *mut c_void, buffer_size: u32) -> i32;
    }

    let pid = i32::try_from(pid)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "pid exceeds i32"))?;
    let mut info: ProcBsdInfo = unsafe { zeroed() };
    let expected_size = i32::try_from(size_of::<ProcBsdInfo>()).unwrap_or(i32::MAX);
    let bytes = unsafe {
        proc_pidinfo(
            pid,
            PROC_PIDTBSDINFO,
            0,
            (&mut info as *mut ProcBsdInfo).cast(),
            expected_size,
        )
    };
    if bytes == 0 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(ESRCH) {
            return Ok(None);
        }
        return Err(error);
    }
    if bytes < expected_size {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("proc_pidinfo returned {bytes} bytes, expected {expected_size}"),
        ));
    }

    let mut buffer = vec![0_u8; 4096];
    let length = unsafe {
        proc_pidpath(
            pid,
            buffer.as_mut_ptr().cast(),
            u32::try_from(buffer.len()).unwrap_or(u32::MAX),
        )
    };
    let executable = if length > 0 {
        buffer.truncate(usize::try_from(length).unwrap_or(0));
        Some(PathBuf::from(String::from_utf8_lossy(&buffer).into_owned()))
    } else {
        None
    };
    Ok(Some(ProcessIdentity {
        start_token: format!(
            "macos:{}:{:06}",
            info.start_seconds, info.start_microseconds
        ),
        executable,
    }))
}

#[cfg(windows)]
fn capture_platform(pid: u32) -> io::Result<Option<ProcessIdentity>> {
    use std::ffi::c_void;
    use std::mem::zeroed;

    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const ERROR_INVALID_PARAMETER: i32 = 87;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct FileTime {
        low: u32,
        high: u32,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(access: u32, inherit: i32, process_id: u32) -> *mut c_void;
        fn CloseHandle(object: *mut c_void) -> i32;
        fn GetProcessTimes(
            process: *mut c_void,
            creation: *mut FileTime,
            exit: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> i32;
        fn QueryFullProcessImageNameW(
            process: *mut c_void,
            flags: u32,
            path: *mut u16,
            size: *mut u32,
        ) -> i32;
    }

    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(ERROR_INVALID_PARAMETER) {
            return Ok(None);
        }
        return Err(error);
    }

    let result = (|| {
        let mut creation: FileTime = unsafe { zeroed() };
        let mut exit: FileTime = unsafe { zeroed() };
        let mut kernel: FileTime = unsafe { zeroed() };
        let mut user: FileTime = unsafe { zeroed() };
        if unsafe { GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user) } == 0
        {
            return Err(io::Error::last_os_error());
        }

        let mut path_buffer = vec![0_u16; 32_768];
        let mut path_length = u32::try_from(path_buffer.len()).unwrap_or(u32::MAX);
        let executable = if unsafe {
            QueryFullProcessImageNameW(handle, 0, path_buffer.as_mut_ptr(), &mut path_length)
        } != 0
        {
            path_buffer.truncate(usize::try_from(path_length).unwrap_or(0));
            Some(PathBuf::from(String::from_utf16_lossy(&path_buffer)))
        } else {
            None
        };
        let creation_ticks = (u64::from(creation.high) << 32) | u64::from(creation.low);
        Ok(Some(ProcessIdentity {
            start_token: format!("windows:{creation_ticks:016x}"),
            executable,
        }))
    })();

    unsafe {
        let _ = CloseHandle(handle);
    }
    result
}

#[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
fn capture_platform(pid: u32) -> io::Result<Option<ProcessIdentity>> {
    let output = std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "lstart="])
        .env("LC_ALL", "C")
        .output()?;
    if !output.status.success() {
        return Ok(None);
    }
    let started = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if started.is_empty() {
        return Ok(None);
    }
    Ok(Some(ProcessIdentity {
        start_token: format!("unix:{started}"),
        executable: None,
    }))
}

#[cfg(test)]
mod tests;
