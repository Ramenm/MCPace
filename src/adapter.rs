use crate::json::JsonValue;
use crate::json_helpers;
use crate::mcp_protocol as mcp;
use crate::server::{load_server_records, ServerRecord};
use crate::upstream;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

const DEFAULT_TOOL_BUDGET: usize = 64;
const DEFAULT_TOOL_TOKEN_BUDGET: usize = 24_000;
const DEFAULT_PROJECTED_DESCRIPTION_CHARS: usize = 360;
const DEFAULT_PROJECTED_SCHEMA_DESCRIPTION_CHARS: usize = 160;
const DEFAULT_SEARCH_DESCRIPTION_CHARS: usize = 220;
const DEFAULT_TOOLS_LIST_TIMEOUT_MS: u64 = 5_000;
const PROJECTED_TOOL_PREFIX: &str = "u";
const PROJECTED_PROMPT_PREFIX: &str = "p";
const PROJECTED_NAME_MAX: usize = 64;
const PROXIED_RESOURCE_SCHEME: &str = "mcpace://upstream-resource";
const PROXIED_TEMPLATE_SCHEME: &str = "mcpace://upstream-resource-template";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolExposureMode {
    /// Decide from the live upstream catalog: native projection when the catalog fits the budget,
    /// otherwise keep a compact broker/search surface.
    Auto,
    /// Never project upstream tools into tools/list. Use upstream_search/upstream_call instead.
    Broker,
    /// Project upstream tools whenever available, truncating to the configured count/token budget.
    Native,
    /// Project the prefix that fits the configured count/token budget and keep broker search for the rest.
    Hybrid,
    /// Keep only the essential adapter tools for very strict clients.
    Minimal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectionSafety {
    /// Project only tools that look read-only by trusted annotations or conservative names.
    Safe,
    /// Default: project unguarded tools unless they look mutating/destructive.
    Review,
    /// Project every discovered tool and rely on the client/human-in-the-loop for review.
    All,
}

const DEFAULT_PROJECTED_TOOL_SAFETY: ProjectionSafety = ProjectionSafety::Review;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToolSurfaceOptions {
    pub include_title: bool,
    pub include_annotations: bool,
}

#[derive(Clone, Debug)]
pub struct ToolExposureOptions {
    pub mode: ToolExposureMode,
    pub budget: usize,
    pub token_budget: usize,
    pub timeout_ms: Option<u64>,
    pub refresh: bool,
    pub projection_safety: ProjectionSafety,
}

#[derive(Clone, Debug)]
pub struct ProjectedToolTarget {
    pub server: String,
    pub tool: String,
}

#[derive(Clone, Debug)]
pub struct ProjectedPromptTarget {
    pub server: String,
    pub prompt: String,
}

#[derive(Clone, Debug)]
pub struct ProjectedToolSet {
    pub tools: Vec<JsonValue>,
    pub raw_upstream_tool_count: usize,
    pub total_upstream_tool_count: usize,
    pub broker_only_tool_count: usize,
    pub projected_tool_count: usize,
    pub estimated_total_tokens: usize,
    pub estimated_projected_tokens: usize,
    pub truncated: bool,
    pub projection_enabled: bool,
    pub reason: String,
    pub catalog: JsonValue,
}

impl ToolSurfaceOptions {
    pub fn current() -> Self {
        Self {
            include_title: true,
            include_annotations: true,
        }
    }

    pub fn legacy() -> Self {
        Self {
            include_title: false,
            include_annotations: false,
        }
    }
}

impl ToolExposureOptions {
    pub fn from_env() -> Self {
        Self {
            mode: tool_exposure_mode_from_env(),
            budget: env_usize("MCPACE_TOOL_BUDGET")
                .or_else(|| env_usize("MCPACE_NATIVE_TOOL_BUDGET"))
                .unwrap_or(DEFAULT_TOOL_BUDGET)
                .clamp(1, 2048),
            token_budget: env_usize("MCPACE_TOOL_TOKEN_BUDGET")
                .or_else(|| env_usize("MCPACE_NATIVE_TOOL_TOKEN_BUDGET"))
                .unwrap_or(DEFAULT_TOOL_TOKEN_BUDGET)
                .clamp(1_000, 1_000_000),
            timeout_ms: env_u64("MCPACE_TOOLS_LIST_TIMEOUT_MS")
                .or(Some(DEFAULT_TOOLS_LIST_TIMEOUT_MS)),
            refresh: env_bool("MCPACE_TOOLS_LIST_REFRESH").unwrap_or(false),
            projection_safety: std::env::var("MCPACE_PROJECTED_TOOL_SAFETY")
                .or_else(|_| std::env::var("MCPACE_TOOL_PROJECTION_SAFETY"))
                .ok()
                .map(|value| parse_projection_safety(&value))
                .unwrap_or(DEFAULT_PROJECTED_TOOL_SAFETY),
        }
    }

    pub fn for_call_resolution() -> Self {
        let mut options = Self::from_env();
        // Keep names stable during a session. tools/list may refresh, but direct calls should use
        // the cached catalog unless the operator explicitly refreshes the list first.
        options.refresh = false;
        options
    }
}

pub fn adapter_capabilities() -> JsonValue {
    JsonValue::object([
        (
            "tools",
            JsonValue::object([("listChanged", JsonValue::bool(false))]),
        ),
        (
            "resources",
            JsonValue::object([
                ("subscribe", JsonValue::bool(false)),
                ("listChanged", JsonValue::bool(false)),
            ]),
        ),
        (
            "prompts",
            JsonValue::object([("listChanged", JsonValue::bool(false))]),
        ),
    ])
}

pub fn adapter_instructions() -> String {
    "MCPace is a dynamic MCP adapter for many clients and many upstream servers. It infers client capabilities from initialize, discovers configured upstream stdio servers through live MCP methods, projects upstream tools natively only when the catalog fits the token budget, and falls back to compact broker tools when it does not. Use adapter_profile for the current routing plan, upstream_search for concise discovery, projected u_<server>_<tool>_<hash> names when present, and upstream_call/upstream_batch when a client/tool budget requires brokered routing."
        .to_string()
}

pub fn tool_surface_options_from_initialize(
    initialize_params: Option<&JsonValue>,
) -> ToolSurfaceOptions {
    let protocol_version = initialize_params
        .and_then(|params| json_helpers::string_at_path(params, &["protocolVersion"]))
        .unwrap_or(mcp::CURRENT_PROTOCOL_VERSION);
    tool_surface_options_from_protocol(protocol_version)
}

pub fn tool_surface_options_from_http_header(protocol_header: Option<&str>) -> ToolSurfaceOptions {
    tool_surface_options_from_protocol(
        protocol_header
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(mcp::STREAMABLE_HTTP_DEFAULT_PROTOCOL_VERSION),
    )
}

pub fn tool_surface_options_from_protocol(protocol_version: &str) -> ToolSurfaceOptions {
    match std::env::var("MCPACE_TOOL_SCHEMA_STYLE")
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("legacy") | Some("old") | Some("compact") => return ToolSurfaceOptions::legacy(),
        Some("native") | Some("current") | Some("modern") => return ToolSurfaceOptions::current(),
        _ => {}
    }

    if protocol_rank(protocol_version) >= protocol_rank("2025-03-26") {
        ToolSurfaceOptions::current()
    } else {
        ToolSurfaceOptions::legacy()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ManagementSurfaceMode {
    /// Adapter/runtime tools that help a model route and execute upstream servers.
    Adapter,
    /// Every MCPace management/debug/install helper.
    Full,
    /// The smallest broker surface for strict or tiny-context clients.
    Minimal,
}

pub fn visible_tool_names(
    all_tool_names: &[String],
    _initialize_params: Option<&JsonValue>,
) -> Vec<String> {
    all_tool_names
        .iter()
        .filter(|name| should_keep_management_tool(name))
        .cloned()
        .collect()
}

pub fn should_keep_management_tool(name: &str) -> bool {
    match management_surface_mode_from_env() {
        ManagementSurfaceMode::Full => true,
        ManagementSurfaceMode::Adapter => adapter_management_tool(name),
        ManagementSurfaceMode::Minimal => minimal_management_tool(name),
    }
}

fn adapter_management_tool(name: &str) -> bool {
    matches!(
        name,
        "adapter_profile"
            | "adapter_route"
            | "upstream_search"
            | "surface_manifest"
            | "upstream_catalog"
            | "upstream_tools"
            | "upstream_call"
            | "upstream_batch"
    )
}

fn minimal_management_tool(name: &str) -> bool {
    matches!(
        name,
        "adapter_profile"
            | "adapter_route"
            | "upstream_search"
            | "upstream_call"
            | "upstream_batch"
    )
}

pub fn tool_list_result(
    root_path: &Path,
    base_tools: Vec<JsonValue>,
    initialize_params: Option<&JsonValue>,
    cursor: Option<&str>,
) -> JsonValue {
    let surface_options = tool_surface_options_from_initialize(initialize_params);
    let base = base_tools
        .into_iter()
        .filter(|tool| {
            json_helpers::string_at_path(tool, &["name"])
                .map(should_keep_management_tool)
                .unwrap_or(false)
        })
        .map(|tool| shape_tool_for_client(tool, surface_options))
        .collect::<Vec<_>>();
    let tools = augment_tool_definitions(root_path, base)
        .into_iter()
        .map(|tool| shape_tool_for_client(tool, surface_options))
        .collect::<Vec<_>>();
    paginated_tool_list(tools, cursor)
}

pub fn augment_tool_definitions(root_path: &Path, base_tools: Vec<JsonValue>) -> Vec<JsonValue> {
    let options = ToolExposureOptions::from_env();
    if matches!(
        options.mode,
        ToolExposureMode::Broker | ToolExposureMode::Minimal
    ) {
        return base_tools;
    }

    let reserved = tool_names(&base_tools).into_iter().collect::<BTreeSet<_>>();
    match projected_tool_set(root_path, &reserved, &options) {
        Ok(projected) if projected.projection_enabled => {
            let mut tools = base_tools;
            tools.extend(projected.tools);
            tools
        }
        _ => base_tools,
    }
}

pub fn projected_tool_set(
    root_path: &Path,
    reserved_names: &BTreeSet<String>,
    options: &ToolExposureOptions,
) -> Result<ProjectedToolSet, String> {
    let catalog = projection_catalog(root_path, options)?;
    let mut projected_all = Vec::new();
    let mut used_names = reserved_names.clone();

    for server in json_helpers::array_at_path(&catalog, &["servers"]).unwrap_or(&[]) {
        if !json_helpers::bool_at_path(server, &["ok"]).unwrap_or(false) {
            continue;
        }
        let Some(server_name) = json_helpers::string_at_path(server, &["name"])
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        for tool in json_helpers::array_at_path(server, &["projectableTools"]).unwrap_or(&[]) {
            let Some(tool_name) = json_helpers::string_at_path(tool, &["name"])
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                continue;
            };
            let projected_name = unique_projected_name(
                PROJECTED_TOOL_PREFIX,
                server_name,
                tool_name,
                &mut used_names,
            );
            projected_all.push(projected_tool_definition(
                server_name,
                tool_name,
                &projected_name,
                tool,
            ));
        }
    }

    projected_all.sort_by(|left, right| {
        tool_projection_rank(left)
            .cmp(&tool_projection_rank(right))
            .then_with(|| {
                json_helpers::string_at_path(left, &["name"])
                    .unwrap_or("")
                    .cmp(json_helpers::string_at_path(right, &["name"]).unwrap_or(""))
            })
    });

    let projectable_total = projected_all.len();
    let raw_total = json_helpers::value_at_path(&catalog, &["rawToolCount"])
        .and_then(JsonValue::as_i64)
        .filter(|value| *value >= 0)
        .map(|value| value as usize)
        .unwrap_or(projectable_total);
    let broker_only_total = json_helpers::value_at_path(&catalog, &["brokerOnlyToolCount"])
        .and_then(JsonValue::as_i64)
        .filter(|value| *value >= 0)
        .map(|value| value as usize)
        .unwrap_or(raw_total.saturating_sub(projectable_total));
    let estimated_total_tokens: usize = projected_all.iter().map(estimate_json_tokens).sum();
    let full_catalog_fits =
        projectable_total <= options.budget && estimated_total_tokens <= options.token_budget;
    let projection_enabled = match options.mode {
        ToolExposureMode::Native | ToolExposureMode::Hybrid => projectable_total > 0,
        ToolExposureMode::Auto => projectable_total > 0 && full_catalog_fits,
        ToolExposureMode::Broker | ToolExposureMode::Minimal => false,
    };
    let projected = if projection_enabled {
        take_tools_with_budget(projected_all, options.budget, options.token_budget)
    } else {
        Vec::new()
    };
    let projected_tool_count = projected.len();
    let estimated_projected_tokens: usize = projected.iter().map(estimate_json_tokens).sum();
    let truncated = projection_enabled
        && (projected_tool_count < projectable_total
            || estimated_projected_tokens < estimated_total_tokens);

    Ok(ProjectedToolSet {
        tools: projected,
        raw_upstream_tool_count: raw_total,
        total_upstream_tool_count: projectable_total,
        broker_only_tool_count: broker_only_total,
        projected_tool_count,
        estimated_total_tokens,
        estimated_projected_tokens,
        truncated,
        projection_enabled,
        reason: projection_reason(
            options,
            projectable_total,
            raw_total,
            broker_only_total,
            estimated_total_tokens,
            projection_enabled,
            truncated,
        ),
        catalog,
    })
}

pub fn resolve_projected_tool(
    root_path: &Path,
    projected_name: &str,
    reserved_names: &BTreeSet<String>,
    options: &ToolExposureOptions,
) -> Result<Option<ProjectedToolTarget>, String> {
    if matches!(
        options.mode,
        ToolExposureMode::Broker | ToolExposureMode::Minimal
    ) {
        return Ok(None);
    }
    let projected = projected_tool_set(root_path, reserved_names, options)?;
    if !projected.projection_enabled {
        return Ok(None);
    }

    for tool in projected.tools {
        if json_helpers::string_at_path(&tool, &["name"]) != Some(projected_name) {
            continue;
        }
        let server =
            match json_helpers::string_at_path(&tool, &["_meta", "mcpace/upstream", "server"]) {
                Some(value) => value,
                None => {
                    return Err(format!(
                        "projected tool '{}' is missing upstream server metadata",
                        projected_name
                    ));
                }
            };
        let upstream_tool =
            match json_helpers::string_at_path(&tool, &["_meta", "mcpace/upstream", "tool"]) {
                Some(value) => value,
                None => {
                    return Err(format!(
                        "projected tool '{}' is missing upstream tool metadata",
                        projected_name
                    ));
                }
            };
        return Ok(Some(ProjectedToolTarget {
            server: server.to_string(),
            tool: upstream_tool.to_string(),
        }));
    }

    Ok(None)
}

#[derive(Clone, Debug)]
struct ProjectionDecision {
    projectable: bool,
    reason: String,
}

fn projection_catalog(
    root_path: &Path,
    options: &ToolExposureOptions,
) -> Result<JsonValue, String> {
    let server_names = upstream::callable_server_names(root_path)?;
    let mut servers = Vec::new();
    let mut errors = Vec::new();
    let mut raw_tool_count = 0usize;
    let mut projectable_tool_count = 0usize;
    let mut broker_only_tool_count = 0usize;

    for server_name in server_names {
        match upstream::list_tools(
            root_path,
            Some(&server_name),
            options.timeout_ms,
            options.refresh,
        ) {
            Ok(listing) => {
                let mut projectable_tools = Vec::new();
                let mut broker_only_tools = Vec::new();
                for tool in json_helpers::array_at_path(&listing, &["tools"]).unwrap_or(&[]) {
                    raw_tool_count = raw_tool_count.saturating_add(1);
                    let Some(tool_name) = json_helpers::string_at_path(tool, &["name"])
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                    else {
                        broker_only_tool_count = broker_only_tool_count.saturating_add(1);
                        broker_only_tools.push(JsonValue::object([
                            ("name", JsonValue::string("<unnamed>")),
                            ("reason", JsonValue::string("missing tool name")),
                        ]));
                        continue;
                    };
                    let policy = upstream::tool_policy_info(root_path, &server_name, tool_name)
                        .unwrap_or_else(|error| {
                            JsonValue::object([
                                ("guardRequired", JsonValue::bool(false)),
                                ("policyLookupError", JsonValue::string(error)),
                            ])
                        });
                    let decision = projection_decision(tool, &policy, options.projection_safety);
                    let decorated = decorate_projectable_tool(tool, &policy, &decision);
                    if decision.projectable {
                        projectable_tool_count = projectable_tool_count.saturating_add(1);
                        projectable_tools.push(decorated);
                    } else {
                        broker_only_tool_count = broker_only_tool_count.saturating_add(1);
                        broker_only_tools.push(compact_broker_only_tool(
                            &server_name,
                            tool_name,
                            tool,
                            &policy,
                            &decision.reason,
                        ));
                    }
                }
                servers.push(JsonValue::object([
                    ("name", JsonValue::string(&server_name)),
                    ("ok", JsonValue::bool(true)),
                    (
                        "rawToolCount",
                        JsonValue::number(
                            json_helpers::array_at_path(&listing, &["tools"])
                                .map(|items| items.len())
                                .unwrap_or(0),
                        ),
                    ),
                    (
                        "projectableToolCount",
                        JsonValue::number(projectable_tools.len()),
                    ),
                    (
                        "brokerOnlyToolCount",
                        JsonValue::number(broker_only_tools.len()),
                    ),
                    (
                        "cacheHit",
                        json_helpers::value_at_path(&listing, &["cacheHit"])
                            .cloned()
                            .unwrap_or(JsonValue::Null),
                    ),
                    ("projectableTools", JsonValue::array(projectable_tools)),
                    ("brokerOnlyTools", JsonValue::array(broker_only_tools)),
                ]));
            }
            Err(error) => {
                errors.push(JsonValue::object([
                    ("server", JsonValue::string(&server_name)),
                    ("method", JsonValue::string("tools/list")),
                    ("error", JsonValue::string(&error)),
                ]));
                servers.push(JsonValue::object([
                    ("name", JsonValue::string(server_name)),
                    ("ok", JsonValue::bool(false)),
                    ("rawToolCount", JsonValue::number(0)),
                    ("projectableToolCount", JsonValue::number(0)),
                    ("brokerOnlyToolCount", JsonValue::number(0)),
                    ("projectableTools", JsonValue::array([])),
                    ("brokerOnlyTools", JsonValue::array([])),
                    ("error", JsonValue::string(error)),
                ]));
            }
        }
    }

    Ok(JsonValue::object([
        ("ok", JsonValue::bool(errors.is_empty())),
        ("mode", JsonValue::string("projection-catalog")),
        (
            "summary",
            JsonValue::string("Discovered full upstream tool definitions for native projection, then kept policy-guarded or clearly mutating tools behind broker calls unless projection safety allows them."),
        ),
        ("projectionSafety", JsonValue::string(projection_safety_name(options.projection_safety))),
        ("serverCount", JsonValue::number(servers.len())),
        ("rawToolCount", JsonValue::number(raw_tool_count)),
        ("projectableToolCount", JsonValue::number(projectable_tool_count)),
        ("brokerOnlyToolCount", JsonValue::number(broker_only_tool_count)),
        ("servers", JsonValue::array(servers)),
        maybe_meta_errors(errors),
    ]))
}

fn projection_decision(
    tool: &JsonValue,
    policy: &JsonValue,
    safety: ProjectionSafety,
) -> ProjectionDecision {
    let guard_required = json_helpers::bool_at_path(policy, &["guardRequired"]).unwrap_or(false);
    let read_only =
        json_helpers::bool_at_path(tool, &["annotations", "readOnlyHint"]) == Some(true);
    let destructive =
        json_helpers::bool_at_path(tool, &["annotations", "destructiveHint"]) == Some(true);
    let tool_name = json_helpers::string_at_path(tool, &["name"]).unwrap_or("");
    let conservative_read_signal = conservative_read_only_name_signal(tool_name);
    let mutation_signal = mutating_name_signal(tool_name);

    match safety {
        ProjectionSafety::All => ProjectionDecision {
            projectable: true,
            reason: "projectionSafety=all".to_string(),
        },
        ProjectionSafety::Review => {
            if guard_required {
                ProjectionDecision {
                    projectable: false,
                    reason: "requires explicit MCPace policy authorization".to_string(),
                }
            } else if destructive || mutation_signal {
                ProjectionDecision {
                    projectable: false,
                    reason: "appears mutating or destructive; broker keeps explicit routing controls available".to_string(),
                }
            } else {
                ProjectionDecision {
                    projectable: true,
                    reason: "not policy-guarded and no destructive signal".to_string(),
                }
            }
        }
        ProjectionSafety::Safe => {
            if guard_required {
                ProjectionDecision {
                    projectable: false,
                    reason: "requires explicit MCPace policy authorization".to_string(),
                }
            } else if read_only || (conservative_read_signal && !destructive && !mutation_signal) {
                ProjectionDecision {
                    projectable: true,
                    reason: "read-only annotation or conservative read-only name signal"
                        .to_string(),
                }
            } else {
                ProjectionDecision {
                    projectable: false,
                    reason: "unknown or mutating semantics under projectionSafety=safe".to_string(),
                }
            }
        }
    }
}

fn decorate_projectable_tool(
    tool: &JsonValue,
    policy: &JsonValue,
    decision: &ProjectionDecision,
) -> JsonValue {
    let mut map = tool.as_object().cloned().unwrap_or_else(BTreeMap::new);
    let mut meta = map
        .get("_meta")
        .and_then(JsonValue::as_object)
        .cloned()
        .unwrap_or_else(BTreeMap::new);
    meta.insert(
        "mcpace/projectionDecision".to_string(),
        JsonValue::object([
            ("projectable", JsonValue::bool(decision.projectable)),
            ("reason", JsonValue::string(&decision.reason)),
        ]),
    );
    meta.insert("mcpace/policy".to_string(), policy.clone());
    map.insert("_meta".to_string(), JsonValue::Object(meta));
    JsonValue::Object(map)
}

fn compact_broker_only_tool(
    server_name: &str,
    tool_name: &str,
    tool: &JsonValue,
    policy: &JsonValue,
    reason: &str,
) -> JsonValue {
    JsonValue::object([
        ("server", JsonValue::string(server_name)),
        ("name", JsonValue::string(tool_name)),
        (
            "title",
            json_helpers::value_at_path(tool, &["title"])
                .cloned()
                .unwrap_or(JsonValue::Null),
        ),
        (
            "description",
            JsonValue::string(truncate_chars(
                json_helpers::string_at_path(tool, &["description"]).unwrap_or(""),
                220,
            )),
        ),
        ("reason", JsonValue::string(reason)),
        (
            "guardRequired",
            JsonValue::bool(
                json_helpers::bool_at_path(policy, &["guardRequired"]).unwrap_or(false),
            ),
        ),
        (
            "call",
            JsonValue::object([
                ("tool", JsonValue::string("upstream_call")),
                (
                    "arguments",
                    JsonValue::object([
                        ("server", JsonValue::string(server_name)),
                        ("tool", JsonValue::string(tool_name)),
                    ]),
                ),
            ]),
        ),
    ])
}

fn conservative_read_only_name_signal(tool_name: &str) -> bool {
    let normalized = normalize(tool_name);
    let mut terms = normalized.split_whitespace();
    let Some(first) = terms.next() else {
        return false;
    };
    matches!(
        first,
        "get"
            | "list"
            | "read"
            | "search"
            | "find"
            | "query"
            | "fetch"
            | "status"
            | "describe"
            | "inspect"
            | "observe"
            | "snapshot"
    )
}

fn mutating_name_signal(tool_name: &str) -> bool {
    let normalized = normalize(tool_name);
    normalized.split_whitespace().any(|term| {
        matches!(
            term,
            "create"
                | "update"
                | "delete"
                | "remove"
                | "write"
                | "save"
                | "edit"
                | "patch"
                | "put"
                | "post"
                | "execute"
                | "exec"
                | "run"
                | "start"
                | "stop"
                | "restart"
                | "kill"
                | "click"
                | "type"
                | "press"
                | "navigate"
                | "drag"
                | "upload"
                | "submit"
        )
    })
}

pub fn strip_projected_adapter_arguments(arguments: &JsonValue) -> JsonValue {
    let JsonValue::Object(map) = arguments else {
        return arguments.clone();
    };
    let mut cleaned = BTreeMap::new();
    for (key, value) in map {
        if key == "_mcpace" || key == "mcpace" {
            continue;
        }
        if legacy_projected_top_level_controls_enabled() && projected_adapter_argument_key(key) {
            continue;
        }
        cleaned.insert(key.clone(), value.clone());
    }
    JsonValue::Object(cleaned)
}

pub fn projected_adapter_control_arguments(arguments: &JsonValue) -> JsonValue {
    let mut controls = BTreeMap::new();
    merge_control_object(
        &mut controls,
        json_helpers::value_at_path(arguments, &["_mcpace"]),
    );
    merge_control_object(
        &mut controls,
        json_helpers::value_at_path(arguments, &["mcpace"]),
    );
    if legacy_projected_top_level_controls_enabled() {
        if let Some(object) = arguments.as_object() {
            for (key, value) in object {
                if projected_adapter_argument_key(key) {
                    controls.insert(key.clone(), value.clone());
                }
            }
        }
    }
    JsonValue::Object(controls)
}

fn merge_control_object(target: &mut BTreeMap<String, JsonValue>, value: Option<&JsonValue>) {
    let Some(object) = value.and_then(JsonValue::as_object) else {
        return;
    };
    for (key, value) in object {
        target.insert(key.clone(), value.clone());
    }
}

fn legacy_projected_top_level_controls_enabled() -> bool {
    env_bool("MCPACE_PROJECTED_LEGACY_TOP_LEVEL_CONTROLS").unwrap_or(false)
}

fn projected_adapter_argument_key(key: &str) -> bool {
    matches!(
        key,
        "clientId"
            | "sessionId"
            | "projectRoot"
            | "transport"
            | "ttlMs"
            | "metadata"
            | "timeoutMs"
            | "resultMode"
            | "toolResultMode"
            | "diagnostics"
            | "upstreamDiagnostics"
            | "nestedContent"
            | "upstreamNestedContent"
            | "tokenReducerPlugins"
            | "resultPlugins"
            | "tokenReducerPluginPolicy"
            | "pluginPolicy"
            | "resultPluginPolicy"
            | "allowArguments"
            | "allowToolRiskClasses"
    )
}

pub fn adapter_profile(
    root_path: &Path,
    initialize_params: Option<&JsonValue>,
    transport: &str,
    management_tool_names: &[String],
    include_live_catalog: bool,
    timeout_ms: Option<u64>,
    refresh: bool,
) -> Result<JsonValue, String> {
    let env_options = ToolExposureOptions::from_env();
    let options = ToolExposureOptions {
        timeout_ms: timeout_ms.or(env_options.timeout_ms),
        refresh: refresh || env_options.refresh,
        ..env_options
    };
    let reserved = management_tool_names
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let projection = projected_tool_set(root_path, &reserved, &options).ok();
    let inventory = upstream::configured_inventory(root_path)?;
    let live_catalog = if include_live_catalog {
        Some(upstream::catalog_tools(
            root_path,
            None,
            options.timeout_ms,
            options.refresh,
        )?)
    } else {
        None
    };

    Ok(JsonValue::object([
        ("ok", JsonValue::bool(true)),
        ("mode", JsonValue::string("dynamic-adapter-profile")),
        (
            "summary",
            JsonValue::string("Capability-inferred adapter profile. MCPace does not depend on hardcoded client/server maps: it derives the client side from initialize, derives upstream tools/resources/prompts from live MCP methods, and uses budgeted native projection with broker fallback."),
        ),
        ("client", client_profile_from_initialize(initialize_params)),
        ("serverCapabilitiesAdvertised", adapter_capabilities()),
        ("transport", JsonValue::string(transport)),
        (
            "toolExposure",
            JsonValue::object([
                ("mode", JsonValue::string(tool_exposure_mode_name(options.mode))),
                (
                    "managementSurface",
                    JsonValue::string(management_surface_mode_name(management_surface_mode_from_env())),
                ),
                ("countBudget", JsonValue::number(options.budget)),
                ("tokenBudget", JsonValue::number(options.token_budget)),
                (
                    "toolsListTimeoutMs",
                    options
                        .timeout_ms
                        .map(JsonValue::number)
                        .unwrap_or(JsonValue::Null),
                ),
                ("refresh", JsonValue::bool(options.refresh)),
                (
                    "projectionSafety",
                    JsonValue::string(projection_safety_name(options.projection_safety)),
                ),
                (
                    "managementToolCount",
                    JsonValue::number(management_tool_names.len()),
                ),
                (
                    "rawUpstreamToolCount",
                    projection
                        .as_ref()
                        .map(|value| JsonValue::number(value.raw_upstream_tool_count))
                        .unwrap_or(JsonValue::Null),
                ),
                (
                    "projectedToolCount",
                    projection
                        .as_ref()
                        .map(|value| JsonValue::number(value.projected_tool_count))
                        .unwrap_or(JsonValue::Null),
                ),
                (
                    "projectableUpstreamToolCount",
                    projection
                        .as_ref()
                        .map(|value| JsonValue::number(value.total_upstream_tool_count))
                        .unwrap_or(JsonValue::Null),
                ),
                (
                    "brokerOnlyToolCount",
                    projection
                        .as_ref()
                        .map(|value| JsonValue::number(value.broker_only_tool_count))
                        .unwrap_or(JsonValue::Null),
                ),
                (
                    "estimatedTotalTokens",
                    projection
                        .as_ref()
                        .map(|value| JsonValue::number(value.estimated_total_tokens))
                        .unwrap_or(JsonValue::Null),
                ),
                (
                    "estimatedProjectedTokens",
                    projection
                        .as_ref()
                        .map(|value| JsonValue::number(value.estimated_projected_tokens))
                        .unwrap_or(JsonValue::Null),
                ),
                (
                    "projectionEnabled",
                    projection
                        .as_ref()
                        .map(|value| JsonValue::bool(value.projection_enabled))
                        .unwrap_or(JsonValue::bool(false)),
                ),
                (
                    "truncated",
                    projection
                        .as_ref()
                        .map(|value| JsonValue::bool(value.truncated))
                        .unwrap_or(JsonValue::bool(false)),
                ),
                (
                    "reason",
                    projection
                        .as_ref()
                        .map(|value| JsonValue::string(value.reason.clone()))
                        .unwrap_or_else(|| JsonValue::string("live upstream catalog unavailable; broker tools remain available")),
                ),
            ]),
        ),
        (
            "routing",
            JsonValue::object([
                (
                    "nativeProjectionPrefix",
                    JsonValue::string("u_<server>_<tool>_<hash>"),
                ),
                (
                    "brokerSearchTool",
                    JsonValue::string("upstream_search"),
                ),
                ("brokerCallTool", JsonValue::string("upstream_call")),
                ("brokerBatchTool", JsonValue::string("upstream_batch")),
                (
                    "nameCollisionStrategy",
                    JsonValue::string("stable u_<server>_<tool>_<hash> names with original server/tool stored in _meta; broker fallback is used when names exceed the budget"),
                ),
                (
                    "projectedToolAdapterControls",
                    JsonValue::string("Projected tools pass upstream arguments through unchanged. Optional MCPace controls belong in a nested _mcpace object, not as top-level upstream arguments."),
                ),
            ]),
        ),
        (
            "concurrency",
            JsonValue::object([
                (
                    "defaultUpstreamModel",
                    JsonValue::string("lease-gated stdio sessions with pooled reuse per client/session/project context"),
                ),
                (
                    "parallelSafetySource",
                    JsonValue::string("MCP has advisory tool annotations, but no universal parallel-safety contract across every server; MCPace therefore treats upstream concurrency as a runtime policy/lease decision rather than a hardcoded client map."),
                ),
                ("statefulBatchTool", JsonValue::string("upstream_batch")),
                ("singleToolFallback", JsonValue::string("upstream_call")),
            ]),
        ),
        (
            "pluginHooks",
            JsonValue::object([
                (
                    "tokenReducers",
                    JsonValue::array([
                        JsonValue::string("mcpace.native-content.v1"),
                        JsonValue::string("mcpace.drop-upstream-diagnostics.v1"),
                        JsonValue::string("mcpace.dedupe-nested-upstream-content.v1"),
                        JsonValue::string("mcpace.schema-compact.v1"),
                        JsonValue::string("mcpace.catalog-budget.v1"),
                    ]),
                ),
                (
                    "futureExternalPlugins",
                    JsonValue::array([
                        JsonValue::string("tool-catalog-ranker"),
                        JsonValue::string("schema-compactor"),
                        JsonValue::string("resource-link-store"),
                        JsonValue::string("client-capability-detector"),
                        JsonValue::string("upstream-parallelism-policy"),
                    ]),
                ),
            ]),
        ),
        ("upstreamInventory", inventory),
        ("liveCatalog", live_catalog.unwrap_or(JsonValue::Null)),
    ]))
}

pub fn upstream_search(
    root_path: &Path,
    server_name: Option<&str>,
    query: Option<&str>,
    limit: usize,
    include_schema: bool,
    timeout_ms: Option<u64>,
    refresh: bool,
) -> Result<JsonValue, String> {
    let limit = limit.clamp(1, 100);
    let catalog = upstream::catalog_tools(root_path, server_name, timeout_ms, refresh)?;
    let tools = json_helpers::array_at_path(&catalog, &["tools"]).unwrap_or(&[]);
    let terms = search_terms(query.unwrap_or(""));
    let mut scored = Vec::new();

    for tool in tools {
        let score = score_tool(tool, &terms);
        if terms.is_empty() || score > 0 {
            let key = format!(
                "{}:{}",
                json_helpers::string_at_path(tool, &["server"]).unwrap_or(""),
                json_helpers::string_at_path(tool, &["name"]).unwrap_or("")
            );
            scored.push((score, key, compact_search_tool(tool, score, include_schema)));
        }
    }

    scored.sort_by_key(|(score, key, _)| (std::cmp::Reverse(*score), key.clone()));
    let total_matches = scored.len();
    let selected = selected_search_result(&scored, !terms.is_empty());
    let results = scored
        .into_iter()
        .take(limit)
        .map(|(_, _, tool)| tool)
        .collect::<Vec<_>>();

    Ok(JsonValue::object([
        ("ok", JsonValue::bool(true)),
        ("mode", JsonValue::string("upstream-search")),
        (
            "summary",
            JsonValue::string("Searched live configured upstream MCP tool catalogs and returned compact ready-to-call results. Use each call object with upstream_call, or use a projected u_<server>_<tool>_<hash> name when it appears in tools/list."),
        ),
        (
            "query",
            query
                .map(|value| JsonValue::string(value.trim()))
                .unwrap_or(JsonValue::Null),
        ),
        (
            "server",
            server_name
                .map(|value| JsonValue::string(value.trim()))
                .unwrap_or(JsonValue::Null),
        ),
        ("limit", JsonValue::number(limit)),
        ("searchSpaceToolCount", JsonValue::number(tools.len())),
        ("matchCount", JsonValue::number(total_matches)),
        ("resultCount", JsonValue::number(results.len())),
        ("includeSchema", JsonValue::bool(include_schema)),
        ("selected", selected),
        ("results", JsonValue::array(results)),
    ]))
}

#[derive(Clone, Debug)]
struct PlannedUpstreamCall {
    server: String,
    tool: String,
    arguments: JsonValue,
    source: String,
}

pub fn adapter_route_plan(
    root_path: &Path,
    calls: Option<&JsonValue>,
    include_live_catalog: bool,
    timeout_ms: Option<u64>,
    refresh: bool,
) -> Result<JsonValue, String> {
    let records = load_server_records(root_path)?;
    let planned_calls = match calls {
        Some(value) => parse_planned_calls(value)?,
        None => Vec::new(),
    };
    let mut record_by_name = BTreeMap::new();
    for record in &records {
        record_by_name.insert(record.name.to_ascii_lowercase(), record);
    }

    let mut warnings = Vec::new();
    let mut calls_by_server: BTreeMap<String, Vec<PlannedUpstreamCall>> = BTreeMap::new();
    for call in planned_calls {
        let key = call.server.trim().to_ascii_lowercase();
        if key.is_empty() {
            warnings.push(format!(
                "call '{}' has no server; use upstream_search first or provide server/tool explicitly",
                call.tool
            ));
            continue;
        }
        if !record_by_name.contains_key(&key) {
            warnings.push(format!(
                "server '{}' is not declared in mcpace.config.json; MCPace can still try mcp_settings.json routing, but topology policy is unknown",
                call.server
            ));
        }
        calls_by_server.entry(key).or_default().push(call);
    }

    let mut lanes = Vec::new();
    let mut conflict_groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (server_key, server_calls) in &calls_by_server {
        let record = record_by_name.get(server_key).copied();
        let conflict_domain = record
            .map(|record| non_empty_or(&record.conflict_domain, &record.name))
            .unwrap_or_else(|| server_key.clone());
        let lane = execution_lane(record, server_key, server_calls);
        let lane_server = json_helpers::string_at_path(&lane, &["server"])
            .unwrap_or(server_key)
            .to_string();
        conflict_groups
            .entry(conflict_domain)
            .or_default()
            .push(lane_server);
        lanes.push(lane);
    }

    let topology = records
        .iter()
        .map(server_topology_item)
        .collect::<Vec<JsonValue>>();
    let live_catalog = if include_live_catalog {
        Some(upstream::catalog_tools(
            root_path, None, timeout_ms, refresh,
        )?)
    } else {
        None
    };

    Ok(JsonValue::object([
        ("ok", JsonValue::bool(true)),
        ("mode", JsonValue::string("adapter-route-plan")),
        (
            "summary",
            JsonValue::string("Dynamic routing plan built from configured server policy plus optional live MCP discovery. Same-server calls are batched; calls sharing a conflict domain are serialized; independent conflict domains can be scheduled concurrently by a client that supports parallel tool calls."),
        ),
        (
            "executionModel",
            JsonValue::object([
                ("batchSameServerCalls", JsonValue::bool(true)),
                ("serializeWithinConflictDomain", JsonValue::bool(true)),
                ("parallelizeDifferentConflictDomains", JsonValue::bool(true)),
                (
                    "parallelismSource",
                    JsonValue::string("mcpace.config.json server policy, live tools/list annotations, and runtime lease arbitration; no hardcoded client/server maps"),
                ),
                (
                    "statefulTool",
                    JsonValue::string("upstream_batch for ordered calls that must share one upstream initialize/session"),
                ),
                (
                    "statelessTool",
                    JsonValue::string("upstream_call for one call or for clients that cannot expose projected tools"),
                ),
            ]),
        ),
        ("callCount", JsonValue::number(calls_by_server.values().map(Vec::len).sum::<usize>())),
        ("laneCount", JsonValue::number(lanes.len())),
        ("lanes", JsonValue::array(lanes)),
        ("conflictGroups", conflict_groups_json(conflict_groups)),
        ("serverTopology", JsonValue::array(topology)),
        ("warnings", JsonValue::array(warnings.into_iter().map(JsonValue::string))),
        ("liveCatalog", live_catalog.unwrap_or(JsonValue::Null)),
    ]))
}

fn parse_planned_calls(value: &JsonValue) -> Result<Vec<PlannedUpstreamCall>, String> {
    let Some(items) = value.as_array() else {
        return Err("adapter_route.calls must be an array".to_string());
    };
    let mut calls = Vec::new();
    for (index, item) in items.iter().enumerate() {
        calls.push(parse_planned_call(item, index)?);
    }
    Ok(calls)
}

fn parse_planned_call(value: &JsonValue, index: usize) -> Result<PlannedUpstreamCall, String> {
    if let Some(items) = value.as_array() {
        if items.len() < 2 || items.len() > 3 {
            return Err(format!(
                "adapter_route.calls[{}] tuple form must be [server, tool] or [server, tool, arguments]",
                index
            ));
        }
        let server = non_empty_array_string(items, 0, index, "server")?;
        let tool = non_empty_array_string(items, 1, index, "tool")?;
        let arguments = items.get(2).cloned().unwrap_or_else(mcp::empty_object);
        return Ok(PlannedUpstreamCall {
            server,
            tool,
            arguments,
            source: "tuple".to_string(),
        });
    }

    if let Some(text) = value.as_str() {
        if let Some((server, tool)) = text.split_once(':').or_else(|| text.split_once('.')) {
            return Ok(PlannedUpstreamCall {
                server: server.trim().to_string(),
                tool: tool.trim().to_string(),
                arguments: mcp::empty_object(),
                source: "qualified-string".to_string(),
            });
        }
        return Err(format!(
            "adapter_route.calls[{}] string must look like server:tool or server.tool",
            index
        ));
    }

    let wrapper_tool = json_helpers::string_at_path(value, &["tool"]);
    if wrapper_tool == Some("upstream_call") {
        return parse_wrapped_upstream_call(value, index, &["arguments"], "upstream-call-object");
    }
    if wrapper_tool == Some("upstream_batch") {
        return Err(format!(
            "adapter_route.calls[{}] is an upstream_batch; pass each inner call separately or omit calls to inspect topology",
            index
        ));
    }
    if json_helpers::string_at_path(value, &["call", "tool"]) == Some("upstream_call") {
        return parse_wrapped_upstream_call(
            value,
            index,
            &["call", "arguments"],
            "upstream-search-result",
        );
    }

    let server = json_helpers::string_at_path(value, &["server"])
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let tool = json_helpers::string_at_path(value, &["name"])
        .or_else(|| json_helpers::string_at_path(value, &["toolName"]))
        .or_else(|| json_helpers::string_at_path(value, &["tool"]))
        .map(str::trim)
        .filter(|value| !value.is_empty());
    match (server, tool) {
        (Some(server), Some(tool)) => Ok(PlannedUpstreamCall {
            server: server.to_string(),
            tool: tool.to_string(),
            arguments: json_helpers::value_at_path(value, &["arguments"])
                .cloned()
                .unwrap_or_else(mcp::empty_object),
            source: "server-tool-object".to_string(),
        }),
        _ => Err(format!(
            "adapter_route.calls[{}] must include server/tool, an upstream_search result, an upstream_call object, or [server, tool]",
            index
        )),
    }
}

fn parse_wrapped_upstream_call(
    value: &JsonValue,
    index: usize,
    path: &[&str],
    source: &str,
) -> Result<PlannedUpstreamCall, String> {
    let Some(args) = json_helpers::value_at_path(value, path) else {
        return Err(format!(
            "adapter_route.calls[{}] upstream_call object is missing arguments",
            index
        ));
    };
    let server = json_helpers::string_at_path(args, &["server"])
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            format!(
                "adapter_route.calls[{}] upstream_call is missing server",
                index
            )
        })?;
    let tool = json_helpers::string_at_path(args, &["tool"])
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            format!(
                "adapter_route.calls[{}] upstream_call is missing tool",
                index
            )
        })?;
    Ok(PlannedUpstreamCall {
        server: server.to_string(),
        tool: tool.to_string(),
        arguments: json_helpers::value_at_path(args, &["arguments"])
            .cloned()
            .unwrap_or_else(mcp::empty_object),
        source: source.to_string(),
    })
}

