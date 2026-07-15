use crate::json::JsonValue;
use crate::json_helpers;
use std::collections::BTreeMap;
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ToolResultError {
    InvalidArgument {
        field: &'static str,
        reason: String,
    },
    UnsupportedMode {
        field: &'static str,
        value: String,
        expected: &'static str,
    },
    UnknownPlugin {
        plugin: String,
        supported: String,
    },
}

pub type ToolResultResult<T> = std::result::Result<T, ToolResultError>;

impl ToolResultError {
    fn invalid_argument(field: &'static str, reason: impl Into<String>) -> Self {
        Self::InvalidArgument {
            field,
            reason: reason.into(),
        }
    }

    fn unsupported_mode(
        field: &'static str,
        value: impl Into<String>,
        expected: &'static str,
    ) -> Self {
        Self::UnsupportedMode {
            field,
            value: value.into(),
            expected,
        }
    }

    fn unknown_plugin(plugin: impl Into<String>, supported: impl Into<String>) -> Self {
        Self::UnknownPlugin {
            plugin: plugin.into(),
            supported: supported.into(),
        }
    }

    pub fn contains(&self, needle: &str) -> bool {
        self.to_string().contains(needle)
    }
}

impl fmt::Display for ToolResultError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidArgument { reason, .. } => formatter.write_str(reason),
            Self::UnsupportedMode {
                field,
                value,
                expected,
            } => write!(
                formatter,
                "unsupported {} '{}'; use {}",
                field, value, expected
            ),
            Self::UnknownPlugin { plugin, supported } => write!(
                formatter,
                "unknown tokenReducerPlugins entry '{}'; supported built-ins are {}",
                plugin, supported
            ),
        }
    }
}

impl std::error::Error for ToolResultError {}

