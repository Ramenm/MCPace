use crate::json::{parse_str, JsonValue};
use crate::runtimepaths;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const OPERATIONS_SCHEMA: &str = "mcpace.retainedOperations.v1";
const MAX_RETAINED_OPERATION_LINE_BYTES: usize = 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
enum BoundedOperationLine {
    Eof,
    Line,
    TooLong,
}

fn read_bounded_operation_line(
    reader: &mut impl BufRead,
    line: &mut Vec<u8>,
    max_bytes: usize,
) -> io::Result<BoundedOperationLine> {
    line.clear();
    let mut too_long = false;
    loop {
        let (consume_len, newline_seen) = {
            let available = reader.fill_buf()?;
            if available.is_empty() {
                return Ok(if too_long {
                    BoundedOperationLine::TooLong
                } else if line.is_empty() {
                    BoundedOperationLine::Eof
                } else {
                    BoundedOperationLine::Line
                });
            }
            let consume_len = available
                .iter()
                .position(|byte| *byte == b'\n')
                .map_or(available.len(), |position| position + 1);
            if !too_long {
                let remaining = max_bytes.saturating_add(1).saturating_sub(line.len());
                let copy_len = consume_len.min(remaining);
                line.extend_from_slice(&available[..copy_len]);
                too_long = line.len() > max_bytes || copy_len < consume_len;
            }
            (consume_len, available[..consume_len].contains(&b'\n'))
        };
        reader.consume(consume_len);
        if newline_seen {
            return Ok(if too_long {
                BoundedOperationLine::TooLong
            } else {
                BoundedOperationLine::Line
            });
        }
    }
}

#[derive(Debug)]
struct RetainedFileSummary {
    role: &'static str,
    path: PathBuf,
    exists: bool,
    bytes: u64,
    parsed_lines: usize,
    parse_errors: usize,
    error: Option<String>,
}

pub(super) fn retained_operations_response(root_path: &Path, limit: usize) -> JsonValue {
    let state_root = runtimepaths::resolve_state_root(root_path);
    let active_path = runtimepaths::hub_log_path(&state_root);
    let archive_path = rotated_log_path(&active_path);
    let mut events = Vec::new();
    let mut total_parsed = 0usize;
    let files = vec![
        read_retained_file(
            "archive",
            &archive_path,
            &mut events,
            &mut total_parsed,
            limit,
        ),
        read_retained_file(
            "active",
            &active_path,
            &mut events,
            &mut total_parsed,
            limit,
        ),
    ];

    retain_latest_events(&mut events, limit);
    let truncated = total_parsed > events.len();

    let oldest_ts = events.first().and_then(event_timestamp_value);
    let newest_ts = events.last().and_then(event_timestamp_value);
    let parse_errors = files.iter().map(|file| file.parse_errors).sum::<usize>();
    let returned = events.len();

    JsonValue::object([
        ("schema", JsonValue::string(OPERATIONS_SCHEMA)),
        ("generatedAtMs", JsonValue::number(now_ms())),
        ("limit", JsonValue::number(limit)),
        ("totalParsed", JsonValue::number(total_parsed)),
        ("returned", JsonValue::number(returned)),
        ("truncated", JsonValue::bool(truncated)),
        ("parseErrors", JsonValue::number(parse_errors)),
        ("oldestTsMs", optional_u128_json(oldest_ts)),
        ("newestTsMs", optional_u128_json(newest_ts)),
        (
            "files",
            JsonValue::array(files.into_iter().map(file_summary_json)),
        ),
        ("events", JsonValue::array(events)),
    ])
}

