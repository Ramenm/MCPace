use crate::json::JsonValue;

/// Shared schema for one item in the `upstream_batch` calls array.
///
/// The native MCP tool surface and the dashboard HTTP bridge expose the same
/// upstream_batch contract; keeping the item schema here prevents their JSON
/// schemas from drifting apart.
pub(crate) fn upstream_batch_call_item_schema() -> JsonValue {
    JsonValue::object([(
        "oneOf",
        JsonValue::array([
            JsonValue::object([
                ("type", JsonValue::string("object")),
                (
                    "properties",
                    JsonValue::object([
                        (
                            "tool",
                            JsonValue::object([
                                ("type", JsonValue::string("string")),
                                ("description", JsonValue::string("Upstream tool name.")),
                            ]),
                        ),
                        (
                            "arguments",
                            JsonValue::object([
                                ("type", JsonValue::string("object")),
                                (
                                    "description",
                                    JsonValue::string("Arguments to pass to the upstream tool."),
                                ),
                                ("additionalProperties", JsonValue::bool(true)),
                            ]),
                        ),
                    ]),
                ),
                ("required", JsonValue::array([JsonValue::string("tool")])),
                ("additionalProperties", JsonValue::bool(false)),
            ]),
            JsonValue::object([
                ("type", JsonValue::string("array")),
                (
                    "description",
                    JsonValue::string("Compact tuple form: [tool] or [tool, arguments]."),
                ),
                ("minItems", JsonValue::number(1)),
                ("maxItems", JsonValue::number(2)),
                (
                    "prefixItems",
                    JsonValue::array([
                        JsonValue::object([
                            ("type", JsonValue::string("string")),
                            ("description", JsonValue::string("Upstream tool name.")),
                        ]),
                        JsonValue::object([
                            ("type", JsonValue::string("object")),
                            (
                                "description",
                                JsonValue::string("Arguments to pass to the upstream tool."),
                            ),
                            ("additionalProperties", JsonValue::bool(true)),
                        ]),
                    ]),
                ),
                ("items", JsonValue::bool(false)),
            ]),
        ]),
    )])
}