fn non_empty_array_string(
    items: &[JsonValue],
    array_index: usize,
    call_index: usize,
    label: &str,
) -> Result<String, String> {
    items
        .get(array_index)
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| {
            format!(
                "adapter_route.calls[{}][{}] must be a non-empty {} string",
                call_index, array_index, label
            )
        })
}

fn execution_lane(
    record: Option<&ServerRecord>,
    fallback_server: &str,
    calls: &[PlannedUpstreamCall],
) -> JsonValue {
    let server = record
        .map(|record| record.name.clone())
        .or_else(|| calls.first().map(|call| call.server.clone()))
        .unwrap_or_else(|| fallback_server.to_string());
    let conflict_domain = record
        .map(|record| non_empty_or(&record.conflict_domain, &record.name))
        .unwrap_or_else(|| server.clone());
    let batch_recommended = calls.len() > 1 || record.map(server_is_stateful).unwrap_or(true);
    let recommended_tool = if calls.len() > 1 {
        "upstream_batch"
    } else {
        "upstream_call"
    };
    let call_object = if calls.len() > 1 {
        JsonValue::object([
            ("tool", JsonValue::string("upstream_batch")),
            (
                "arguments",
                JsonValue::object([
                    ("server", JsonValue::string(&server)),
                    (
                        "calls",
                        JsonValue::array(calls.iter().map(planned_call_tuple)),
                    ),
                ]),
            ),
        ])
    } else if let Some(call) = calls.first() {
        JsonValue::object([
            ("tool", JsonValue::string("upstream_call")),
            (
                "arguments",
                JsonValue::object([
                    ("server", JsonValue::string(&server)),
                    ("tool", JsonValue::string(&call.tool)),
                    ("arguments", call.arguments.clone()),
                ]),
            ),
        ])
    } else {
        JsonValue::Null
    };

    JsonValue::object([
        ("server", JsonValue::string(&server)),
        ("callCount", JsonValue::number(calls.len())),
        (
            "tools",
            JsonValue::array(calls.iter().map(|call| {
                JsonValue::object([
                    ("name", JsonValue::string(&call.tool)),
                    ("source", JsonValue::string(&call.source)),
                ])
            })),
        ),
        ("recommendedTool", JsonValue::string(recommended_tool)),
        ("batchRecommended", JsonValue::bool(batch_recommended)),
        ("call", call_object),
        ("conflictDomain", JsonValue::string(conflict_domain)),
        (
            "routingGroup",
            JsonValue::string(
                record
                    .map(|record| non_empty_or(&record.routing_group, &record.name))
                    .unwrap_or_else(|| server.clone()),
            ),
        ),
        (
            "concurrencyPolicy",
            JsonValue::string(
                record
                    .map(|record| record.concurrency_policy.clone())
                    .unwrap_or_else(|| "unknown".to_string()),
            ),
        ),
        (
            "parallelismLimit",
            JsonValue::number(record.map(|record| record.parallelism_limit).unwrap_or(1)),
        ),
        (
            "stateBinding",
            JsonValue::string(
                record
                    .map(|record| record.state_binding.clone())
                    .unwrap_or_else(|| "unknown".to_string()),
            ),
        ),
        (
            "serializeWithinLane",
            JsonValue::bool(record.map(server_requires_serialization).unwrap_or(true)),
        ),
    ])
}

