use super::{
    audit_server, catalog_server, load_servers, policy_suggestions, probe_server, run_server_tasks,
    select_servers, server_inventory_item, server_runtime_callable,
};
use crate::json::JsonValue;
use crate::json_helpers;
use std::path::Path;
use std::time::Instant;

pub fn configured_inventory(root_path: &Path) -> Result<JsonValue, String> {
    let servers = load_servers(root_path)?;
    let items = servers
        .values()
        .map(|server| server_inventory_item(root_path, server))
        .collect::<Vec<_>>();
    let callable_stdio_count = servers
        .values()
        .filter(|server| {
            server.source_type == "stdio" && server_runtime_callable(root_path, server).0
        })
        .count();
    let callable_http_count = servers
        .values()
        .filter(|server| {
            server.source_type == "http" && server_runtime_callable(root_path, server).0
        })
        .count();
    let callable_upstream_count = callable_stdio_count + callable_http_count;

    Ok(JsonValue::object([
        ("ok", JsonValue::bool(true)),
        ("mode", JsonValue::string("inventory")),
        (
            "summary",
            JsonValue::string(
                "Use upstream_tools with a server name to list a configured stdio or plain HTTP upstream; use upstream_call with server/tool/arguments for one call or upstream_batch for stateful sequences. HTTPS remote MCP endpoints should be connected through a stdio adapter such as mcp-remote until a TLS client is configured.",
            ),
        ),
        ("stdioForwardingImplemented", JsonValue::bool(true)),
        ("plainHttpForwardingImplemented", JsonValue::bool(true)),
        ("callableConfiguredStdioServerCount", JsonValue::number(callable_stdio_count)),
        ("callableConfiguredHttpServerCount", JsonValue::number(callable_http_count)),
        ("callableConfiguredUpstreamServerCount", JsonValue::number(callable_upstream_count)),
        ("servers", JsonValue::array(items)),
    ]))
}

