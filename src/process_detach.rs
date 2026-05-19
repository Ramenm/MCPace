//! Small, reviewed process-detach helpers.
//!
//! Background runtime launches need platform FFI in a few places. Keep that
//! unsafe surface centralized here (and in `windows_process.rs`) so command
//! modules can stay focused on orchestration and tests can audit the allowed
//! unsafe boundary.

#[cfg(unix)]
pub(crate) fn configure_unix_new_session(command: &mut std::process::Command) {
    use std::os::unix::process::CommandExt;

    extern "C" {
        fn setsid() -> i32;
    }

    // SAFETY: `pre_exec` is required for the POSIX-safe child-side `setsid`
    // call before `exec`. The closure does not capture locks, allocate, or
    // touch shared Rust state; it only calls `setsid` and converts errno into
    // an `io::Error` when the syscall fails.
    unsafe {
        command.pre_exec(|| {
            // SAFETY: `setsid` has no Rust-side aliasing or lifetime
            // invariants. The return value is checked and errno is reported
            // through `last_os_error` on failure.
            if setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

#[cfg(unix)]
pub(crate) fn configure_unix_process_group(command: &mut std::process::Command) {
    use std::os::unix::process::CommandExt;

    command.process_group(0);
}

#[cfg(unix)]
pub(crate) fn kill_unix_process_group(pid: u32, signal: i32) {
    unsafe extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }

    let Ok(pid) = i32::try_from(pid) else {
        return;
    };
    if pid <= 1 {
        return;
    }

    // SAFETY: This is the reviewed Unix process-boundary wrapper for sending a
    // signal to the child process group created with `process_group(0)`. The
    // pid is range-checked, pid 1 is guarded, and the negative pid intentionally
    // addresses the process group rather than an unrelated single process.
    let _ = unsafe { kill(-pid, signal) };
}
