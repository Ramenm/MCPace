use super::args::ParsedArgs;
use crate::client::{runtime_plan_json, RuntimePlanRequest};
use crate::diagnostics;
use crate::json::JsonValue;
use crate::json_helpers;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::PathBuf;

pub(super) fn run(
    parsed: &ParsedArgs,
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let root_path = parsed.root_override.clone().or(default_root);
    let Some(root_path) = root_path else {
        diagnostics::stderr_line(
            stderr,
            format_args!("mcpace root not found; expected mcpace.config.json"),
        );
        return 1;
    };

    let plan = match runtime_plan_json(
        &root_path,
        RuntimePlanRequest {
            client_id: parsed.client_id.clone(),
            session_id: parsed.session_id.clone(),
            project_root: parsed.project_root.clone(),
            transport: parsed.transport.clone(),
            metadata_json: parsed.metadata_json.clone(),
        },
    ) {
        Ok(value) => value,
        Err(error) => {
            diagnostics::stderr_line(stderr, format_args!("{}", error));
            return 1;
        }
    };

    let instances = build_instances(&plan);
    let summary = JsonValue::object([
        ("serverCount", JsonValue::number(instances.len())),
        (
            "clientId",
            json_helpers::value_at_path(&plan, &["context", "clientId"])
                .cloned()
                .unwrap_or(JsonValue::Null),
        ),
        (
            "sessionLeaseId",
            json_helpers::value_at_path(&plan, &["context", "sessionLeaseId"])
                .cloned()
                .unwrap_or(JsonValue::Null),
        ),
        (
            "projectRoot",
            json_helpers::value_at_path(&plan, &["context", "projectRoot"])
                .cloned()
                .unwrap_or(JsonValue::Null),
        ),
        (
            "requiresHubOwnedStdio",
            json_helpers::value_at_path(&plan, &["requiresHubOwnedStdio"])
                .cloned()
                .unwrap_or(JsonValue::Bool(false)),
        ),
        (
            "serializedServerCount",
            json_helpers::value_at_path(&plan, &["serializedServerCount"])
                .cloned()
                .unwrap_or(JsonValue::number(0)),
        ),
        (
            "parallelSafeServerCount",
            json_helpers::value_at_path(&plan, &["parallelSafeServerCount"])
                .cloned()
                .unwrap_or(JsonValue::number(0)),
        ),
    ]);

    let result = JsonValue::object([
        ("status", JsonValue::string("planned")),
        ("summary", summary),
        ("instances", JsonValue::array(instances.clone())),
        ("plan", plan),
    ]);

    if parsed.json_output {
        let _ = writeln!(stdout, "{}", result.to_pretty_string());
        return 0;
    }

    let _ = writeln!(
        stdout,
        "Planned MCPace server instances: {}",
        instances.len()
    );
    for instance in instances {
        let server = string_at(&instance, "server");
        let mode = string_at(&instance, "mode");
        let trace = string_at(&instance, "trace");
        let lane = string_at(&instance, "schedulerLane");
        let request = string_at(&instance, "requestStrategy");
        let runtime_type = string_at(&instance, "runtimeType");
        let state_class = string_at(&instance, "stateClass");
        let effect_class = string_at(&instance, "effectClass");
        let workers = number_at(&instance, "maxWorkers");
        let in_flight = number_at(&instance, "maxInFlightPerWorker");
        let affinity = string_at(&instance, "sessionAffinityKey");
        let mutex = string_at(&instance, "requestMutexKey");
        let _ = writeln!(stdout, "- {} [{}] {}", server, mode, trace);
        let _ = writeln!(
            stdout,
            "    lane={} request={} type={}/{} effect={} workers={} inFlight={} affinity={} mutex={}",
            lane, request, runtime_type, state_class, effect_class, workers, in_flight, affinity, mutex
        );
    }
    0
}

fn build_instances(plan: &JsonValue) -> Vec<JsonValue> {
    let context = json_helpers::value_at_path(plan, &["context"]);
    let session = context
        .and_then(|value| json_helpers::string_at_path(value, &["sessionId"]))
        .or_else(|| {
            context.and_then(|value| json_helpers::string_at_path(value, &["sessionLeaseId"]))
        })
        .unwrap_or("anonymous");
    let client = context
        .and_then(|value| json_helpers::string_at_path(value, &["clientId"]))
        .unwrap_or("unknown-client");
    let project = context
        .and_then(|value| json_helpers::string_at_path(value, &["projectRoot"]))
        .unwrap_or("unresolved-project");

    json_helpers::array_at_path(plan, &["servers"])
        .unwrap_or(&[])
        .iter()
        .map(|server| instance_from_server(server, client, session, project))
        .collect()
}