fn planned_call_tuple(call: &PlannedUpstreamCall) -> JsonValue {
    JsonValue::array([JsonValue::string(&call.tool), call.arguments.clone()])
}

fn server_topology_item(record: &ServerRecord) -> JsonValue {
    JsonValue::object([
        ("name", JsonValue::string(&record.name)),
        (
            "effectiveEnabled",
            JsonValue::bool(record.effective_enabled),
        ),
        ("sourceEnabled", JsonValue::bool(record.source_enabled)),
        ("sourceType", JsonValue::string(&record.source_type)),
        (
            "transportPreference",
            JsonValue::string(&record.transport_preference),
        ),
        (
            "routingGroup",
            JsonValue::string(non_empty_or(&record.routing_group, &record.name)),
        ),
        (
            "conflictDomain",
            JsonValue::string(non_empty_or(&record.conflict_domain, &record.name)),
        ),
        (
            "concurrencyPolicy",
            JsonValue::string(&record.concurrency_policy),
        ),
        (
            "parallelismLimit",
            JsonValue::number(record.parallelism_limit),
        ),
        ("stateBinding", JsonValue::string(&record.state_binding)),
        ("scopeClass", JsonValue::string(&record.scope_class)),
        ("hostLock", JsonValue::string(&record.host_lock)),
        (
            "batchRecommended",
            JsonValue::bool(server_is_stateful(record)),
        ),
        (
            "serializeByDefault",
            JsonValue::bool(server_requires_serialization(record)),
        ),
    ])
}

