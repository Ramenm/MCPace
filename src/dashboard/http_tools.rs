use super::{empty_object, http_boundary, HttpRequest};
use crate::adapter;
use crate::json::JsonValue;
use crate::json_helpers;

pub(super) fn http_tool_definitions() -> Vec<JsonValue> {
    vec![
        http_tool("doctor", "Inspect MCPace readiness"),
        http_tool("hub_status", "Inspect hub status"),
        http_tool("hub_up", "Start the hub"),
        http_tool("hub_down", "Stop the hub"),
        http_tool("hub_repair", "Repair stopped or stale hub state"),
        http_tool("hub_logs", "Read hub logs"),
        http_tool("server_list", "List configured servers"),
        http_tool(
            "runtime_diagnostics",
            "Explain MCPace runtime and upstream tool availability",
        ),
        http_tool_with_schema(
            "adapter_profile",
            "Infer current client/protocol/transport and server coordination profile dynamically",
            JsonValue::object([
                (
                    "includeLiveCatalog",
                    JsonValue::object([
                        ("type", JsonValue::string("boolean")),
                        (
                            "description",
                            JsonValue::string(
                                "When true, include a live upstream catalog/projection sample. Defaults to false.",
                            ),
                        ),
                    ]),
                ),
                (
                    "timeoutMs",
                    JsonValue::object([
                        ("type", JsonValue::string("integer")),
                        (
                            "description",
                            JsonValue::string("Optional live upstream catalog timeout in milliseconds."),
                        ),
                    ]),
                ),
                (
                    "refresh",
                    JsonValue::object([
                        ("type", JsonValue::string("boolean")),
                        (
                            "description",
                            JsonValue::string("Bypass the short successful upstream tools/list cache."),
                        ),
                    ]),
                ),
            ]),
            vec![],
        ),
        http_tool_with_schema(
            "adapter_route",
            "Plan upstream call routing, batching, serialization, and parallel-safe lanes dynamically",
            JsonValue::object([
                (
                    "calls",
                    JsonValue::object([
                        ("type", JsonValue::string("array")),
                        (
                            "description",
                            JsonValue::string(
                                "Optional calls to plan. Each item may be [server, tool, arguments], {server, name/tool, arguments}, an upstream_search result, or an upstream_call object.",
                            ),
                        ),
                        ("items", JsonValue::object([(
                            "oneOf",
                            JsonValue::array([
                                JsonValue::object([
                                    ("type", JsonValue::string("array")),
                                    ("minItems", JsonValue::number(2)),
                                    ("maxItems", JsonValue::number(3)),
                                ]),
                                JsonValue::object([("type", JsonValue::string("object"))]),
                                JsonValue::object([("type", JsonValue::string("string"))]),
                            ]),
                        )])),
                    ]),
                ),
                (
                    "includeLiveCatalog",
                    JsonValue::object([
                        ("type", JsonValue::string("boolean")),
                        (
                            "description",
                            JsonValue::string("When true, include live upstream catalog context in the route plan."),
                        ),
                    ]),
                ),
                (
                    "timeoutMs",
                    JsonValue::object([
                        ("type", JsonValue::string("integer")),
                        (
                            "description",
                            JsonValue::string("Optional live upstream catalog timeout in milliseconds."),
                        ),
                    ]),
                ),
                (
                    "refresh",
                    JsonValue::object([
                        ("type", JsonValue::string("boolean")),
                        (
                            "description",
                            JsonValue::string("Bypass the short tools/list cache when includeLiveCatalog=true."),
                        ),
                    ]),
                ),
            ]),
            vec![],
        ),
        http_tool_with_schema(
            "upstream_search",
            "Search upstream tools without exposing every upstream schema as a top-level tool",
            JsonValue::object([
                (
                    "query",
                    JsonValue::object([
                        ("type", JsonValue::string("string")),
                        (
                            "description",
                            JsonValue::string(
                                "Optional keyword query over server, tool name, title, and description.",
                            ),
                        ),
                    ]),
                ),
                (
                    "server",
                    JsonValue::object([
                        ("type", JsonValue::string("string")),
                        (
                            "description",
                            JsonValue::string("Optional configured upstream server name to search inside."),
                        ),
                    ]),
                ),
                (
                    "limit",
                    JsonValue::object([
                        ("type", JsonValue::string("integer")),
                        (
                            "description",
                            JsonValue::string("Maximum results to return, clamped to 1..100."),
                        ),
                    ]),
                ),
                (
                    "includeSchema",
                    JsonValue::object([
                        ("type", JsonValue::string("boolean")),
                        (
                            "description",
                            JsonValue::string(
                                "When true, include compact input schemas in search results.",
                            ),
                        ),
                    ]),
                ),
                (
                    "timeoutMs",
                    JsonValue::object([
                        ("type", JsonValue::string("integer")),
                        (
                            "description",
                            JsonValue::string("Optional per-server catalog timeout from 1000 to 300000 ms."),
                        ),
                    ]),
                ),
                (
                    "refresh",
                    JsonValue::object([
                        ("type", JsonValue::string("boolean")),
                        (
                            "description",
                            JsonValue::string("Bypass the short tools/list cache before searching."),
                        ),
                    ]),
                ),
            ]),
            vec![],
        ),
        http_tool_with_schema(
            "surface_manifest",
            "Explain exact native MCPace tools versus configured upstream MCP tools",
            JsonValue::object([
                (
                    "includeLiveCatalog",
                    JsonValue::object([
                        ("type", JsonValue::string("boolean")),
                        (
                            "description",
                            JsonValue::string(
                                "When true, launch/probe configured callable upstreams and include the live upstream_catalog output.",
                            ),
                        ),
                    ]),
                ),
                (
                    "timeoutMs",
                    JsonValue::object([
                        ("type", JsonValue::string("integer")),
                        (
                            "description",
                            JsonValue::string(
                                "Optional per-server catalog timeout from 1000 to 300000 ms when includeLiveCatalog=true.",
                            ),
                        ),
                    ]),
                ),
                (
                    "refresh",
                    JsonValue::object([
                        ("type", JsonValue::string("boolean")),
                        (
                            "description",
                            JsonValue::string(
                                "Bypass the short tools/list cache when includeLiveCatalog=true.",
                            ),
                        ),
                    ]),
                ),
            ]),
            vec![],
        ),
        http_tool_with_schema(
            "upstream_tools",
            "List callable tools for one configured stdio upstream MCP server",
            JsonValue::object([
                (
                    "server",
                    JsonValue::object([
                        ("type", JsonValue::string("string")),
                        (
                            "description",
                            JsonValue::string(
                                "Configured upstream server name from the merged MCP settings registry. Omit to return inventory without launching anything.",
                            ),
                        ),
                    ]),
                ),
                (
                    "timeoutMs",
                    JsonValue::object([
                        ("type", JsonValue::string("integer")),
                        (
                            "description",
                            JsonValue::string("Optional per-call timeout from 1000 to 300000 ms."),
                        ),
                    ]),
                ),
                (
                    "refresh",
                    JsonValue::object([
                        ("type", JsonValue::string("boolean")),
                        (
                            "description",
                            JsonValue::string(
                                "Bypass the short in-process tools/list cache and refresh from the upstream server.",
                            ),
                        ),
                    ]),
                ),
            ]),
            vec![],
        ),
        http_tool_with_schema(
            "upstream_catalog",
            "List configured upstream MCP tools as a flat server-qualified catalog with concise descriptions",
            JsonValue::object([
                (
                    "server",
                    JsonValue::object([
                        ("type", JsonValue::string("string")),
                        (
                            "description",
                            JsonValue::string(
                                "Optional configured upstream server name. Omit to discover all configured upstream tool summaries.",
                            ),
                        ),
                    ]),
                ),
                (
                    "timeoutMs",
                    JsonValue::object([
                        ("type", JsonValue::string("integer")),
                        (
                            "description",
                            JsonValue::string("Optional per-server catalog timeout from 1000 to 300000 ms."),
                        ),
                    ]),
                ),
                (
                    "refresh",
                    JsonValue::object([
                        ("type", JsonValue::string("boolean")),
                        (
                            "description",
                            JsonValue::string(
                                "Bypass the short in-process tools/list cache and refresh from the upstream server.",
                            ),
                        ),
                    ]),
                ),
            ]),
            vec![],
        ),
        http_tool_with_schema(
            "upstream_probe",
            "Probe configured upstream MCP servers with short successful tools/list cache reuse",
            JsonValue::object([
                (
                    "server",
                    JsonValue::object([
                        ("type", JsonValue::string("string")),
                        (
                            "description",
                            JsonValue::string(
                                "Optional configured upstream server name. Omit to probe all configured servers.",
                            ),
                        ),
                    ]),
                ),
                (
                    "timeoutMs",
                    JsonValue::object([
                        ("type", JsonValue::string("integer")),
                        (
                            "description",
                            JsonValue::string(
                                "Optional per-server probe timeout from 1000 to 300000 ms. Default probe timeout is capped so one broken future server cannot stall the whole check.",
                            ),
                        ),
                    ]),
                ),
                (
                    "refresh",
                    JsonValue::object([
                        ("type", JsonValue::string("boolean")),
                        (
                            "description",
                            JsonValue::string(
                                "Bypass the short successful tools/list cache and force a fresh upstream probe.",
                            ),
                        ),
                    ]),
                ),
            ]),
            vec![],
        ),
        http_tool_with_schema(
            "upstream_policy_audit",
            "Audit configured upstream MCP tool annotations and declarative MCPace toolPolicies",
            JsonValue::object([
                (
                    "server",
                    JsonValue::object([
                        ("type", JsonValue::string("string")),
                        (
                            "description",
                            JsonValue::string(
                                "Optional configured upstream server name. Omit to audit all configured upstream tools.",
                            ),
                        ),
                    ]),
                ),
                (
                    "timeoutMs",
                    JsonValue::object([
                        ("type", JsonValue::string("integer")),
                        (
                            "description",
                            JsonValue::string("Optional per-server audit timeout from 1000 to 300000 ms."),
                        ),
                    ]),
                ),
                (
                    "refresh",
                    JsonValue::object([
                        ("type", JsonValue::string("boolean")),
                        (
                            "description",
                            JsonValue::string(
                                "Bypass the short tools/list cache and force a fresh upstream audit.",
                            ),
                        ),
                    ]),
                ),
            ]),
            vec![],
        ),
        http_tool_with_schema(
            "upstream_policy_suggest",
            "Generate declarative MCPace toolPolicies suggestions from live upstream MCP risk signals",
            JsonValue::object([
                (
                    "server",
                    JsonValue::object([
                        ("type", JsonValue::string("string")),
                        (
                            "description",
                            JsonValue::string(
                                "Optional configured upstream server name. Omit to suggest policies for all configured upstream tools.",
                            ),
                        ),
                    ]),
                ),
                (
                    "timeoutMs",
                    JsonValue::object([
                        ("type", JsonValue::string("integer")),
                        (
                            "description",
                            JsonValue::string("Optional per-server suggestion timeout from 1000 to 300000 ms."),
                        ),
                    ]),
                ),
                (
                    "refresh",
                    JsonValue::object([
                        ("type", JsonValue::string("boolean")),
                        (
                            "description",
                            JsonValue::string(
                                "Bypass the short tools/list cache and force fresh upstream suggestions.",
                            ),
                        ),
                    ]),
                ),
            ]),
            vec![],
        ),
        http_tool_with_schema(
            "upstream_call",
            "Call a tool on a configured stdio upstream server",
            JsonValue::object([
                (
                    "server",
                    JsonValue::object([
                        ("type", JsonValue::string("string")),
                        ("description", JsonValue::string("Configured upstream server name.")),
                    ]),
                ),
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
                (
                    "timeoutMs",
                    JsonValue::object([
                        ("type", JsonValue::string("integer")),
                        (
                            "description",
                            JsonValue::string("Optional per-call timeout from 1000 to 300000 ms."),
                        ),
                    ]),
                ),
                (
                    "clientId",
                    JsonValue::object([("type", JsonValue::string("string"))]),
                ),
                (
                    "sessionId",
                    JsonValue::object([("type", JsonValue::string("string"))]),
                ),
                (
                    "projectRoot",
                    JsonValue::object([("type", JsonValue::string("string"))]),
                ),
                (
                    "transport",
                    JsonValue::object([("type", JsonValue::string("string"))]),
                ),
                (
                    "ttlMs",
                    JsonValue::object([
                        ("type", JsonValue::string("integer")),
                        (
                            "description",
                            JsonValue::string("Optional runtime lease TTL in milliseconds."),
                        ),
                    ]),
                ),
                (
                    "metadata",
                    JsonValue::object([("type", JsonValue::string("object"))]),
                ),
                (
                    "resultMode",
                    JsonValue::object([
                        ("type", JsonValue::string("string")),
                        (
                            "enum",
                            JsonValue::array([
                                JsonValue::string("native"),
                                JsonValue::string("compat"),
                                JsonValue::string("compact"),
                                JsonValue::string("summary"),
                            ]),
                        ),
                        (
                            "description",
                            JsonValue::string(
                                "Tool-result content mode: native, compat, compact, or summary.",
                            ),
                        ),
                    ]),
                ),
                (
                    "diagnostics",
                    JsonValue::object([
                        ("type", JsonValue::string("string")),
                        (
                            "enum",
                            JsonValue::array([
                                JsonValue::string("full"),
                                JsonValue::string("summary"),
                                JsonValue::string("none"),
                            ]),
                        ),
                        (
                            "description",
                            JsonValue::string("MCPace lease/session diagnostics to retain."),
                        ),
                    ]),
                ),
                (
                    "nestedContent",
                    JsonValue::object([
                        ("type", JsonValue::string("string")),
                        (
                            "enum",
                            JsonValue::array([
                                JsonValue::string("full"),
                                JsonValue::string("compact"),
                            ]),
                        ),
                        (
                            "description",
                            JsonValue::string("Use compact to dedupe nested upstream content text."),
                        ),
                    ]),
                ),
                (
                    "tokenReducerPlugins",
                    JsonValue::object([
                        ("type", JsonValue::string("array")),
                        (
                            "description",
                            JsonValue::string(
                                "Optional built-in token reducers, e.g. mcpace.native-content.v1.",
                            ),
                        ),
                        (
                            "items",
                            JsonValue::object([("type", JsonValue::string("string"))]),
                        ),
                    ]),
                ),
                (
                    "allowToolRiskClasses",
                    JsonValue::object([
                        ("type", JsonValue::string("array")),
                        (
                            "description",
                            JsonValue::string(
                                "Generic risk-class opt-in for config-declared upstream tool policies, for example ['desktop-observation'].",
                            ),
                        ),
                        (
                            "items",
                            JsonValue::object([("type", JsonValue::string("string"))]),
                        ),
                    ]),
                ),
                (
                    "allowArguments",
                    JsonValue::object([
                        ("type", JsonValue::string("array")),
                        (
                            "description",
                            JsonValue::string(
                                "Generic allow-argument opt-in names for config-declared upstream tool policies, for example ['allowCustomRisk'].",
                            ),
                        ),
                        (
                            "items",
                            JsonValue::object([("type", JsonValue::string("string"))]),
                        ),
                    ]),
                ),
                (
                    "allowUnknownTool",
                    JsonValue::object([
                        ("type", JsonValue::string("boolean")),
                        (
                            "description",
                            JsonValue::string(
                                "Explicit opt-in to call a trusted dynamic upstream tool that is not currently advertised by tools/list. Default is false.",
                            ),
                        ),
                    ]),
                ),
            ]),
            vec!["server", "tool"],
        ),
        http_tool_with_schema(
            "upstream_batch",
            "Call multiple tools on one configured stdio upstream server in a single state-preserving session",
            JsonValue::object([
                (
                    "server",
                    JsonValue::object([
                        ("type", JsonValue::string("string")),
                        ("description", JsonValue::string("Configured upstream server name.")),
                    ]),
                ),
                (
                    "calls",
                    JsonValue::object([
                        ("type", JsonValue::string("array")),
                        (
                            "description",
                            JsonValue::string(
                                "Ordered upstream calls to execute after one initialize handshake. Use this for any stateful upstream server that needs a shared session.",
                            ),
                        ),
                        ("items", http_upstream_batch_call_item_schema()),
                    ]),
                ),
                (
                    "timeoutMs",
                    JsonValue::object([
                        ("type", JsonValue::string("integer")),
                        (
                            "description",
                            JsonValue::string("Optional total batch timeout from 1000 to 300000 ms."),
                        ),
                    ]),
                ),
                (
                    "clientId",
                    JsonValue::object([("type", JsonValue::string("string"))]),
                ),
                (
                    "sessionId",
                    JsonValue::object([("type", JsonValue::string("string"))]),
                ),
                (
                    "projectRoot",
                    JsonValue::object([("type", JsonValue::string("string"))]),
                ),
                (
                    "transport",
                    JsonValue::object([("type", JsonValue::string("string"))]),
                ),
                (
                    "ttlMs",
                    JsonValue::object([
                        ("type", JsonValue::string("integer")),
                        (
                            "description",
                            JsonValue::string("Optional runtime lease TTL in milliseconds."),
                        ),
                    ]),
                ),
                (
                    "metadata",
                    JsonValue::object([("type", JsonValue::string("object"))]),
                ),
                (
                    "resultMode",
                    JsonValue::object([
                        ("type", JsonValue::string("string")),
                        (
                            "enum",
                            JsonValue::array([
                                JsonValue::string("native"),
                                JsonValue::string("compat"),
                                JsonValue::string("compact"),
                                JsonValue::string("summary"),
                            ]),
                        ),
                        (
                            "description",
                            JsonValue::string(
                                "Tool-result content mode: native, compat, compact, or summary.",
                            ),
                        ),
                    ]),
                ),
                (
                    "diagnostics",
                    JsonValue::object([
                        ("type", JsonValue::string("string")),
                        (
                            "enum",
                            JsonValue::array([
                                JsonValue::string("full"),
                                JsonValue::string("summary"),
                                JsonValue::string("none"),
                            ]),
                        ),
                        (
                            "description",
                            JsonValue::string("MCPace lease/session diagnostics to retain."),
                        ),
                    ]),
                ),
                (
                    "nestedContent",
                    JsonValue::object([
                        ("type", JsonValue::string("string")),
                        (
                            "enum",
                            JsonValue::array([
                                JsonValue::string("full"),
                                JsonValue::string("compact"),
                            ]),
                        ),
                        (
                            "description",
                            JsonValue::string("Use compact to dedupe nested upstream content text."),
                        ),
                    ]),
                ),
                (
                    "tokenReducerPlugins",
                    JsonValue::object([
                        ("type", JsonValue::string("array")),
                        (
                            "description",
                            JsonValue::string(
                                "Optional built-in token reducers, e.g. mcpace.native-content.v1.",
                            ),
                        ),
                        (
                            "items",
                            JsonValue::object([("type", JsonValue::string("string"))]),
                        ),
                    ]),
                ),
                (
                    "allowToolRiskClasses",
                    JsonValue::object([
                        ("type", JsonValue::string("array")),
                        (
                            "description",
                            JsonValue::string(
                                "Generic risk-class opt-in for config-declared upstream tool policies, for example ['desktop-observation'].",
                            ),
                        ),
                        (
                            "items",
                            JsonValue::object([("type", JsonValue::string("string"))]),
                        ),
                    ]),
                ),
                (
                    "allowArguments",
                    JsonValue::object([
                        ("type", JsonValue::string("array")),
                        (
                            "description",
                            JsonValue::string(
                                "Generic allow-argument opt-in names for config-declared upstream tool policies, for example ['allowCustomRisk'].",
                            ),
                        ),
                        (
                            "items",
                            JsonValue::object([("type", JsonValue::string("string"))]),
                        ),
                    ]),
                ),
                (
                    "allowUnknownTool",
                    JsonValue::object([
                        ("type", JsonValue::string("boolean")),
                        (
                            "description",
                            JsonValue::string(
                                "Explicit opt-in to call a trusted dynamic upstream tool that is not currently advertised by tools/list. Default is false.",
                            ),
                        ),
                    ]),
                ),
            ]),
            vec!["server", "calls"],
        ),
        http_tool("client_list", "List known client targets"),
    ]
}

