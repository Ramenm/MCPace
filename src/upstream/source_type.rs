pub(super) fn infer_source_type(raw_source_type: &str, command: &str, url: &str) -> String {
    let normalized = normalize_source_type(raw_source_type);
    if !normalized.is_empty() {
        return normalized;
    }
    if !command.trim().is_empty() {
        "stdio".to_string()
    } else if !url.trim().is_empty() {
        "http".to_string()
    } else {
        "stdio".to_string()
    }
}

fn normalize_source_type(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "" => String::new(),
        "streamablehttp" | "streamable-http" | "streamable_http" | "http-stream"
        | "remote-http" | "remote" | "http" | "url" => "http".to_string(),
        "legacy-sse" | "http+sse" | "http-sse" | "remote-sse" | "sse" => "legacy-sse".to_string(),
        "stdio" | "local" | "local-stdio" | "local-command" | "command" => "stdio".to_string(),
        other => other.to_string(),
    }
}
