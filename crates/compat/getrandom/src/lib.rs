use std::fmt;

#[derive(Debug, Clone)]
pub struct Error(String);
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Error {}

impl Error {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

pub fn getrandom(bytes: &mut [u8]) -> Result<(), Error> {
    if bytes.is_empty() {
        return Ok(());
    }
    fill_os_random(bytes)
}

#[cfg(windows)]
fn fill_os_random(bytes: &mut [u8]) -> Result<(), Error> {
    use std::ffi::c_void;
    use std::sync::Mutex;

    #[link(name = "bcrypt")]
    extern "system" {
        fn BCryptGenRandom(
            h_algorithm: *mut c_void,
            pb_buffer: *mut u8,
            cb_buffer: u32,
            dw_flags: u32,
        ) -> i32;
    }

    const BCRYPT_USE_SYSTEM_PREFERRED_RNG: u32 = 0x0000_0002;
    const STATUS_SUCCESS: i32 = 0;
    static WINDOWS_RANDOM_LOCK: Mutex<()> = Mutex::new(());

    let _guard = WINDOWS_RANDOM_LOCK
        .lock()
        .map_err(|_| Error::new("Windows random generator lock is poisoned"))?;
    for chunk in bytes.chunks_mut(u32::MAX as usize) {
        let status = unsafe {
            BCryptGenRandom(
                std::ptr::null_mut(),
                chunk.as_mut_ptr(),
                chunk.len() as u32,
                BCRYPT_USE_SYSTEM_PREFERRED_RNG,
            )
        };
        if status != STATUS_SUCCESS {
            return Err(Error::new(format!(
                "BCryptGenRandom failed with NTSTATUS 0x{status:08x}"
            )));
        }
    }
    Ok(())
}

#[cfg(not(windows))]
fn fill_os_random(bytes: &mut [u8]) -> Result<(), Error> {
    use std::fs::File;
    use std::io::Read;

    let mut file = File::open("/dev/urandom")
        .map_err(|error| Error::new(format!("open /dev/urandom: {error}")))?;
    file.read_exact(bytes)
        .map_err(|error| Error::new(format!("read /dev/urandom: {error}")))
}