fn conflict_groups_json(groups: BTreeMap<String, Vec<String>>) -> JsonValue {
    JsonValue::array(groups.into_iter().map(|(domain, servers)| {
        JsonValue::object([
            ("conflictDomain", JsonValue::string(domain)),
            ("servers", JsonValue::array(servers.into_iter().map(JsonValue::string))),
            (
                "guidance",
                JsonValue::string("Run at most one lane in this conflict domain at a time unless an explicit server policy raises parallelismLimit."),
            ),
        ])
    }))
}

fn server_is_stateful(record: &ServerRecord) -> bool {
    !matches!(record.state_binding.as_str(), "" | "none" | "stateless")
        || matches!(
            record.concurrency_policy.as_str(),
            "single-session" | "single-writer"
        )
        || record.parallelism_limit <= 1
}

fn server_requires_serialization(record: &ServerRecord) -> bool {
    let policy = record.concurrency_policy.to_ascii_lowercase();
    record.parallelism_limit <= 1
        || policy.contains("single")
        || policy.contains("exclusive")
        || policy.contains("writer")
}

fn non_empty_or(value: &str, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

pub fn list_prompts(root_path: &Path, timeout_ms: Option<u64>, cursor: Option<&str>) -> JsonValue {
    let mut prompts = Vec::new();
    let mut errors = Vec::new();
    let mut used_names = BTreeSet::new();
    for server in callable_server_names(root_path, &mut errors) {
        match upstream::request_once(root_path, &server, "prompts/list", None, timeout_ms) {
            Ok(result) => {
                for prompt in json_helpers::array_at_path(&result, &["prompts"]).unwrap_or(&[]) {
                    let Some(prompt_name) = json_helpers::string_at_path(prompt, &["name"])
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                    else {
                        continue;
                    };
                    let projected = unique_projected_name(
                        PROJECTED_PROMPT_PREFIX,
                        &server,
                        prompt_name,
                        &mut used_names,
                    );
                    prompts.push(projected_prompt_definition(
                        &server,
                        prompt_name,
                        &projected,
                        prompt,
                    ));
                }
            }
            Err(error) if is_unsupported_method_error(&error) => {}
            Err(error) => errors.push(JsonValue::object([
                ("server", JsonValue::string(&server)),
                ("method", JsonValue::string("prompts/list")),
                ("error", JsonValue::string(error)),
            ])),
        }
    }

    paginated_list("prompts", prompts, cursor, vec![maybe_meta_errors(errors)])
}

pub fn get_prompt(
    root_path: &Path,
    projected_name: &str,
    arguments: JsonValue,
    timeout_ms: Option<u64>,
) -> Result<JsonValue, String> {
    let target = resolve_projected_prompt(root_path, projected_name, timeout_ms)?
        .ok_or_else(|| format!("unknown prompt '{}'", projected_name))?;
    upstream::request_once(
        root_path,
        &target.server,
        "prompts/get",
        Some(JsonValue::object([
            ("name", JsonValue::string(target.prompt)),
            ("arguments", arguments),
        ])),
        timeout_ms,
    )
}

pub fn list_resources(
    root_path: &Path,
    timeout_ms: Option<u64>,
    cursor: Option<&str>,
) -> JsonValue {
    let mut resources = Vec::new();
    let mut errors = Vec::new();
    for server in callable_server_names(root_path, &mut errors) {
        match upstream::request_once(root_path, &server, "resources/list", None, timeout_ms) {
            Ok(result) => {
                for resource in json_helpers::array_at_path(&result, &["resources"]).unwrap_or(&[])
                {
                    let Some(uri) = json_helpers::string_at_path(resource, &["uri"])
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                    else {
                        continue;
                    };
                    resources.push(projected_resource_definition(&server, uri, resource));
                }
            }
            Err(error) if is_unsupported_method_error(&error) => {}
            Err(error) => errors.push(JsonValue::object([
                ("server", JsonValue::string(&server)),
                ("method", JsonValue::string("resources/list")),
                ("error", JsonValue::string(error)),
            ])),
        }
    }

    paginated_list(
        "resources",
        resources,
        cursor,
        vec![maybe_meta_errors(errors)],
    )
}

pub fn list_resource_templates(
    root_path: &Path,
    timeout_ms: Option<u64>,
    cursor: Option<&str>,
) -> JsonValue {
    let mut templates = Vec::new();
    let mut errors = Vec::new();
    for server in callable_server_names(root_path, &mut errors) {
        match upstream::request_once(
            root_path,
            &server,
            "resources/templates/list",
            None,
            timeout_ms,
        ) {
            Ok(result) => {
                for template in
                    json_helpers::array_at_path(&result, &["resourceTemplates"]).unwrap_or(&[])
                {
                    templates.push(projected_resource_template_definition(&server, template));
                }
            }
            Err(error) if is_unsupported_method_error(&error) => {}
            Err(error) => errors.push(JsonValue::object([
                ("server", JsonValue::string(&server)),
                ("method", JsonValue::string("resources/templates/list")),
                ("error", JsonValue::string(error)),
            ])),
        }
    }

    paginated_list(
        "resourceTemplates",
        templates,
        cursor,
        vec![maybe_meta_errors(errors)],
    )
}

pub fn read_resource(
    root_path: &Path,
    proxied_uri: &str,
    timeout_ms: Option<u64>,
) -> Result<JsonValue, String> {
    let (server, upstream_uri) = decode_resource_uri(proxied_uri)?;
    let result = upstream::request_once(
        root_path,
        &server,
        "resources/read",
        Some(JsonValue::object([(
            "uri",
            JsonValue::string(&upstream_uri),
        )])),
        timeout_ms,
    )?;
    Ok(rewrite_resource_contents(&server, result))
}

fn callable_server_names(root_path: &Path, errors: &mut Vec<JsonValue>) -> Vec<String> {
    match upstream::callable_server_names(root_path) {
        Ok(names) => names,
        Err(error) => {
            errors.push(JsonValue::object([
                ("server", JsonValue::string("*")),
                ("error", JsonValue::string(error)),
            ]));
            Vec::new()
        }
    }
}

fn resolve_projected_prompt(
    root_path: &Path,
    projected_name: &str,
    timeout_ms: Option<u64>,
) -> Result<Option<ProjectedPromptTarget>, String> {
    let mut errors = Vec::new();
    let mut used_names = BTreeSet::new();
    for server in callable_server_names(root_path, &mut errors) {
        let result =
            match upstream::request_once(root_path, &server, "prompts/list", None, timeout_ms) {
                Ok(result) => result,
                Err(error) if is_unsupported_method_error(&error) => continue,
                Err(_) => continue,
            };
        for prompt in json_helpers::array_at_path(&result, &["prompts"]).unwrap_or(&[]) {
            let Some(prompt_name) = json_helpers::string_at_path(prompt, &["name"])
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                continue;
            };
            let name = unique_projected_name(
                PROJECTED_PROMPT_PREFIX,
                &server,
                prompt_name,
                &mut used_names,
            );
            if name == projected_name {
                return Ok(Some(ProjectedPromptTarget {
                    server,
                    prompt: prompt_name.to_string(),
                }));
            }
        }
    }
    Ok(None)
}

