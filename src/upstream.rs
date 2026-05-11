use crate::json::JsonValue;
use crate::json_helpers;
use crate::mcp_protocol as mcp;
use crate::resources;
use std::collections::{hash_map::DefaultHasher, BTreeMap, BTreeSet};
#[cfg(test)]
use std::env;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::{mpsc, Mutex};
use std::time::{Duration, Instant};

mod diagnostics;
mod inventory;
mod lease_runtime;
mod policy_audit;
mod policy_suggestions;
mod process_config;
mod projection;
mod server_config;
mod session_pool;
mod source_type;
mod stdio_runtime;
mod tool_cache;

pub use self::inventory::{
    audit_tool_policies, catalog_tools, configured_inventory, probe_servers, suggest_tool_policies,
    surface_manifest,
};
pub use self::lease_runtime::{
    call_tool, call_tool_with_context, call_tool_with_pooled_context, call_tools,
    call_tools_with_context, call_tools_with_pooled_context, callable_server_names,
    collect_allow_arguments, collect_allowed_tool_risk_classes, list_tools, request_once,
    tool_policy_info,
};
pub use self::projection::{
    decode_projected_tool_name, encode_projected_tool_name, projected_tool_catalog,
    UpstreamProjectionSafety,
};
pub use self::session_pool::UpstreamSessionPool;

#[cfg(test)]
use self::diagnostics::stderr_suffix;
#[cfg(test)]
use self::diagnostics::DIAGNOSTIC_REDACTION;
#[cfg(test)]
use self::inventory::{catalog_cache_counts, flatten_catalog_tools};
#[cfg(test)]
use self::lease_runtime::{validate_upstream_batch_tool_policy, validate_upstream_tool_policy};
use self::policy_audit::{audit_tool, tool_policy_summaries};
#[cfg(test)]
use self::policy_suggestions::report as policy_suggestion_report;
use self::process_config::{
    expand_template, redact_command, resolve_command_for_cwd, validate_stdio_cwd,
};
#[cfg(test)]
use self::server_config::current_platform_alias;
#[cfg(test)]
use self::server_config::env_var_names_from_array;
use self::server_config::{
    context_string, load_servers, optional_json_string, run_server_tasks, select_servers,
};
use self::source_type::infer_source_type;
use self::stdio_runtime::run_stdio_request;
use self::tool_cache::cached_tools_list;
#[cfg(test)]
use self::tool_cache::{
    prune_tool_list_cache, read_cached_tools, tool_list_cache_key, write_cached_tools,
    CachedToolList, ToolListCacheKey, TOOL_LIST_CACHE,
};
#[cfg(test)]
use crate::json::parse_str;

const DEFAULT_TIMEOUT_MS: u64 = 120_000;
const DEFAULT_PROBE_TIMEOUT_MS: u64 = 30_000;
const TOOL_LIST_CACHE_TTL: Duration = Duration::from_secs(30);
const TOOL_LIST_CACHE_MAX_ENTRIES: usize = 128;
const UPSTREAM_SESSION_IDLE_TTL: Duration = Duration::from_secs(300);
const INITIALIZE_ID: i64 = 1;
const METHOD_ID: i64 = 2;

fn max_pooled_upstream_sessions() -> usize {
    resources::default_upstream_session_pool_limit()
}

#[derive(Clone, Debug)]
struct UpstreamServerConfig {
    name: String,
    enabled: bool,
    disabled_reason: Option<String>,
    source_type: String,
    command: Option<String>,
    args: Vec<String>,
    env: BTreeMap<String, String>,
    cwd: Option<PathBuf>,
    url: Option<String>,
    timeout_ms: u64,
    tool_policies: Vec<ToolRiskPolicy>,
}

#[derive(Clone, Debug)]
struct ToolRiskPolicy {
    tools: Vec<String>,
    risk_class: Option<String>,
    allow_argument: Option<String>,
    description: Option<String>,
}

#[derive(Clone, Debug)]
struct UpstreamServerPolicy {
    profile_enabled: bool,
    platform_supported: bool,
    tool_policies: Vec<ToolRiskPolicy>,
}

#[derive(Clone, Debug)]
pub struct UpstreamToolCall {
    pub tool: String,
    pub arguments: JsonValue,
}

