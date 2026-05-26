use crate::client_catalog::ClientTargetRecord;
use crate::platform_utils;
use crate::runtimepaths;
use crate::server::ServerRecord;
use std::path::Path;

use super::model::{
    ClientPlan, RequestStrategy, ResolvedContext, ScopeResolution, ServerCoordinationPlan,
};
use super::pathing::{normalize_transport, sanitize_key};

pub(super) fn build_plan(
    root_path: String,
    config_version: Option<String>,
    configured_client_key_name: Option<String>,
    context: ResolvedContext,
    client_target: Option<&ClientTargetRecord>,
    server_records: &[ServerRecord],
) -> ClientPlan {
    let mut warnings = context.warnings.clone();
    let mut parallel_safe_servers = 0usize;
    let mut serialized_servers = 0usize;
    let mut exclusive_servers = 0usize;
    let mut requires_hub_owned_stdio = false;
    let mut server_plans = Vec::new();

    let hub_supported_ingresses = ["stdio", "streamable-http"];
    let mut supported_ingresses = match client_target {
        Some(target) => target
            .supported_ingresses
            .iter()
            .filter(|transport| hub_supported_ingresses.contains(&transport.as_str()))
            .cloned()
            .collect::<Vec<_>>(),
        None => hub_supported_ingresses
            .iter()
            .map(|value| value.to_string())
            .collect(),
    };
    if supported_ingresses.is_empty() {
        supported_ingresses = hub_supported_ingresses
            .iter()
            .map(|value| value.to_string())
            .collect();
    }

    if let Some(target) = client_target {
        if !target.supports_ingress(&context.preferred_ingress) {
            warnings.push(format!(
                "Client target '{}' does not document '{}' as a native MCP ingress; the plan keeps the hub preference but export/install logic should not assume native support.",
                target.id, context.preferred_ingress
            ));
        }
        if target.has_constraint("tools-only") {
            warnings.push(format!(
                "Client surface '{}' is documented as tools-only; resources and prompts should be treated as unavailable on this surface.",
                target.id
            ));
        }
        if target.has_constraint("public-http-only") {
            warnings.push(format!(
                "Client surface '{}' only reaches public HTTP MCP servers; a future MCPace relay/public HTTP lane is required for this surface.",
                target.id
            ));
        }
        if target.has_constraint("no-remote-oauth") {
            warnings.push(format!(
                "Client surface '{}' does not document remote OAuth MCP support; prefer PAT/header-based remote config or a different surface.",
                target.id
            ));
        }
        if target.has_constraint("tool-budget-100") {
            warnings.push(format!(
                "Client surface '{}' has a documented enabled-tool budget of 100; install/export logic should budget tool exposure instead of enabling every tool by default.",
                target.id
            ));
        }
        if target.has_constraint("sse-deprecated") {
            warnings.push(format!(
                "Client surface '{}' still documents SSE, but that transport is deprecated there; plan around stdio or Streamable HTTP instead.",
                target.id
            ));
        }
    } else {
        warnings.push(
            "Client id did not match a verified client target catalog entry; the plan falls back to generic MCP host assumptions.".to_string(),
        );
    }

    for record in server_records {
        let plan = build_server_plan(record, &context);
        requires_hub_owned_stdio = requires_hub_owned_stdio || plan.upstream_transport == "stdio";
        if plan.request_strategy == "parallel-safe" {
            parallel_safe_servers += 1;
        } else {
            serialized_servers += 1;
        }
        if plan.request_strategy.starts_with("exclusive") {
            exclusive_servers += 1;
        }
        warnings.extend(plan.warnings.iter().cloned());
        server_plans.push(plan);
    }

    if context.project_root.is_none()
        && server_records.iter().any(|record| {
            record.scope_class == "project-local"
                || record.concurrency_policy == "isolated-per-project"
        })
    {
        warnings.push(
            "Project-local servers exist but no project root was resolved; the hub should avoid sharing those servers until a project is bound.".to_string(),
        );
    }
    if context.session_id.is_none()
        && server_records
            .iter()
            .any(|record| record.concurrency_policy == "single-session")
    {
        warnings.push(format!(
            "At least one server is single-session and no external session id was resolved; the future hub must keep derived lease '{}' sticky instead of collapsing traffic under one anonymous route.",
            context.session_lease_id
        ));
    }
    if context.preferred_ingress == "streamable-http" {
        warnings.push(
            format!(
                "Streamable HTTP is available through the one-port local MCPace server on {}; keep cloud/public relay expectations separate from this localhost lane.",
                runtimepaths::configured_mcp_url(Path::new(&root_path))
            ),
        );
    }
    if requires_hub_owned_stdio {
        warnings.push(
            "At least one routed server uses stdio; the hub must own the child process and arbitrate access instead of letting unrelated clients write directly to the same stream.".to_string(),
        );
    }
    if let Some(target) = client_target {
        if target.has_constraint("public-http-only") && requires_hub_owned_stdio {
            warnings.push(format!(
                "Client surface '{}' cannot consume MCPace as a local stdio launcher; expose MCPace through a public HTTP or relay lane before targeting this surface.",
                target.id
            ));
        }
    }

    warnings.sort();
    warnings.dedup();

    let session_binding_key = format!(
        "client:{}|session:{}|project:{}",
        sanitize_key(&context.client_id),
        sanitize_key(&context.session_lease_id),
        sanitize_key(context.project_root.as_deref().unwrap_or("unresolved"))
    );

    ClientPlan {
        root_path,
        config_version,
        configured_client_key_name,
        client_target_id: client_target.map(|target| target.id.clone()),
        client_target_family_id: client_target.map(|target| target.family_id.clone()),
        client_target_maturity: client_target
            .map(|target| target.maturity.clone())
            .unwrap_or_else(|| "unknown".to_string()),
        client_target_surface_class: client_target
            .map(|target| target.surface_class.clone())
            .unwrap_or_else(|| "unknown".to_string()),
        client_target_surface_kind: client_target
            .map(|target| target.surface_kind.clone())
            .unwrap_or_else(|| "unknown".to_string()),
        client_target_documented_features: client_target
            .map(|target| target.documented_features.clone())
            .unwrap_or_default(),
        client_target_documented_constraints: client_target
            .map(|target| target.documented_constraints.clone())
            .unwrap_or_default(),
        entrypoint_mode: "single-local-hub".to_string(),
        launcher_command: "mcpace".to_string(),
        current_grouped_action: "client plan".to_string(),
        preferred_ingress: context.preferred_ingress.clone(),
        preferred_ingress_source: context.preferred_ingress_source.clone(),
        supported_ingresses,
        hub_lifecycle_implemented: true,
        client_install_implemented: client_target
            .map(|target| target.supports_client_install())
            .unwrap_or(false),
        client_export_implemented: true,
        context,
        session_binding_key,
        requires_hub_owned_stdio,
        parallel_safe_servers,
        serialized_servers,
        exclusive_servers,
        warnings,
        servers: server_plans,
    }
}