fn shape_tool_for_client(tool: JsonValue, options: ToolSurfaceOptions) -> JsonValue {
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

fn paginated_tool_list(tools: Vec<JsonValue>, cursor: Option<&str>) -> JsonValue {
    paginated_list("tools", tools, cursor, Vec::new())
}

fn paginated_list(
    field: &'static str,
    items: Vec<JsonValue>,
    cursor: Option<&str>,
    mut extra_entries: Vec<(&'static str, JsonValue)>,
) -> JsonValue {
    let page_size_env = match field {
        "tools" => "MCPACE_TOOL_PAGE_SIZE",
        "prompts" => "MCPACE_PROMPT_PAGE_SIZE",
        "resources" => "MCPACE_RESOURCE_PAGE_SIZE",
        "resourceTemplates" => "MCPACE_RESOURCE_TEMPLATE_PAGE_SIZE",
        _ => "MCPACE_PAGE_SIZE",
    };
    let page_size = env_usize(page_size_env).or_else(|| env_usize("MCPACE_PAGE_SIZE"));
    let Some(page_size) = page_size else {
        let mut entries = vec![(field, JsonValue::array(items))];
        entries.append(&mut extra_entries);
        return JsonValue::object(entries);
    };
    let page_size = page_size.clamp(1, 512);
    let start = parse_cursor(cursor).unwrap_or(0).min(items.len());
    let end = start.saturating_add(page_size).min(items.len());
    let mut entries = vec![(field, JsonValue::array(items[start..end].to_vec()))];
    if end < items.len() {
        entries.push(("nextCursor", JsonValue::string(format!("offset:{}", end))));
    }
    entries.append(&mut extra_entries);
    JsonValue::object(entries)
}

fn parse_cursor(cursor: Option<&str>) -> Option<usize> {
    let raw = cursor?.trim();
    if raw.is_empty() {
        return Some(0);
    }
    raw.strip_prefix("offset:")
        .unwrap_or(raw)
        .parse::<usize>()
        .ok()
}

fn tool_names(tools: &[JsonValue]) -> Vec<String> {
    tools
        .iter()
        .filter_map(|tool| json_helpers::string_at_path(tool, &["name"]))
        .map(ToOwned::to_owned)
        .collect()
}

fn projected_tool_definition(
    server_name: &str,
    tool_name: &str,
    projected_name: &str,
    tool: &JsonValue,
) -> JsonValue {
    let description = json_helpers::string_at_path(tool, &["description"]).unwrap_or("");
    let mut map = tool.as_object().cloned().unwrap_or_else(BTreeMap::new);
    map.insert("name".to_string(), JsonValue::string(projected_name));
    map.insert(
        "description".to_string(),
        JsonValue::string(prefixed_description(server_name, tool_name, description)),
    );
    map.entry("title".to_string())
        .or_insert_with(|| JsonValue::string(format!("{} / {}", server_name, tool_name)));
    map.insert(
        "inputSchema".to_string(),
        json_helpers::value_at_path(tool, &["inputSchema"])
            .cloned()
            .map(projected_schema)
            .unwrap_or_else(|| JsonValue::object([("type", JsonValue::string("object"))])),
    );
    if let Some(output_schema) = json_helpers::value_at_path(tool, &["outputSchema"]) {
        map.insert(
            "outputSchema".to_string(),
            projected_schema(output_schema.clone()),
        );
    }

    let mut meta = map
        .get("_meta")
        .and_then(JsonValue::as_object)
        .cloned()
        .unwrap_or_else(BTreeMap::new);
    meta.insert("mcpace/projected".to_string(), JsonValue::bool(true));
    meta.insert(
        "mcpace/upstream".to_string(),
        JsonValue::object([
            ("server", JsonValue::string(server_name)),
            ("tool", JsonValue::string(tool_name)),
        ]),
    );
    meta.insert(
        "mcpace/tags".to_string(),
        JsonValue::array(
            tool_capability_tags(tool)
                .into_iter()
                .map(JsonValue::string),
        ),
    );
    meta.insert(
        "mcpace/parallelSafety".to_string(),
        tool_parallel_safety(tool),
    );
    map.insert("_meta".to_string(), JsonValue::Object(meta));
    JsonValue::Object(map)
}

fn projected_schema(schema: JsonValue) -> JsonValue {
    if env_bool("MCPACE_PROJECTED_SCHEMA_COMPACTION").unwrap_or(true) {
        compact_schema(schema, 0)
    } else {
        schema
    }
}

fn projected_prompt_definition(
    server_name: &str,
    prompt_name: &str,
    projected_name: &str,
    prompt: &JsonValue,
) -> JsonValue {
    let mut map = prompt.as_object().cloned().unwrap_or_else(BTreeMap::new);
    map.insert("name".to_string(), JsonValue::string(projected_name));
    let description = json_helpers::string_at_path(prompt, &["description"]).unwrap_or("");
    map.insert(
        "description".to_string(),
        JsonValue::string(prefixed_description(server_name, prompt_name, description)),
    );
    if !map.contains_key("title") {
        map.insert(
            "title".to_string(),
            JsonValue::string(format!("{} / {}", server_name, prompt_name)),
        );
    }
    JsonValue::Object(map)
}

fn projected_resource_definition(server_name: &str, uri: &str, resource: &JsonValue) -> JsonValue {
    let mut map = resource.as_object().cloned().unwrap_or_else(BTreeMap::new);
    map.insert(
        "uri".to_string(),
        JsonValue::string(encode_resource_uri(server_name, uri)),
    );
    let name = json_helpers::string_at_path(resource, &["name"]).unwrap_or(uri);
    map.insert(
        "name".to_string(),
        JsonValue::string(format!("{} / {}", server_name, name)),
    );
    JsonValue::Object(map)
}

fn projected_resource_template_definition(server_name: &str, template: &JsonValue) -> JsonValue {
    let mut map = template.as_object().cloned().unwrap_or_else(BTreeMap::new);
    if let Some(uri_template) = json_helpers::string_at_path(template, &["uriTemplate"]) {
        map.insert(
            "uriTemplate".to_string(),
            JsonValue::string(format!(
                "{}/{}/{}",
                PROXIED_TEMPLATE_SCHEME,
                hex_encode(server_name.as_bytes()),
                hex_encode(uri_template.as_bytes())
            )),
        );
    }
    let name = json_helpers::string_at_path(template, &["name"]).unwrap_or("template");
    map.insert(
        "name".to_string(),
        JsonValue::string(format!("{} / {}", server_name, name)),
    );
    JsonValue::Object(map)
}

fn rewrite_resource_contents(server_name: &str, result: JsonValue) -> JsonValue {
    let Some(contents) = json_helpers::array_at_path(&result, &["contents"]) else {
        return result;
    };
    let mut rewritten = Vec::new();
    for content in contents {
        let mut map = content.as_object().cloned().unwrap_or_else(BTreeMap::new);
        if let Some(uri) = json_helpers::string_at_path(content, &["uri"]) {
            map.insert(
                "uri".to_string(),
                JsonValue::string(encode_resource_uri(server_name, uri)),
            );
        }
        rewritten.push(JsonValue::Object(map));
    }
    JsonValue::object([("contents", JsonValue::array(rewritten))])
}

fn search_terms(query: &str) -> Vec<String> {
    normalize(query)
        .split_whitespace()
        .map(str::trim)
        .filter(|term| term.len() >= 2)
        .map(str::to_string)
        .collect()
}

fn score_tool(tool: &JsonValue, terms: &[String]) -> usize {
    if terms.is_empty() {
        return 1;
    }
    let server = normalize(json_helpers::string_at_path(tool, &["server"]).unwrap_or(""));
    let name = normalize(json_helpers::string_at_path(tool, &["name"]).unwrap_or(""));
    let qualified = normalize(json_helpers::string_at_path(tool, &["qualifiedName"]).unwrap_or(""));
    let title = normalize(json_helpers::string_at_path(tool, &["title"]).unwrap_or(""));
    let description = normalize(json_helpers::string_at_path(tool, &["description"]).unwrap_or(""));
    let all = format!(
        "{} {} {} {} {}",
        server, name, qualified, title, description
    );
    terms
        .iter()
        .map(|term| {
            let mut score = 0usize;
            if name == *term || qualified == *term {
                score += 80;
            }
            if name.contains(term) {
                score += 40;
            }
            if qualified.contains(term) {
                score += 30;
            }
            if title.contains(term) {
                score += 20;
            }
            if description.contains(term) {
                score += 10;
            }
            if server.contains(term) {
                score += 5;
            }
            if all.contains(term) {
                score += 1;
            }
            score
        })
        .sum()
}

fn compact_search_tool(tool: &JsonValue, score: usize, include_schema: bool) -> JsonValue {
    let mut entries = vec![
        (
            "server",
            JsonValue::string(json_helpers::string_at_path(tool, &["server"]).unwrap_or("")),
        ),
        (
            "name",
            JsonValue::string(json_helpers::string_at_path(tool, &["name"]).unwrap_or("")),
        ),
        (
            "qualifiedName",
            JsonValue::string(json_helpers::string_at_path(tool, &["qualifiedName"]).unwrap_or("")),
        ),
        (
            "title",
            json_helpers::value_at_path(tool, &["title"])
                .cloned()
                .unwrap_or(JsonValue::Null),
        ),
        (
            "description",
            JsonValue::string(truncate_chars(
                json_helpers::string_at_path(tool, &["description"]).unwrap_or(""),
                DEFAULT_SEARCH_DESCRIPTION_CHARS,
            )),
        ),
        (
            "call",
            json_helpers::value_at_path(tool, &["call"])
                .cloned()
                .unwrap_or(JsonValue::Null),
        ),
        ("score", JsonValue::number(score)),
        (
            "tags",
            JsonValue::array(
                tool_capability_tags(tool)
                    .into_iter()
                    .map(JsonValue::string),
            ),
        ),
        ("parallelSafety", tool_parallel_safety(tool)),
    ];
    if include_schema {
        if let Some(schema) = json_helpers::value_at_path(tool, &["inputSchema"]) {
            entries.push(("inputSchema", compact_schema(schema.clone(), 0)));
        }
    }
    JsonValue::object(entries)
}

fn selected_search_result(scored: &[(usize, String, JsonValue)], has_terms: bool) -> JsonValue {
    if !has_terms || scored.is_empty() {
        return JsonValue::Null;
    }
    let top_score = scored[0].0;
    if top_score == 0 {
        return JsonValue::Null;
    }
    let second_score = scored.get(1).map(|item| item.0).unwrap_or(0);
    let confident = top_score >= 30 && (second_score == 0 || top_score >= second_score + 20);
    JsonValue::object([
        ("confident", JsonValue::bool(confident)),
        ("topScore", JsonValue::number(top_score)),
        ("nextScore", JsonValue::number(second_score)),
        ("result", scored[0].2.clone()),
    ])
}

fn take_tools_with_budget(
    tools: Vec<JsonValue>,
    count_budget: usize,
    token_budget: usize,
) -> Vec<JsonValue> {
    let mut out = Vec::new();
    let mut used_tokens = 0usize;
    for tool in tools.into_iter().take(count_budget) {
        let tool_tokens = estimate_json_tokens(&tool).max(1);
        if !out.is_empty() && used_tokens.saturating_add(tool_tokens) > token_budget {
            break;
        }
        used_tokens = used_tokens.saturating_add(tool_tokens);
        out.push(tool);
    }
    out
}

fn estimate_json_tokens(value: &JsonValue) -> usize {
    // Approximation only: enough for routing decisions without pulling a tokenizer dependency.
    // Compact JSON is a better proxy than pretty JSON because MCP payloads are serialized compactly.
    value.to_compact_string().len().saturating_add(3) / 4
}

fn tool_projection_rank(tool: &JsonValue) -> usize {
    let tags = tool_capability_tags(tool);
    let parallel = tool_parallel_safety(tool);
    let mut rank = 100usize;
    if tags
        .iter()
        .any(|tag| matches!(tag.as_str(), "read" | "search" | "memory" | "docs" | "time"))
    {
        rank = rank.saturating_sub(20);
    }
    if json_helpers::bool_at_path(&parallel, &["readOnly"]).unwrap_or(false) {
        rank = rank.saturating_sub(10);
    }
    if json_helpers::bool_at_path(&parallel, &["destructive"]).unwrap_or(false) {
        rank = rank.saturating_add(40);
    }
    rank
}

fn tool_capability_tags(tool: &JsonValue) -> Vec<String> {
    let haystack = normalize(&format!(
        "{} {} {} {} {}",
        json_helpers::string_at_path(tool, &["server"]).unwrap_or(""),
        json_helpers::string_at_path(tool, &["name"]).unwrap_or(""),
        json_helpers::string_at_path(tool, &["qualifiedName"]).unwrap_or(""),
        json_helpers::string_at_path(tool, &["title"]).unwrap_or(""),
        json_helpers::string_at_path(tool, &["description"]).unwrap_or("")
    ));
    let mut tags = BTreeSet::new();
    for (tag, needles) in [
        (
            "read",
            &["read", "get", "list", "show", "status", "inspect", "fetch"] as &[&str],
        ),
        (
            "write",
            &[
                "write", "create", "update", "delete", "remove", "edit", "patch", "commit",
                "insert",
            ],
        ),
        ("search", &["search", "find", "query", "lookup", "grep"]),
        (
            "file",
            &["file", "filesystem", "path", "directory", "folder"],
        ),
        ("web", &["web", "http", "url", "fetch", "scrape", "page"]),
        (
            "interactive",
            &["page", "click", "navigate", "screenshot", "javascript"],
        ),
        (
            "git",
            &["git", "commit", "branch", "repo", "diff", "pull", "push"],
        ),
        ("memory", &["memory", "remember", "knowledge", "note"]),
        ("db", &["sql", "sqlite", "database", "query", "table"]),
        ("docs", &["docs", "documentation", "context7", "reference"]),
        ("time", &["time", "date", "timezone", "clock"]),
    ] {
        if needles.iter().any(|needle| haystack.contains(needle)) {
            tags.insert(tag.to_string());
        }
    }
    tags.into_iter().collect()
}

fn tool_parallel_safety(tool: &JsonValue) -> JsonValue {
    let annotations = json_helpers::value_at_path(tool, &["annotations"]);
    let read_only = annotations
        .and_then(|value| json_helpers::bool_at_path(value, &["readOnlyHint"]))
        .unwrap_or_else(|| inferred_read_only(tool));
    let destructive = annotations
        .and_then(|value| json_helpers::bool_at_path(value, &["destructiveHint"]))
        .unwrap_or_else(|| inferred_destructive(tool));
    let idempotent = annotations
        .and_then(|value| json_helpers::bool_at_path(value, &["idempotentHint"]))
        .unwrap_or(read_only && !destructive);
    let can_parallelize = read_only && idempotent && !destructive;
    JsonValue::object([
        ("readOnly", JsonValue::bool(read_only)),
        ("idempotent", JsonValue::bool(idempotent)),
        ("destructive", JsonValue::bool(destructive)),
        (
            "crossServerParallelCandidate",
            JsonValue::bool(can_parallelize),
        ),
        (
            "basis",
            JsonValue::string(if annotations.is_some() {
                "tool annotations with conservative fallback"
            } else {
                "name/description inference because upstream annotations were missing"
            }),
        ),
    ])
}

fn inferred_read_only(tool: &JsonValue) -> bool {
    let haystack = normalize(&format!(
        "{} {} {}",
        json_helpers::string_at_path(tool, &["name"]).unwrap_or(""),
        json_helpers::string_at_path(tool, &["title"]).unwrap_or(""),
        json_helpers::string_at_path(tool, &["description"]).unwrap_or("")
    ));
    [
        "read", "get", "list", "show", "status", "inspect", "search", "find", "query", "fetch",
    ]
    .iter()
    .any(|needle| haystack.contains(needle))
        && !inferred_destructive(tool)
}

fn inferred_destructive(tool: &JsonValue) -> bool {
    let haystack = normalize(&format!(
        "{} {} {}",
        json_helpers::string_at_path(tool, &["name"]).unwrap_or(""),
        json_helpers::string_at_path(tool, &["title"]).unwrap_or(""),
        json_helpers::string_at_path(tool, &["description"]).unwrap_or("")
    ));
    [
        "delete",
        "remove",
        "destroy",
        "drop",
        "kill",
        "shutdown",
        "write",
        "create",
        "update",
        "edit",
        "patch",
        "commit",
        "push",
        "click",
        "type",
        "navigate",
        "javascript",
    ]
    .iter()
    .any(|needle| haystack.contains(needle))
}

fn compact_schema(value: JsonValue, depth: usize) -> JsonValue {
    if depth > 8 {
        return JsonValue::string("<schema truncated>");
    }
    match value {
        JsonValue::Object(map) => {
            let mut out = BTreeMap::new();
            for (key, value) in map {
                if should_drop_schema_key(&key) {
                    continue;
                }
                let next = if key == "description" || key == "markdownDescription" || key == "title"
                {
                    match value {
                        JsonValue::String(text) => JsonValue::string(truncate_chars(
                            &text,
                            env_usize("MCPACE_PROJECTED_SCHEMA_DESCRIPTION_CHARS")
                                .unwrap_or(DEFAULT_PROJECTED_SCHEMA_DESCRIPTION_CHARS),
                        )),
                        other => compact_schema(other, depth + 1),
                    }
                } else {
                    compact_schema(value, depth + 1)
                };
                out.insert(key, next);
            }
            JsonValue::Object(out)
        }
        JsonValue::Array(items) => {
            let limit = env_usize("MCPACE_PROJECTED_SCHEMA_ARRAY_LIMIT").unwrap_or(40);
            JsonValue::Array(
                items
                    .into_iter()
                    .take(limit)
                    .map(|item| compact_schema(item, depth + 1))
                    .collect(),
            )
        }
        other => other,
    }
}

fn should_drop_schema_key(key: &str) -> bool {
    matches!(
        key,
        "$comment" | "examples" | "example" | "markdownDescription" | "x-docs" | "x-examples"
    )
}

fn client_profile_from_initialize(initialize_params: Option<&JsonValue>) -> JsonValue {
    let params = initialize_params.cloned().unwrap_or_else(mcp::empty_object);
    let caps = json_helpers::value_at_path(&params, &["capabilities"])
        .cloned()
        .unwrap_or_else(mcp::empty_object);
    let client_info = json_helpers::value_at_path(&params, &["clientInfo"])
        .cloned()
        .unwrap_or(JsonValue::Null);
    JsonValue::object([
        (
            "protocolVersionRequested",
            json_helpers::string_at_path(&params, &["protocolVersion"])
                .map(JsonValue::string)
                .unwrap_or(JsonValue::Null),
        ),
        ("clientInfo", client_info),
        ("capabilities", caps.clone()),
        (
            "supportsRoots",
            JsonValue::bool(json_helpers::value_at_path(&caps, &["roots"]).is_some()),
        ),
        (
            "supportsSampling",
            JsonValue::bool(json_helpers::value_at_path(&caps, &["sampling"]).is_some()),
        ),
        (
            "supportsElicitation",
            JsonValue::bool(json_helpers::value_at_path(&caps, &["elicitation"]).is_some()),
        ),
        (
            "profileSource",
            JsonValue::string("initialize.capabilities; no hardcoded client map"),
        ),
    ])
}

fn prefixed_description(server: &str, name: &str, description: &str) -> String {
    let trimmed = description.trim();
    if trimmed.is_empty() {
        format!("Upstream {} on MCP server {}.", name, server)
    } else {
        format!(
            "[{}] {}",
            server,
            truncate_chars(
                trimmed,
                env_usize("MCPACE_PROJECTED_DESCRIPTION_CHARS")
                    .unwrap_or(DEFAULT_PROJECTED_DESCRIPTION_CHARS),
            )
        )
    }
}

fn unique_projected_name(
    prefix: &str,
    server: &str,
    item: &str,
    used_names: &mut BTreeSet<String>,
) -> String {
    let hash = short_hash(&(server.to_string() + "\u{1f}" + item));
    let mut suffix = 0usize;
    loop {
        let candidate = projected_safe_name(prefix, server, item, &hash, suffix);
        if used_names.insert(candidate.clone()) {
            return candidate;
        }
        suffix = suffix.saturating_add(1);
    }
}

fn projected_safe_name(
    prefix: &str,
    server: &str,
    item: &str,
    hash: &str,
    suffix: usize,
) -> String {
    let prefix = safe_token(prefix);
    let server = safe_token(server);
    let item = safe_token(item);
    let suffix_text = if suffix == 0 {
        String::new()
    } else {
        format!("_{}", suffix)
    };
    let reserved = prefix.len() + hash.len() + suffix_text.len() + 3;
    let body_budget = PROJECTED_NAME_MAX.saturating_sub(reserved).max(2);
    let server_budget = server.len().min(18).min((body_budget / 2).max(1));
    let item_budget = body_budget.saturating_sub(server_budget).max(1);
    let server = trim_token_edges(&truncate_ascii(&server, server_budget));
    let item = trim_token_edges(&truncate_ascii(&item, item_budget));
    let mut candidate = format!("{}_{}_{}_{}{}", prefix, server, item, hash, suffix_text);
    while candidate.contains("__") {
        candidate = candidate.replace("__", "_");
    }
    if candidate.len() <= PROJECTED_NAME_MAX {
        candidate
    } else {
        truncate_ascii(&candidate, PROJECTED_NAME_MAX)
            .trim_matches('_')
            .to_string()
    }
}

fn safe_token(value: &str) -> String {
    let mut out = String::new();
    let mut last_was_sep = false;
    for ch in value.trim().chars() {
        let lower = ch.to_ascii_lowercase();
        if lower.is_ascii_alphanumeric() {
            out.push(lower);
            last_was_sep = false;
        } else if !last_was_sep {
            out.push('_');
            last_was_sep = true;
        }
    }
    let trimmed = trim_token_edges(&out);
    if trimmed.is_empty() {
        "item".to_string()
    } else {
        trimmed
    }
}

fn trim_token_edges(value: &str) -> String {
    value.trim_matches('_').to_string()
}

fn short_hash(value: &str) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in value.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", hash).chars().take(10).collect()
}

fn truncate_ascii(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (index, ch) in value.chars().enumerate() {
        if index >= max_chars {
            out.push('…');
            return out;
        }
        out.push(ch);
    }
    out
}

fn normalize(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
}

fn projection_reason(
    options: &ToolExposureOptions,
    projectable_total: usize,
    raw_total: usize,
    broker_only_total: usize,
    estimated_total_tokens: usize,
    enabled: bool,
    truncated: bool,
) -> String {
    let safety = projection_safety_name(options.projection_safety);
    match (options.mode, enabled, truncated) {
        (ToolExposureMode::Auto, true, false) => format!(
            "auto projection enabled because {} projectable upstream tools fit countBudget={} and tokenBudget={}; estimatedTokens={}; rawToolCount={}, brokerOnlyToolCount={}, projectionSafety={}",
            projectable_total, options.budget, options.token_budget, estimated_total_tokens, raw_total, broker_only_total, safety
        ),
        (ToolExposureMode::Auto, false, _) if raw_total == 0 => {
            "auto projection disabled because no callable upstream tools were discovered".to_string()
        }
        (ToolExposureMode::Auto, false, _) if projectable_total == 0 => format!(
            "auto projection disabled because no tools passed projectionSafety={}; rawToolCount={}, brokerOnlyToolCount={}",
            safety, raw_total, broker_only_total
        ),
        (ToolExposureMode::Auto, false, _) => format!(
            "auto projection disabled because {} projectable upstream tools or ~{} tokens exceed countBudget={} / tokenBudget={}; broker tools remain available; rawToolCount={}, brokerOnlyToolCount={}, projectionSafety={}",
            projectable_total, estimated_total_tokens, options.budget, options.token_budget, raw_total, broker_only_total, safety
        ),
        (ToolExposureMode::Native, true, true) => format!(
            "native projection enabled and truncated from {} projectable tools by countBudget={} / tokenBudget={}; estimatedTokens={}; rawToolCount={}, brokerOnlyToolCount={}, projectionSafety={}",
            projectable_total, options.budget, options.token_budget, estimated_total_tokens, raw_total, broker_only_total, safety
        ),
        (ToolExposureMode::Native, true, false) => format!(
            "native projection enabled for {} projectable upstream tools; estimatedTokens={}; rawToolCount={}, brokerOnlyToolCount={}, projectionSafety={}",
            projectable_total, estimated_total_tokens, raw_total, broker_only_total, safety
        ),
        (ToolExposureMode::Hybrid, true, true) => format!(
            "hybrid projection exposed the highest-ranked prefix of {} projectable tools within countBudget={} / tokenBudget={}; broker search remains available for the rest; rawToolCount={}, brokerOnlyToolCount={}, projectionSafety={}",
            projectable_total, options.budget, options.token_budget, raw_total, broker_only_total, safety
        ),
        (ToolExposureMode::Hybrid, true, false) => format!(
            "hybrid projection exposed all {} projectable tools and keeps broker search available; rawToolCount={}, brokerOnlyToolCount={}, projectionSafety={}",
            projectable_total, raw_total, broker_only_total, safety
        ),
        (ToolExposureMode::Broker, _, _) => {
            "broker mode keeps upstream tools behind upstream_search/upstream_call".to_string()
        }
        (ToolExposureMode::Minimal, _, _) => {
            "minimal mode keeps only essential adapter tools".to_string()
        }
        _ => "projection disabled".to_string(),
    }
}

fn projection_safety_name(safety: ProjectionSafety) -> &'static str {
    match safety {
        ProjectionSafety::Safe => "safe",
        ProjectionSafety::Review => "review",
        ProjectionSafety::All => "all",
    }
}