#[derive(Clone, Debug, Default)]
pub struct UpstreamLeaseContext {
    pub client_id: Option<String>,
    pub session_id: Option<String>,
    pub project_root: Option<String>,
    pub transport: Option<String>,
    pub metadata: Option<JsonValue>,
    pub ttl_ms: Option<u128>,
    pub allow_arguments: BTreeSet<String>,
    pub allowed_tool_risk_classes: BTreeSet<String>,
}

fn cache_root_path(root_path: &Path) -> String {
    root_path
        .canonicalize()
        .unwrap_or_else(|_| root_path.to_path_buf())
        .display()
        .to_string()
}

fn server_fingerprint(server: &UpstreamServerConfig) -> String {
    let env_values = server
        .env
        .iter()
        .map(|(key, value)| format!("{}:{}", key, fingerprint_env_value(value)))
        .collect::<Vec<_>>()
        .join("\u{1f}");
    format!(
        "protocol={}|enabled={}|type={}|command={}|args={}|env={}|cwd={}|url={}|timeout={}",
        mcp::CURRENT_PROTOCOL_VERSION,
        server.enabled,
        server.source_type,
        server.command.as_deref().unwrap_or_default(),
        server.args.join("\u{1f}"),
        env_values,
        server
            .cwd
            .as_ref()
            .map(|value| value.display().to_string())
            .unwrap_or_default(),
        server.url.as_deref().unwrap_or_default(),
        server.timeout_ms
    )
}

fn fingerprint_env_value(value: &str) -> String {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    format!("len{}-hash{:016x}", value.len(), hasher.finish())
}

fn ensure_callable_stdio(root_path: &Path, server: &UpstreamServerConfig) -> Result<(), String> {
    let (runtime_callable, _resolved_command, command_error) =
        server_runtime_callable(root_path, server);
    if runtime_callable {
        return Ok(());
    }
    if !server.enabled {
        return Err(format!(
            "upstream server '{}' is disabled: {}",
            server.name,
            server
                .disabled_reason
                .as_deref()
                .unwrap_or("server is disabled by source or policy")
        ));
    }
    if server.source_type != "stdio" {
        return Err(format!(
            "upstream server '{}' uses '{}' transport. This MCPace bridge currently forwards stdio upstreams only; configure a stdio adapter or call runtime_diagnostics for exact status.",
            server.name, server.source_type
        ));
    }
    if let Some(error) = command_error {
        return Err(error);
    }
    Err(format!(
        "upstream server '{}' is not callable through the stdio bridge",
        server.name
    ))
}

fn timeout_for(server: &UpstreamServerConfig, requested_ms: Option<u64>) -> Duration {
    let millis = requested_ms
        .unwrap_or(server.timeout_ms)
        .clamp(1_000, 300_000);
    Duration::from_millis(millis)
}

fn probe_timeout_for(server: &UpstreamServerConfig, requested_ms: Option<u64>) -> Duration {
    let default_ms = server.timeout_ms.min(DEFAULT_PROBE_TIMEOUT_MS);
    let millis = requested_ms.unwrap_or(default_ms).clamp(1_000, 300_000);
    Duration::from_millis(millis)
}

fn server_runtime_callable(
    root_path: &Path,
    server: &UpstreamServerConfig,
) -> (bool, Option<PathBuf>, Option<String>) {
    if !server.enabled {
        return (false, None, None);
    }
    if server.source_type != "stdio" {
        return (false, None, None);
    }
    let Some(command) = server
        .command
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return (
            false,
            None,
            Some(format!(
                "upstream server '{}' has no stdio command configured",
                server.name
            )),
        );
    };

    let cwd = server.cwd.as_deref().unwrap_or(root_path);
    if let Some(cwd_error) = validate_stdio_cwd(cwd, &server.name) {
        return (false, None, Some(cwd_error));
    }

    match resolve_command_for_cwd(command, cwd) {
        Ok(path) => (true, Some(path), None),
        Err(error) => (
            false,
            None,
            Some(format!(
                "failed to resolve command '{}' for upstream server '{}': {}",
                command, server.name, error
            )),
        ),
    }
}