pub fn surface_manifest(
    root_path: &Path,
    transport: &str,
    top_level_tools: Vec<String>,
    include_live_catalog: bool,
    timeout_ms: Option<u64>,
    refresh: bool,
) -> Result<JsonValue, String> {
    let upstream_inventory = configured_inventory(root_path)?;
    let configured_server_count = json_helpers::array_at_path(&upstream_inventory, &["servers"])
        .map_or(0, |items| items.len());
    let callable_stdio_count =
        json_helpers::value_at_path(&upstream_inventory, &["callableConfiguredStdioServerCount"])
            .and_then(JsonValue::as_i64)
            .unwrap_or(0);
    let callable_http_count =
        json_helpers::value_at_path(&upstream_inventory, &["callableConfiguredHttpServerCount"])
            .and_then(JsonValue::as_i64)
            .unwrap_or(0);
    let callable_upstream_count = callable_stdio_count + callable_http_count;

    let live_catalog = if include_live_catalog {
        Some(catalog_tools(root_path, None, timeout_ms, refresh)?)
    } else {
        None
    };
    let live_catalog_tool_count = live_catalog
        .as_ref()
        .and_then(|catalog| json_helpers::value_at_path(catalog, &["toolCount"]))
        .and_then(JsonValue::as_i64);

    Ok(JsonValue::object([
        ("ok", JsonValue::bool(true)),
        ("mode", JsonValue::string("surface-manifest")),
        (
            "summary",
            JsonValue::string(
                "Transparent MCPace surface contract: only MCPace management and wrapper tools are advertised as top-level MCP tools. Configured upstream tools are still real MCP tools, but they remain upstream and are discovered/called through explicit wrapper tools instead of being disguised as native MCPace tools.",
            ),
        ),
        (
            "transport",
            JsonValue::string(transport),
        ),
        (
            "configurationModel",
            JsonValue::object([
                ("name", JsonValue::string("bring-your-own-mcp-servers")),
                (
                    "serverSourceOfTruth",
                    JsonValue::string("mcp_settings.json.mcpServers"),
                ),
                (
                    "policyOverlay",
                    JsonValue::string(
                        "mcpace.config.json.servers is optional metadata for routing, concurrency, platform gates, required commands, and tool risk policies.",
                    ),
                ),
                (
                    "packagedDefaults",
                    JsonValue::object([
                        ("upstreamServersEnabled", JsonValue::bool(false)),
                        ("candidateRecommendations", JsonValue::bool(false)),
                        ("requiresHardcodedServerNames", JsonValue::bool(false)),
                    ]),
                ),
                ("arbitraryServerNames", JsonValue::bool(true)),
                ("requiresRecompileForNewServers", JsonValue::bool(false)),
                ("installsUpstreamPackages", JsonValue::bool(false)),
                (
                    "userInstallResponsibility",
                    JsonValue::string(
                        "Users install any upstream MCP server package or binary they want, then reference its command/url in the merged MCP settings registry.",
                    ),
                ),
                ("stdioAutoDiscovery", JsonValue::bool(true)),
                ("httpUpstreamForwardingImplemented", JsonValue::bool(true)),
                ("httpsUpstreamForwardingImplemented", JsonValue::bool(false)),
            ]),
        ),
        (
            "topLevelTools",
            JsonValue::object([
                (
                    "claim",
                    JsonValue::string(
                        "These are the exact tool names returned by this MCPace endpoint's tools/list.",
                    ),
                ),
                ("count", JsonValue::number(top_level_tools.len())),
                (
                    "names",
                    JsonValue::array(top_level_tools.into_iter().map(JsonValue::string)),
                ),
            ]),
        ),
        (
            "upstreamTools",
            JsonValue::object([
                (
                    "claim",
                    JsonValue::string(
                        "MCPace can advertise configured upstream tools as projected native top-level tools when the live catalog fits the configured budget. Broker tools remain available for large catalogs, strict clients, and explicit routing.",
                    ),
                ),
                ("configuredServerCount", JsonValue::number(configured_server_count)),
                (
                    "callableConfiguredStdioServerCount",
                    JsonValue::number(callable_stdio_count),
                ),
                (
                    "callableConfiguredHttpServerCount",
                    JsonValue::number(callable_http_count),
                ),
                (
                    "callableConfiguredUpstreamServerCount",
                    JsonValue::number(callable_upstream_count),
                ),
                (
                    "liveCatalogIncluded",
                    JsonValue::bool(include_live_catalog),
                ),
                (
                    "liveCatalogToolCount",
                    live_catalog_tool_count
                        .map(JsonValue::number)
                        .unwrap_or(JsonValue::Null),
                ),
                (
                    "directTopLevelProjection",
                    JsonValue::object([
                        ("enabled", JsonValue::bool(true)),
                        ("default", JsonValue::string("broker")),
                        ("mode", JsonValue::string("opt-in-budgeted-live-catalog")),
                        (
                            "reason",
                            JsonValue::string(
                                "Startup defaults avoid live upstream tools/list fan-out. Set MCPACE_TOOL_EXPOSURE=auto|hybrid|native when a client should opt into budgeted native projection from the live catalog; projected calls still go through MCPace leases and declarative tool policies.",
                            ),
                        ),
                    ]),
                ),
            ]),
        ),
        (
            "routingAndSafety",
            JsonValue::object([
                ("schedulerLeases", JsonValue::bool(true)),
                ("sessionAwareUpstreamBatch", JsonValue::bool(true)),
                ("configDrivenToolPolicies", JsonValue::bool(true)),
                ("autoPolicyWeakening", JsonValue::bool(false)),
                (
                    "policyStatement",
                    JsonValue::string(
                        "Policy suggestions are dry-run evidence only; runtime enforcement changes only when mcpace.config.json toolPolicies are updated.",
                    ),
                ),
            ]),
        ),
        (
            "recommendedFlow",
            JsonValue::array([
                JsonValue::string("surface_manifest"),
                JsonValue::string("upstream_probe"),
                JsonValue::string("upstream_policy_audit"),
                JsonValue::string("upstream_catalog"),
                JsonValue::string("upstream_tools(server)"),
                JsonValue::string("upstream_call or upstream_batch"),
            ]),
        ),
        ("upstreamInventory", upstream_inventory),
        (
            "liveCatalog",
            live_catalog.unwrap_or(JsonValue::Null),
        ),
    ]))
}