fn build_server_plan(record: &ServerRecord, context: &ResolvedContext) -> ServerCoordinationPlan {
    let scope = resolve_scope(record, context);
    let request = resolve_request_strategy(record, &scope, context);
    let mut warnings = scope.warnings.clone();
    warnings.extend(request.warnings.iter().cloned());
    if record.transport_status == "legacy-compat" {
        warnings.push(format!(
            "{} uses legacy SSE compatibility; MCPace treats it as disabled-for-auto-parallelism and prefers Streamable HTTP or stdio.",
            record.name
        ));
    }
    if record.parallel_safety_class.starts_with("P0_") {
        warnings.push(format!(
            "{} has no evidence-backed parallel profile yet; the adaptive scheduler should start conservative and raise concurrency only after safe probes/runtime evidence.",
            record.name
        ));
    }
    if !platform_utils::supports_current_platform(&record.platforms) {
        warnings.push(format!(
            "{} is not declared for the current platform '{}'; installer/startup logic should skip it unless an override proves compatibility.",
            record.name,
            platform_utils::current_platform_alias()
        ));
    }
    warnings.sort();
    warnings.dedup();

    ServerCoordinationPlan {
        name: record.name.clone(),
        admission_state: admission_state(record),
        scope_class: record.scope_class.clone(),
        concurrency_policy: record.concurrency_policy.clone(),
        upstream_transport: resolve_upstream_transport(record),
        process_partition: scope.process_partition,
        process_scope_key: scope.process_scope_key,
        project_binding_key: scope.project_binding_key,
        worktree_binding_key: scope.worktree_binding_key,
        conflict_domain: scope.conflict_domain,
        host_lock_key: scope.host_lock_key,
        state_profile_key: scope.state_profile_key,
        parallelism_limit: request.parallelism_limit,
        parallel_safety_class: record.parallel_safety_class.clone(),
        runtime_type: record.runtime_type.clone(),
        state_class: record.state_class.clone(),
        effect_class: record.effect_class.clone(),
        default_pool_model: record.default_pool_model.clone(),
        worker_pool_key: scope.worker_pool_key,
        max_workers: record.max_workers,
        max_in_flight_per_worker: record.max_in_flight_per_worker,
        lock_domains: record.lock_domains.clone(),
        transport_status: record.transport_status.clone(),
        launcher_kind: record.launcher_kind.clone(),
        scheduler_lane: request.scheduler_lane,
        startup_strategy: scope.startup_strategy,
        request_strategy: request.name,
        request_mutex_key: request.mutex_key,
        session_affinity_key: scope.session_affinity_key,
        warnings,
    }
}