fn catalog_server(
    root_path: &Path,
    server: &UpstreamServerConfig,
    timeout_ms: Option<u64>,
    refresh: bool,
) -> JsonValue {
    let started = Instant::now();
    let (runtime_callable, resolved_command, command_error) =
        server_runtime_callable(root_path, server);
    let effective_timeout = probe_timeout_for(server, timeout_ms);
    if !runtime_callable {
        let status = if !server.enabled {
            "disabled"
        } else if server.source_type != "stdio" {
            "blocked-non-stdio"
        } else {
            "blocked-command-not-found"
        };
        return JsonValue::object([
            ("name", JsonValue::string(&server.name)),
            ("ok", JsonValue::bool(false)),
            ("enabled", JsonValue::bool(server.enabled)),
            ("sourceType", JsonValue::string(&server.source_type)),
            ("runtimeCallable", JsonValue::bool(false)),
            ("status", JsonValue::string(status)),
            (
                "command",
                server
                    .command
                    .as_ref()
                    .map(|value| JsonValue::string(redact_command(value)))
                    .unwrap_or(JsonValue::Null),
            ),
            (
                "resolvedCommand",
                resolved_command
                    .as_ref()
                    .map(|value| JsonValue::string(value.display().to_string()))
                    .unwrap_or(JsonValue::Null),
            ),
            (
                "url",
                server
                    .url
                    .as_ref()
                    .map(JsonValue::string)
                    .unwrap_or(JsonValue::Null),
            ),
            ("toolCount", JsonValue::number(0)),
            ("tools", JsonValue::array([])),
            (
                "elapsedMs",
                JsonValue::number(started.elapsed().as_millis()),
            ),
            ("cacheHit", JsonValue::bool(false)),
            (
                "cacheTtlMs",
                JsonValue::number(TOOL_LIST_CACHE_TTL.as_millis()),
            ),
            (
                "error",
                command_error.map(JsonValue::string).unwrap_or_else(|| {
                    JsonValue::string(
                        server
                            .disabled_reason
                            .as_deref()
                            .unwrap_or(status)
                            .to_string(),
                    )
                }),
            ),
        ]);
    }

    match cached_tools_list(root_path, server, effective_timeout, refresh) {
        Ok((raw_tools, cache_hit)) => {
            let tools = raw_tools
                .as_array()
                .unwrap_or(&[])
                .iter()
                .map(tool_summary)
                .collect::<Vec<_>>();
            JsonValue::object([
                ("name", JsonValue::string(&server.name)),
                ("ok", JsonValue::bool(true)),
                ("enabled", JsonValue::bool(server.enabled)),
                ("sourceType", JsonValue::string(&server.source_type)),
                ("runtimeCallable", JsonValue::bool(true)),
                ("status", JsonValue::string("listed-tools")),
                (
                    "timeoutMs",
                    JsonValue::number(effective_timeout.as_millis()),
                ),
                (
                    "elapsedMs",
                    JsonValue::number(started.elapsed().as_millis()),
                ),
                ("cacheHit", JsonValue::bool(cache_hit)),
                (
                    "cacheTtlMs",
                    JsonValue::number(TOOL_LIST_CACHE_TTL.as_millis()),
                ),
                (
                    "command",
                    server
                        .command
                        .as_ref()
                        .map(|value| JsonValue::string(redact_command(value)))
                        .unwrap_or(JsonValue::Null),
                ),
                (
                    "resolvedCommand",
                    resolved_command
                        .as_ref()
                        .map(|value| JsonValue::string(value.display().to_string()))
                        .unwrap_or(JsonValue::Null),
                ),
                ("toolCount", JsonValue::number(tools.len())),
                ("tools", JsonValue::array(tools)),
            ])
        }
        Err(error) => JsonValue::object([
            ("name", JsonValue::string(&server.name)),
            ("ok", JsonValue::bool(false)),
            ("enabled", JsonValue::bool(server.enabled)),
            ("sourceType", JsonValue::string(&server.source_type)),
            ("runtimeCallable", JsonValue::bool(true)),
            ("status", JsonValue::string("catalog-failed")),
            (
                "timeoutMs",
                JsonValue::number(effective_timeout.as_millis()),
            ),
            (
                "elapsedMs",
                JsonValue::number(started.elapsed().as_millis()),
            ),
            ("cacheHit", JsonValue::bool(false)),
            (
                "cacheTtlMs",
                JsonValue::number(TOOL_LIST_CACHE_TTL.as_millis()),
            ),
            (
                "command",
                server
                    .command
                    .as_ref()
                    .map(|value| JsonValue::string(redact_command(value)))
                    .unwrap_or(JsonValue::Null),
            ),
            (
                "resolvedCommand",
                resolved_command
                    .as_ref()
                    .map(|value| JsonValue::string(value.display().to_string()))
                    .unwrap_or(JsonValue::Null),
            ),
            ("toolCount", JsonValue::number(0)),
            ("tools", JsonValue::array([])),
            ("error", JsonValue::string(error)),
        ]),
    }
}

