use super::args::ParsedArgs;
use crate::json::JsonValue;
use crate::json_helpers;
use crate::runtimepaths;
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Clone)]
struct ExecutionPreset {
    mode: String,
    scope_class: &'static str,
    concurrency_policy: &'static str,
    state_binding: &'static str,
    credential_binding: &'static str,
    parallelism_limit: usize,
    conflict_domain_prefix: &'static str,
    project_root_mode: &'static str,
    worktree_binding: &'static str,
    state_profile_mode: &'static str,
    host_lock: &'static str,
    startup_strategy: &'static str,
    routing_group: &'static str,
    discovery_requires_lease: bool,
    default_affinity: &'static [&'static str],
    default_reuse_policy: &'static str,
    default_queue_timeout_ms: u64,
}

pub(super) fn run(
    parsed: &ParsedArgs,
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let root_path = parsed.root_override.clone().or(default_root);
    let Some(root_path) = root_path else {
        let _ = writeln!(stderr, "mcpace root not found; expected mcpace.config.json");
        return 1;
    };
    let Some(server_name) = parsed
        .name_filter
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
    else {
        let _ = writeln!(
            stderr,
            "server set-policy requires a server name, for example: mcpace server set-policy filesystem --mode session-isolated"
        );
        return 2;
    };

    let mode = parsed.execution_mode.as_deref().unwrap_or("serialized");
    let preset = match preset_for_mode(mode) {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 2;
        }
    };

    let config_path = root_path.join("mcpace.config.json");
    let raw = match fs::read_to_string(&config_path) {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(
                stderr,
                "failed to read {}: {}",
                config_path.display(),
                error
            );
            return 1;
        }
    };
    let mut config = match crate::json::parse_str(&raw) {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(
                stderr,
                "failed to parse {}: {}",
                config_path.display(),
                error
            );
            return 1;
        }
    };

    let affinity = normalized_affinity(parsed, &preset);
    let queue_timeout_ms = parsed
        .queue_timeout_ms
        .unwrap_or(preset.default_queue_timeout_ms);
    let reuse_policy = parsed
        .reuse_policy
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(preset.default_reuse_policy)
        .to_string();
    let max_workers = if preset.mode == "disabled" {
        0
    } else {
        parsed
            .max_workers
            .unwrap_or_else(|| default_max_workers_for_preset(&preset))
    };
    let max_in_flight = if preset.mode == "disabled" {
        0
    } else {
        parsed.max_in_flight_per_worker.unwrap_or(1)
    };
    let conflict_domain = format!("{}:{}", preset.conflict_domain_prefix, server_name);

    let policy = policy_json(&preset, &conflict_domain, max_workers, max_in_flight);
    let execution = execution_json(
        &preset.mode,
        &affinity,
        queue_timeout_ms,
        &reuse_policy,
        max_workers,
        max_in_flight,
    );

    if let Err(error) = upsert_policy(&mut config, server_name, policy.clone(), execution.clone()) {
        let _ = writeln!(stderr, "{}", error);
        return 1;
    }

    let result = JsonValue::object([
        (
            "status",
            JsonValue::string(if parsed.dry_run { "planned" } else { "updated" }),
        ),
        ("server", JsonValue::string(server_name)),
        ("mode", JsonValue::string(preset.mode.clone())),
        (
            "affinity",
            JsonValue::array(affinity.iter().cloned().map(JsonValue::string)),
        ),
        ("queueTimeoutMs", JsonValue::number(queue_timeout_ms)),
        ("reusePolicy", JsonValue::string(reuse_policy.clone())),
        ("maxWorkers", JsonValue::number(max_workers)),
        ("maxInFlightPerWorker", JsonValue::number(max_in_flight)),
        (
            "configPath",
            JsonValue::string(config_path.display().to_string()),
        ),
        ("dryRun", JsonValue::bool(parsed.dry_run)),
        ("policy", policy),
        ("execution", execution),
    ]);

    if !parsed.dry_run {
        if let Err(error) = runtimepaths::write_text_atomic(
            &config_path,
            &format!("{}\n", config.to_pretty_string()),
        ) {
            let _ = writeln!(stderr, "{}", error);
            return 1;
        }
    }

    if parsed.json_output {
        let _ = writeln!(stdout, "{}", result.to_pretty_string());
        return 0;
    }

    let _ = writeln!(
        stdout,
        "{} policy for {}: mode={} affinity={} queueTimeoutMs={} reusePolicy={} workers={} inFlight={}",
        if parsed.dry_run { "Planned" } else { "Updated" },
        server_name,
        preset.mode,
        affinity.join(","),
        queue_timeout_ms,
        reuse_policy,
        max_workers,
        max_in_flight,
    );
    let _ = writeln!(
        stdout,
        "  canonical policy: scope={} concurrency={} state={} routing={}",
        preset.scope_class, preset.concurrency_policy, preset.state_binding, preset.routing_group
    );
    0
}