pub fn probe_servers(
    root_path: &Path,
    server_name: Option<&str>,
    timeout_ms: Option<u64>,
    refresh: bool,
) -> Result<JsonValue, String> {
    let servers = load_servers(root_path)?;
    let selected = server_name
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let selected_servers = select_servers(&servers, selected.as_deref());
    if let Some(name) = selected.as_deref() {
        if selected_servers.is_empty() {
            return Err(format!("upstream server '{}' is not configured", name));
        }
    }

    let results = run_server_tasks(
        root_path,
        selected_servers,
        timeout_ms,
        move |root, server, timeout| probe_server(root, server, timeout, refresh),
    );
    let ok_count = results
        .iter()
        .filter(|item| json_helpers::bool_at_path(item, &["ok"]).unwrap_or(false))
        .count();
    let skipped_count = results
        .iter()
        .filter(|item| json_helpers::string_at_path(item, &["status"]) == Some("disabled"))
        .count();
    let failed_count = results
        .len()
        .saturating_sub(ok_count)
        .saturating_sub(skipped_count);
    let (cache_hit_count, cache_miss_count) = catalog_cache_counts(&results);

    Ok(JsonValue::object([
        ("ok", JsonValue::bool(failed_count == 0)),
        ("mode", JsonValue::string("probe")),
        (
            "summary",
            JsonValue::string(
                "Probed configured upstream MCP servers from the merged MCP settings registry without hardcoded server names. Callable stdio servers use the short successful tools/list cache unless refresh=true is supplied; fresh probes launch the helper, request tools/list, and clean it up.",
            ),
        ),
        ("serverCount", JsonValue::number(results.len())),
        ("okCount", JsonValue::number(ok_count)),
        ("skippedCount", JsonValue::number(skipped_count)),
        ("failedCount", JsonValue::number(failed_count)),
        ("cacheHitCount", JsonValue::number(cache_hit_count)),
        ("cacheMissCount", JsonValue::number(cache_miss_count)),
        ("results", JsonValue::array(results)),
    ]))
}

pub fn catalog_tools(
    root_path: &Path,
    server_name: Option<&str>,
    timeout_ms: Option<u64>,
    refresh: bool,
) -> Result<JsonValue, String> {
    let started = Instant::now();
    let servers = load_servers(root_path)?;
    let selected = server_name
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let selected_servers = select_servers(&servers, selected.as_deref());
    if let Some(name) = selected.as_deref() {
        if selected_servers.is_empty() {
            return Err(format!("upstream server '{}' is not configured", name));
        }
    }

    let results = run_server_tasks(
        root_path,
        selected_servers,
        timeout_ms,
        move |root, server, timeout| catalog_server(root, server, timeout, refresh),
    );
    let ok_count = results
        .iter()
        .filter(|item| json_helpers::bool_at_path(item, &["ok"]).unwrap_or(false))
        .count();
    let skipped_count = results
        .iter()
        .filter(|item| json_helpers::string_at_path(item, &["status"]) == Some("disabled"))
        .count();
    let failed_count = results
        .len()
        .saturating_sub(ok_count)
        .saturating_sub(skipped_count);
    let tool_count = results
        .iter()
        .filter_map(|item| json_helpers::value_at_path(item, &["toolCount"]))
        .filter_map(JsonValue::as_i64)
        .sum::<i64>();
    let (cache_hit_count, cache_miss_count) = catalog_cache_counts(&results);
    let tools = flatten_catalog_tools(&results);

    Ok(JsonValue::object([
        ("ok", JsonValue::bool(failed_count == 0)),
        ("mode", JsonValue::string("catalog")),
        (
            "summary",
            JsonValue::string(
                "Discovered configured upstream MCP tools from the merged MCP settings registry and upstream tools/list responses without hardcoded server or tool names. The top-level tools array is a flat server-qualified catalog; use each call object with upstream_call or use upstream_batch for a stateful sequence.",
            ),
        ),
        ("serverCount", JsonValue::number(results.len())),
        ("okCount", JsonValue::number(ok_count)),
        ("skippedCount", JsonValue::number(skipped_count)),
        ("failedCount", JsonValue::number(failed_count)),
        ("toolCount", JsonValue::number(tool_count)),
        ("cacheHitCount", JsonValue::number(cache_hit_count)),
        ("cacheMissCount", JsonValue::number(cache_miss_count)),
        ("elapsedMs", JsonValue::number(started.elapsed().as_millis())),
        ("tools", JsonValue::array(tools)),
        ("servers", JsonValue::array(results)),
    ]))
}