fn tool_summary(tool: &JsonValue) -> JsonValue {
    let name = json_helpers::string_at_path(tool, &["name"])
        .unwrap_or("<unnamed>")
        .to_string();
    let title = json_helpers::string_at_path(tool, &["title"]).map(str::to_string);
    let description = json_helpers::string_at_path(tool, &["description"])
        .or(title.as_deref())
        .unwrap_or("")
        .to_string();
    JsonValue::object([
        ("name", JsonValue::string(name)),
        (
            "title",
            title.map(JsonValue::string).unwrap_or(JsonValue::Null),
        ),
        ("description", JsonValue::string(description)),
    ])
}

fn probe_server(
    root_path: &Path,
    server: &UpstreamServerConfig,
    timeout_ms: Option<u64>,
    refresh: bool,
) -> JsonValue {
    let started = Instant::now();
    let (runtime_callable, resolved_command, command_error) =
        server_runtime_callable(root_path, server);
    let effective_timeout = probe_timeout_for(server, timeout_ms);
    if !runtime_callable {
        let status = if !server.enabled {
            "disabled"
        } else if server.source_type != "stdio" {
            "blocked-non-stdio"
        } else {
            "blocked-command-not-found"
        };
        return JsonValue::object([
            ("name", JsonValue::string(&server.name)),
            ("ok", JsonValue::bool(false)),
            ("enabled", JsonValue::bool(server.enabled)),
            ("sourceType", JsonValue::string(&server.source_type)),
            ("runtimeCallable", JsonValue::bool(false)),
            ("status", JsonValue::string(status)),
            (
                "command",
                server
                    .command
                    .as_ref()
                    .map(|value| JsonValue::string(redact_command(value)))
                    .unwrap_or(JsonValue::Null),
            ),
            (
                "resolvedCommand",
                resolved_command
                    .as_ref()
                    .map(|value| JsonValue::string(value.display().to_string()))
                    .unwrap_or(JsonValue::Null),
            ),
            (
                "url",
                server
                    .url
                    .as_ref()
                    .map(JsonValue::string)
                    .unwrap_or(JsonValue::Null),
            ),
            ("toolCount", JsonValue::Null),
            (
                "elapsedMs",
                JsonValue::number(started.elapsed().as_millis()),
            ),
            ("cacheHit", JsonValue::bool(false)),
            (
                "cacheTtlMs",
                JsonValue::number(TOOL_LIST_CACHE_TTL.as_millis()),
            ),
            (
                "error",
                command_error.map(JsonValue::string).unwrap_or_else(|| {
                    JsonValue::string(
                        server
                            .disabled_reason
                            .as_deref()
                            .unwrap_or(status)
                            .to_string(),
                    )
                }),
            ),
        ]);
    }

    match cached_tools_list(root_path, server, effective_timeout, refresh) {
        Ok((tools, cache_hit)) => {
            let tool_names = tools
                .as_array()
                .unwrap_or(&[])
                .iter()
                .filter_map(|tool| json_helpers::string_at_path(tool, &["name"]))
                .map(JsonValue::string)
                .collect::<Vec<_>>();
            JsonValue::object([
                ("name", JsonValue::string(&server.name)),
                ("ok", JsonValue::bool(true)),
                ("enabled", JsonValue::bool(server.enabled)),
                ("sourceType", JsonValue::string(&server.source_type)),
                ("runtimeCallable", JsonValue::bool(true)),
                ("status", JsonValue::string("listed-tools")),
                (
                    "timeoutMs",
                    JsonValue::number(effective_timeout.as_millis()),
                ),
                (
                    "elapsedMs",
                    JsonValue::number(started.elapsed().as_millis()),
                ),
                ("cacheHit", JsonValue::bool(cache_hit)),
                (
                    "cacheTtlMs",
                    JsonValue::number(TOOL_LIST_CACHE_TTL.as_millis()),
                ),
                (
                    "command",
                    server
                        .command
                        .as_ref()
                        .map(|value| JsonValue::string(redact_command(value)))
                        .unwrap_or(JsonValue::Null),
                ),
                (
                    "resolvedCommand",
                    resolved_command
                        .as_ref()
                        .map(|value| JsonValue::string(value.display().to_string()))
                        .unwrap_or(JsonValue::Null),
                ),
                (
                    "toolCount",
                    JsonValue::number(tools.as_array().map(|items| items.len()).unwrap_or(0)),
                ),
                ("toolNames", JsonValue::array(tool_names)),
            ])
        }
        Err(error) => JsonValue::object([
            ("name", JsonValue::string(&server.name)),
            ("ok", JsonValue::bool(false)),
            ("enabled", JsonValue::bool(server.enabled)),
            ("sourceType", JsonValue::string(&server.source_type)),
            ("runtimeCallable", JsonValue::bool(true)),
            ("status", JsonValue::string("probe-failed")),
            (
                "timeoutMs",
                JsonValue::number(effective_timeout.as_millis()),
            ),
            (
                "elapsedMs",
                JsonValue::number(started.elapsed().as_millis()),
            ),
            ("cacheHit", JsonValue::bool(false)),
            (
                "cacheTtlMs",
                JsonValue::number(TOOL_LIST_CACHE_TTL.as_millis()),
            ),
            (
                "command",
                server
                    .command
                    .as_ref()
                    .map(|value| JsonValue::string(redact_command(value)))
                    .unwrap_or(JsonValue::Null),
            ),
            (
                "resolvedCommand",
                resolved_command
                    .as_ref()
                    .map(|value| JsonValue::string(value.display().to_string()))
                    .unwrap_or(JsonValue::Null),
            ),
            ("toolCount", JsonValue::Null),
            ("error", JsonValue::string(error)),
        ]),
    }
}

