use super::sanitize_path_for_display;
use std::path::Path;

pub(super) fn build_unified_config_diff(path: &Path, before: &str, after: &str) -> String {
    if before == after {
        return String::new();
    }

    let display_path = sanitize_path_for_display(path);
    let mut diff = Vec::new();
    diff.push(format!("--- {} (current)", display_path));
    diff.push(format!("+++ {} (candidate)", display_path));

    let mut before_state = DiffSanitizeState::default();
    if !before.is_empty() {
        for line in before.lines() {
            diff.push(format!(
                "-{}",
                sanitize_config_diff_line(line, &mut before_state)
            ));
        }
    }
    let mut after_state = DiffSanitizeState::default();
    if !after.is_empty() {
        for line in after.lines() {
            diff.push(format!(
                "+{}",
                sanitize_config_diff_line(line, &mut after_state)
            ));
        }
    }

    diff.join("\n")
}

#[derive(Default)]
struct DiffSanitizeState {
    in_sensitive_multiline_value: bool,
    close_marker: Option<&'static str>,
}

fn sanitize_config_diff_line(line: &str, state: &mut DiffSanitizeState) -> String {
    let escaped = escape_diff_control_chars(line);
    if state.in_sensitive_multiline_value {
        if let Some(marker) = state.close_marker {
            if escaped.contains(marker) {
                state.in_sensitive_multiline_value = false;
                state.close_marker = None;
            }
            return "[REDACTED]".to_string();
        }
        if is_top_level_config_boundary(&escaped) {
            state.in_sensitive_multiline_value = false;
        } else {
            return "[REDACTED]".to_string();
        }
    }

    let separator_index = match (escaped.find('='), escaped.find(':')) {
        (Some(equal), Some(colon)) => Some(equal.min(colon)),
        (Some(equal), None) => Some(equal),
        (None, Some(colon)) => Some(colon),
        (None, None) => None,
    };
    let lower_line = escaped.to_ascii_lowercase();
    let key_area = separator_index
        .map(|index| &escaped[..index])
        .unwrap_or(&escaped)
        .to_ascii_lowercase();
    let sensitive_keys = [
        "token",
        "api_key",
        "apikey",
        "api-key",
        "private_key",
        "private-key",
        "secret",
        "password",
        "passwd",
        "auth",
        "authorization",
        "credential",
    ];
    if !sensitive_keys
        .iter()
        .any(|sensitive_key| lower_line.contains(sensitive_key))
    {
        return escaped;
    }

    state.in_sensitive_multiline_value = true;
    state.close_marker = sensitive_multiline_close_marker(&escaped);

    if !sensitive_keys
        .iter()
        .any(|sensitive_key| key_area.contains(sensitive_key))
    {
        return "[REDACTED]".to_string();
    }

    let Some(separator_index) = separator_index else {
        return "[REDACTED]".to_string();
    };
    let prefix = escaped[..=separator_index].trim_end();
    let suffix = if escaped.trim_end().ends_with(',') {
        ","
    } else {
        ""
    };
    format!("{} \"[redacted]\"{}", prefix, suffix)
}

fn sensitive_multiline_close_marker(line: &str) -> Option<&'static str> {
    if line.matches("\"\"\"").count() % 2 == 1 {
        return Some("\"\"\"");
    }
    if line.matches("'''").count() % 2 == 1 {
        return Some("'''");
    }
    None
}

fn is_top_level_config_boundary(line: &str) -> bool {
    let trimmed = line.trim_start();
    if trimmed.is_empty() || line.len() != trimmed.len() {
        return false;
    }
    if trimmed.starts_with('[') {
        return true;
    }
    let Some(separator_index) = trimmed.find('=').or_else(|| trimmed.find(':')) else {
        return false;
    };
    let key = trimmed[..separator_index].trim();
    !key.is_empty()
        && key
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | '"' | '\''))
}

fn escape_diff_control_chars(line: &str) -> String {
    let mut escaped = String::new();
    for ch in line.chars() {
        if ch == '\x1b' {
            escaped.push_str("\\x1b");
        } else if ch.is_control() && ch != '\t' {
            escaped.push_str(&format!("\\u{{{:x}}}", ch as u32));
        } else {
            escaped.push(ch);
        }
    }
    escaped
}
