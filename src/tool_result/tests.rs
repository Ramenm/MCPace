use super::*;
use crate::json_helpers;

#[test]
fn native_mode_defaults_to_short_content_for_non_upstream_tools() {
    let value = JsonValue::object([
        ("ok", JsonValue::bool(true)),
        ("server", JsonValue::string("demo")),
        ("tool", JsonValue::string("demo_status")),
    ]);
    let payload = tool_result_payload(
        value,
        false,
        ToolResultOptions {
            result_mode: ToolResultMode::Native,
            ..ToolResultOptions::default()
        },
    );
    let items = payload
        .get("content")
        .and_then(JsonValue::as_array)
        .unwrap();
    assert!(items[0]
        .get("text")
        .and_then(JsonValue::as_str)
        .unwrap()
        .contains("server=demo"));
}

#[test]
fn native_upstream_mode_preserves_upstream_content_at_top_level() {
    let value = JsonValue::object([
        ("ok", JsonValue::bool(true)),
        ("upstreamOk", JsonValue::bool(true)),
        (
            "upstreamResult",
            JsonValue::object([
                (
                    "content",
                    JsonValue::array([JsonValue::object([
                        ("type", JsonValue::string("text")),
                        ("text", JsonValue::string("hello from upstream")),
                    ])]),
                ),
                (
                    "structuredContent",
                    JsonValue::object([("value", JsonValue::number(7))]),
                ),
            ]),
        ),
    ]);
    let payload = upstream_tool_result_payload(
        value,
        false,
        ToolResultOptions {
            result_mode: ToolResultMode::Native,
            ..ToolResultOptions::default()
        },
    );
    let content = payload
        .get("content")
        .and_then(JsonValue::as_array)
        .unwrap();
    assert_eq!(
        content[0].get("text").and_then(JsonValue::as_str),
        Some("hello from upstream")
    );
    assert_eq!(
        json_helpers::value_at_path(&payload, &["structuredContent", "value"])
            .and_then(JsonValue::as_i64),
        Some(7)
    );
    assert_eq!(
        payload.get("isError").and_then(JsonValue::as_bool),
        Some(false)
    );
}

#[test]
fn native_upstream_mode_propagates_upstream_errors() {
    let value = JsonValue::object([
        ("ok", JsonValue::bool(false)),
        ("upstreamOk", JsonValue::bool(false)),
        ("upstreamIsError", JsonValue::bool(true)),
        (
            "upstreamResult",
            JsonValue::object([("content", JsonValue::array([]))]),
        ),
    ]);
    let payload = upstream_tool_result_payload(
        value,
        false,
        ToolResultOptions {
            result_mode: ToolResultMode::Native,
            ..ToolResultOptions::default()
        },
    );
    assert_eq!(
        payload.get("isError").and_then(JsonValue::as_bool),
        Some(true)
    );
}

#[test]
fn diagnostics_none_drops_lease_and_session_fields() {
    let value = JsonValue::object([
        ("ok", JsonValue::bool(true)),
        ("bridgeOk", JsonValue::bool(true)),
        (
            "lease",
            JsonValue::object([("id", JsonValue::string("lease:1"))]),
        ),
        ("leaseReleased", JsonValue::bool(true)),
        ("sessionPoolHit", JsonValue::bool(true)),
        (
            "upstreamResult",
            JsonValue::object([("content", JsonValue::array([]))]),
        ),
    ]);
    let shaped = shape_upstream_diagnostics(value, UpstreamDiagnosticsMode::None);
    assert!(shaped.get("ok").is_some());
    assert!(shaped.get("upstreamResult").is_some());
    assert!(shaped.get("bridgeOk").is_none());
    assert!(shaped.get("lease").is_none());
    assert!(shaped.get("leaseReleased").is_none());
    assert!(shaped.get("sessionPoolHit").is_none());
}

#[test]
fn nested_content_compaction_keeps_structured_content() {
    let value = JsonValue::object([(
        "upstreamResult",
        JsonValue::object([
            (
                "content",
                JsonValue::array([JsonValue::object([
                    ("type", JsonValue::string("text")),
                    ("text", JsonValue::string("{\"large\":true}")),
                ])]),
            ),
            (
                "structuredContent",
                JsonValue::object([("large", JsonValue::bool(true))]),
            ),
        ]),
    )]);
    let shaped = shape_nested_upstream_content(value, NestedUpstreamContentMode::Compact);
    assert_eq!(
        json_helpers::bool_at_path(&shaped, &["upstreamResult", "structuredContent", "large"]),
        Some(true)
    );
    let result = shaped.get("upstreamResult").unwrap();
    let content = result.get("content").and_then(JsonValue::as_array).unwrap();
    assert!(content[0]
        .get("text")
        .and_then(JsonValue::as_str)
        .unwrap()
        .contains("compacted"));
}

#[test]
fn supported_token_reducer_plugins_are_valid_in_strict_mode() {
    for plugin in supported_token_reducer_plugins() {
        let args = JsonValue::object([
            (
                "tokenReducerPlugins",
                JsonValue::array([JsonValue::string(*plugin)]),
            ),
            ("pluginPolicy", JsonValue::string("strict")),
        ]);
        options_from_arguments(&args).unwrap_or_else(|error| {
            panic!("advertised token reducer plugin {plugin} was rejected: {error}")
        });
    }
}

#[test]
fn unknown_token_reducer_plugin_is_rejected_only_in_strict_mode() {
    let args = JsonValue::object([(
        "tokenReducerPlugins",
        JsonValue::array([JsonValue::string("mcpace.schema-compact.v1")]),
    )]);
    assert!(options_from_arguments(&args).is_ok());

    let strict_args = JsonValue::object([
        (
            "tokenReducerPlugins",
            JsonValue::array([JsonValue::string("mcpace.schema-compact.v1")]),
        ),
        ("pluginPolicy", JsonValue::string("strict")),
    ]);
    let error = options_from_arguments(&strict_args).expect_err("strict rejects unknown plugin");
    assert!(error.contains("unknown tokenReducerPlugins entry"));
    assert!(error.contains("mcpace.native-content.v1"));
}
