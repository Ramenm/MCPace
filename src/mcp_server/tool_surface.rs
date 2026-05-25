use crate::adapter;
use crate::json::JsonValue;
use crate::mcp_protocol as mcp;
use crate::tool_schemas;

#[derive(Clone, Copy, Debug)]
pub(super) struct ToolSpec {
    name: &'static str,
    title: &'static str,
    description: &'static str,
}

pub(super) const TOOL_SPECS: &[ToolSpec] = &[
    ToolSpec {
        name: "doctor",
        title: "Inspect MCPace readiness",
        description: "Return the native MCPace doctor report for this root.",
    },
    ToolSpec {
        name: "hub_status",
        title: "Inspect hub status",
        description: "Return the local hub status report without changing state.",
    },
    ToolSpec {
        name: "hub_up",
        title: "Start the hub",
        description: "Start the local MCPace hub runtime for this root.",
    },
    ToolSpec {
        name: "hub_down",
        title: "Stop the hub",
        description: "Stop the local MCPace hub runtime for this root.",
    },
    ToolSpec {
        name: "hub_logs",
        title: "Read hub logs",
        description: "Return recent MCPace hub log events.",
    },
    ToolSpec {
        name: "runtime_leases",
        title: "List runtime leases",
        description: "Return the current MCPace runtime lease store after pruning expired leases.",
    },
    ToolSpec {
        name: "runtime_acquire",
        title: "Acquire a runtime lease",
        description:
            "Acquire the scheduler lease for one configured server before routing work to it.",
    },
    ToolSpec {
        name: "runtime_renew",
        title: "Renew a runtime lease",
        description: "Extend a previously acquired MCPace scheduler lease before it expires.",
    },
    ToolSpec {
        name: "runtime_release",
        title: "Release a runtime lease",
        description: "Release a previously acquired MCPace scheduler lease.",
    },
    ToolSpec {
        name: "server_list",
        title: "List configured servers",
        description: "Return the grouped MCPace server inventory for this root.",
    },
    ToolSpec {
        name: "server_capabilities",
        title: "Inspect one server",
        description: "Return grouped capability details for one configured server.",
    },
    ToolSpec {
        name: "client_list",
        title: "List known client targets",
        description: "Return the documented client surface catalog.",
    },
    ToolSpec {
        name: "client_plan",
        title: "Build a client routing plan",
        description: "Resolve routing, session, and server arbitration for a client.",
    },
    ToolSpec {
        name: "client_export",
        title: "Build a client connection contract",
        description: "Return the client launcher contract for a target surface.",
    },
    ToolSpec {
        name: "adapter_profile",
        title: "Explain dynamic adapter profile",
        description: "Infer the current MCP client, protocol, transport, tool surface, and upstream server coordination profile without relying on static brand-only maps.",
    },
    ToolSpec {
        name: "adapter_route",
        title: "Plan upstream routing",
        description: "Build a dynamic routing plan for upstream calls: same-server batching, conflict-domain serialization, and parallel-safe lanes without client/server maps.",
    },
    ToolSpec {
        name: "upstream_search",
        title: "Search upstream tools",
        description: "Search the live configured upstream tool catalog and return concise ready-to-call results instead of loading every upstream schema into tools/list.",
    },
    ToolSpec {
        name: "surface_manifest",
        title: "Explain the MCPace tool surface",
        description: "Return the transparent contract for native MCPace tools versus configured upstream MCP tools; optionally include a live upstream catalog.",
    },
    ToolSpec {
        name: "upstream_tools",
        title: "List one upstream server's tools",
        description: "List callable tools for one configured stdio upstream MCP server; omit server for fast inventory only.",
    },
    ToolSpec {
        name: "upstream_catalog",
        title: "Catalog configured upstream tools",
        description: "Discover configured upstream MCP tools with concise flat server-qualified descriptions and upstream_call arguments without hardcoded server names.",
    },
    ToolSpec {
        name: "upstream_probe",
        title: "Probe configured upstream servers",
        description: "Probe configured upstream MCP servers without hardcoded server names; uses the short successful tools/list cache unless refresh=true is supplied.",
    },
    ToolSpec {
        name: "upstream_policy_audit",
        title: "Audit upstream tool policies",
        description: "Audit configured upstream MCP tool annotations and declarative MCPace toolPolicies to find tools that need review or explicit guards.",
    },
    ToolSpec {
        name: "upstream_policy_suggest",
        title: "Suggest upstream tool policies",
        description: "Generate declarative mcpace.config.json toolPolicies suggestions from live upstream MCP annotations and advisory risk signals.",
    },
    ToolSpec {
        name: "upstream_call",
        title: "Call one upstream tool",
        description: "Call a tool on a configured stdio upstream MCP server.",
    },
    ToolSpec {
        name: "upstream_batch",
        title: "Call upstream tools in one session",
        description: "Call multiple tools on one configured stdio upstream MCP server in a single state-preserving session.",
    },
];