fn preset_for_mode(raw_mode: &str) -> Result<ExecutionPreset, String> {
    let normalized = raw_mode.trim().to_ascii_lowercase().replace('_', "-");
    let mode = match normalized.as_str() {
        "shared" | "parallel" | "parallel-safe" | "multi-reader" => "shared",
        "serialized" | "serial" | "single-writer" | "queue" | "queued" => "serialized",
        "session" | "session-isolated" | "per-session" | "single-session" => {
            "session-isolated"
        }
        "project" | "project-isolated" | "per-project" | "isolated-per-project" => {
            "project-isolated"
        }
        "pool" | "process-pool" | "worker-pool" => "pool",
        "disabled" | "off" => "disabled",
        other => {
            return Err(format!(
                "unsupported execution mode '{}'; expected shared, serialized, session-isolated, project-isolated, pool, or disabled",
                other
            ))
        }
    };

    Ok(match mode {
        "shared" => ExecutionPreset {
            mode: mode.to_string(),
            scope_class: "stateless-local",
            concurrency_policy: "multi-reader",
            state_binding: "none",
            credential_binding: "none",
            parallelism_limit: 4,
            conflict_domain_prefix: "stateless-local",
            project_root_mode: "none",
            worktree_binding: "none",
            state_profile_mode: "none",
            host_lock: "none",
            startup_strategy: "lazy-shared",
            routing_group: "shared-parallel",
            discovery_requires_lease: false,
            default_affinity: &[],
            default_reuse_policy: "shared",
            default_queue_timeout_ms: 2_000,
        },
        "serialized" => ExecutionPreset {
            mode: mode.to_string(),
            scope_class: "configured-source",
            concurrency_policy: "single-writer",
            state_binding: "runtime-source",
            credential_binding: "source-config",
            parallelism_limit: 1,
            conflict_domain_prefix: "serialized-source",
            project_root_mode: "optional",
            worktree_binding: "none",
            state_profile_mode: "optional",
            host_lock: "none",
            startup_strategy: "lazy-shared",
            routing_group: "serialized-queue",
            discovery_requires_lease: true,
            default_affinity: &["client"],
            default_reuse_policy: "sticky",
            default_queue_timeout_ms: 10_000,
        },
        "session-isolated" => ExecutionPreset {
            mode: mode.to_string(),
            scope_class: "state-profile",
            concurrency_policy: "single-session",
            state_binding: "context-store",
            credential_binding: "none",
            parallelism_limit: 1,
            conflict_domain_prefix: "state-profile",
            project_root_mode: "optional",
            worktree_binding: "none",
            state_profile_mode: "required",
            host_lock: "none",
            startup_strategy: "lazy-per-profile",
            routing_group: "session-isolated",
            discovery_requires_lease: true,
            default_affinity: &["client", "project", "chat"],
            default_reuse_policy: "sticky-session",
            default_queue_timeout_ms: 10_000,
        },
        "project-isolated" => ExecutionPreset {
            mode: mode.to_string(),
            scope_class: "project-local",
            concurrency_policy: "isolated-per-project",
            state_binding: "file",
            credential_binding: "none",
            parallelism_limit: 1,
            conflict_domain_prefix: "project-local",
            project_root_mode: "required",
            worktree_binding: "project-root",
            state_profile_mode: "none",
            host_lock: "none",
            startup_strategy: "lazy-per-project",
            routing_group: "project-isolated",
            discovery_requires_lease: true,
            default_affinity: &["client", "project"],
            default_reuse_policy: "sticky-project",
            default_queue_timeout_ms: 10_000,
        },
        "pool" => ExecutionPreset {
            mode: mode.to_string(),
            scope_class: "stateless-local",
            concurrency_policy: "multi-reader",
            state_binding: "none",
            credential_binding: "none",
            parallelism_limit: 4,
            conflict_domain_prefix: "process-pool",
            project_root_mode: "none",
            worktree_binding: "none",
            state_profile_mode: "none",
            host_lock: "none",
            startup_strategy: "warm-pool",
            routing_group: "process-pool",
            discovery_requires_lease: false,
            default_affinity: &["client"],
            default_reuse_policy: "least-busy",
            default_queue_timeout_ms: 5_000,
        },
        "disabled" => ExecutionPreset {
            mode: mode.to_string(),
            scope_class: "not-runnable",
            concurrency_policy: "plan-only",
            state_binding: "none",
            credential_binding: "none",
            parallelism_limit: 0,
            conflict_domain_prefix: "disabled-source",
            project_root_mode: "none",
            worktree_binding: "none",
            state_profile_mode: "none",
            host_lock: "none",
            startup_strategy: "disabled",
            routing_group: "disabled",
            discovery_requires_lease: false,
            default_affinity: &[],
            default_reuse_policy: "never",
            default_queue_timeout_ms: 0,
        },
        _ => unreachable!(),
    })
}

fn normalized_affinity(parsed: &ParsedArgs, preset: &ExecutionPreset) -> Vec<String> {
    let mut affinity = if parsed.affinity.is_empty() {
        preset
            .default_affinity
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>()
    } else {
        parsed.affinity.clone()
    };
    for item in &mut affinity {
        *item = item.trim().to_ascii_lowercase().replace('_', "-");
        if *item == "conversation" {
            *item = "chat".to_string();
        }
        if *item == "workspace" {
            *item = "project".to_string();
        }
    }
    affinity.retain(|item| !item.is_empty());
    affinity.sort();
    affinity.dedup();
    affinity
}