fn audit_server(
    root_path: &Path,
    server: &UpstreamServerConfig,
    timeout_ms: Option<u64>,
    refresh: bool,
) -> JsonValue {
    let started = Instant::now();
    let (runtime_callable, resolved_command, command_error) =
        server_runtime_callable(root_path, server);
    let effective_timeout = probe_timeout_for(server, timeout_ms);
    let declared_policies = tool_policy_summaries(&server.tool_policies);
    if !runtime_callable {
        let status = if !server.enabled {
            "disabled"
        } else if server.source_type != "stdio" {
            "blocked-non-stdio"
        } else {
            "blocked-command-not-found"
        };
        return JsonValue::object([
            ("name", JsonValue::string(&server.name)),
            ("ok", JsonValue::bool(false)),
            ("enabled", JsonValue::bool(server.enabled)),
            ("sourceType", JsonValue::string(&server.source_type)),
            ("runtimeCallable", JsonValue::bool(false)),
            ("status", JsonValue::string(status)),
            (
                "command",
                server
                    .command
                    .as_ref()
                    .map(|value| JsonValue::string(redact_command(value)))
                    .unwrap_or(JsonValue::Null),
            ),
            (
                "resolvedCommand",
                resolved_command
                    .as_ref()
                    .map(|value| JsonValue::string(value.display().to_string()))
                    .unwrap_or(JsonValue::Null),
            ),
            (
                "url",
                server
                    .url
                    .as_ref()
                    .map(JsonValue::string)
                    .unwrap_or(JsonValue::Null),
            ),
            (
                "declaredPolicyCount",
                JsonValue::number(declared_policies.len()),
            ),
            ("declaredPolicies", JsonValue::array(declared_policies)),
            ("toolCount", JsonValue::number(0)),
            ("annotatedToolCount", JsonValue::number(0)),
            ("unannotatedToolCount", JsonValue::number(0)),
            ("advisoryRiskToolCount", JsonValue::number(0)),
            ("guardRecommendedToolCount", JsonValue::number(0)),
            ("policyCoveredToolCount", JsonValue::number(0)),
            ("unprotectedAdvisoryRiskToolCount", JsonValue::number(0)),
            ("unprotectedGuardRecommendedToolCount", JsonValue::number(0)),
            ("unknownSemanticsToolCount", JsonValue::number(0)),
            ("reviewRecommendedToolCount", JsonValue::number(0)),
            ("tools", JsonValue::array([])),
            (
                "elapsedMs",
                JsonValue::number(started.elapsed().as_millis()),
            ),
            ("cacheHit", JsonValue::bool(false)),
            (
                "cacheTtlMs",
                JsonValue::number(TOOL_LIST_CACHE_TTL.as_millis()),
            ),
            (
                "error",
                command_error.map(JsonValue::string).unwrap_or_else(|| {
                    JsonValue::string(
                        server
                            .disabled_reason
                            .as_deref()
                            .unwrap_or(status)
                            .to_string(),
                    )
                }),
            ),
        ]);
    }

    match cached_tools_list(root_path, server, effective_timeout, refresh) {
        Ok((raw_tools, cache_hit)) => {
            let audits = raw_tools
                .as_array()
                .unwrap_or(&[])
                .iter()
                .map(|tool| audit_tool(server, tool))
                .collect::<Vec<_>>();
            let annotated_tool_count = audits.iter().filter(|item| item.has_annotations).count();
            let unannotated_tool_count = audits.len().saturating_sub(annotated_tool_count);
            let advisory_risk_tool_count =
                audits.iter().filter(|item| item.has_advisory_risk).count();
            let guard_recommended_tool_count =
                audits.iter().filter(|item| item.guard_recommended).count();
            let policy_covered_tool_count =
                audits.iter().filter(|item| item.policy_covered).count();
            let unprotected_advisory_risk_tool_count = audits
                .iter()
                .filter(|item| item.has_advisory_risk && !item.policy_covered)
                .count();
            let unprotected_guard_recommended_tool_count = audits
                .iter()
                .filter(|item| item.guard_recommended && !item.policy_covered)
                .count();
            let unknown_semantics_tool_count =
                audits.iter().filter(|item| item.unknown_semantics).count();
            let review_recommended_tool_count =
                audits.iter().filter(|item| item.review_recommended).count();
            let tools = audits
                .into_iter()
                .map(|item| item.value)
                .collect::<Vec<_>>();

            JsonValue::object([
                ("name", JsonValue::string(&server.name)),
                ("ok", JsonValue::bool(true)),
                (
                    "policyOk",
                    JsonValue::bool(unprotected_guard_recommended_tool_count == 0),
                ),
                ("enabled", JsonValue::bool(server.enabled)),
                ("sourceType", JsonValue::string(&server.source_type)),
                ("runtimeCallable", JsonValue::bool(true)),
                ("status", JsonValue::string("audited-tools")),
                (
                    "timeoutMs",
                    JsonValue::number(effective_timeout.as_millis()),
                ),
                (
                    "elapsedMs",
                    JsonValue::number(started.elapsed().as_millis()),
                ),
                ("cacheHit", JsonValue::bool(cache_hit)),
                (
                    "cacheTtlMs",
                    JsonValue::number(TOOL_LIST_CACHE_TTL.as_millis()),
                ),
                (
                    "command",
                    server
                        .command
                        .as_ref()
                        .map(|value| JsonValue::string(redact_command(value)))
                        .unwrap_or(JsonValue::Null),
                ),
                (
                    "resolvedCommand",
                    resolved_command
                        .as_ref()
                        .map(|value| JsonValue::string(value.display().to_string()))
                        .unwrap_or(JsonValue::Null),
                ),
                (
                    "declaredPolicyCount",
                    JsonValue::number(declared_policies.len()),
                ),
                ("declaredPolicies", JsonValue::array(declared_policies)),
                ("toolCount", JsonValue::number(tools.len())),
                (
                    "annotatedToolCount",
                    JsonValue::number(annotated_tool_count),
                ),
                (
                    "unannotatedToolCount",
                    JsonValue::number(unannotated_tool_count),
                ),
                (
                    "advisoryRiskToolCount",
                    JsonValue::number(advisory_risk_tool_count),
                ),
                (
                    "guardRecommendedToolCount",
                    JsonValue::number(guard_recommended_tool_count),
                ),
                (
                    "policyCoveredToolCount",
                    JsonValue::number(policy_covered_tool_count),
                ),
                (
                    "unprotectedAdvisoryRiskToolCount",
                    JsonValue::number(unprotected_advisory_risk_tool_count),
                ),
                (
                    "unprotectedGuardRecommendedToolCount",
                    JsonValue::number(unprotected_guard_recommended_tool_count),
                ),
                (
                    "unknownSemanticsToolCount",
                    JsonValue::number(unknown_semantics_tool_count),
                ),
                (
                    "reviewRecommendedToolCount",
                    JsonValue::number(review_recommended_tool_count),
                ),
                ("tools", JsonValue::array(tools)),
            ])
        }
        Err(error) => JsonValue::object([
            ("name", JsonValue::string(&server.name)),
            ("ok", JsonValue::bool(false)),
            ("enabled", JsonValue::bool(server.enabled)),
            ("sourceType", JsonValue::string(&server.source_type)),
            ("runtimeCallable", JsonValue::bool(true)),
            ("status", JsonValue::string("audit-failed")),
            (
                "timeoutMs",
                JsonValue::number(effective_timeout.as_millis()),
            ),
            (
                "elapsedMs",
                JsonValue::number(started.elapsed().as_millis()),
            ),
            ("cacheHit", JsonValue::bool(false)),
            (
                "cacheTtlMs",
                JsonValue::number(TOOL_LIST_CACHE_TTL.as_millis()),
            ),
            (
                "command",
                server
                    .command
                    .as_ref()
                    .map(|value| JsonValue::string(redact_command(value)))
                    .unwrap_or(JsonValue::Null),
            ),
            (
                "resolvedCommand",
                resolved_command
                    .as_ref()
                    .map(|value| JsonValue::string(value.display().to_string()))
                    .unwrap_or(JsonValue::Null),
            ),
            (
                "declaredPolicyCount",
                JsonValue::number(declared_policies.len()),
            ),
            ("declaredPolicies", JsonValue::array(declared_policies)),
            ("toolCount", JsonValue::number(0)),
            ("annotatedToolCount", JsonValue::number(0)),
            ("unannotatedToolCount", JsonValue::number(0)),
            ("advisoryRiskToolCount", JsonValue::number(0)),
            ("guardRecommendedToolCount", JsonValue::number(0)),
            ("policyCoveredToolCount", JsonValue::number(0)),
            ("unprotectedAdvisoryRiskToolCount", JsonValue::number(0)),
            ("unprotectedGuardRecommendedToolCount", JsonValue::number(0)),
            ("unknownSemanticsToolCount", JsonValue::number(0)),
            ("reviewRecommendedToolCount", JsonValue::number(0)),
            ("tools", JsonValue::array([])),
            ("error", JsonValue::string(error)),
        ]),
    }
}