fn resolve_scope(record: &ServerRecord, context: &ResolvedContext) -> ScopeResolution {
    let mut warnings = Vec::new();
    let conflict_domain = if record.conflict_domain.trim().is_empty() {
        record.name.clone()
    } else {
        record.conflict_domain.clone()
    };
    let project_binding_key = if requires_project_binding(record) {
        Some(resolve_project_binding_key(record, context, &mut warnings))
    } else {
        None
    };
    let worktree_binding_key = if requires_worktree_binding(record) {
        let project_key = project_binding_key
            .clone()
            .unwrap_or_else(|| resolve_project_binding_key(record, context, &mut warnings));
        Some(format!(
            "worktree:{}|{}",
            sanitize_key(&record.worktree_binding),
            project_key
        ))
    } else {
        None
    };
    let state_profile_key = if is_state_profiled(record) {
        let project_key = project_binding_key
            .clone()
            .unwrap_or_else(|| optional_project_or_cwd_key(context));
        Some(format!(
            "state-profile:{}|{}|session:{}",
            sanitize_key(&conflict_domain),
            project_key,
            state_session_partition(record, context)
        ))
    } else {
        None
    };
    let host_lock_key = if requires_host_lock(record) {
        Some(format!(
            "host-lock:{}|kind:{}",
            sanitize_key(&conflict_domain),
            sanitize_key(if record.host_lock.trim().is_empty() {
                "host-session"
            } else {
                &record.host_lock
            })
        ))
    } else {
        None
    };

    let process_partition = if let Some(key) = &state_profile_key {
        key.clone()
    } else if let Some(key) = &host_lock_key {
        key.clone()
    } else if let Some(key) = &project_binding_key {
        key.clone()
    } else if record.scope_class == "credential-scoped" {
        credential_partition(record, context, &mut warnings)
    } else if record.scope_class == "shared-global" {
        format!("shared:{}", sanitize_key(&conflict_domain))
    } else if record.scope_class == "shared-exclusive" {
        format!("exclusive:{}", sanitize_key(&conflict_domain))
    } else {
        warnings.push(format!(
            "{} has unknown scopeClass '{}'; treating it as lease-local until the policy is tightened.",
            record.name, record.scope_class
        ));
        format!("lease:{}", sanitize_key(&context.session_lease_id))
    };

    let process_scope_key = format!(
        "server:{}|kind:{}|scope:{}|partition:{}",
        sanitize_key(&record.name),
        sanitize_key(if record.kind.trim().is_empty() {
            "unknown"
        } else {
            &record.kind
        }),
        sanitize_key(if record.scope_class.trim().is_empty() {
            "unspecified"
        } else {
            &record.scope_class
        }),
        sanitize_key(&process_partition)
    );

    let session_affinity_key = Some(if let Some(key) = &state_profile_key {
        format!("session-affinity:{}", sanitize_key(key))
    } else if let Some(key) = &host_lock_key {
        format!("session-affinity:{}", sanitize_key(key))
    } else {
        format!("session:{}", sanitize_key(&context.session_lease_id))
    });

    let worker_pool_key = format!(
        "pool:{}|model:{}|{}",
        sanitize_key(&record.name),
        sanitize_key(&record.default_pool_model),
        sanitize_key(&process_partition)
    );

    let scheduler_lane = if record.transport_status == "legacy-compat" {
        "legacy-disabled".to_string()
    } else if state_profile_key.is_some() {
        "state-profile-queue".to_string()
    } else if host_lock_key.is_some() {
        "host-lock-queue".to_string()
    } else if record.routing_group == "settings-only" || record.routing_group == "unknown-source" {
        "settings-only-conservative".to_string()
    } else if project_binding_key.is_some() {
        "project-queue".to_string()
    } else if record.concurrency_policy == "multi-reader" && record.parallelism_limit != 1 {
        "parallel-pool".to_string()
    } else {
        "shared-queue".to_string()
    };

    ScopeResolution {
        process_partition,
        process_scope_key,
        project_binding_key,
        worktree_binding_key,
        conflict_domain,
        host_lock_key,
        state_profile_key,
        parallelism_limit: record.parallelism_limit,
        parallel_safety_class: record.parallel_safety_class.clone(),
        runtime_type: record.runtime_type.clone(),
        state_class: record.state_class.clone(),
        effect_class: record.effect_class.clone(),
        default_pool_model: record.default_pool_model.clone(),
        worker_pool_key,
        max_workers: record.max_workers,
        max_in_flight_per_worker: record.max_in_flight_per_worker,
        lock_domains: record.lock_domains.clone(),
        transport_status: record.transport_status.clone(),
        launcher_kind: record.launcher_kind.clone(),
        scheduler_lane,
        startup_strategy: record.startup_strategy.clone(),
        session_affinity_key,
        warnings,
    }
}