pub(super) fn http_tool_definitions_for_request(request: &HttpRequest) -> Vec<JsonValue> {
    let protocol = http_boundary::request_header_string(Some(request), "mcp-protocol-version");
    http_tool_definitions_for_protocol(protocol.as_deref())
}

pub(super) fn http_tool_definitions_for_protocol(protocol: Option<&str>) -> Vec<JsonValue> {
    let options = adapter::tool_surface_options_from_http_header(protocol);
    let names = http_tool_definitions()
        .iter()
        .filter_map(|tool| json_helpers::string_at_path(tool, &["name"]).map(str::to_string))
        .collect::<Vec<_>>();
    let visible_names = adapter::visible_tool_names(&names, None);
    let visible = visible_names
        .iter()
        .map(String::as_str)
        .collect::<std::collections::BTreeSet<_>>();
    http_tool_definitions()
        .into_iter()
        .filter(|tool| {
            json_helpers::string_at_path(tool, &["name"])
                .map(|name| visible.contains(name))
                .unwrap_or(false)
        })
        .map(|tool| shape_http_tool_for_client(tool, options))
        .collect()
}

fn shape_http_tool_for_client(tool: JsonValue, options: adapter::ToolSurfaceOptions) -> JsonValue {
    let JsonValue::Object(mut map) = tool else {
        return tool;
    };
    if !options.include_title {
        map.remove("title");
    }
    if !options.include_annotations {
        map.remove("annotations");
    }
    JsonValue::Object(map)
}

