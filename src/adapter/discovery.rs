use super::{
    decode_resource_uri, encode_resource_uri, env_bool, env_usize, hex_encode,
    is_unsupported_method_error, maybe_meta_errors, mcp, normalize, prefixed_description,
    truncate_chars, unique_projected_name, ProjectedPromptTarget, ToolSurfaceOptions,
    DEFAULT_PROJECTED_SCHEMA_DESCRIPTION_CHARS, DEFAULT_SEARCH_DESCRIPTION_CHARS,
    PROJECTED_PROMPT_PREFIX, PROXIED_TEMPLATE_SCHEME,
};
use crate::json::JsonValue;
use crate::json_helpers;
use crate::server::{load_server_records, ServerRecord};
use crate::upstream;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

const DEFAULT_UPSTREAM_SEARCH_SERVER_LIMIT: usize = 6;
const DEFAULT_UPSTREAM_SEARCH_MIN_SERVER_SCORE: usize = 40;

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
    let keep_limit = limit.max(2);
    let terms = search_terms(query.unwrap_or(""));
    let selected_server = server_name.map(str::trim).filter(|value| !value.is_empty());
    let mut scored: Vec<ScoredSearchTool> = Vec::new();
    let mut search_space_tool_count = 0usize;
    let mut total_matches = 0usize;
    let mut catalog_ok = true;
    let mut errors = Vec::new();
    let mut search_strategy = "live-catalog-all".to_string();
    let mut candidate_server_count = 0usize;
    let mut candidate_server_limit = JsonValue::Null;
    let mut min_candidate_server_score = JsonValue::Null;
    let mut searched_servers = Vec::new();

    if let Some(server) = selected_server {
        search_strategy = "selected-server".to_string();
        searched_servers.push(server.to_string());
        let listing = upstream::list_tools(root_path, Some(server), timeout_ms, refresh)?;
        scan_search_listing(
            server,
            &listing,
            &terms,
            include_schema,
            keep_limit,
            &mut search_space_tool_count,
            &mut total_matches,
            &mut scored,
        );
    } else if !terms.is_empty() && !env_bool("MCPACE_UPSTREAM_SEARCH_EXHAUSTIVE").unwrap_or(false) {
        let limit = upstream_search_server_limit();
        let min_score = upstream_search_min_server_score();
        candidate_server_limit = JsonValue::number(limit);
        min_candidate_server_score = JsonValue::number(min_score);
        match ranked_upstream_search_servers(root_path, &terms, limit, min_score) {
            Ok((ranked_count, candidate_servers)) if !candidate_servers.is_empty() => {
                candidate_server_count = ranked_count;
                search_strategy = "query-ranked-candidate-catalog".to_string();
                let catalog = upstream::callable_tools_raw_catalog_for_servers(
                    root_path,
                    &candidate_servers,
                    timeout_ms,
                    refresh,
                )?;
                catalog_ok = json_helpers::bool_at_path(&catalog, &["ok"]).unwrap_or(false);
                scan_search_catalog(
                    &catalog,
                    &terms,
                    include_schema,
                    keep_limit,
                    &mut search_space_tool_count,
                    &mut total_matches,
                    &mut scored,
                    &mut searched_servers,
                    &mut errors,
                );
            }
            Ok((ranked_count, _)) => {
                candidate_server_count = ranked_count;
                let fallback_servers =
                    fallback_upstream_search_servers(root_path, limit, &mut errors);
                if fallback_servers.is_empty() {
                    search_strategy = "query-ranked-no-candidates".to_string();
                } else {
                    search_strategy = "query-ranked-fallback-catalog".to_string();
                    let catalog = upstream::callable_tools_raw_catalog_for_servers(
                        root_path,
                        &fallback_servers,
                        timeout_ms,
                        refresh,
                    )?;
                    catalog_ok = json_helpers::bool_at_path(&catalog, &["ok"]).unwrap_or(false);
                    scan_search_catalog(
                        &catalog,
                        &terms,
                        include_schema,
                        keep_limit,
                        &mut search_space_tool_count,
                        &mut total_matches,
                        &mut scored,
                        &mut searched_servers,
                        &mut errors,
                    );
                }
            }
            Err(error) => {
                errors.push(JsonValue::object([
                    ("server", JsonValue::string("*")),
                    ("method", JsonValue::string("server-candidate-ranking")),
                    ("error", JsonValue::string(error)),
                ]));
                let catalog = upstream::callable_tools_raw_catalog(root_path, timeout_ms, refresh)?;
                catalog_ok = json_helpers::bool_at_path(&catalog, &["ok"]).unwrap_or(false);
                scan_search_catalog(
                    &catalog,
                    &terms,
                    include_schema,
                    keep_limit,
                    &mut search_space_tool_count,
                    &mut total_matches,
                    &mut scored,
                    &mut searched_servers,
                    &mut errors,
                );
            }
        }
    } else {
        let catalog = upstream::callable_tools_raw_catalog(root_path, timeout_ms, refresh)?;
        catalog_ok = json_helpers::bool_at_path(&catalog, &["ok"]).unwrap_or(false);
        scan_search_catalog(
            &catalog,
            &terms,
            include_schema,
            keep_limit,
            &mut search_space_tool_count,
            &mut total_matches,
            &mut scored,
            &mut searched_servers,
            &mut errors,
        );
    }

    let selected = selected_search_result(&scored, !terms.is_empty());
    let results = scored
        .into_iter()
        .take(limit)
        .map(|(_, _, tool)| tool)
        .collect::<Vec<_>>();

    Ok(JsonValue::object([
        ("ok", JsonValue::bool(catalog_ok || !results.is_empty())),
        ("mode", JsonValue::string("upstream-search")),
        (
            "summary",
            JsonValue::string("Searched configured upstream MCP tool catalogs with bounded top-k selection and returned compact ready-to-call results. Query searches first rank candidate servers from local metadata; if metadata has no candidate, MCPace falls back to a bounded callable-server catalog scan before requiring MCPACE_UPSTREAM_SEARCH_EXHAUSTIVE=true for full fan-out. Use each call object with upstream_call, or use a projected u_<server>_<tool>_<hash> name when it appears in tools/list."),
        ),
        (
            "query",
            query
                .map(|value| JsonValue::string(value.trim()))
                .unwrap_or(JsonValue::Null),
        ),
        (
            "server",
            selected_server
                .map(JsonValue::string)
                .unwrap_or(JsonValue::Null),
        ),
        ("limit", JsonValue::number(limit)),
        ("searchStrategy", JsonValue::string(search_strategy)),
        (
            "candidateServerLimit",
            candidate_server_limit,
        ),
        (
            "minCandidateServerScore",
            min_candidate_server_score,
        ),
        (
            "candidateServerCount",
            JsonValue::number(candidate_server_count),
        ),
        (
            "searchedServerCount",
            JsonValue::number(searched_servers.len()),
        ),
        (
            "searchedServers",
            JsonValue::array(searched_servers.into_iter().map(JsonValue::string)),
        ),
        ("searchSpaceToolCount", JsonValue::number(search_space_tool_count)),
        ("matchCount", JsonValue::number(total_matches)),
        ("resultCount", JsonValue::number(results.len())),
        ("includeSchema", JsonValue::bool(include_schema)),
        ("selected", selected),
        ("results", JsonValue::array(results)),
        maybe_meta_errors(errors),
    ]))
}

