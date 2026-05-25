use super::{
    expand_template, infer_source_type, ToolRiskPolicy, UpstreamServerConfig, UpstreamServerPolicy,
    DEFAULT_TIMEOUT_MS,
};
use crate::json::JsonValue;
use crate::json_helpers;
use crate::mcp_sources;
use crate::platform_utils;
use crate::profile;
use crate::resources;
use crate::text_utils;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

pub(super) fn context_string(value: Option<&String>) -> Option<String> {
    text_utils::trimmed_non_empty_owned(value)
}

fn normalize_policy_token(value: &str) -> String {
    text_utils::normalize_flag(value)
}

pub(super) fn optional_json_string(value: Option<String>) -> JsonValue {
    json_helpers::json_string_or_null(value)
}

pub(super) fn env_var_names_from_array(values: Option<&[JsonValue]>) -> Vec<String> {
    let mut names = Vec::new();
    let mut seen = BTreeSet::new();
    for value in values.unwrap_or(&[]) {
        match value {
            JsonValue::String(name) => insert_env_var_name(&mut names, &mut seen, name),
            JsonValue::Object(object) => {
                let source = object
                    .get("source")
                    .and_then(JsonValue::as_str)
                    .unwrap_or("local")
                    .trim();
                if source != "local" {
                    continue;
                }
                if let Some(name) = object
                    .get("name")
                    .or_else(|| object.get("key"))
                    .and_then(JsonValue::as_str)
                {
                    insert_env_var_name(&mut names, &mut seen, name);
                }
            }
            _ => {}
        }
    }
    names
}

fn insert_env_var_name(names: &mut Vec<String>, seen: &mut BTreeSet<String>, value: &str) {
    let value = value.trim();
    if value.is_empty()
        || value.contains('=')
        || value.contains('\0')
        || value.contains('\r')
        || value.contains('\n')
    {
        return;
    }
    if seen.insert(value.to_string()) {
        names.push(value.to_string());
    }
}

pub(super) fn load_servers(
    root_path: &Path,
) -> Result<BTreeMap<String, UpstreamServerConfig>, String> {
    let registry = mcp_sources::load_mcp_server_registry(root_path)?;
    let server_policies = load_upstream_server_policies(root_path)?;

    let mut parsed = BTreeMap::new();
    for entry in registry.servers.values() {
        let name = &entry.name;
        let raw = &entry.value;
        let source_enabled = json_helpers::bool_at_path(raw, &["enabled"]).unwrap_or(true);
        let policy = server_policies.get(&entry.normalized_name);
        let disabled_reason = if !source_enabled {
            Some(format!(
                "server is disabled in MCP settings source {}",
                entry.source
            ))
        } else if policy
            .map(|policy| !policy.platform_supported)
            .unwrap_or(false)
        {
            Some(format!(
                "server is not declared for the current platform '{}'",
                platform_utils::current_platform_alias()
            ))
        } else if policy
            .map(|policy| !policy.profile_enabled)
            .unwrap_or(false)
        {
            Some("server is disabled by the active MCPace runtime profile".to_string())
        } else {
            None
        };
        let enabled = disabled_reason.is_none();
        let command = json_helpers::string_at_path(raw, &["command"])
            .map(|value| expand_template(value, root_path));
        let args = json_helpers::strings_from_array(json_helpers::array_at_path(raw, &["args"]))
            .into_iter()
            .map(|value| expand_template(&value, root_path))
            .collect::<Vec<_>>();
        let url = json_helpers::string_at_path(raw, &["url"]).map(str::to_string);
        let source_type = infer_source_type(
            json_helpers::string_at_path(raw, &["type"]).unwrap_or(""),
            command.as_deref().unwrap_or(""),
            url.as_deref().unwrap_or(""),
        );
        let mut env_values = BTreeMap::new();
        if let Some(env_object) = json_helpers::object_at_path(raw, &["env"]) {
            for (key, value) in env_object {
                if let Some(text) = value.as_str() {
                    env_values.insert(key.clone(), expand_template(text, root_path));
                }
            }
        }
        for key in env_var_names_from_array(json_helpers::array_at_path(raw, &["env_vars"])) {
            if env_values.contains_key(&key) {
                continue;
            }
            if let Ok(value) = env::var(&key) {
                env_values.insert(key, value);
            }
        }
        let cwd = json_helpers::string_at_path(raw, &["cwd"])
            .map(|value| expand_template(value, root_path))
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from);
        let timeout_ms = json_helpers::value_at_path(raw, &["options", "timeout"])
            .and_then(JsonValue::as_i64)
            .or_else(|| {
                json_helpers::value_at_path(raw, &["initTimeout"]).and_then(JsonValue::as_i64)
            })
            .filter(|value| *value > 0)
            .map(|value| value as u64)
            .unwrap_or(DEFAULT_TIMEOUT_MS);

        parsed.insert(
            name.clone(),
            UpstreamServerConfig {
                name: name.clone(),
                enabled,
                disabled_reason,
                source_type,
                command,
                args,
                env: env_values,
                cwd,
                url,
                timeout_ms,
                tool_policies: policy
                    .map(|policy| policy.tool_policies.clone())
                    .unwrap_or_default(),
            },
        );
    }
    Ok(parsed)
}