fn http_tool(name: &str, description: &str) -> JsonValue {
    http_tool_with_schema(name, description, empty_object(), vec![])
}

fn http_tool_annotations(name: &str) -> JsonValue {
    let read_only = matches!(
        name,
        "doctor"
            | "hub_status"
            | "hub_logs"
            | "runtime_leases"
            | "server_list"
            | "server_capabilities"
            | "client_list"
            | "client_plan"
            | "client_export"
            | "adapter_profile"
            | "adapter_route"
            | "upstream_search"
            | "surface_manifest"
            | "upstream_tools"
            | "upstream_catalog"
            | "upstream_probe"
            | "upstream_policy_audit"
            | "upstream_policy_suggest"
    );
    let open_world = matches!(
        name,
        "adapter_route"
            | "upstream_search"
            | "upstream_tools"
            | "upstream_catalog"
            | "upstream_probe"
            | "upstream_policy_audit"
            | "upstream_policy_suggest"
            | "upstream_call"
            | "upstream_batch"
    );
    let destructive = matches!(name, "hub_down" | "upstream_call" | "upstream_batch");
    let idempotent = matches!(
        name,
        "doctor"
            | "hub_status"
            | "hub_logs"
            | "runtime_leases"
            | "server_list"
            | "server_capabilities"
            | "client_list"
            | "client_plan"
            | "client_export"
            | "adapter_profile"
            | "adapter_route"
            | "upstream_search"
            | "surface_manifest"
            | "upstream_tools"
            | "upstream_catalog"
            | "upstream_probe"
            | "upstream_policy_audit"
            | "upstream_policy_suggest"
    );

    JsonValue::object([
        ("readOnlyHint", JsonValue::bool(read_only)),
        ("destructiveHint", JsonValue::bool(destructive)),
        ("idempotentHint", JsonValue::bool(idempotent)),
        ("openWorldHint", JsonValue::bool(open_world)),
    ])
}

fn http_tool_with_schema(
    name: &str,
    description: &str,
    properties: JsonValue,
    required: Vec<&str>,
) -> JsonValue {
    JsonValue::object([
        ("name", JsonValue::string(name)),
        ("title", JsonValue::string(description)),
        ("description", JsonValue::string(description)),
        ("annotations", http_tool_annotations(name)),
        (
            "inputSchema",
            JsonValue::object([
                ("type", JsonValue::string("object")),
                ("properties", properties),
                (
                    "required",
                    JsonValue::array(required.into_iter().map(JsonValue::string)),
                ),
                ("additionalProperties", JsonValue::bool(false)),
            ]),
        ),
    ])
}

fn http_upstream_batch_call_item_schema() -> JsonValue {
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

pub(super) fn http_tool_names() -> Vec<String> {
    http_tool_definitions()
        .into_iter()
        .filter_map(|tool| {
            tool.get("name")
                .and_then(JsonValue::as_str)
                .map(str::to_string)
        })
        .collect()
}
