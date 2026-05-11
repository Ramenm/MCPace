use std::sync::mpsc::Receiver;

pub(super) const STDERR_DIAGNOSTIC_MAX_LINES: usize = 6;
pub(super) const STDERR_DIAGNOSTIC_MAX_CHARS_PER_LINE: usize = 320;
pub(super) const DIAGNOSTIC_REDACTION: &str = "<redacted>";

pub(super) fn stderr_suffix(stderr_rx: &Receiver<String>) -> String {
    let mut lines = Vec::new();
    while let Ok(line) = stderr_rx.try_recv() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            lines.push(sanitize_stderr_diagnostic(trimmed));
        }
        if lines.len() >= STDERR_DIAGNOSTIC_MAX_LINES {
            break;
        }
    }
    if lines.is_empty() {
        String::new()
    } else {
        format!("; stderr: {}", lines.join(" | "))
    }
}

fn sanitize_stderr_diagnostic(value: &str) -> String {
    truncate_diagnostic_text(&redact_sensitive_assignments(&redact_bearer_tokens(value)))
}

fn redact_sensitive_assignments(value: &str) -> String {
    let chars = value.chars().collect::<Vec<_>>();
    let mut redacted = String::new();
    let mut index = 0;

    while index < chars.len() {
        let current = chars[index];
        if current == '=' || current == ':' {
            let key_end = diagnostic_key_end(&chars, index);
            let key_start = diagnostic_key_start(&chars, key_end);
            let key = chars[key_start..key_end]
                .iter()
                .collect::<String>()
                .trim()
                .trim_matches(|ch| ch == '\'' || ch == '"')
                .to_string();
            if diagnostic_key_is_sensitive(&key) {
                redacted.push(current);
                index += 1;
                while index < chars.len() && chars[index].is_whitespace() {
                    redacted.push(chars[index]);
                    index += 1;
                }
                redacted.push_str(DIAGNOSTIC_REDACTION);
                index = skip_sensitive_diagnostic_value(&chars, index);
                continue;
            }
        }
        redacted.push(current);
        index += 1;
    }

    redacted
}

fn diagnostic_key_end(chars: &[char], separator_index: usize) -> usize {
    let mut end = separator_index;
    while end > 0 && chars[end - 1].is_whitespace() {
        end -= 1;
    }
    end
}

fn diagnostic_key_start(chars: &[char], key_end: usize) -> usize {
    let mut start = key_end;
    while start > 0 {
        let previous = chars[start - 1];
        if previous.is_alphanumeric()
            || matches!(previous, '_' | '-' | '.')
            || (matches!(previous, '\'' | '"') && start == key_end)
        {
            start -= 1;
        } else {
            break;
        }
    }
    start
}

fn diagnostic_key_is_sensitive(key: &str) -> bool {
    let normalized = key
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .collect::<String>()
        .to_ascii_lowercase();
    if normalized.is_empty() {
        return false;
    }
    [
        "token",
        "secret",
        "password",
        "passwd",
        "api_key",
        "apikey",
        "access_key",
        "accesskey",
        "private_key",
        "privatekey",
        "authorization",
        "credential",
        "credentials",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
}

fn skip_sensitive_diagnostic_value(chars: &[char], mut index: usize) -> usize {
    if index >= chars.len() {
        return index;
    }

    if matches!(chars[index], '\'' | '"') {
        let quote = chars[index];
        index += 1;
        while index < chars.len() {
            let current = chars[index];
            index += 1;
            if current == quote {
                break;
            }
        }
        return index;
    }

    while index < chars.len()
        && !chars[index].is_whitespace()
        && !matches!(chars[index], ',' | ';' | '|' | ')' | '}')
    {
        index += 1;
    }
    index
}

fn redact_bearer_tokens(value: &str) -> String {
    let mut redacted = String::new();
    let lower = value.to_ascii_lowercase();
    let mut index = 0;

    while let Some(relative) = lower[index..].find("bearer ") {
        let start = index + relative;
        redacted.push_str(&value[index..start]);
        redacted.push_str("Bearer ");
        redacted.push_str(DIAGNOSTIC_REDACTION);
        index = skip_bearer_token(value, start + "bearer ".len());
    }

    redacted.push_str(&value[index..]);
    redacted
}

fn skip_bearer_token(value: &str, start: usize) -> usize {
    let mut end = start;
    for (relative, ch) in value[start..].char_indices() {
        if ch.is_whitespace() || matches!(ch, ',' | ';' | '|' | ')' | '}') {
            return start + relative;
        }
        end = start + relative + ch.len_utf8();
    }
    end
}

fn truncate_diagnostic_text(value: &str) -> String {
    if value.chars().count() <= STDERR_DIAGNOSTIC_MAX_CHARS_PER_LINE {
        return value.to_string();
    }

    let mut truncated = value
        .chars()
        .take(STDERR_DIAGNOSTIC_MAX_CHARS_PER_LINE)
        .collect::<String>();
    truncated.push_str("…<truncated>");
    truncated
}
