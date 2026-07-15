use std::fmt;
use std::io::Write;

/// Write one diagnostic record to stderr-like sinks without touching stdout.
///
/// MCP stdio uses stdout as a protocol stream, so runtime diagnostics must stay
/// on stderr or in files. This helper intentionally avoids `println!`/`eprintln!`
/// call sites in protocol/runtime modules while preserving existing tests that
/// pass in-memory stderr buffers.
pub(crate) fn stderr_line(stderr: &mut dyn Write, args: fmt::Arguments<'_>) {
    let _ = stderr.write_fmt(args);
    let _ = stderr.write_all(b"\n");
}