fn resolve_request_strategy(
    record: &ServerRecord,
    scope: &ScopeResolution,
    context: &ResolvedContext,
) -> RequestStrategy {
    if record.transport_status == "legacy-compat" {
        return RequestStrategy {
            name: "legacy-compat-disabled".to_string(),
            mutex_key: Some(format!(
                "server:{}|legacy-transport-mutex",
                sanitize_key(&record.name)
            )),
            scheduler_lane: "legacy-disabled".to_string(),
            parallelism_limit: 0,
            warnings: vec![format!(
                "{} is on a legacy SSE compatibility transport; do not auto-parallelize or auto-probe it without an explicit compatibility override.",
                record.name
            )],
        };
    }

    match record.concurrency_policy.as_str() {
        "multi-reader" => {
            if let Some(host_lock_key) = &scope.host_lock_key {
                return RequestStrategy {
                    name: "serialize-per-host-lock".to_string(),
                    mutex_key: Some(host_lock_key.clone()),
                    scheduler_lane: scope.scheduler_lane.clone(),
                    parallelism_limit: 1,
                    warnings: Vec::new(),
                };
            }
            if scope.parallelism_limit == 1 {
                RequestStrategy {
                    name: "serialize-per-instance".to_string(),
                    mutex_key: Some(format!(
                        "server:{}|instance-mutex:{}",
                        sanitize_key(&record.name),
                        sanitize_key(&scope.process_scope_key)
                    )),
                    scheduler_lane: scope.scheduler_lane.clone(),
                    parallelism_limit: 1,
                    warnings: Vec::new(),
                }
            } else if record.parallel_safety_class.starts_with("P0_") {
                RequestStrategy {
                    name: "bounded-worker-pool-pending-probe".to_string(),
                    mutex_key: Some(format!(
                        "server:{}|worker-inflight:{}",
                        sanitize_key(&record.name),
                        sanitize_key(&scope.worker_pool_key)
                    )),
                    scheduler_lane: "adaptive-probe-gated-pool".to_string(),
                    parallelism_limit: record.max_workers.max(1),
                    warnings: vec![format!(
                        "{} requested multi-reader behavior but has no positive parallel-safety evidence; MCPace may use multiple isolated workers but must keep maxInFlightPerWorker=1 until probes pass.",
                        record.name
                    )],
                }
            } else {
                RequestStrategy {
                    name: "parallel-safe".to_string(),
                    mutex_key: None,
                    scheduler_lane: "parallel-pool".to_string(),
                    parallelism_limit: record.max_workers.max(scope.parallelism_limit),
                    warnings: Vec::new(),
                }
            }
        }
        "isolated-per-project" => {
            let mut warnings = Vec::new();
            if context.project_root.is_none() {
                warnings.push(format!(
                    "{} declares isolated-per-project but the current plan has no project root; the hub should refuse shared routing until a project is resolved.",
                    record.name
                ));
            }
            let mutex_source = scope
                .project_binding_key
                .as_ref()
                .unwrap_or(&scope.process_partition);
            RequestStrategy {
                name: "serialize-per-project-instance".to_string(),
                mutex_key: Some(format!(
                    "server:{}|project-mutex:{}",
                    sanitize_key(&record.name),
                    sanitize_key(mutex_source)
                )),
                scheduler_lane: scope.scheduler_lane.clone(),
                parallelism_limit: 1,
                warnings,
            }
        }
        "single-writer" => {
            let (name, mutex_key) = if let Some(key) = &scope.state_profile_key {
                ("serialize-per-state-profile", key.clone())
            } else if let Some(key) = &scope.host_lock_key {
                ("serialize-per-host-lock", key.clone())
            } else {
                (
                    "serialize-per-instance",
                    format!(
                        "server:{}|instance-mutex:{}",
                        sanitize_key(&record.name),
                        sanitize_key(&scope.process_scope_key)
                    ),
                )
            };
            RequestStrategy {
                name: name.to_string(),
                mutex_key: Some(mutex_key),
                scheduler_lane: scope.scheduler_lane.clone(),
                parallelism_limit: 1,
                warnings: Vec::new(),
            }
        }
        "single-session" => {
            let (name, mutex_key) = if let Some(key) = &scope.state_profile_key {
                ("exclusive-state-profile", key.clone())
            } else if let Some(key) = &scope.host_lock_key {
                ("exclusive-host-lock", key.clone())
            } else {
                (
                    "exclusive-lease",
                    format!(
                        "server:{}|exclusive-lease:{}",
                        sanitize_key(&record.name),
                        sanitize_key(&context.session_lease_id)
                    ),
                )
            };
            RequestStrategy {
                name: name.to_string(),
                mutex_key: Some(mutex_key),
                scheduler_lane: scope.scheduler_lane.clone(),
                parallelism_limit: 1,
                warnings: Vec::new(),
            }
        }
        other => RequestStrategy {
            name: "serialize-conservatively".to_string(),
            mutex_key: Some(format!(
                "server:{}|fallback-mutex:{}",
                sanitize_key(&record.name),
                sanitize_key(&scope.process_scope_key)
            )),
            scheduler_lane: scope.scheduler_lane.clone(),
            parallelism_limit: 1,
            warnings: vec![format!(
                "{} has unknown concurrencyPolicy '{}'; the plan falls back to conservative serialization.",
                record.name, other
            )],
        },
    }
}

