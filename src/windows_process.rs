#[cfg(windows)]
use std::ffi::{OsStr, OsString};
#[cfg(windows)]
use std::fmt;
#[cfg(windows)]
use std::mem::{size_of, zeroed};
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
use std::path::Path;
#[cfg(windows)]
use std::ptr::null_mut;

#[cfg(windows)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WindowsProcessError {
    message: String,
}

#[cfg(windows)]
impl WindowsProcessError {
    fn new(message: impl Into<String>) -> Self {
        WindowsProcessError {
            message: message.into(),
        }
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn contains(&self, needle: &str) -> bool {
        self.message.contains(needle)
    }
}

#[cfg(windows)]
impl fmt::Display for WindowsProcessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

#[cfg(windows)]
impl std::error::Error for WindowsProcessError {}

#[cfg(windows)]
impl From<WindowsProcessError> for String {
    fn from(error: WindowsProcessError) -> Self {
        error.to_string()
    }
}

#[cfg(windows)]
pub(crate) fn spawn_detached_no_window(
    program: &Path,
    args: &[OsString],
    current_dir: Option<&Path>,
) -> Result<u32, WindowsProcessError> {
    use std::ffi::c_void;

    const CREATE_NO_WINDOW: u32 = 0x08000000;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
    const DETACHED_PROCESS: u32 = 0x00000008;
    const STARTF_USESHOWWINDOW: u32 = 0x00000001;
    const SW_HIDE: u16 = 0;

    #[repr(C)]
    struct StartupInfoW {
        cb: u32,
        lp_reserved: *mut u16,
        lp_desktop: *mut u16,
        lp_title: *mut u16,
        dw_x: u32,
        dw_y: u32,
        dw_x_size: u32,
        dw_y_size: u32,
        dw_fill_attribute: u32,
        dw_flags: u32,
        w_show_window: u16,
        cb_reserved2: u16,
        lp_reserved2: *mut u8,
        h_std_input: *mut c_void,
        h_std_output: *mut c_void,
        h_std_error: *mut c_void,
    }

    #[repr(C)]
    struct ProcessInformation {
        h_process: *mut c_void,
        h_thread: *mut c_void,
        dw_process_id: u32,
        dw_thread_id: u32,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateProcessW(
            lp_application_name: *const u16,
            lp_command_line: *mut u16,
            lp_process_attributes: *mut c_void,
            lp_thread_attributes: *mut c_void,
            b_inherit_handles: i32,
            dw_creation_flags: u32,
            lp_environment: *mut c_void,
            lp_current_directory: *const u16,
            lp_startup_info: *mut StartupInfoW,
            lp_process_information: *mut ProcessInformation,
        ) -> i32;
        fn CloseHandle(h_object: *mut c_void) -> i32;
    }

    let command_line = windows_command_line(
        std::iter::once(program.as_os_str()).chain(args.iter().map(OsString::as_os_str)),
    );
    let mut command_wide = wide_null_os(OsStr::new(&command_line));
    let application_wide = wide_null_os(program.as_os_str());
    let current_dir_wide = current_dir.map(|path| wide_null_os(path.as_os_str()));
    let current_dir_ptr = current_dir_wide
        .as_ref()
        .map(|value| value.as_ptr())
        .unwrap_or(std::ptr::null());
    let mut startup: StartupInfoW = unsafe { zeroed() };
    startup.cb = size_of::<StartupInfoW>() as u32;
    startup.dw_flags = STARTF_USESHOWWINDOW;
    startup.w_show_window = SW_HIDE;
    let mut process_info: ProcessInformation = unsafe { zeroed() };

    let created = unsafe {
        CreateProcessW(
            application_wide.as_ptr(),
            command_wide.as_mut_ptr(),
            null_mut(),
            null_mut(),
            0,
            CREATE_NO_WINDOW | DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP,
            null_mut(),
            current_dir_ptr,
            &mut startup,
            &mut process_info,
        )
    };
    if created == 0 {
        return Err(WindowsProcessError::new(format!(
            "failed to start hidden Windows process '{}': {}",
            program.display(),
            std::io::Error::last_os_error()
        )));
    }

    unsafe {
        let _ = CloseHandle(process_info.h_thread);
        let _ = CloseHandle(process_info.h_process);
    }
    Ok(process_info.dw_process_id)
}

#[cfg(windows)]
pub(crate) fn configure_no_window(command: &mut std::process::Command) {
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(windows)]
pub(crate) fn enable_kill_on_exit_job() -> Result<(), WindowsProcessError> {
    use std::ffi::c_void;
    use std::sync::OnceLock;

    const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION_CLASS: i32 = 9;
    const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x0000_2000;

    #[repr(C)]
    #[derive(Default)]
    struct BasicLimitInformation {
        per_process_user_time_limit: i64,
        per_job_user_time_limit: i64,
        limit_flags: u32,
        minimum_working_set_size: usize,
        maximum_working_set_size: usize,
        active_process_limit: u32,
        affinity: usize,
        priority_class: u32,
        scheduling_class: u32,
    }

    #[repr(C)]
    #[derive(Default)]
    struct IoCounters {
        read_operation_count: u64,
        write_operation_count: u64,
        other_operation_count: u64,
        read_transfer_count: u64,
        write_transfer_count: u64,
        other_transfer_count: u64,
    }

    #[repr(C)]
    #[derive(Default)]
    struct ExtendedLimitInformation {
        basic_limit_information: BasicLimitInformation,
        io_info: IoCounters,
        process_memory_limit: usize,
        job_memory_limit: usize,
        peak_process_memory_used: usize,
        peak_job_memory_used: usize,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateJobObjectW(job_attributes: *mut c_void, name: *const u16) -> *mut c_void;
        fn SetInformationJobObject(
            job: *mut c_void,
            information_class: i32,
            information: *mut c_void,
            information_length: u32,
        ) -> i32;
        fn AssignProcessToJobObject(job: *mut c_void, process: *mut c_void) -> i32;
        fn GetCurrentProcess() -> *mut c_void;
        fn CloseHandle(object: *mut c_void) -> i32;
    }

    static KILL_ON_EXIT_JOB: OnceLock<usize> = OnceLock::new();
    if KILL_ON_EXIT_JOB.get().is_some() {
        return Ok(());
    }

    let job = unsafe { CreateJobObjectW(null_mut(), std::ptr::null()) };
    if job.is_null() {
        return Err(WindowsProcessError::new(format!(
            "failed to create Windows kill-on-exit job: {}",
            std::io::Error::last_os_error()
        )));
    }

    let mut limits = ExtendedLimitInformation::default();
    limits.basic_limit_information.limit_flags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    let configured = unsafe {
        SetInformationJobObject(
            job,
            JOB_OBJECT_EXTENDED_LIMIT_INFORMATION_CLASS,
            (&mut limits as *mut ExtendedLimitInformation).cast(),
            size_of::<ExtendedLimitInformation>() as u32,
        )
    };
    if configured == 0 {
        let error = std::io::Error::last_os_error();
        unsafe {
            let _ = CloseHandle(job);
        }
        return Err(WindowsProcessError::new(format!(
            "failed to configure Windows kill-on-exit job: {}",
            error
        )));
    }

    let assigned = unsafe { AssignProcessToJobObject(job, GetCurrentProcess()) };
    if assigned == 0 {
        let error = std::io::Error::last_os_error();
        unsafe {
            let _ = CloseHandle(job);
        }
        return Err(WindowsProcessError::new(format!(
            "failed to assign MCPace to the Windows kill-on-exit job: {}",
            error
        )));
    }

    if KILL_ON_EXIT_JOB.set(job as usize).is_err() {
        unsafe {
            let _ = CloseHandle(job);
        }
    }
    Ok(())
}

#[cfg(windows)]
fn wide_null_os(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(Some(0)).collect()
}

#[cfg(windows)]
fn windows_command_line<'a, I>(args: I) -> String
where
    I: IntoIterator<Item = &'a OsStr>,
{
    args.into_iter()
        .map(|arg| quote_windows_arg(&arg.to_string_lossy()))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(windows)]
#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn windows_command_line_from_strs<'a, I>(args: I) -> String
where
    I: IntoIterator<Item = &'a str>,
{
    args.into_iter()
        .map(quote_windows_arg)
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(windows)]
pub(crate) fn quote_windows_arg(arg: &str) -> String {
    if !arg.is_empty()
        && !arg
            .chars()
            .any(|ch| ch.is_whitespace() || ch == '"' || ch == '\\')
    {
        return arg.to_string();
    }

    let mut quoted = String::from("\"");
    let mut backslashes = 0usize;
    for ch in arg.chars() {
        match ch {
            '\\' => backslashes += 1,
            '"' => {
                quoted.push_str(&"\\".repeat(backslashes * 2 + 1));
                quoted.push('"');
                backslashes = 0;
            }
            _ => {
                quoted.push_str(&"\\".repeat(backslashes));
                quoted.push(ch);
                backslashes = 0;
            }
        }
    }
    quoted.push_str(&"\\".repeat(backslashes * 2));
    quoted.push('"');
    quoted
}

#[cfg(test)]
mod tests;