fn parse_projection_safety(value: &str) -> ProjectionSafety {
    match value.trim().to_ascii_lowercase().replace('_', "-").as_str() {
        "all" | "max" | "maximum" | "everything" => ProjectionSafety::All,
        "safe" | "readonly" | "read-only" | "conservative" => ProjectionSafety::Safe,
        _ => ProjectionSafety::Review,
    }
}

fn management_surface_mode_from_env() -> ManagementSurfaceMode {
    if tool_exposure_mode_from_env() == ToolExposureMode::Minimal {
        return ManagementSurfaceMode::Minimal;
    }
    match std::env::var("MCPACE_MANAGEMENT_SURFACE")
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("full") | Some("debug") | Some("management")
            if env_bool("MCPACE_ALLOW_FULL_MANAGEMENT").unwrap_or(false) =>
        {
            ManagementSurfaceMode::Full
        }
        Some("minimal") | Some("tiny") | Some("strict") => ManagementSurfaceMode::Minimal,
        Some("adapter") | Some("runtime") | Some("dynamic") | None | Some("") => {
            ManagementSurfaceMode::Adapter
        }
        _ => ManagementSurfaceMode::Adapter,
    }
}

fn tool_exposure_mode_from_env() -> ToolExposureMode {
    std::env::var("MCPACE_TOOL_EXPOSURE")
        .or_else(|_| std::env::var("MCPACE_UPSTREAM_TOOL_EXPOSURE"))
        .ok()
        .map(|value| parse_tool_exposure_mode(&value))
        .unwrap_or(ToolExposureMode::Auto)
}

