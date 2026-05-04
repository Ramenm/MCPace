use std::fmt;
use std::fs::File;
use std::io::Read;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct Error(String);
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Error {}

pub fn getrandom(bytes: &mut [u8]) -> Result<(), Error> {
    if bytes.is_empty() {
        return Ok(());
    }
    if let Ok(mut file) = File::open("/dev/urandom") {
        if file.read_exact(bytes).is_ok() {
            return Ok(());
        }
    }
    let mut state = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9E37_79B9_7F4A_7C15)
        ^ ((std::process::id() as u64) << 32)
        ^ (bytes.as_ptr() as usize as u64);
    for byte in bytes {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        *byte = state as u8;
    }
    Ok(())
}