fn tool_annotations(name: &str) -> JsonValue {
    let read_only = matches!(
        name,
        "adapter_profile"
            | "doctor"
            | "hub_status"
            | "hub_logs"
            | "runtime_leases"
            | "server_list"
            | "server_capabilities"
            | "client_list"
            | "client_plan"
            | "client_export"
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
        "adapter_profile"
            | "doctor"
            | "hub_status"
            | "hub_logs"
            | "runtime_leases"
            | "server_list"
            | "server_capabilities"
            | "client_list"
            | "client_plan"
            | "client_export"
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

pub(super) fn tool_definition(
    tool: &ToolSpec,
    surface_options: adapter::ToolSurfaceOptions,
) -> JsonValue {
    let input_schema = match tool.name {
                "hub_logs" => JsonValue::object([
                    ("type", JsonValue::string("object")),
                    (
                        "properties",
                        JsonValue::object([(
                            "tail",
                            JsonValue::object([
                                ("type", JsonValue::string("integer")),
                                (
                                    "description",
                                    JsonValue::string("Optional number of log lines to return."),
                                ),
                            ]),
                        )]),
                    ),
                    ("additionalProperties", JsonValue::bool(false)),
                ]),
                "runtime_acquire" => JsonValue::object([
                    ("type", JsonValue::string("object")),
                    (
                        "properties",
                        JsonValue::object([
                            (
                                "server",
                                JsonValue::object([
                                    ("type", JsonValue::string("string")),
                                    (
                                        "description",
                                        JsonValue::string(
                                            "Configured MCPace server name to lease.",
                                        ),
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
                                JsonValue::object([("type", JsonValue::string("integer"))]),
                            ),
                            (
                                "metadata",
                                JsonValue::object([("type", JsonValue::string("object"))]),
                            ),
                        ]),
                    ),
                    ("required", JsonValue::array([JsonValue::string("server")])),
                    ("additionalProperties", JsonValue::bool(false)),
                ]),
                "runtime_renew" => JsonValue::object([
                    ("type", JsonValue::string("object")),
                    (
                        "properties",
                        JsonValue::object([
                            (
                                "leaseId",
                                JsonValue::object([
                                    ("type", JsonValue::string("string")),
                                    (
                                        "description",
                                        JsonValue::string("Lease id returned by runtime_acquire."),
                                    ),
                                ]),
                            ),
                            (
                                "ttlMs",
                                JsonValue::object([
                                    ("type", JsonValue::string("integer")),
                                    (
                                        "description",
                                        JsonValue::string(
                                            "Optional renewed lease TTL in milliseconds.",
                                        ),
                                    ),
                                ]),
                            ),
                        ]),
                    ),
                    ("required", JsonValue::array([JsonValue::string("leaseId")])),
                    ("additionalProperties", JsonValue::bool(false)),
                ]),
                "runtime_release" => JsonValue::object([
                    ("type", JsonValue::string("object")),
                    (
                        "properties",
                        JsonValue::object([(
                            "leaseId",
                            JsonValue::object([
                                ("type", JsonValue::string("string")),
                                (
                                    "description",
                                    JsonValue::string("Lease id returned by runtime_acquire."),
                                ),
                            ]),
                        )]),
                    ),
                    ("required", JsonValue::array([JsonValue::string("leaseId")])),
                    ("additionalProperties", JsonValue::bool(false)),
                ]),
                "server_capabilities" => JsonValue::object([
                    ("type", JsonValue::string("object")),
                    (
                        "properties",
                        JsonValue::object([(
                            "name",
                            JsonValue::object([
                                ("type", JsonValue::string("string")),
                                (
                                    "description",
                                    JsonValue::string("Configured MCPace server name to inspect."),
                                ),
                            ]),
                        )]),
                    ),
                    ("required", JsonValue::array([JsonValue::string("name")])),
                    ("additionalProperties", JsonValue::bool(false)),
                ]),
                "client_plan" | "client_export" => JsonValue::object([
                    ("type", JsonValue::string("object")),
                    (
                        "properties",
                        JsonValue::object([
                            (
                                "clientId",
                                JsonValue::object([
                                    ("type", JsonValue::string("string")),
                                    (
                                        "description",
                                        JsonValue::string(
                                            "Optional client target override. Defaults to the \
                                             server launch client id.",
                                        ),
                                    ),
                                ]),
                            ),
                            (
                                "sessionId",
                                JsonValue::object([
                                    ("type", JsonValue::string("string")),
                                    (
                                        "description",
                                        JsonValue::string("Optional external session id override."),
                                    ),
                                ]),
                            ),
                            (
                                "projectRoot",
                                JsonValue::object([
                                    ("type", JsonValue::string("string")),
                                    (
                                        "description",
                                        JsonValue::string("Optional project root override."),
                                    ),
                                ]),
                            ),
                            (
                                "transport",
                                JsonValue::object([
                                    ("type", JsonValue::string("string")),
                                    (
                                        "description",
                                        JsonValue::string(
                                            "Optional ingress override such as stdio or \
                                             streamable-http.",
                                        ),
                                    ),
                                ]),
                            ),
                            (
                                "metadata",
                                JsonValue::object([
                                    ("type", JsonValue::string("object")),
                                    (
                                        "description",
                                        JsonValue::string(
                                            "Optional MCP metadata object forwarded as \
                                             metadata-json.",
                                        ),
                                    ),
                                ]),
                            ),
                        ]),
                    ),
                    ("additionalProperties", JsonValue::bool(false)),
                ]),
                "adapter_profile" => JsonValue::object([
                    ("type", JsonValue::string("object")),
                    (
                        "properties",
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
                                        JsonValue::string(
                                            "Optional live upstream catalog timeout in milliseconds.",
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
                                            "Bypass the short successful upstream tools/list cache.",
                                        ),
                                    ),
                                ]),
                            ),
                        ]),
                    ),
                    ("additionalProperties", JsonValue::bool(false)),
                ]),
                "adapter_route" => JsonValue::object([
                    ("type", JsonValue::string("object")),
                    (
                        "properties",
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
                                        JsonValue::string(
                                            "When true, include live upstream catalog context in the route plan.",
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
                                            "Optional live upstream catalog timeout in milliseconds.",
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
                                            "Bypass the short successful upstream tools/list cache when includeLiveCatalog=true.",
                                        ),
                                    ),
                                ]),
                            ),
                        ]),
                    ),
                    ("additionalProperties", JsonValue::bool(false)),
                ]),
                "upstream_search" => JsonValue::object([
                    ("type", JsonValue::string("object")),
                    (
                        "properties",
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
                                        JsonValue::string(
                                            "Optional configured upstream server name to search inside.",
                                        ),
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
                                        JsonValue::string(
                                            "Optional per-server catalog timeout from 1000 to 300000 ms.",
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
                                            "Bypass the short tools/list cache before searching.",
                                        ),
                                    ),
                                ]),
                            ),
                        ]),
                    ),
                    ("additionalProperties", JsonValue::bool(false)),
                ]),
                "surface_manifest" => JsonValue::object([
                    ("type", JsonValue::string("object")),
                    (
                        "properties",
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
                    ),
                    ("additionalProperties", JsonValue::bool(false)),
                ]),
                "upstream_tools"
                | "upstream_catalog"
                | "upstream_policy_audit"
                | "upstream_policy_suggest" => JsonValue::object([
                    ("type", JsonValue::string("object")),
                    (
                        "properties",
                        JsonValue::object([
                            (
                                "server",
                                JsonValue::object([
                                    ("type", JsonValue::string("string")),
                                    (
                                        "description",
                                        JsonValue::string(
                                            "Optional configured upstream server name from the merged MCP settings registry.",
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
                                            "Optional per-server timeout from 1000 to 300000 ms.",
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
                                            "Bypass the short in-process tools/list cache and refresh from the upstream server.",
                                        ),
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
                                        JsonValue::string(
                                            "Optional runtime lease TTL in milliseconds.",
                                        ),
                                    ),
                                ]),
                            ),
                            (
                                "metadata",
                                JsonValue::object([("type", JsonValue::string("object"))]),
                            ),
                        ]),
                    ),
                    ("additionalProperties", JsonValue::bool(false)),
                ]),
                "upstream_probe" => JsonValue::object([
                    ("type", JsonValue::string("object")),
                    (
                        "properties",
                        JsonValue::object([
                            (
                                "server",
                                JsonValue::object([
                                    ("type", JsonValue::string("string")),
                                    (
                                        "description",
                                        JsonValue::string(
                                            "Optional configured upstream server name from the merged MCP settings registry.",
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
                                            "Optional per-server timeout from 1000 to 300000 ms.",
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
                                        JsonValue::string(
                                            "Optional runtime lease TTL in milliseconds.",
                                        ),
                                    ),
                                ]),
                            ),
                            (
                                "metadata",
                                JsonValue::object([("type", JsonValue::string("object"))]),
                            ),
                        ]),
                    ),
                    ("additionalProperties", JsonValue::bool(false)),
                ]),
                "upstream_call" => JsonValue::object([
                    ("type", JsonValue::string("object")),
                    (
                        "properties",
                        JsonValue::object([
                            (
                                "server",
                                JsonValue::object([
                                    ("type", JsonValue::string("string")),
                                    (
                                        "description",
                                        JsonValue::string("Configured upstream server name."),
                                    ),
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
                                        JsonValue::string(
                                            "Optional per-call timeout from 1000 to 300000 ms.",
                                        ),
                                    ),
                                ]),
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
                                            "Tool-result content mode. native preserves upstream content at the top level and avoids duplicate JSON; compat keeps pretty JSON text; compact uses compact JSON; summary uses short text plus structuredContent.",
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
                                        JsonValue::string(
                                            "How much MCPace lease/session diagnostic data to retain in structuredContent.",
                                        ),
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
                                        JsonValue::string(
                                            "Use compact to replace duplicated nested upstream content text when structuredContent is present.",
                                        ),
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
                                            "Optional built-in token reducers, e.g. mcpace.native-content.v1 or mcpace.dedupe-nested-upstream-content.v1.",
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
                    ),
                    (
                        "required",
                        JsonValue::array([JsonValue::string("server"), JsonValue::string("tool")]),
                    ),
                    ("additionalProperties", JsonValue::bool(false)),
                ]),
                "upstream_batch" => JsonValue::object([
                    ("type", JsonValue::string("object")),
                    (
                        "properties",
                        JsonValue::object([
                            (
                                "server",
                                JsonValue::object([
                                    ("type", JsonValue::string("string")),
                                    (
                                        "description",
                                        JsonValue::string("Configured upstream server name."),
                                    ),
                                ]),
                            ),
                            (
                                "calls",
                                JsonValue::object([
                                    ("type", JsonValue::string("array")),
                                    (
                                        "description",
                                        JsonValue::string(
                                            "Ordered upstream calls to execute after one initialize handshake.",
                                        ),
                                    ),
                                    ("items", tool_schemas::upstream_batch_call_item_schema()),
                                ]),
                            ),
                            (
                                "timeoutMs",
                                JsonValue::object([
                                    ("type", JsonValue::string("integer")),
                                    (
                                        "description",
                                        JsonValue::string(
                                            "Optional total batch timeout from 1000 to 300000 ms.",
                                        ),
                                    ),
                                ]),
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
                                            "Tool-result content mode. native preserves upstream content at the top level and avoids duplicate JSON; compat keeps pretty JSON text; compact uses compact JSON; summary uses short text plus structuredContent.",
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
                                        JsonValue::string(
                                            "How much MCPace lease/session diagnostic data to retain in structuredContent.",
                                        ),
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
                                        JsonValue::string(
                                            "Use compact to replace duplicated nested upstream content text when structuredContent is present.",
                                        ),
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
                                            "Optional built-in token reducers, e.g. mcpace.native-content.v1 or mcpace.dedupe-nested-upstream-content.v1.",
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
                    ),
                    (
                        "required",
                        JsonValue::array([JsonValue::string("server"), JsonValue::string("calls")]),
                    ),
                    ("additionalProperties", JsonValue::bool(false)),
                ]),
                _ => JsonValue::object([
                    ("type", JsonValue::string("object")),
                    ("properties", mcp::empty_object()),
                    ("additionalProperties", JsonValue::bool(false)),
                ]),
            };

    let mut entries = vec![
        ("name", JsonValue::string(tool.name)),
        ("description", JsonValue::string(tool.description)),
        ("inputSchema", input_schema),
    ];
    if surface_options.include_title {
        entries.push(("title", JsonValue::string(tool.title)));
    }
    if surface_options.include_annotations {
        entries.push(("annotations", tool_annotations(tool.name)));
    }
    JsonValue::object(entries)
}

pub(super) fn mcp_tool_names() -> Vec<String> {
    TOOL_SPECS
        .iter()
        .map(|tool| tool.name.to_string())
        .collect()
}
