#[cfg(windows)]
use std::ffi::{OsStr, OsString};
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
pub(crate) fn spawn_detached_no_window(
    program: &Path,
    args: &[OsString],
    current_dir: Option<&Path>,
) -> Result<u32, String> {
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
        return Err(format!(
            "failed to start hidden Windows process '{}': {}",
            program.display(),
            std::io::Error::last_os_error()
        ));
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