pub fn audit_tool_policies(
    root_path: &Path,
    server_name: Option<&str>,
    timeout_ms: Option<u64>,
    refresh: bool,
) -> Result<JsonValue, String> {
    let started = Instant::now();
    let servers = load_servers(root_path)?;
    let selected = server_name
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let selected_servers = select_servers(&servers, selected.as_deref());
    if let Some(name) = selected.as_deref() {
        if selected_servers.is_empty() {
            return Err(format!("upstream server '{}' is not configured", name));
        }
    }

    let results = run_server_tasks(
        root_path,
        selected_servers,
        timeout_ms,
        move |root, server, timeout| audit_server(root, server, timeout, refresh),
    );
    let ok_count = results
        .iter()
        .filter(|item| json_helpers::bool_at_path(item, &["ok"]).unwrap_or(false))
        .count();
    let skipped_count = results
        .iter()
        .filter(|item| json_helpers::string_at_path(item, &["status"]) == Some("disabled"))
        .count();
    let failed_count = results
        .len()
        .saturating_sub(ok_count)
        .saturating_sub(skipped_count);
    let tool_count = sum_i64_at_path(&results, &["toolCount"]);
    let annotated_tool_count = sum_i64_at_path(&results, &["annotatedToolCount"]);
    let unannotated_tool_count = sum_i64_at_path(&results, &["unannotatedToolCount"]);
    let advisory_risk_tool_count = sum_i64_at_path(&results, &["advisoryRiskToolCount"]);
    let guard_recommended_tool_count = sum_i64_at_path(&results, &["guardRecommendedToolCount"]);
    let policy_covered_tool_count = sum_i64_at_path(&results, &["policyCoveredToolCount"]);
    let unprotected_advisory_risk_tool_count =
        sum_i64_at_path(&results, &["unprotectedAdvisoryRiskToolCount"]);
    let unprotected_guard_recommended_tool_count =
        sum_i64_at_path(&results, &["unprotectedGuardRecommendedToolCount"]);
    let unknown_semantics_tool_count = sum_i64_at_path(&results, &["unknownSemanticsToolCount"]);
    let review_recommended_tool_count = sum_i64_at_path(&results, &["reviewRecommendedToolCount"]);
    let (cache_hit_count, cache_miss_count) = catalog_cache_counts(&results);

    Ok(JsonValue::object([
        ("ok", JsonValue::bool(failed_count == 0)),
        (
            "policyOk",
            JsonValue::bool(failed_count == 0 && unprotected_guard_recommended_tool_count == 0),
        ),
        ("mode", JsonValue::string("policy-audit")),
        (
            "summary",
            JsonValue::string(
                "Audited configured upstream MCP tools against MCP ToolAnnotations plus explicit mcpace.config.json toolPolicies. Annotation and name-based findings are advisory; MCPace still enforces only declarative toolPolicies, because the MCP protocol does not standardize parallel-safety or mutation semantics for every server.",
            ),
        ),
        ("serverCount", JsonValue::number(results.len())),
        ("okCount", JsonValue::number(ok_count)),
        ("skippedCount", JsonValue::number(skipped_count)),
        ("failedCount", JsonValue::number(failed_count)),
        ("toolCount", JsonValue::number(tool_count)),
        ("annotatedToolCount", JsonValue::number(annotated_tool_count)),
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
        ("cacheHitCount", JsonValue::number(cache_hit_count)),
        ("cacheMissCount", JsonValue::number(cache_miss_count)),
        ("elapsedMs", JsonValue::number(started.elapsed().as_millis())),
        ("servers", JsonValue::array(results)),
    ]))
}

