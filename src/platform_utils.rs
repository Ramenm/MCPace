pub(crate) fn current_platform_alias() -> &'static str {
    match std::env::consts::OS {
        "macos" => "macos",
        "windows" => "windows",
        "linux" => "linux",
        other => other,
    }
}

pub(crate) fn normalize_platform(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "darwin" | "mac" | "osx" | "macos" => "macos".to_string(),
        "win" | "windows" => "windows".to_string(),
        "linux" => "linux".to_string(),
        other => other.to_string(),
    }
}

pub(crate) fn supports_current_platform<T: AsRef<str>>(platforms: &[T]) -> bool {
    if platforms.is_empty() {
        return true;
    }
    let current = current_platform_alias();
    platforms.iter().any(|platform| {
        let normalized = normalize_platform(platform.as_ref());
        normalized == current || normalized == "any" || normalized == "all" || normalized == "*"
    })
}
