use crate::json::JsonValue;
use crate::json_helpers;
use crate::mcp_protocol as mcp;
use crate::upstream;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
mod discovery;
mod profile;
mod proxy_uri;
pub use self::discovery::{
    adapter_route_plan, get_prompt, list_prompts, list_resource_templates, list_resources,
    read_resource, upstream_search,
};
use self::discovery::{
    estimate_json_tokens, paginated_tool_list, projected_tool_definition, shape_tool_for_client,
    take_tools_with_budget, tool_names, tool_projection_rank,
};
pub use self::profile::adapter_profile;
use self::proxy_uri::{
    decode_resource_uri, encode_resource_uri, hex_encode, is_unsupported_method_error,
    maybe_meta_errors,
};
const DEFAULT_TOOL_BUDGET: usize = 64;
const DEFAULT_TOOL_TOKEN_BUDGET: usize = 24_000;
const DEFAULT_PROJECTION_CANDIDATE_MULTIPLIER: usize = 8;
const DEFAULT_PROJECTION_CANDIDATE_LIMIT_MAX: usize = 8_192;
const DEFAULT_PROJECTION_BROKER_SAMPLE_LIMIT: usize = 64;
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
    /// Project unguarded tools unless they look mutating/destructive. This is opt-in.
    Review,
    /// Project every discovered tool and rely on the client/human-in-the-loop for review.
    All,
}
const DEFAULT_PROJECTED_TOOL_SAFETY: ProjectionSafety = ProjectionSafety::Safe;
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
    "MCPace is a dynamic MCP adapter for many clients and many upstream servers. It infers client capabilities from initialize, keeps startup tools/list small by default, discovers configured upstream stdio/plain HTTP servers through live MCP methods when requested, and can opt into native projected upstream tools only when the catalog fits the token budget and projection safety allows them. Use adapter_profile for the current routing plan, upstream_search for concise discovery, projected u_<server>_<tool>_<hash> names when projection is enabled, and upstream_call/upstream_batch for brokered routing with known-tool and policy validation."
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
    augment_tool_definitions_with_options(root_path, base_tools, &options)
}
fn augment_tool_definitions_with_options(
    root_path: &Path,
    base_tools: Vec<JsonValue>,
    options: &ToolExposureOptions,
) -> Vec<JsonValue> {
    if matches!(
        options.mode,
        ToolExposureMode::Broker | ToolExposureMode::Minimal
    ) {
        return base_tools;
    }
    let reserved = tool_names(&base_tools).into_iter().collect::<BTreeSet<_>>();
    match projected_tool_set(root_path, &reserved, options) {
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
    let projectable_total = json_helpers::value_at_path(&catalog, &["projectableToolCount"])
        .and_then(JsonValue::as_i64)
        .filter(|value| *value >= 0)
        .map(|value| value as usize)
        .unwrap_or(projected_all.len());
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
    let catalog_ok = json_helpers::bool_at_path(&catalog, &["ok"]).unwrap_or(false);
    let estimated_total_tokens: usize = projected_all.iter().map(estimate_json_tokens).sum();
    let full_catalog_fits =
        projectable_total <= options.budget && estimated_total_tokens <= options.token_budget;
    let projection_enabled = match options.mode {
        ToolExposureMode::Native | ToolExposureMode::Hybrid => projectable_total > 0,
        ToolExposureMode::Auto => catalog_ok && projectable_total > 0 && full_catalog_fits,
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
            ProjectionReasonMetrics {
                projectable_total,
                raw_total,
                broker_only_total,
                estimated_total_tokens,
                catalog_ok,
                enabled: projection_enabled,
                truncated,
            },
        ),
        catalog,
    })
}
struct ProjectionReasonMetrics {
    projectable_total: usize,
    raw_total: usize,
    broker_only_total: usize,
    estimated_total_tokens: usize,
    catalog_ok: bool,
    enabled: bool,
    truncated: bool,
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
    let catalog = if matches!(options.mode, ToolExposureMode::Auto) && !options.refresh {
        upstream::callable_tools_cached_catalog(root_path)?
    } else {
        upstream::callable_tools_raw_catalog(root_path, options.timeout_ms, options.refresh)?
    };
    let mut servers = Vec::new();
    let mut errors = Vec::new();
    let mut raw_tool_count = 0usize;
    let mut projectable_tool_count = 0usize;
    let mut broker_only_tool_count = 0usize;
    let candidate_limit = projection_candidate_limit(options);
    let broker_sample_limit = projection_broker_sample_limit();
    let mut stored_projectable_count = 0usize;
    let mut stored_broker_only_count = 0usize;
    for listing in json_helpers::array_at_path(&catalog, &["servers"]).unwrap_or(&[]) {
        let server_name = json_helpers::string_at_path(listing, &["name"])
            .map(str::to_string)
            .unwrap_or_else(|| "unknown".to_string());
        if json_helpers::bool_at_path(listing, &["ok"]).unwrap_or(false) {
            let mut projectable_tools = Vec::new();
            let mut broker_only_tools = Vec::new();
            let mut server_projectable_tool_count = 0usize;
            let mut server_broker_only_tool_count = 0usize;
            for tool in json_helpers::array_at_path(listing, &["tools"]).unwrap_or(&[]) {
                raw_tool_count = raw_tool_count.saturating_add(1);
                let Some(tool_name) = json_helpers::string_at_path(tool, &["name"])
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                else {
                    broker_only_tool_count = broker_only_tool_count.saturating_add(1);
                    server_broker_only_tool_count = server_broker_only_tool_count.saturating_add(1);
                    if stored_broker_only_count < broker_sample_limit {
                        broker_only_tools.push(JsonValue::object([
                        ("name", JsonValue::string("<unnamed>")),
                        ("reason", JsonValue::string("missing tool name")),
                    ]));
                        stored_broker_only_count = stored_broker_only_count.saturating_add(1);
                    }
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
                    server_projectable_tool_count = server_projectable_tool_count.saturating_add(1);
                    if stored_projectable_count < candidate_limit {
                        projectable_tools.push(decorated);
                        stored_projectable_count = stored_projectable_count.saturating_add(1);
                    }
                } else {
                    broker_only_tool_count = broker_only_tool_count.saturating_add(1);
                    server_broker_only_tool_count = server_broker_only_tool_count.saturating_add(1);
                    if stored_broker_only_count < broker_sample_limit {
                        broker_only_tools.push(compact_broker_only_tool(
                            &server_name,
                            tool_name,
                            tool,
                            &policy,
                            &decision.reason,
                        ));
                        stored_broker_only_count = stored_broker_only_count.saturating_add(1);
                    }
                }
            }
            servers.push(JsonValue::object([
                ("name", JsonValue::string(&server_name)),
                ("ok", JsonValue::bool(true)),
                (
                    "rawToolCount",
                    JsonValue::number(
                        json_helpers::array_at_path(listing, &["tools"])
                            .map(|items| items.len())
                            .unwrap_or(0),
                    ),
                ),
                (
                    "projectableToolCount",
                    JsonValue::number(server_projectable_tool_count),
                ),
                (
                    "brokerOnlyToolCount",
                    JsonValue::number(server_broker_only_tool_count),
                ),
                (
                    "projectableToolSampleCount",
                    JsonValue::number(projectable_tools.len()),
                ),
                (
                    "brokerOnlyToolSampleCount",
                    JsonValue::number(broker_only_tools.len()),
                ),
                (
                    "projectableToolsTruncated",
                    JsonValue::bool(server_projectable_tool_count > projectable_tools.len()),
                ),
                (
                    "brokerOnlyToolsTruncated",
                    JsonValue::bool(server_broker_only_tool_count > broker_only_tools.len()),
                ),
                (
                    "cacheHit",
                    json_helpers::value_at_path(listing, &["cacheHit"])
                        .cloned()
                        .unwrap_or(JsonValue::Null),
                ),
                ("projectableTools", JsonValue::array(projectable_tools)),
                ("brokerOnlyTools", JsonValue::array(broker_only_tools)),
            ]));
        } else {
            let error = json_helpers::string_at_path(listing, &["error"])
                .unwrap_or("tools/list failed")
                .to_string();
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
        ("projectableCandidateLimit", JsonValue::number(candidate_limit)),
        ("projectableCandidateCount", JsonValue::number(stored_projectable_count)),
        ("brokerOnlySampleLimit", JsonValue::number(broker_sample_limit)),
        ("brokerOnlySampleCount", JsonValue::number(stored_broker_only_count)),
        (
            "projectionCandidatesTruncated",
            JsonValue::bool(projectable_tool_count > stored_projectable_count),
        ),
        ("servers", JsonValue::array(servers)),
        maybe_meta_errors(errors),
    ]))
}
fn projection_candidate_limit(options: &ToolExposureOptions) -> usize {
    let default = options
        .budget
        .saturating_mul(DEFAULT_PROJECTION_CANDIDATE_MULTIPLIER)
        .max(options.budget)
        .max(1);
    env_usize("MCPACE_PROJECTION_CANDIDATE_LIMIT")
        .unwrap_or(default)
        .clamp(options.budget.max(1), DEFAULT_PROJECTION_CANDIDATE_LIMIT_MAX)
}
fn projection_broker_sample_limit() -> usize {
    env_usize("MCPACE_PROJECTION_BROKER_SAMPLE_LIMIT")
        .unwrap_or(DEFAULT_PROJECTION_BROKER_SAMPLE_LIMIT)
        .clamp(0, 10_000)
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
            | "allowUnknownTool"
            | "allowUnknownUpstreamTool"
    )
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
fn projection_reason(options: &ToolExposureOptions, metrics: ProjectionReasonMetrics) -> String {
    let safety = projection_safety_name(options.projection_safety);
    let ProjectionReasonMetrics {
        projectable_total,
        raw_total,
        broker_only_total,
        estimated_total_tokens,
        catalog_ok,
        enabled,
        truncated,
    } = metrics;
    match (options.mode, catalog_ok, enabled, truncated) {
        (ToolExposureMode::Auto, true, true, false) => format!(
            "auto projection enabled because {} projectable upstream tools fit countBudget={} and tokenBudget={}; estimatedTokens={}; rawToolCount={}, brokerOnlyToolCount={}, projectionSafety={}",
            projectable_total, options.budget, options.token_budget, estimated_total_tokens, raw_total, broker_only_total, safety
        ),
        (ToolExposureMode::Auto, false, _, _) => "auto projection disabled because the cached upstream catalog is incomplete or stale; broker tools remain available while refresh/warmup can repopulate the cache".to_string(),
        (ToolExposureMode::Auto, true, false, _) if raw_total == 0 => {
            "auto projection disabled because no callable upstream tools were discovered".to_string()
        }
        (ToolExposureMode::Auto, true, false, _) if projectable_total == 0 => format!(
            "auto projection disabled because no tools passed projectionSafety={}; rawToolCount={}, brokerOnlyToolCount={}",
            safety, raw_total, broker_only_total
        ),
        (ToolExposureMode::Auto, true, false, _) => format!(
            "auto projection disabled because {} projectable upstream tools or ~{} tokens exceed countBudget={} / tokenBudget={}; broker tools remain available; rawToolCount={}, brokerOnlyToolCount={}, projectionSafety={}",
            projectable_total, estimated_total_tokens, options.budget, options.token_budget, raw_total, broker_only_total, safety
        ),
        (ToolExposureMode::Native, _, true, true) => format!(
            "native projection enabled and truncated from {} projectable tools by countBudget={} / tokenBudget={}; estimatedTokens={}; rawToolCount={}, brokerOnlyToolCount={}, projectionSafety={}",
            projectable_total, options.budget, options.token_budget, estimated_total_tokens, raw_total, broker_only_total, safety
        ),
        (ToolExposureMode::Native, _, true, false) => format!(
            "native projection enabled for {} projectable upstream tools; estimatedTokens={}; rawToolCount={}, brokerOnlyToolCount={}, projectionSafety={}",
            projectable_total, estimated_total_tokens, raw_total, broker_only_total, safety
        ),
        (ToolExposureMode::Hybrid, _, true, true) => format!(
            "hybrid projection exposed the highest-ranked prefix of {} projectable tools within countBudget={} / tokenBudget={}; broker search remains available for the rest; rawToolCount={}, brokerOnlyToolCount={}, projectionSafety={}",
            projectable_total, options.budget, options.token_budget, raw_total, broker_only_total, safety
        ),
        (ToolExposureMode::Hybrid, _, true, false) => format!(
            "hybrid projection exposed all {} projectable tools and keeps broker search available; rawToolCount={}, brokerOnlyToolCount={}, projectionSafety={}",
            projectable_total, raw_total, broker_only_total, safety
        ),
        (ToolExposureMode::Broker, _, _, _) => {
            "broker mode keeps upstream tools behind upstream_search/upstream_call".to_string()
        }
        (ToolExposureMode::Minimal, _, _, _) => {
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
        .unwrap_or_else(default_tool_exposure_mode)
}
fn default_tool_exposure_mode() -> ToolExposureMode {
    ToolExposureMode::Broker
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
#[cfg(test)]
mod tests;