fn requires_project_binding(record: &ServerRecord) -> bool {
    record.scope_class == "project-local"
        || record.concurrency_policy == "isolated-per-project"
        || record.project_root_mode == "required"
}

fn requires_worktree_binding(record: &ServerRecord) -> bool {
    !is_none_marker(&record.worktree_binding)
        || matches!(
            record.state_binding.as_str(),
            "repo"
                | "repo-path"
                | "file"
                | "file-path"
                | "db"
                | "db-file-path"
                | "project"
                | "project-index"
        )
}

fn requires_host_lock(record: &ServerRecord) -> bool {
    record.scope_class == "shared-exclusive"
        || record.state_binding == "host-desktop"
        || !is_none_marker(&record.host_lock)
}

fn is_state_profiled(record: &ServerRecord) -> bool {
    record.routing_group == "stateful"
        || record.routing_group == "interactive"
        || record.state_binding == "host-session"
        || !is_none_marker(&record.state_profile_mode)
}

fn resolve_project_binding_key(
    record: &ServerRecord,
    context: &ResolvedContext,
    warnings: &mut Vec<String>,
) -> String {
    match &context.project_root {
        Some(project_root) => format!("project:{}", sanitize_key(project_root)),
        None => {
            warnings.push(format!(
                "{} requires a project root but no project root was resolved; routing should pause or isolate this server until a project is known.",
                record.name
            ));
            format!(
                "project:pending:{}",
                sanitize_key(&context.session_lease_id)
            )
        }
    }
}

