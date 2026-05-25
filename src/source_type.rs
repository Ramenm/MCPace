pub(crate) fn normalize_public_source_type(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "" => String::new(),
        "streamablehttp" | "streamable-http" | "streamable_http" | "http-stream"
        | "remote-http" | "remote" | "http" | "url" => "streamable-http".to_string(),
        "legacy-sse" | "sse-legacy" | "http+sse" | "http-sse" | "remote-sse" | "sse" => {
            "sse-legacy".to_string()
        }
        "stdio" | "local" | "local-stdio" | "local-command" | "command" => "stdio".to_string(),
        other => other.to_string(),
    }
}

pub(crate) fn normalize_runtime_source_type(value: &str) -> String {
    match normalize_public_source_type(value).as_str() {
        "" => String::new(),
        "streamable-http" => "http".to_string(),
        "sse-legacy" => "legacy-sse".to_string(),
        other => other.to_string(),
    }
}

pub(crate) fn infer_public_source_type(raw_source_type: &str, command: &str, url: &str) -> String {
    infer_source_type_with_remote_default(
        raw_source_type,
        command,
        url,
        "streamable-http",
        normalize_public_source_type,
    )
}

pub(crate) fn infer_runtime_source_type(raw_source_type: &str, command: &str, url: &str) -> String {
    infer_source_type_with_remote_default(
        raw_source_type,
        command,
        url,
        "http",
        normalize_runtime_source_type,
    )
}

fn infer_source_type_with_remote_default<F>(
    raw_source_type: &str,
    command: &str,
    url: &str,
    remote_default: &str,
    normalize: F,
) -> String
where
    F: FnOnce(&str) -> String,
{
    let normalized = normalize(raw_source_type);
    if !normalized.is_empty() {
        return normalized;
    }
    if !command.trim().is_empty() {
        "stdio".to_string()
    } else if !url.trim().is_empty() {
        remote_default.to_string()
    } else {
        "stdio".to_string()
    }
}