type ScoredSearchTool = (usize, String, JsonValue);

#[allow(clippy::too_many_arguments)]
fn scan_search_catalog(
    catalog: &JsonValue,
    terms: &[String],
    include_schema: bool,
    keep_limit: usize,
    search_space_tool_count: &mut usize,
    total_matches: &mut usize,
    scored: &mut Vec<ScoredSearchTool>,
    searched_servers: &mut Vec<String>,
    errors: &mut Vec<JsonValue>,
) {
    for listing in json_helpers::array_at_path(catalog, &["servers"]).unwrap_or(&[]) {
        let server = json_helpers::string_at_path(listing, &["name"]).unwrap_or("unknown");
        searched_servers.push(server.to_string());
        if !json_helpers::bool_at_path(listing, &["ok"]).unwrap_or(false) {
            if let Some(error) = json_helpers::string_at_path(listing, &["error"]) {
                errors.push(JsonValue::object([
                    ("server", JsonValue::string(server)),
                    ("method", JsonValue::string("tools/list")),
                    ("error", JsonValue::string(error)),
                ]));
            }
            continue;
        }
        scan_search_listing(
            server,
            listing,
            terms,
            include_schema,
            keep_limit,
            search_space_tool_count,
            total_matches,
            scored,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn scan_search_listing(
    server: &str,
    listing: &JsonValue,
    terms: &[String],
    include_schema: bool,
    keep_limit: usize,
    search_space_tool_count: &mut usize,
    total_matches: &mut usize,
    scored: &mut Vec<ScoredSearchTool>,
) {
    for raw_tool in json_helpers::array_at_path(listing, &["tools"]).unwrap_or(&[]) {
        *search_space_tool_count = search_space_tool_count.saturating_add(1);
        let Some(tool) = search_tool_view(server, raw_tool, include_schema) else {
            continue;
        };
        let score = score_tool(&tool, terms);
        if terms.is_empty() || score > 0 {
            *total_matches = total_matches.saturating_add(1);
            let key = format!(
                "{}:{}",
                json_helpers::string_at_path(&tool, &["server"]).unwrap_or(""),
                json_helpers::string_at_path(&tool, &["name"]).unwrap_or("")
            );
            insert_scored_tool_bounded(
                scored,
                (
                    score,
                    key,
                    compact_search_tool(&tool, score, include_schema),
                ),
                keep_limit,
            );
        }
    }
}

fn search_tool_view(server: &str, tool: &JsonValue, include_schema: bool) -> Option<JsonValue> {
    let name = json_helpers::string_at_path(tool, &["name"])
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let mut entries = vec![
        ("server", JsonValue::string(server)),
        ("name", JsonValue::string(name)),
        (
            "qualifiedName",
            JsonValue::string(format!("{}.{}", server, name)),
        ),
        (
            "title",
            json_helpers::value_at_path(tool, &["title"])
                .cloned()
                .unwrap_or(JsonValue::Null),
        ),
        (
            "description",
            JsonValue::string(json_helpers::string_at_path(tool, &["description"]).unwrap_or("")),
        ),
        (
            "call",
            JsonValue::object([
                ("tool", JsonValue::string("upstream_call")),
                (
                    "arguments",
                    JsonValue::object([
                        ("server", JsonValue::string(server)),
                        ("tool", JsonValue::string(name)),
                    ]),
                ),
            ]),
        ),
    ];
    if include_schema {
        if let Some(schema) = json_helpers::value_at_path(tool, &["inputSchema"]) {
            entries.push(("inputSchema", schema.clone()));
        }
    }
    Some(JsonValue::object(entries))
}

fn insert_scored_tool_bounded(
    scored: &mut Vec<ScoredSearchTool>,
    item: ScoredSearchTool,
    keep_limit: usize,
) {
    if keep_limit == 0 {
        return;
    }
    let position = scored
        .iter()
        .position(|(score, key, _)| {
            item.0 > *score || (item.0 == *score && item.1.as_str() < key.as_str())
        })
        .unwrap_or(scored.len());
    if position >= keep_limit {
        return;
    }
    scored.insert(position, item);
    if scored.len() > keep_limit {
        scored.pop();
    }
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
                "server '{}' is not declared in mcpace.config.json; MCPace can still try the merged MCP settings registry, but topology policy is unknown",
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

pub(super) fn shape_tool_for_client(tool: JsonValue, options: ToolSurfaceOptions) -> JsonValue {
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

pub(super) fn paginated_tool_list(tools: Vec<JsonValue>, cursor: Option<&str>) -> JsonValue {
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

pub(super) fn tool_names(tools: &[JsonValue]) -> Vec<String> {
    tools
        .iter()
        .filter_map(|tool| json_helpers::string_at_path(tool, &["name"]))
        .map(ToOwned::to_owned)
        .collect()
}

pub(super) fn projected_tool_definition(
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

fn upstream_search_server_limit() -> usize {
    env_usize("MCPACE_UPSTREAM_SEARCH_SERVER_LIMIT")
        .unwrap_or(DEFAULT_UPSTREAM_SEARCH_SERVER_LIMIT)
        .clamp(1, 128)
}

fn upstream_search_min_server_score() -> usize {
    env_usize("MCPACE_UPSTREAM_SEARCH_MIN_SERVER_SCORE")
        .unwrap_or(DEFAULT_UPSTREAM_SEARCH_MIN_SERVER_SCORE)
        .clamp(0, 1000)
}

fn fallback_upstream_search_servers(
    root_path: &Path,
    limit: usize,
    errors: &mut Vec<JsonValue>,
) -> Vec<String> {
    let mut names = callable_server_names(root_path, errors);
    names.sort_by_key(|name| name.to_ascii_lowercase());
    names.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    names.truncate(limit);
    names
}

fn ranked_upstream_search_servers(
    root_path: &Path,
    terms: &[String],
    limit: usize,
    min_score: usize,
) -> Result<(usize, Vec<String>), String> {
    let records = load_server_records(root_path)?;
    let mut scored = Vec::new();
    let mut seen = BTreeSet::new();

    for record in &records {
        if !record.effective_enabled || !record.source_enabled {
            continue;
        }
        let score = score_server_record_for_search(record, terms);
        if score < min_score {
            continue;
        }
        let name = record.name.trim();
        if name.is_empty() || !seen.insert(name.to_ascii_lowercase()) {
            continue;
        }
        scored.push((score, name.to_string()));
    }

    scored.sort_by(|left, right| {
        right.0.cmp(&left.0).then_with(|| {
            left.1
                .to_ascii_lowercase()
                .cmp(&right.1.to_ascii_lowercase())
        })
    });
    let ranked_count = scored.len();
    Ok((
        ranked_count,
        scored
            .into_iter()
            .take(limit)
            .map(|(_, name)| name)
            .collect(),
    ))
}

fn score_server_record_for_search(record: &ServerRecord, terms: &[String]) -> usize {
    let metadata = format!(
        "{} {} {} {} {} {} {} {} {} {} {} {} {} {}",
        record.kind,
        record.source_type,
        record.source_command,
        record.source_url,
        record.transport_preference,
        record.supported_transports.join(" "),
        record.required_commands.join(" "),
        record.launcher_kind,
        record.routing_group,
        record.scope_class,
        record.state_binding,
        record
            .tool_policies
            .iter()
            .map(JsonValue::to_compact_string)
            .collect::<Vec<_>>()
            .join(" "),
        record.installer_target,
        record.installer_package
    );
    score_search_server_fields(&record.name, &metadata, terms)
}

fn score_search_server_fields(name: &str, metadata: &str, terms: &[String]) -> usize {
    if terms.is_empty() {
        return 0;
    }
    let name = normalize(name);
    let metadata = normalize(metadata);
    let haystack = format!("{} {}", name, metadata);
    let mut score = 0usize;

    for term in terms {
        if name == *term {
            score += 120;
        }
        if name.contains(term) {
            score += 60;
        }
        if metadata.contains(term) {
            score += 12;
        }
        if haystack.contains(term) {
            score += 2;
        }
    }

    score + capability_affinity_score(&name, &haystack, terms)
}

fn capability_affinity_score(name: &str, haystack: &str, terms: &[String]) -> usize {
    let wants_browser = any_term_matches(
        terms,
        &[
            "browser",
            "page",
            "tab",
            "click",
            "navigate",
            "screenshot",
            "dom",
            "javascript",
            "localhost",
            "playwright",
            "chrome",
            "url",
        ],
    );
    if wants_browser
        && (name.contains("browser")
            || name.contains("playwright")
            || haystack.contains("browser")
            || haystack.contains("playwright")
            || haystack.contains("chrome")
            || haystack.contains("cdp"))
    {
        return 90;
    }

    let wants_git = any_term_matches(
        terms,
        &[
            "git", "repo", "branch", "commit", "pull", "push", "issue", "pr",
        ],
    );
    if wants_git
        && (name.contains("git")
            || name.contains("github")
            || haystack.contains("git")
            || haystack.contains("github"))
    {
        return 60;
    }

    let wants_docs = any_term_matches(
        terms,
        &["docs", "documentation", "reference", "library", "package"],
    );
    if wants_docs
        && (name.contains("docs")
            || name.contains("context")
            || haystack.contains("docs")
            || haystack.contains("documentation"))
    {
        return 50;
    }

    0
}

fn any_term_matches(terms: &[String], needles: &[&str]) -> bool {
    terms.iter().any(|term| {
        needles
            .iter()
            .any(|needle| term == needle || (needle.len() >= 4 && term.contains(needle)))
    })
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

pub(super) fn take_tools_with_budget(
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

pub(super) fn estimate_json_tokens(value: &JsonValue) -> usize {
    // Approximation only: enough for routing decisions without pulling a tokenizer dependency.
    // Compact JSON is a better proxy than pretty JSON because MCP payloads are serialized compactly.
    value.to_compact_string().len().saturating_add(3) / 4
}

pub(super) fn tool_projection_rank(tool: &JsonValue) -> usize {
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

#[cfg(test)]
mod tests {
    use super::{
        score_search_server_fields, search_terms, DEFAULT_UPSTREAM_SEARCH_MIN_SERVER_SCORE,
    };

    #[test]
    fn browser_action_terms_rank_browser_servers() {
        let terms = search_terms("click screenshot localhost");
        let browser_score = score_search_server_fields("browser", "stdio chrome cdp", &terms);
        let playwright_score =
            score_search_server_fields("playwright", "stdio browser automation", &terms);
        let filesystem_score = score_search_server_fields("filesystem", "stdio file read", &terms);

        assert!(browser_score > filesystem_score);
        assert!(playwright_score > filesystem_score);
        assert!(browser_score >= 90);
    }

    #[test]
    fn unrelated_terms_do_not_force_browser_candidates() {
        let terms = search_terms("database table query");
        let browser_score = score_search_server_fields("browser", "stdio chrome cdp", &terms);
        let sqlite_score =
            score_search_server_fields("sqlite", "stdio database table query", &terms);

        assert_eq!(browser_score, 0);
        assert!(sqlite_score > browser_score);
    }

    #[test]
    fn weak_metadata_matches_stay_below_candidate_threshold() {
        let terms = search_terms("totally unknown quantum banana spaceship");
        let weak_score = score_search_server_fields("everything", "unknown stdio server", &terms);
        let browser_score = score_search_server_fields("browser", "stdio chrome cdp", &terms);

        assert!(weak_score > 0);
        assert!(weak_score < DEFAULT_UPSTREAM_SEARCH_MIN_SERVER_SCORE);
        assert_eq!(browser_score, 0);
    }
}