fn parse_tool_exposure_mode(value: &str) -> ToolExposureMode {
    match value.trim().to_ascii_lowercase().replace('_', "-").as_str() {
        "native" | "project" | "projected" | "direct" => ToolExposureMode::Native,
        "hybrid" | "mixed" | "budgeted" => ToolExposureMode::Hybrid,
        "broker" | "wrapper" | "wrapped" | "catalog" => ToolExposureMode::Broker,
        "minimal" | "small" | "tiny" => ToolExposureMode::Minimal,
        _ => ToolExposureMode::Auto,
    }
}

fn management_surface_mode_name(mode: ManagementSurfaceMode) -> &'static str {
    match mode {
        ManagementSurfaceMode::Adapter => "adapter",
        ManagementSurfaceMode::Full => "full",
        ManagementSurfaceMode::Minimal => "minimal",
    }
}

fn tool_exposure_mode_name(mode: ToolExposureMode) -> &'static str {
    match mode {
        ToolExposureMode::Auto => "auto",
        ToolExposureMode::Broker => "broker",
        ToolExposureMode::Native => "native",
        ToolExposureMode::Hybrid => "hybrid",
        ToolExposureMode::Minimal => "minimal",
    }
}

fn protocol_rank(protocol_version: &str) -> usize {
    match protocol_version {
        "2024-11-05" => 1,
        "2025-03-26" => 2,
        "2025-06-18" => 3,
        "2025-11-25" => 4,
        _ => 4,
    }
}