fn instance_from_server(
    server: &JsonValue,
    client: &str,
    session: &str,
    project: &str,
) -> JsonValue {
    let name = json_helpers::string_at_path(server, &["name"]).unwrap_or("server");
    let process_scope_key =
        json_helpers::string_at_path(server, &["processScopeKey"]).unwrap_or(name);
    let worker_pool_key = json_helpers::string_at_path(server, &["workerPoolKey"]).unwrap_or(name);
    let request_strategy =
        json_helpers::string_at_path(server, &["requestStrategy"]).unwrap_or("unknown");
    let mode = execution_mode(server);
    let instance_id = stable_short_id(&format!(
        "{}|{}|{}",
        name, process_scope_key, worker_pool_key
    ));
    let trace_left = if mode.contains("session") {
        format!("chat={}", session)
    } else if mode.contains("project") {
        format!("project={}", project)
    } else {
        format!("client={}", client)
    };
    let trace = format!("{} -> {}#{}", trace_left, name, instance_id);
    JsonValue::object([
        ("server", JsonValue::string(name)),
        ("mode", JsonValue::string(mode)),
        ("trace", JsonValue::string(trace)),
        ("instanceId", JsonValue::string(instance_id)),
        (
            "processScopeKey",
            json_helpers::value_at_path(server, &["processScopeKey"])
                .cloned()
                .unwrap_or(JsonValue::Null),
        ),
        (
            "workerPoolKey",
            json_helpers::value_at_path(server, &["workerPoolKey"])
                .cloned()
                .unwrap_or(JsonValue::Null),
        ),
        (
            "sessionAffinityKey",
            json_helpers::value_at_path(server, &["sessionAffinityKey"])
                .cloned()
                .unwrap_or(JsonValue::Null),
        ),
        (
            "requestMutexKey",
            json_helpers::value_at_path(server, &["requestMutexKey"])
                .cloned()
                .unwrap_or(JsonValue::Null),
        ),
        (
            "schedulerLane",
            json_helpers::value_at_path(server, &["schedulerLane"])
                .cloned()
                .unwrap_or(JsonValue::Null),
        ),
        ("requestStrategy", JsonValue::string(request_strategy)),
        (
            "runtimeType",
            json_helpers::value_at_path(server, &["runtimeType"])
                .cloned()
                .unwrap_or(JsonValue::Null),
        ),
        (
            "stateClass",
            json_helpers::value_at_path(server, &["stateClass"])
                .cloned()
                .unwrap_or(JsonValue::Null),
        ),
        (
            "effectClass",
            json_helpers::value_at_path(server, &["effectClass"])
                .cloned()
                .unwrap_or(JsonValue::Null),
        ),
        (
            "parallelismLimit",
            json_helpers::value_at_path(server, &["parallelismLimit"])
                .cloned()
                .unwrap_or(JsonValue::number(1)),
        ),
        (
            "maxWorkers",
            json_helpers::value_at_path(server, &["maxWorkers"])
                .cloned()
                .unwrap_or(JsonValue::number(1)),
        ),
        (
            "maxInFlightPerWorker",
            json_helpers::value_at_path(server, &["maxInFlightPerWorker"])
                .cloned()
                .unwrap_or(JsonValue::number(1)),
        ),
        (
            "upstreamTransport",
            json_helpers::value_at_path(server, &["upstreamTransport"])
                .cloned()
                .unwrap_or(JsonValue::Null),
        ),
    ])
}

fn execution_mode(server: &JsonValue) -> String {
    let concurrency = json_helpers::string_at_path(server, &["concurrencyPolicy"]).unwrap_or("");
    let scope = json_helpers::string_at_path(server, &["scopeClass"]).unwrap_or("");
    let pool = json_helpers::string_at_path(server, &["defaultPoolModel"]).unwrap_or("");
    let parallelism = json_helpers::value_at_path(server, &["parallelismLimit"])
        .and_then(JsonValue::as_i64)
        .unwrap_or(1);
    if concurrency == "single-session" || scope == "state-profile" {
        "session-isolated".to_string()
    } else if concurrency == "isolated-per-project" || scope == "project-local" {
        "project-isolated".to_string()
    } else if concurrency == "multi-reader" && (pool.contains("pool") || parallelism > 1) {
        "pool".to_string()
    } else if concurrency == "multi-reader" {
        "shared".to_string()
    } else {
        "serialized".to_string()
    }
}

fn stable_short_id(input: &str) -> String {
    let mut hasher = DefaultHasher::new();
    input.hash(&mut hasher);
    format!("{:08x}", hasher.finish() & 0xffff_ffff)
}

fn string_at(value: &JsonValue, key: &str) -> String {
    json_helpers::value_at_path(value, &[key])
        .and_then(JsonValue::as_str)
        .unwrap_or("—")
        .to_string()
}

fn number_at(value: &JsonValue, key: &str) -> String {
    json_helpers::value_at_path(value, &[key])
        .and_then(JsonValue::as_i64)
        .map(|value| value.to_string())
        .unwrap_or_else(|| "—".to_string())
}
