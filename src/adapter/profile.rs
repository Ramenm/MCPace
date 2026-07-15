use super::{
    adapter_capabilities, management_surface_mode_from_env, management_surface_mode_name,
    projected_tool_set, projection_safety_name, tool_exposure_mode_name, ToolExposureMode,
    ToolExposureOptions,
};
use crate::json::JsonValue;
use crate::json_helpers;
use crate::mcp_protocol as mcp;
use crate::tool_result;
use crate::upstream;
use std::collections::BTreeSet;
use std::fmt;
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdapterProfileError {
    message: String,
}

pub type AdapterProfileResult<T> = std::result::Result<T, AdapterProfileError>;

impl AdapterProfileError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for AdapterProfileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for AdapterProfileError {}

impl From<String> for AdapterProfileError {
    fn from(message: String) -> Self {
        Self::new(message)
    }
}

impl From<&str> for AdapterProfileError {
    fn from(message: &str) -> Self {
        Self::new(message)
    }
}

impl From<AdapterProfileError> for String {
    fn from(error: AdapterProfileError) -> Self {
        error.message
    }
}

pub fn adapter_profile(
    root_path: &Path,
    initialize_params: Option<&JsonValue>,
    transport: &str,
    management_tool_names: &[String],
    include_live_catalog: bool,
    timeout_ms: Option<u64>,
    refresh: bool,
) -> AdapterProfileResult<JsonValue> {
    let env_options = ToolExposureOptions::from_env();
    let options = ToolExposureOptions {
        timeout_ms: timeout_ms.or(env_options.timeout_ms),
        refresh: refresh || env_options.refresh,
        ..env_options
    };
    let projection = if matches!(
        options.mode,
        ToolExposureMode::Broker | ToolExposureMode::Minimal
    ) {
        None
    } else {
        let reserved = management_tool_names
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        projected_tool_set(root_path, &reserved, &options).ok()
    };
    let inventory = upstream::configured_inventory(root_path)
        .map_err(|error| AdapterProfileError::from(error.to_string()))?;
    let live_catalog = if include_live_catalog {
        Some(
            upstream::catalog_tools(root_path, None, options.timeout_ms, options.refresh)
                .map_err(|error| AdapterProfileError::from(error.to_string()))?,
        )
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
                    JsonValue::array(
                        tool_result::supported_token_reducer_plugins()
                            .iter()
                            .map(|plugin| JsonValue::string(*plugin)),
                    ),
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