fn env_usize(name: &str) -> Option<usize> {
    std::env::var(name).ok()?.trim().parse::<usize>().ok()
}

fn env_u64(name: &str) -> Option<u64> {
    std::env::var(name).ok()?.trim().parse::<u64>().ok()
}

fn env_bool(name: &str) -> Option<bool> {
    match std::env::var(name)
        .ok()?
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn encode_resource_uri(server: &str, upstream_uri: &str) -> String {
    format!(
        "{}/{}/{}",
        PROXIED_RESOURCE_SCHEME,
        hex_encode(server.as_bytes()),
        hex_encode(upstream_uri.as_bytes())
    )
}

fn decode_resource_uri(value: &str) -> Result<(String, String), String> {
    let prefix = format!("{}/", PROXIED_RESOURCE_SCHEME);
    let rest = value.strip_prefix(&prefix).ok_or_else(|| {
        format!(
            "resource uri '{}' is not a MCPace proxied upstream resource",
            value
        )
    })?;
    let (encoded_server, encoded_uri) = rest.split_once('/').ok_or_else(|| {
        format!(
            "resource uri '{}' is missing upstream server or payload",
            value
        )
    })?;
    let server = String::from_utf8(hex_decode(encoded_server)?)
        .map_err(|error| format!("proxied resource server is not UTF-8: {}", error))?;
    let uri = String::from_utf8(hex_decode(encoded_uri)?)
        .map_err(|error| format!("proxied resource uri is not UTF-8: {}", error))?;
    Ok((server, uri))
}

fn hex_encode(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    out
}

fn hex_decode(value: &str) -> Result<Vec<u8>, String> {
    if !value.len().is_multiple_of(2) {
        return Err("hex payload length must be even".to_string());
    }
    let mut bytes = Vec::with_capacity(value.len() / 2);
    let mut chars = value.as_bytes().iter().copied();
    while let (Some(high), Some(low)) = (chars.next(), chars.next()) {
        bytes.push((hex_value(high)? << 4) | hex_value(low)?);
    }
    Ok(bytes)
}

fn hex_value(byte: u8) -> Result<u8, String> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(format!("invalid hex byte '{}'", byte as char)),
    }
}

fn maybe_meta_errors(errors: Vec<JsonValue>) -> (&'static str, JsonValue) {
    (
        "_meta",
        JsonValue::object([("mcpace/errors", JsonValue::array(errors))]),
    )
}

fn is_unsupported_method_error(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("method not found")
        || lower.contains("unsupported")
        || lower.contains("unknown method")
        || lower.contains("-32601")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_projected_names_are_bounded_and_unique() {
        let mut used = BTreeSet::new();
        let first = unique_projected_name("u", "Example Server", "read/file", &mut used);
        let second = unique_projected_name("u", "Example Server", "read file", &mut used);
        assert!(first.len() <= PROJECTED_NAME_MAX);
        assert!(second.len() <= PROJECTED_NAME_MAX);
        assert_ne!(first, second);
    }

    #[test]
    fn resource_uri_round_trips() {
        let uri = encode_resource_uri("filesystem", "file:///tmp/hello world.txt");
        let (server, upstream_uri) = decode_resource_uri(&uri).unwrap();
        assert_eq!(server, "filesystem");
        assert_eq!(upstream_uri, "file:///tmp/hello world.txt");
    }
}
