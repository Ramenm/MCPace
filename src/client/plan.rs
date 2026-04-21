use crate::client_catalog;
use crate::server::ServerRecord;

use super::pathing::{normalize_transport, sanitize_key};
use super::model::{
    ClientPlan, RequestStrategy, ResolvedContext, ScopeResolution, ServerCoordinationPlan,
};

pub(super) fn build_plan(
    root_path: String,
    config_version: Option<String>,
    configured_client_key_name: Option<String>,
    context: ResolvedContext,
    server_records: &[ServerRecord],
) -> ClientPlan {
    let mut warnings = context.warnings.clone();
    let mut parallel_safe_servers = 0usize;
    let mut serialized_servers = 0usize;
    let mut exclusive_servers = 0usize;
    let mut requires_hub_owned_stdio = false;
    let mut server_plans = Vec::new();

    let hub_supported_ingresses = ["stdio", "streamable-http"];
    let client_target = client_catalog::find(&context.client_id);
    let mut supported_ingresses = match client_target {
        Some(target) => target
            .supported_ingresses
            .iter()
            .filter(|transport| hub_supported_ingresses.contains(transport))
            .map(|value| value.to_string())
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
        if plan.request_strategy == "exclusive-lease" {
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
            "Streamable HTTP is part of the target MCP surface, but the grouped Streamable HTTP ingress is not implemented yet in this repo; treat this as a plan, not runtime proof.".to_string(),
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
        client_target_id: client_target.map(|target| target.id.to_string()),
        client_target_family_id: client_target.map(|target| target.family_id.to_string()),
        client_target_maturity: client_target
            .map(|target| target.maturity.to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        client_target_surface_class: client_target
            .map(|target| target.surface_class.to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        client_target_surface_kind: client_target
            .map(|target| target.surface_kind.to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        client_target_documented_features: client_target
            .map(|target| {
                target
                    .documented_features
                    .iter()
                    .map(|value| (*value).to_string())
                    .collect()
            })
            .unwrap_or_default(),
        client_target_documented_constraints: client_target
            .map(|target| {
                target
                    .documented_constraints
                    .iter()
                    .map(|value| (*value).to_string())
                    .collect()
            })
            .unwrap_or_default(),
        entrypoint_mode: "single-local-hub".to_string(),
        launcher_command: "mcpace".to_string(),
        current_grouped_action: "client plan".to_string(),
        preferred_ingress: context.preferred_ingress.clone(),
        preferred_ingress_source: context.preferred_ingress_source.clone(),
        supported_ingresses,
        hub_lifecycle_implemented: true,
        client_install_implemented: false,
        client_export_implemented: false,
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
    let mut warnings = scope.warnings;
    warnings.extend(request.warnings.iter().cloned());
    warnings.sort();
    warnings.dedup();

    ServerCoordinationPlan {
        name: record.name.clone(),
        admission_state: admission_state(record),
        scope_class: record.scope_class.clone(),
        concurrency_policy: record.concurrency_policy.clone(),
        upstream_transport: resolve_upstream_transport(record),
        process_scope_key: scope.process_scope_key,
        request_strategy: request.name,
        request_mutex_key: request.mutex_key,
        session_affinity_key: scope.session_affinity_key,
        warnings,
    }
}

fn resolve_scope(record: &ServerRecord, context: &ResolvedContext) -> ScopeResolution {
    let mut warnings = Vec::new();
    let session_affinity_key = Some(format!(
        "session:{}",
        sanitize_key(&context.session_lease_id)
    ));

    let process_partition = match record.scope_class.as_str() {
        "shared-global" => "global".to_string(),
        "project-local" => match &context.project_root {
            Some(project_root) => format!("project:{}", sanitize_key(project_root)),
            None => {
                warnings.push(format!(
                    "{} is project-local but no project root was resolved; routing should pause or isolate this server until a project is known.",
                    record.name
                ));
                format!(
                    "project:pending:{}",
                    sanitize_key(&context.session_lease_id)
                )
            }
        },
        "credential-scoped" => {
            if let Some(profile_id) = &context.credential_profile_id {
                format!("credential-profile:{}", sanitize_key(profile_id))
            } else if record.credential_binding.trim().is_empty()
                || record.credential_binding == "none"
            {
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
        "shared-exclusive" => "exclusive".to_string(),
        other => {
            warnings.push(format!(
                "{} has unknown scopeClass '{}'; treating it as lease-local until the policy is tightened.",
                record.name, other
            ));
            format!("lease:{}", sanitize_key(&context.session_lease_id))
        }
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
        process_partition
    );

    let resolved_partition = process_scope_key
        .split("|partition:")
        .last()
        .unwrap_or("lease:unresolved")
        .to_string();

    ScopeResolution {
        process_partition: resolved_partition,
        process_scope_key,
        session_affinity_key,
        warnings,
    }
}

fn resolve_request_strategy(
    record: &ServerRecord,
    scope: &ScopeResolution,
    context: &ResolvedContext,
) -> RequestStrategy {
    match record.concurrency_policy.as_str() {
        "multi-reader" => RequestStrategy {
            name: "parallel-safe".to_string(),
            mutex_key: None,
            warnings: Vec::new(),
        },
        "isolated-per-project" => {
            let mut warnings = Vec::new();
            if context.project_root.is_none() {
                warnings.push(format!(
                    "{} declares isolated-per-project but the current plan has no project root; the hub should refuse shared routing until a project is resolved.",
                    record.name
                ));
            }
            RequestStrategy {
                name: "serialize-per-project-instance".to_string(),
                mutex_key: Some(format!(
                    "server:{}|project-mutex:{}",
                    sanitize_key(&record.name),
                    scope.process_partition
                )),
                warnings,
            }
        }
        "single-writer" => RequestStrategy {
            name: "serialize-per-instance".to_string(),
            mutex_key: Some(format!(
                "server:{}|instance-mutex:{}",
                sanitize_key(&record.name),
                sanitize_key(&scope.process_scope_key)
            )),
            warnings: Vec::new(),
        },
        "single-session" => RequestStrategy {
            name: "exclusive-lease".to_string(),
            mutex_key: Some(format!(
                "server:{}|exclusive-lease:{}",
                sanitize_key(&record.name),
                sanitize_key(&context.session_lease_id)
            )),
            warnings: Vec::new(),
        },
        other => RequestStrategy {
            name: "serialize-conservatively".to_string(),
            mutex_key: Some(format!(
                "server:{}|fallback-mutex:{}",
                sanitize_key(&record.name),
                sanitize_key(&scope.process_scope_key)
            )),
            warnings: vec![format!(
                "{} has unknown concurrencyPolicy '{}'; the plan falls back to conservative serialization.",
                record.name, other
            )],
        },
    }
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

