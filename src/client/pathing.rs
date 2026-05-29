use crate::text_utils;

pub(super) fn path_is_within(path: &str, root: &str) -> bool {
    let path_key = path_compare_key(path);
    let root_key = path_compare_key(root);
    if root_key.is_empty() {
        return false;
    }
    path_key == root_key || path_key.starts_with(&(root_key + "/"))
}

fn path_compare_key(value: &str) -> String {
    let normalized = lexical_path_key(value);
    if looks_like_windows_path(&normalized) {
        normalized.to_ascii_lowercase()
    } else {
        normalized
    }
}

fn looks_like_windows_path(value: &str) -> bool {
    let bytes = value.as_bytes();
    (bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic())
        || value.starts_with("//")
}

fn lexical_path_key(value: &str) -> String {
    let normalized = normalize_path(value);
    let mut rest = normalized.trim();
    if rest.is_empty() {
        return String::new();
    }

    let prefix;
    if rest.starts_with("//") {
        prefix = "//";
        rest = &rest[2..];
    } else if rest.as_bytes().len() >= 2
        && rest.as_bytes()[1] == b':'
        && rest.as_bytes()[0].is_ascii_alphabetic()
    {
        prefix = &rest[..2];
        rest = &rest[2..];
        rest = rest.trim_start_matches('/');
    } else if rest.starts_with('/') {
        prefix = "/";
        rest = rest.trim_start_matches('/');
    } else {
        prefix = "";
    }

    let mut parts: Vec<&str> = Vec::new();
    for part in rest.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if parts.last().is_some_and(|last| *last != "..") {
                    parts.pop();
                } else if prefix.is_empty() {
                    parts.push(part);
                }
            }
            _ => parts.push(part),
        }
    }

    match prefix {
        "/" => {
            if parts.is_empty() {
                "/".to_string()
            } else {
                format!("/{}", parts.join("/"))
            }
        }
        "//" => {
            if parts.is_empty() {
                "//".to_string()
            } else {
                format!("//{}", parts.join("/"))
            }
        }
        "" => parts.join("/"),
        drive => {
            if parts.is_empty() {
                drive.to_string()
            } else {
                format!("{}/{}", drive, parts.join("/"))
            }
        }
    }
}

pub(super) fn normalize(value: &str) -> String {
    text_utils::normalize_flag(value)
}

pub(super) fn normalize_transport(value: &str) -> String {
    match normalize(value).as_str() {
        "http" | "streamable-http" | "streamable_http" => "streamable-http".to_string(),
        "stdio" | "local-stdio" => "stdio".to_string(),
        "sse" => "sse".to_string(),
        "" => "stdio".to_string(),
        other => other.to_string(),
    }
}

pub(super) fn normalize_path(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let normalized = trimmed.replace('\\', "/");
    if let Some(decoded) = decode_file_uri(&normalized) {
        return decoded;
    }
    normalized
}

fn decode_file_uri(value: &str) -> Option<String> {
    if !value.to_ascii_lowercase().starts_with("file://") {
        return None;
    }
    let mut rest = trim_uri_suffix(&value[7..]);
    let lower = rest.to_ascii_lowercase();
    if lower.starts_with("localhost/") {
        rest = &rest[9..];
    } else if !rest.starts_with('/') {
        return None;
    }

    let mut decoded = percent_decode(rest);
    if decoded.len() >= 3 {
        let bytes = decoded.as_bytes();
        if bytes[0] == b'/' && bytes[2] == b':' && bytes[1].is_ascii_alphabetic() {
            decoded = decoded[1..].to_string();
        }
    }
    Some(decoded)
}

fn trim_uri_suffix(value: &str) -> &str {
    let query_index = value.find('?');
    let fragment_index = value.find('#');
    let cutoff = match (query_index, fragment_index) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (Some(index), None) | (None, Some(index)) => Some(index),
        (None, None) => None,
    };
    cutoff.map(|index| &value[..index]).unwrap_or(value)
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hi = hex_value(bytes[index + 1]);
            let lo = hex_value(bytes[index + 2]);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                output.push(hi * 16 + lo);
                index += 3;
                continue;
            }
        }
        output.push(bytes[index]);
        index += 1;
    }
    String::from_utf8(output)
        .unwrap_or_else(|error| String::from_utf8_lossy(error.as_bytes()).into_owned())
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub(super) fn sanitize_key(value: &str) -> String {
    normalize_path(value)
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '/' | '-' | '_' | '.' | ':') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

pub(super) fn stable_hash_hex(input: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in input.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", hash)
}


#[cfg(test)]
mod tests {
    use super::path_is_within;

    #[test]
    fn path_containment_uses_lexical_segments_not_raw_prefixes() {
        assert!(path_is_within(
            "/work/project/src/lib.rs",
            "/work/project"
        ));
        assert!(!path_is_within(
            "/work/project/../secret/file.txt",
            "/work/project"
        ));
        assert!(!path_is_within(
            "/work/project-other/file.txt",
            "/work/project"
        ));
    }

    #[test]
    fn path_containment_handles_windows_drive_roots_case_insensitively() {
        assert!(path_is_within("c:/Users/alice/project", "C:/"));
        assert!(path_is_within("C:/Work/Project", "c:/work"));
        assert!(!path_is_within("C:/work/../Windows", "C:/work"));
        assert!(!path_is_within("D:/Users/alice", "C:/"));
    }
}