pub fn suggest_tool_policies(
    root_path: &Path,
    server_name: Option<&str>,
    timeout_ms: Option<u64>,
    refresh: bool,
) -> Result<JsonValue, String> {
    let started = Instant::now();
    let audit = audit_tool_policies(root_path, server_name, timeout_ms, refresh)?;
    let report = policy_suggestions::report(&audit);

    Ok(JsonValue::object([
        ("ok", JsonValue::bool(true)),
        ("mode", JsonValue::string("policy-suggest")),
        (
            "summary",
            JsonValue::string(
                "Generated declarative mcpace.config.json toolPolicies suggestions from live upstream tools/list, MCP ToolAnnotations, and generic name signals. Suggestions are safe to review and copy; runtime enforcement still only changes when the declarative config is updated.",
            ),
        ),
        ("auditOk", policy_suggestions::value_or_null(&audit, &["ok"])),
        ("auditPolicyOk", policy_suggestions::value_or_null(&audit, &["policyOk"])),
        ("auditServerCount", policy_suggestions::value_or_null(&audit, &["serverCount"])),
        ("auditToolCount", policy_suggestions::value_or_null(&audit, &["toolCount"])),
        (
            "suggestedPolicyCount",
            policy_suggestions::value_or_null(&report, &["suggestedPolicyCount"]),
        ),
        (
            "suggestedToolCount",
            policy_suggestions::value_or_null(&report, &["suggestedToolCount"]),
        ),
        (
            "unknownReviewToolCount",
            policy_suggestions::value_or_null(&report, &["unknownReviewToolCount"]),
        ),
        (
            "autoApplySafety",
            JsonValue::string(
                "dry-run-by-design: MCPace can infer policy candidates, but it should not silently weaken or mutate project policy without an explicit config update path.",
            ),
        ),
        (
            "suggestions",
            policy_suggestions::value_or_null(&report, &["suggestions"]),
        ),
        ("servers", policy_suggestions::value_or_null(&report, &["servers"])),
        ("elapsedMs", JsonValue::number(started.elapsed().as_millis())),
    ]))
}

pub(super) fn catalog_cache_counts(results: &[JsonValue]) -> (usize, usize) {
    let mut hit_count = 0;
    let mut miss_count = 0;
    for item in results {
        if !json_helpers::bool_at_path(item, &["ok"]).unwrap_or(false) {
            continue;
        }
        if json_helpers::bool_at_path(item, &["cacheHit"]).unwrap_or(false) {
            hit_count += 1;
        } else {
            miss_count += 1;
        }
    }
    (hit_count, miss_count)
}

fn sum_i64_at_path(items: &[JsonValue], path: &[&str]) -> i64 {
    items
        .iter()
        .filter_map(|item| json_helpers::value_at_path(item, path))
        .filter_map(JsonValue::as_i64)
        .sum()
}

pub(super) fn flatten_catalog_tools(results: &[JsonValue]) -> Vec<JsonValue> {
    let mut flattened = Vec::new();
    for server in results {
        if !json_helpers::bool_at_path(server, &["ok"]).unwrap_or(false) {
            continue;
        }
        let Some(server_name) = json_helpers::string_at_path(server, &["name"])
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        for tool in json_helpers::array_at_path(server, &["tools"]).unwrap_or(&[]) {
            let tool_name = json_helpers::string_at_path(tool, &["name"])
                .unwrap_or("<unnamed>")
                .to_string();
            let title = json_helpers::value_at_path(tool, &["title"])
                .cloned()
                .unwrap_or(JsonValue::Null);
            let description = json_helpers::string_at_path(tool, &["description"])
                .unwrap_or("")
                .to_string();
            flattened.push(JsonValue::object([
                ("server", JsonValue::string(server_name)),
                ("name", JsonValue::string(tool_name.clone())),
                (
                    "qualifiedName",
                    JsonValue::string(format!("{}.{}", server_name, tool_name)),
                ),
                ("title", title),
                ("description", JsonValue::string(description)),
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
            ]));
        }
    }
    flattened
}