fn server_inventory_item(root_path: &Path, server: &UpstreamServerConfig) -> JsonValue {
    let (runtime_callable, resolved_command, command_error) =
        server_runtime_callable(root_path, server);
    let status = if !server.enabled {
        "disabled"
    } else if runtime_callable {
        "callable-stdio"
    } else if server.source_type == "http" {
        "blocked-http-upstream"
    } else if server.source_type == "stdio" {
        "blocked-command-not-found"
    } else {
        "blocked-missing-command"
    };
    let reason = if !server.enabled {
        server
            .disabled_reason
            .clone()
            .unwrap_or_else(|| "server is disabled by source or policy".to_string())
    } else if runtime_callable {
        "enabled stdio server; list with upstream_tools and call with upstream_call".to_string()
    } else if let Some(error) = command_error {
        error
    } else if server.source_type == "http" {
        "non-stdio HTTP upstream fan-out is not implemented in this stdio bridge".to_string()
    } else {
        "server does not have a callable stdio command".to_string()
    };

    JsonValue::object([
        ("name", JsonValue::string(&server.name)),
        ("enabled", JsonValue::bool(server.enabled)),
        ("sourceType", JsonValue::string(&server.source_type)),
        ("runtimeCallable", JsonValue::bool(runtime_callable)),
        ("status", JsonValue::string(status)),
        ("reason", JsonValue::string(reason)),
        (
            "command",
            server
                .command
                .as_ref()
                .map(|value| JsonValue::string(redact_command(value)))
                .unwrap_or(JsonValue::Null),
        ),
        (
            "resolvedCommand",
            resolved_command
                .as_ref()
                .map(|value| JsonValue::string(value.display().to_string()))
                .unwrap_or(JsonValue::Null),
        ),
        ("argCount", JsonValue::number(server.args.len())),
        (
            "url",
            server
                .url
                .as_ref()
                .map(JsonValue::string)
                .unwrap_or(JsonValue::Null),
        ),
        (
            "cwd",
            server
                .cwd
                .as_ref()
                .map(|value| JsonValue::string(value.display().to_string()))
                .unwrap_or(JsonValue::Null),
        ),
    ])
}

fn empty_object() -> JsonValue {
    JsonValue::object::<String, Vec<(String, JsonValue)>>(Vec::new())
}

#[cfg(test)]
mod tests;