fn load_upstream_server_policies(
    root_path: &Path,
) -> Result<BTreeMap<String, UpstreamServerPolicy>, String> {
    let config_path = root_path.join("mcpace.config.json");
    if !config_path.is_file() {
        return Ok(BTreeMap::new());
    }

    let value = json_helpers::read_json_file(&config_path)?;
    let runtime_profile = profile::load_runtime_profile_selection(root_path)?;
    let mut policies = BTreeMap::new();
    let Some(servers) = json_helpers::object_at_path(&value, &["servers"]) else {
        return Ok(policies);
    };

    for (server_name, raw_server) in servers {
        let required = json_helpers::bool_at_path(raw_server, &["required"]).unwrap_or(false);
        let default_enabled =
            json_helpers::bool_at_path(raw_server, &["defaultEnabled"]).unwrap_or(false);
        let override_enabled = runtime_profile
            .server_overrides
            .get(&server_name.trim().to_ascii_lowercase())
            .copied();
        let profile_enabled = if required {
            true
        } else {
            override_enabled.unwrap_or(default_enabled)
        };
        let platform_supported =
            platform_utils::supports_current_platform(&json_helpers::strings_from_array(
                json_helpers::array_at_path(raw_server, &["platforms"]),
            ));
        let mut tool_policies = Vec::new();
        if let Some(raw_policies) = json_helpers::array_at_path(raw_server, &["toolPolicies"]) {
            for raw_policy in raw_policies {
                if let Some(policy) = parse_tool_policy(raw_policy) {
                    tool_policies.push(policy);
                }
            }
        }
        policies.insert(
            server_name.trim().to_ascii_lowercase(),
            UpstreamServerPolicy {
                profile_enabled,
                platform_supported,
                tool_policies,
            },
        );
    }

    Ok(policies)
}

fn parse_tool_policy(raw: &JsonValue) -> Option<ToolRiskPolicy> {
    let tools = json_helpers::strings_from_array(json_helpers::array_at_path(raw, &["tools"]))
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    if tools.is_empty() {
        return None;
    }

    let risk_class = json_helpers::string_at_path(raw, &["riskClass"])
        .map(normalize_policy_token)
        .filter(|value| !value.is_empty());
    let allow_argument = json_helpers::string_at_path(raw, &["allowArgument"])
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    if risk_class.is_none() && allow_argument.is_none() {
        return None;
    }

    Some(ToolRiskPolicy {
        tools,
        risk_class,
        allow_argument,
        description: json_helpers::string_at_path(raw, &["description"])
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
    })
}

pub(super) fn find_server<'a>(
    servers: &'a BTreeMap<String, UpstreamServerConfig>,
    server_name: &str,
) -> Option<&'a UpstreamServerConfig> {
    servers.get(server_name).or_else(|| {
        servers
            .values()
            .find(|server| server.name.eq_ignore_ascii_case(server_name))
    })
}

pub(super) fn select_servers(
    servers: &BTreeMap<String, UpstreamServerConfig>,
    selected: Option<&str>,
) -> Vec<UpstreamServerConfig> {
    servers
        .values()
        .filter(|server| {
            selected
                .map(|name| server.name.eq_ignore_ascii_case(name))
                .unwrap_or(true)
        })
        .cloned()
        .collect()
}

pub(super) fn run_server_tasks<F>(
    root_path: &Path,
    servers: Vec<UpstreamServerConfig>,
    timeout_ms: Option<u64>,
    task: F,
) -> Vec<JsonValue>
where
    F: Fn(&Path, &UpstreamServerConfig, Option<u64>) -> JsonValue + Copy + Send + 'static,
{
    let total = servers.len();
    if total <= 1 {
        return servers
            .iter()
            .map(|server| task(root_path, server, timeout_ms))
            .collect();
    }

    let worker_limit = resources::default_worker_limit(total);
    let names = servers
        .iter()
        .map(|server| server.name.clone())
        .collect::<Vec<_>>();
    let pending = Arc::new(Mutex::new(servers.into_iter().enumerate()));
    let (tx, rx) = mpsc::channel();
    let mut handles = Vec::with_capacity(worker_limit);

    for _ in 0..worker_limit {
        let root_path = root_path.to_path_buf();
        let pending = Arc::clone(&pending);
        let tx = tx.clone();
        handles.push(thread::spawn(move || loop {
            let next = {
                let mut guard = pending
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                guard.next()
            };
            let Some((index, server)) = next else {
                break;
            };
            let name = server.name.clone();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                task(&root_path, &server, timeout_ms)
            }))
            .unwrap_or_else(|_| upstream_worker_panic_result(name));
            if tx.send((index, result)).is_err() {
                break;
            }
        }));
    }
    drop(tx);

    let mut results = (0..total).map(|_| None).collect::<Vec<_>>();
    for (index, result) in rx {
        if let Some(slot) = results.get_mut(index) {
            *slot = Some(result);
        }
    }
    for handle in handles {
        let _ = handle.join();
    }

    results
        .into_iter()
        .enumerate()
        .map(|(index, result)| {
            result.unwrap_or_else(|| {
                upstream_worker_panic_result(
                    names
                        .get(index)
                        .cloned()
                        .unwrap_or_else(|| "unknown".to_string()),
                )
            })
        })
        .collect()
}

fn upstream_worker_panic_result(name: String) -> JsonValue {
    JsonValue::object([
        ("name", JsonValue::string(name)),
        ("ok", JsonValue::bool(false)),
        ("status", JsonValue::string("worker-panicked")),
        (
            "error",
            JsonValue::string("internal upstream discovery worker panicked"),
        ),
    ])
}