fn read_retained_file(
    role: &'static str,
    path: &Path,
    events: &mut Vec<JsonValue>,
    total_parsed: &mut usize,
    limit: usize,
) -> RetainedFileSummary {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return RetainedFileSummary {
                role,
                path: path.to_path_buf(),
                exists: false,
                bytes: 0,
                parsed_lines: 0,
                parse_errors: 0,
                error: None,
            };
        }
        Err(error) => {
            return RetainedFileSummary {
                role,
                path: path.to_path_buf(),
                exists: true,
                bytes: 0,
                parsed_lines: 0,
                parse_errors: 0,
                error: Some(format!("failed to inspect {}: {}", path.display(), error)),
            };
        }
    };

    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) => {
            return RetainedFileSummary {
                role,
                path: path.to_path_buf(),
                exists: true,
                bytes: metadata.len(),
                parsed_lines: 0,
                parse_errors: 0,
                error: Some(format!("failed to open {}: {}", path.display(), error)),
            };
        }
    };

    let mut parsed_lines = 0usize;
    let mut parse_errors = 0usize;
    let mut read_error = None;
    let mut reader = BufReader::new(file);
    let mut line = Vec::new();
    loop {
        match read_bounded_operation_line(&mut reader, &mut line, MAX_RETAINED_OPERATION_LINE_BYTES)
        {
            Ok(BoundedOperationLine::Eof) => break,
            Ok(BoundedOperationLine::TooLong) => {
                parse_errors = parse_errors.saturating_add(1);
                continue;
            }
            Ok(BoundedOperationLine::Line) => {}
            Err(error) => {
                read_error = Some(format!("failed to read {}: {}", path.display(), error));
                break;
            }
        }
        let Ok(line_text) = std::str::from_utf8(&line) else {
            parse_errors = parse_errors.saturating_add(1);
            continue;
        };
        let trimmed = line_text.trim();
        if trimmed.is_empty() {
            continue;
        }
        match parse_str(trimmed) {
            Ok(value @ JsonValue::Object(_)) => {
                parsed_lines = parsed_lines.saturating_add(1);
                *total_parsed = total_parsed.saturating_add(1);
                if limit > 0 {
                    events.push(value);
                    if events.len() > limit.saturating_mul(2) {
                        retain_latest_events(events, limit);
                    }
                }
            }
            Ok(_) | Err(_) => {
                parse_errors = parse_errors.saturating_add(1);
            }
        }
    }

    RetainedFileSummary {
        role,
        path: path.to_path_buf(),
        exists: true,
        bytes: metadata.len(),
        parsed_lines,
        parse_errors,
        error: read_error,
    }
}

fn retain_latest_events(events: &mut Vec<JsonValue>, limit: usize) {
    events.sort_by_key(event_timestamp);
    if events.len() > limit {
        events.drain(..events.len().saturating_sub(limit));
    }
}

fn file_summary_json(summary: RetainedFileSummary) -> JsonValue {
    JsonValue::object([
        ("role", JsonValue::string(summary.role)),
        (
            "path",
            JsonValue::string(summary.path.display().to_string()),
        ),
        ("exists", JsonValue::bool(summary.exists)),
        ("bytes", JsonValue::number(summary.bytes)),
        ("parsedLines", JsonValue::number(summary.parsed_lines)),
        ("parseErrors", JsonValue::number(summary.parse_errors)),
        (
            "error",
            summary
                .error
                .map(JsonValue::string)
                .unwrap_or(JsonValue::Null),
        ),
    ])
}

fn event_timestamp(value: &JsonValue) -> u128 {
    event_timestamp_value(value).unwrap_or_default()
}

fn event_timestamp_value(value: &JsonValue) -> Option<u128> {
    value
        .get("tsMs")
        .and_then(JsonValue::as_i64)
        .and_then(|value| u128::try_from(value).ok())
}

fn optional_u128_json(value: Option<u128>) -> JsonValue {
    value.map(JsonValue::number).unwrap_or(JsonValue::Null)
}

fn rotated_log_path(log_path: &Path) -> PathBuf {
    let file_name = log_path
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| "events.log".to_string());
    log_path.with_file_name(format!("{}.1", file_name))
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;