fn default_max_workers_for_preset(preset: &ExecutionPreset) -> usize {
    if preset.mode == "disabled" {
        0
    } else if preset.mode == "pool" {
        preset.parallelism_limit.max(4)
    } else {
        preset.parallelism_limit.max(1)
    }
}

fn policy_json(
    preset: &ExecutionPreset,
    conflict_domain: &str,
    max_workers: usize,
    max_in_flight: usize,
) -> JsonValue {
    let parallelism = if preset.mode == "disabled" {
        0
    } else if preset.mode == "pool" {
        max_workers.max(1)
    } else {
        preset.parallelism_limit.max(1)
    };
    let max_workers = if preset.mode == "disabled" { 0 } else { max_workers };
    let max_in_flight = if preset.mode == "disabled" {
        0
    } else {
        max_in_flight.max(1)
    };
    JsonValue::object([
        ("scopeClass", JsonValue::string(preset.scope_class)),
        (
            "concurrencyPolicy",
            JsonValue::string(preset.concurrency_policy),
        ),
        ("stateBinding", JsonValue::string(preset.state_binding)),
        (
            "credentialBinding",
            JsonValue::string(preset.credential_binding),
        ),
        ("parallelismLimit", JsonValue::number(parallelism)),
        ("maxWorkers", JsonValue::number(max_workers)),
        ("maxInFlightPerWorker", JsonValue::number(max_in_flight)),
        ("conflictDomain", JsonValue::string(conflict_domain)),
        (
            "projectRootMode",
            JsonValue::string(preset.project_root_mode),
        ),
        (
            "worktreeBinding",
            JsonValue::string(preset.worktree_binding),
        ),
        (
            "stateProfileMode",
            JsonValue::string(preset.state_profile_mode),
        ),
        ("hostLock", JsonValue::string(preset.host_lock)),
        (
            "startupStrategy",
            JsonValue::string(preset.startup_strategy),
        ),
        ("routingGroup", JsonValue::string(preset.routing_group)),
        (
            "discoveryRequiresLease",
            JsonValue::bool(preset.discovery_requires_lease),
        ),
    ])
}

fn execution_json(
    mode: &str,
    affinity: &[String],
    queue_timeout_ms: u64,
    reuse_policy: &str,
    max_workers: usize,
    max_in_flight: usize,
) -> JsonValue {
    JsonValue::object([
        ("protocol", JsonValue::string("mcpace.execution.v1")),
        ("mode", JsonValue::string(mode)),
        (
            "affinity",
            JsonValue::array(affinity.iter().cloned().map(JsonValue::string)),
        ),
        ("queueTimeoutMs", JsonValue::number(queue_timeout_ms)),
        ("reusePolicy", JsonValue::string(reuse_policy)),
        ("maxWorkers", JsonValue::number(max_workers)),
        ("maxInFlightPerWorker", JsonValue::number(max_in_flight)),
    ])
}

fn upsert_policy(
    config: &mut JsonValue,
    server_name: &str,
    policy: JsonValue,
    execution: JsonValue,
) -> Result<(), String> {
    let root = ensure_object(config, "mcpace.config.json root")?;
    let servers = ensure_child_object(root, "servers");
    let server = ensure_child_object(servers, server_name);
    if !server.contains_key("kind") {
        server.insert("kind".to_string(), JsonValue::string("configured"));
    }
    if !server.contains_key("required") {
        server.insert("required".to_string(), JsonValue::bool(false));
    }
    if !server.contains_key("defaultEnabled") {
        server.insert("defaultEnabled".to_string(), JsonValue::bool(true));
    }
    server.insert("policy".to_string(), policy);
    server.insert("execution".to_string(), execution);
    Ok(())
}

fn ensure_object<'a>(
    value: &'a mut JsonValue,
    label: &str,
) -> Result<&'a mut BTreeMap<String, JsonValue>, String> {
    if !matches!(value, JsonValue::Object(_)) {
        return Err(format!("{} must be a JSON object", label));
    }
    match value {
        JsonValue::Object(map) => Ok(map),
        _ => unreachable!(),
    }
}

fn ensure_child_object<'a>(
    parent: &'a mut BTreeMap<String, JsonValue>,
    key: &str,
) -> &'a mut BTreeMap<String, JsonValue> {
    let replace = !matches!(parent.get(key), Some(JsonValue::Object(_)));
    if replace {
        parent.insert(key.to_string(), JsonValue::Object(BTreeMap::new()));
    }
    match parent.get_mut(key) {
        Some(JsonValue::Object(map)) => map,
        _ => unreachable!(),
    }
}

#[allow(dead_code)]
fn existing_config_value(config_path: &std::path::Path) -> Result<JsonValue, String> {
    json_helpers::read_json_file(config_path)
}