fn optional_project_or_cwd_key(context: &ResolvedContext) -> String {
    if let Some(project_root) = &context.project_root {
        format!("project:{}", sanitize_key(project_root))
    } else if let Some(cwd) = &context.cwd {
        format!("cwd:{}", sanitize_key(cwd))
    } else {
        format!("session:{}", sanitize_key(&context.session_lease_id))
    }
}

fn state_session_partition(record: &ServerRecord, context: &ResolvedContext) -> String {
    match record.state_profile_mode.as_str() {
        "project" | "project-shared" => "project-shared".to_string(),
        "host" | "global" => "host-shared".to_string(),
        _ => sanitize_key(&context.session_lease_id),
    }
}

fn credential_partition(
    record: &ServerRecord,
    context: &ResolvedContext,
    warnings: &mut Vec<String>,
) -> String {
    if let Some(profile_id) = &context.credential_profile_id {
        format!("credential-profile:{}", sanitize_key(profile_id))
    } else if record.credential_binding.trim().is_empty() || record.credential_binding == "none" {
        warnings.push(format!(
            "{} is credential-scoped but no credential profile id was resolved; the plan falls back to an opaque credential partition.",
            record.name
        ));
        "credential:opaque".to_string()
    } else {
        warnings.push(format!(
            "{} is credential-scoped but no credential profile id was resolved; the plan falls back to the coarse credential binding '{}'.",
            record.name, record.credential_binding
        ));
        format!("credential:{}", sanitize_key(&record.credential_binding))
    }
}

fn is_none_marker(value: &str) -> bool {
    let normalized = value.trim().to_ascii_lowercase();
    normalized.is_empty() || normalized == "none" || normalized == "false"
}

fn admission_state(record: &ServerRecord) -> String {
    if record.source_enabled {
        "configured-source".to_string()
    } else if record.required {
        "required-needs-source".to_string()
    } else if record.default_enabled {
        "default-needs-source".to_string()
    } else {
        "optional-disabled".to_string()
    }
}

fn resolve_upstream_transport(record: &ServerRecord) -> String {
    if record.source_type == "stdio" || record.kind.contains("stdio") {
        return "stdio".to_string();
    }
    if record.source_type == "sse-legacy" || record.source_type == "sse" {
        return "sse-legacy".to_string();
    }
    if record.source_type == "http"
        || record.source_type == "streamable-http"
        || record.transport_preference == "http"
        || record.transport_preference == "streamable-http"
        || record.kind.contains("http")
    {
        return "streamable-http".to_string();
    }
    if record
        .supported_transports
        .iter()
        .any(|transport| normalize_transport(transport) == "stdio")
    {
        return "stdio".to_string();
    }
    if record
        .supported_transports
        .iter()
        .any(|transport| normalize_transport(transport) == "streamable-http")
    {
        return "streamable-http".to_string();
    }
    "unspecified".to_string()
}
