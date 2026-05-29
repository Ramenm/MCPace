pub(crate) fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

pub(crate) fn normalize_flag(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

pub(crate) fn join_or_none<T: AsRef<str>>(values: &[T]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values
            .iter()
            .map(AsRef::as_ref)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

pub(crate) fn ascii_alnum_dash_underscore(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
}


pub(crate) fn valid_http_header_value(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
}

pub(crate) fn uses_shell_composition(value: &str) -> bool {
    let chars = value.chars().collect::<Vec<_>>();
    let mut single_quoted = false;
    let mut double_quoted = false;
    let mut escaped = false;

    for (index, ch) in chars.iter().copied().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' && !single_quoted {
            escaped = true;
            continue;
        }
        if ch == '\'' && !double_quoted {
            single_quoted = !single_quoted;
            continue;
        }
        if ch == '"' && !single_quoted {
            double_quoted = !double_quoted;
            continue;
        }
        if single_quoted || double_quoted {
            continue;
        }
        if matches!(ch, '`' | ';' | '|' | '<' | '>' | '&') {
            return true;
        }
        if ch == '$' && chars.get(index + 1) == Some(&'(') {
            return true;
        }
    }

    false
}

pub(crate) fn trimmed_non_empty_owned(value: Option<&String>) -> Option<String> {
    value
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(crate) fn sorted_unique(mut values: Vec<String>) -> Vec<String> {
    values.sort();
    values.dedup();
    values
}