impl From<ToolResultError> for String {
    fn from(error: ToolResultError) -> Self {
        error.to_string()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolResultMode {
    /// Native-first MCP behavior: preserve upstream content items at the top level when
    /// possible, keep structuredContent useful, and avoid duplicating large JSON text.
    Native,
    /// Pretty JSON text plus structuredContent for clients that prefer serialized output.
    Compat,
    /// MCP-compatible compact serialized JSON text plus structuredContent.
    Compact,
    /// Short human/model-readable text plus structuredContent.
    Summary,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpstreamDiagnosticsMode {
    /// Preserve every MCPace diagnostic field.
    Full,
    /// Keep success/failure counters and booleans, but drop large lease/session internals.
    Summary,
    /// Drop MCPace bridge diagnostics and keep the useful upstream result envelope only.
    None,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NestedUpstreamContentMode {
    /// Preserve nested upstream tool results exactly as returned.
    Full,
    /// If nested upstream results already have top-level content preserved by MCPace or have
    /// structuredContent, replace duplicated nested text content with a short marker.
    Compact,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ToolResultOptions {
    pub result_mode: ToolResultMode,
    pub upstream_diagnostics: UpstreamDiagnosticsMode,
    pub nested_upstream_content: NestedUpstreamContentMode,
}

const SUPPORTED_TOKEN_REDUCER_PLUGINS: &[&str] = &[
    "mcpace.native-content.v1",
    "mcpace.summary-content.v1",
    "mcpace.compat-content.v1",
    "mcpace.compact-content.v1",
    "mcpace.trim-upstream-diagnostics.v1",
    "mcpace.drop-upstream-diagnostics.v1",
    "mcpace.dedupe-nested-upstream-content.v1",
];

pub fn supported_token_reducer_plugins() -> &'static [&'static str] {
    SUPPORTED_TOKEN_REDUCER_PLUGINS
}

impl Default for ToolResultOptions {
    fn default() -> Self {
        Self {
            result_mode: default_result_mode(),
            upstream_diagnostics: default_upstream_diagnostics(),
            nested_upstream_content: default_nested_upstream_content(),
        }
    }
}

pub fn options_from_arguments(arguments: &JsonValue) -> ToolResultResult<ToolResultOptions> {
    let mut options = ToolResultOptions::default();

    if let Some(mode) = first_string_argument(arguments, &["resultMode", "toolResultMode"]) {
        options.result_mode = parse_result_mode(mode)?;
    }
    if let Some(mode) = first_string_argument(arguments, &["diagnostics", "upstreamDiagnostics"]) {
        options.upstream_diagnostics = parse_diagnostics_mode(mode)?;
    }
    if let Some(mode) =
        first_string_argument(arguments, &["nestedContent", "upstreamNestedContent"])
    {
        options.nested_upstream_content = parse_nested_content_mode(mode)?;
    }

    let strict_plugins = first_string_argument(
        arguments,
        &[
            "tokenReducerPluginPolicy",
            "pluginPolicy",
            "resultPluginPolicy",
        ],
    )
    .map(|value| normalized_token(value) == "strict")
    .unwrap_or(false);

    if let Some(plugins) = json_helpers::array_at_path(arguments, &["tokenReducerPlugins"])
        .or_else(|| json_helpers::array_at_path(arguments, &["resultPlugins"]))
    {
        for plugin in plugins {
            let Some(plugin_name) = plugin.as_str() else {
                return Err(ToolResultError::invalid_argument(
                    "tokenReducerPlugins",
                    "tokenReducerPlugins must be an array of strings",
                ));
            };
            apply_builtin_token_reducer(plugin_name, &mut options, strict_plugins)?;
        }
    }

    Ok(options)
}

pub fn tool_result_payload(
    structured: JsonValue,
    is_error: bool,
    options: ToolResultOptions,
) -> JsonValue {
    let content = match options.result_mode {
        ToolResultMode::Native | ToolResultMode::Summary => {
            text_content(summarize_tool_result(&structured, is_error))
        }
        ToolResultMode::Compat => text_content(structured.to_pretty_string()),
        ToolResultMode::Compact => text_content(structured.to_compact_string()),
    };

    tool_result_object(content, Some(structured), is_error)
}

/// Build a native-first result for wrapper tools that call upstream MCP servers.
///
/// In native mode this preserves the actual upstream `content` items at the top level, so images,
/// resource links, and text look like a direct upstream client result. The structured payload is
/// compacted separately to avoid repeating the same nested `content` text.
pub fn upstream_tool_result_payload(
    structured: JsonValue,
    wrapper_is_error: bool,
    options: ToolResultOptions,
) -> JsonValue {
    let diagnostics_shaped = shape_upstream_diagnostics(structured, options.upstream_diagnostics);
    let effective_is_error =
        wrapper_is_error || upstream_structured_indicates_error(&diagnostics_shaped);

    match options.result_mode {
        ToolResultMode::Native => {
            let native_content = native_upstream_content(&diagnostics_shaped);
            let top_level_content_preserved = native_content.is_some();
            let content = native_content.unwrap_or_else(|| {
                text_content(summarize_tool_result(
                    &diagnostics_shaped,
                    effective_is_error,
                ))
            });
            let structured_content =
                if options.upstream_diagnostics == UpstreamDiagnosticsMode::None {
                    native_upstream_structured_content(&diagnostics_shaped)
                } else {
                    None
                }
                .unwrap_or_else(|| {
                    shape_nested_upstream_content_for_payload(
                        diagnostics_shaped.clone(),
                        options.nested_upstream_content,
                        top_level_content_preserved,
                    )
                });
            tool_result_object(content, Some(structured_content), effective_is_error)
        }
        ToolResultMode::Compat | ToolResultMode::Compact | ToolResultMode::Summary => {
            let shaped =
                shape_nested_upstream_content(diagnostics_shaped, options.nested_upstream_content);
            tool_result_payload(shaped, effective_is_error, options)
        }
    }
}

fn default_result_mode() -> ToolResultMode {
    std::env::var("MCPACE_TOOL_RESULT_MODE")
        .ok()
        .and_then(|value| parse_result_mode(&value).ok())
        .unwrap_or(ToolResultMode::Native)
}

fn default_upstream_diagnostics() -> UpstreamDiagnosticsMode {
    std::env::var("MCPACE_UPSTREAM_DIAGNOSTICS")
        .ok()
        .and_then(|value| parse_diagnostics_mode(&value).ok())
        .unwrap_or(UpstreamDiagnosticsMode::None)
}

fn default_nested_upstream_content() -> NestedUpstreamContentMode {
    std::env::var("MCPACE_NESTED_UPSTREAM_CONTENT")
        .ok()
        .and_then(|value| parse_nested_content_mode(&value).ok())
        .unwrap_or(NestedUpstreamContentMode::Compact)
}

fn first_string_argument<'a>(arguments: &'a JsonValue, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| json_helpers::string_at_path(arguments, &[*key]))
}

fn parse_result_mode(value: &str) -> ToolResultResult<ToolResultMode> {
    match normalized_token(value).as_str() {
        "native" | "passthrough" | "direct" | "auto" => Ok(ToolResultMode::Native),
        "compat" | "compatible" | "pretty" | "full" => Ok(ToolResultMode::Compat),
        "compact" | "json" => Ok(ToolResultMode::Compact),
        "summary" | "summarized" | "brief" => Ok(ToolResultMode::Summary),
        other => Err(ToolResultError::unsupported_mode(
            "resultMode",
            other,
            "native, compat, compact, or summary",
        )),
    }
}

fn parse_diagnostics_mode(value: &str) -> ToolResultResult<UpstreamDiagnosticsMode> {
    match normalized_token(value).as_str() {
        "full" | "all" | "compat" => Ok(UpstreamDiagnosticsMode::Full),
        "summary" | "summarized" | "brief" => Ok(UpstreamDiagnosticsMode::Summary),
        "none" | "off" | "minimal" | "native" | "false" => Ok(UpstreamDiagnosticsMode::None),
        other => Err(ToolResultError::unsupported_mode(
            "diagnostics mode",
            other,
            "full, summary, or none",
        )),
    }
}

fn parse_nested_content_mode(value: &str) -> ToolResultResult<NestedUpstreamContentMode> {
    match normalized_token(value).as_str() {
        "full" | "all" | "compat" => Ok(NestedUpstreamContentMode::Full),
        "compact" | "summary" | "summarized" | "dedupe" | "deduplicated" | "native" => {
            Ok(NestedUpstreamContentMode::Compact)
        }
        other => Err(ToolResultError::unsupported_mode(
            "nestedContent mode",
            other,
            "full or compact",
        )),
    }
}

fn normalized_token(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace('_', "-")
}

fn apply_builtin_token_reducer(
    plugin_name: &str,
    options: &mut ToolResultOptions,
    strict: bool,
) -> ToolResultResult<()> {
    match normalized_token(plugin_name).as_str() {
        "mcpace.native-content.v1" | "native-content" | "native" => {
            options.result_mode = ToolResultMode::Native;
        }
        "mcpace.summary-content.v1" | "summary-content" => {
            options.result_mode = ToolResultMode::Summary;
        }
        "mcpace.compat-content.v1" | "compat-content" => {
            options.result_mode = ToolResultMode::Compat;
        }
        "mcpace.compact-content.v1" | "compact-content" => {
            options.result_mode = ToolResultMode::Compact;
        }
        "mcpace.trim-upstream-diagnostics.v1" | "trim-upstream-diagnostics" => {
            options.upstream_diagnostics = UpstreamDiagnosticsMode::Summary;
        }
        "mcpace.drop-upstream-diagnostics.v1" | "drop-upstream-diagnostics" => {
            options.upstream_diagnostics = UpstreamDiagnosticsMode::None;
        }
        "mcpace.dedupe-nested-upstream-content.v1" | "dedupe-nested-upstream-content" => {
            options.nested_upstream_content = NestedUpstreamContentMode::Compact;
        }
        other => {
            if strict {
                return Err(ToolResultError::unknown_plugin(
                    other,
                    SUPPORTED_TOKEN_REDUCER_PLUGINS.join(", "),
                ));
            }
        }
    }
    Ok(())
}

fn tool_result_object(
    content: Vec<JsonValue>,
    structured: Option<JsonValue>,
    is_error: bool,
) -> JsonValue {
    let mut entries = vec![("content", JsonValue::array(content))];
    if let Some(structured) = structured {
        entries.push(("structuredContent", structured));
    }
    entries.push(("isError", JsonValue::bool(is_error)));
    JsonValue::object(entries)
}

fn text_content(text: String) -> Vec<JsonValue> {
    vec![JsonValue::object([
        ("type", JsonValue::string("text")),
        ("text", JsonValue::string(text)),
    ])]
}

fn native_upstream_content(structured: &JsonValue) -> Option<Vec<JsonValue>> {
    if let Some(items) = json_helpers::array_at_path(structured, &["upstreamResult", "content"]) {
        if !items.is_empty() {
            return Some(items.to_vec());
        }
    }

    let results = json_helpers::array_at_path(structured, &["results"])?;
    let mut content = Vec::new();
    for result in results {
        if let Some(items) = json_helpers::array_at_path(result, &["upstreamResult", "content"]) {
            content.extend(items.iter().cloned());
        }
    }
    if content.is_empty() {
        None
    } else {
        Some(content)
    }
}

fn native_upstream_structured_content(structured: &JsonValue) -> Option<JsonValue> {
    json_helpers::value_at_path(structured, &["upstreamResult", "structuredContent"]).cloned()
}

fn upstream_structured_indicates_error(structured: &JsonValue) -> bool {
    json_helpers::bool_at_path(structured, &["upstreamIsError"]) == Some(true)
        || json_helpers::bool_at_path(structured, &["upstreamOk"]) == Some(false)
        || json_helpers::bool_at_path(structured, &["ok"]) == Some(false)
        || json_helpers::value_at_path(structured, &["upstreamFailedCount"])
            .and_then(JsonValue::as_i64)
            .map(|value| value > 0)
            .unwrap_or(false)
        || json_helpers::array_at_path(structured, &["results"])
            .map(|results| {
                results.iter().any(|result| {
                    json_helpers::bool_at_path(result, &["upstreamIsError"]) == Some(true)
                        || json_helpers::bool_at_path(result, &["upstreamOk"]) == Some(false)
                })
            })
            .unwrap_or(false)
}

fn summarize_tool_result(structured: &JsonValue, is_error: bool) -> String {
    let mut parts = Vec::new();
    if is_error {
        parts.push("error".to_string());
    }
    for key in [
        "ok",
        "server",
        "tool",
        "upstreamOk",
        "upstreamIsError",
        "callCount",
        "upstreamOkCount",
        "upstreamFailedCount",
        "activeLeaseCount",
    ] {
        if let Some(value) = json_helpers::value_at_path(structured, &[key]) {
            parts.push(format!("{}={}", key, short_json(value)));
        }
    }
    if let Some(error) = json_helpers::string_at_path(structured, &["error"]) {
        parts.push(format!("error={}", truncate(error, 220)));
    }
    if parts.is_empty() {
        if is_error {
            "Tool call failed; full details are in structuredContent.".to_string()
        } else {
            "Tool call succeeded; full result is in structuredContent.".to_string()
        }
    } else {
        format!("{}; full result is in structuredContent.", parts.join(" "))
    }
}

fn short_json(value: &JsonValue) -> String {
    match value {
        JsonValue::String(text) => truncate(text, 160),
        JsonValue::Bool(value) => value.to_string(),
        JsonValue::Number(value) => value.clone(),
        JsonValue::Null => "null".to_string(),
        JsonValue::Array(items) => format!("[{} items]", items.len()),
        JsonValue::Object(object) => format!("{{{} keys}}", object.len()),
    }
}

fn truncate(value: &str, limit: usize) -> String {
    let mut result = String::new();
    for ch in value.chars().take(limit) {
        result.push(ch);
    }
    if value.chars().count() > limit {
        result.push('…');
    }
    result
}

fn shape_upstream_diagnostics(value: JsonValue, mode: UpstreamDiagnosticsMode) -> JsonValue {
    match mode {
        UpstreamDiagnosticsMode::Full => value,
        UpstreamDiagnosticsMode::Summary => prune_object(value, should_drop_summary_diagnostic),
        UpstreamDiagnosticsMode::None => prune_object(value, should_drop_all_diagnostic),
    }
}

fn prune_object<F>(value: JsonValue, should_drop: F) -> JsonValue
where
    F: Fn(&str) -> bool,
{
    match value {
        JsonValue::Object(map) => JsonValue::Object(
            map.into_iter()
                .filter(|(key, _)| !should_drop(key))
                .collect::<BTreeMap<_, _>>(),
        ),
        other => other,
    }
}

fn should_drop_summary_diagnostic(key: &str) -> bool {
    matches!(
        key,
        "lease"
            | "leaseRelease"
            | "leaseId"
            | "sessionPoolSessionAgeMs"
            | "sessionPoolIdleTtlMs"
            | "sessionPoolEvictedIdleCount"
            | "sessionPoolEvictedCapacityCount"
    )
}

fn should_drop_all_diagnostic(key: &str) -> bool {
    key == "bridgeOk"
        || key == "timeoutMs"
        || key.starts_with("lease")
        || key.starts_with("sessionPool")
}

fn shape_nested_upstream_content(value: JsonValue, mode: NestedUpstreamContentMode) -> JsonValue {
    shape_nested_upstream_content_for_payload(value, mode, false)
}

fn shape_nested_upstream_content_for_payload(
    value: JsonValue,
    mode: NestedUpstreamContentMode,
    compact_plain_content: bool,
) -> JsonValue {
    match mode {
        NestedUpstreamContentMode::Full => value,
        NestedUpstreamContentMode::Compact => {
            compact_nested_upstream_results(value, compact_plain_content)
        }
    }
}

fn compact_nested_upstream_results(value: JsonValue, compact_plain_content: bool) -> JsonValue {
    match value {
        JsonValue::Object(map) => JsonValue::Object(
            map.into_iter()
                .map(|(key, value)| {
                    if key == "upstreamResult" {
                        (
                            key,
                            compact_one_upstream_result(value, compact_plain_content),
                        )
                    } else if key == "results" {
                        (
                            key,
                            compact_nested_result_array(value, compact_plain_content),
                        )
                    } else {
                        (key, value)
                    }
                })
                .collect(),
        ),
        other => other,
    }
}

fn compact_nested_result_array(value: JsonValue, compact_plain_content: bool) -> JsonValue {
    match value {
        JsonValue::Array(items) => JsonValue::Array(
            items
                .into_iter()
                .map(|item| match item {
                    JsonValue::Object(map) => JsonValue::Object(
                        map.into_iter()
                            .map(|(key, value)| {
                                if key == "upstreamResult" {
                                    (
                                        key,
                                        compact_one_upstream_result(value, compact_plain_content),
                                    )
                                } else {
                                    (key, value)
                                }
                            })
                            .collect(),
                    ),
                    other => other,
                })
                .collect(),
        ),
        other => other,
    }
}

fn compact_one_upstream_result(value: JsonValue, compact_plain_content: bool) -> JsonValue {
    let JsonValue::Object(mut map) = value else {
        return value;
    };
    let has_content = map.contains_key("content");
    let has_structured_content = map.contains_key("structuredContent");
    if has_content && (has_structured_content || compact_plain_content) {
        map.insert(
            "content".to_string(),
            JsonValue::array([JsonValue::object([
                ("type", JsonValue::string("text")),
                (
                    "text",
                    JsonValue::string(
                        "Nested upstream content was compacted by MCPace; see top-level content or structuredContent.",
                    ),
                ),
            ])]),
        );
    }
    JsonValue::Object(map)
}

#[cfg(test)]
mod tests;
