use super::{empty_object, ToolRiskPolicy, UpstreamServerConfig};
use crate::json::JsonValue;
use crate::json_helpers;
use std::collections::BTreeSet;

pub(super) struct ToolPolicyAudit {
    pub(super) value: JsonValue,
    pub(super) has_annotations: bool,
    pub(super) has_advisory_risk: bool,
    pub(super) guard_recommended: bool,
    pub(super) policy_covered: bool,
    pub(super) unknown_semantics: bool,
    pub(super) review_recommended: bool,
}

struct AdvisoryClassification {
    risk_classes: BTreeSet<String>,
    signals: Vec<String>,
}

pub(super) fn audit_tool(server: &UpstreamServerConfig, tool: &JsonValue) -> ToolPolicyAudit {
    let name = json_helpers::string_at_path(tool, &["name"])
        .unwrap_or("<unnamed>")
        .to_string();
    let title = json_helpers::string_at_path(tool, &["title"]).map(str::to_string);
    let description = json_helpers::string_at_path(tool, &["description"])
        .or(title.as_deref())
        .unwrap_or("")
        .to_string();
    let annotation_keys = tool_annotation_keys(tool);
    let has_annotations = !annotation_keys.is_empty();
    let annotations = json_helpers::value_at_path(tool, &["annotations"])
        .cloned()
        .unwrap_or_else(empty_object);
    let classification = classify_tool_advisory(tool);
    let matching_policies = server
        .tool_policies
        .iter()
        .filter(|policy| policy.matches_tool(&name))
        .map(tool_policy_summary)
        .collect::<Vec<_>>();
    let policy_covered = !matching_policies.is_empty();
    let has_advisory_risk = !classification.risk_classes.is_empty();
    let guard_recommended = classification
        .risk_classes
        .iter()
        .any(|risk_class| risk_class_recommends_policy(risk_class));
    let unknown_semantics = !has_annotations && !has_advisory_risk && !policy_covered;
    let review_recommended =
        ((has_advisory_risk || guard_recommended) && !policy_covered) || unknown_semantics;
    let policy_status = if guard_recommended && !policy_covered {
        "unprotected-guard-recommended"
    } else if has_advisory_risk && !policy_covered {
        "unprotected-advisory-risk"
    } else if has_advisory_risk && policy_covered {
        "covered-advisory-risk"
    } else if unknown_semantics {
        "review-unknown-semantics"
    } else if json_helpers::bool_at_path(tool, &["annotations", "readOnlyHint"]) == Some(true) {
        "read-only-annotated"
    } else if policy_covered {
        "policy-covered"
    } else {
        "no-risk-detected"
    };
    let recommendation = audit_recommendation(
        policy_status,
        guard_recommended,
        policy_covered,
        unknown_semantics,
    );

    ToolPolicyAudit {
        value: JsonValue::object([
            ("name", JsonValue::string(&name)),
            (
                "title",
                title.map(JsonValue::string).unwrap_or(JsonValue::Null),
            ),
            ("description", JsonValue::string(description)),
            ("policyStatus", JsonValue::string(policy_status)),
            ("policyCovered", JsonValue::bool(policy_covered)),
            ("guardRecommended", JsonValue::bool(guard_recommended)),
            ("reviewRecommended", JsonValue::bool(review_recommended)),
            ("hasAnnotations", JsonValue::bool(has_annotations)),
            (
                "annotationKeys",
                JsonValue::array(annotation_keys.into_iter().map(JsonValue::string)),
            ),
            ("annotations", annotations),
            (
                "advisoryRiskClasses",
                JsonValue::array(
                    classification
                        .risk_classes
                        .iter()
                        .cloned()
                        .map(JsonValue::string),
                ),
            ),
            (
                "advisorySignals",
                JsonValue::array(classification.signals.into_iter().map(JsonValue::string)),
            ),
            ("matchingPolicies", JsonValue::array(matching_policies)),
            ("recommendation", JsonValue::string(recommendation)),
        ]),
        has_annotations,
        has_advisory_risk,
        guard_recommended,
        policy_covered,
        unknown_semantics,
        review_recommended,
    }
}

fn classify_tool_advisory(tool: &JsonValue) -> AdvisoryClassification {
    let mut risk_classes = BTreeSet::new();
    let mut signals = Vec::new();
    if json_helpers::bool_at_path(tool, &["annotations", "destructiveHint"]) == Some(true) {
        add_advisory_signal(
            &mut risk_classes,
            &mut signals,
            "mutation",
            "mcp.destructiveHint=true",
        );
    }
    if json_helpers::bool_at_path(tool, &["annotations", "readOnlyHint"]) == Some(false) {
        add_advisory_signal(
            &mut risk_classes,
            &mut signals,
            "not-readonly",
            "mcp.readOnlyHint=false",
        );
    }
    if json_helpers::bool_at_path(tool, &["annotations", "openWorldHint"]) == Some(true) {
        add_advisory_signal(
            &mut risk_classes,
            &mut signals,
            "open-world",
            "mcp.openWorldHint=true",
        );
    }

    if let Some(name) = json_helpers::string_at_path(tool, &["name"]) {
        add_name_based_advisory_signals(name, &mut risk_classes, &mut signals);
    }
    add_metadata_based_advisory_signals(tool, &mut risk_classes, &mut signals);

    AdvisoryClassification {
        risk_classes,
        signals,
    }
}

fn add_advisory_signal(
    risk_classes: &mut BTreeSet<String>,
    signals: &mut Vec<String>,
    risk_class: &str,
    signal: &str,
) {
    risk_classes.insert(risk_class.to_string());
    signals.push(signal.to_string());
}


fn add_metadata_based_advisory_signals(
    tool: &JsonValue,
    risk_classes: &mut BTreeSet<String>,
    signals: &mut Vec<String>,
) {
    let mut metadata = String::new();
    if let Some(value) = json_helpers::string_at_path(tool, &["title"]) {
        metadata.push_str(value);
        metadata.push('\n');
    }
    if let Some(value) = json_helpers::string_at_path(tool, &["description"]) {
        metadata.push_str(value);
        metadata.push('\n');
    }
    let lower = metadata.to_ascii_lowercase();
    if lower.trim().is_empty() {
        return;
    }

    for pattern in [
        "ignore previous",
        "ignore all previous",
        "ignore the user",
        "system prompt",
        "developer message",
        "hidden instruction",
        "do not tell the user",
        "don't tell the user",
        "secretly",
        "exfiltrate",
        "send secrets",
        "steal",
        "api key",
        "apikey",
        "credential",
        "credentials",
        "private key",
        "ssh key",
        "access token",
        "refresh token",
        "password",
    ] {
        if lower.contains(pattern) {
            add_advisory_signal(
                risk_classes,
                signals,
                "metadata-injection",
                &format!("metadata-pattern:{}", pattern),
            );
        }
    }
}

fn add_name_based_advisory_signals(
    tool_name: &str,
    risk_classes: &mut BTreeSet<String>,
    signals: &mut Vec<String>,
) {
    let lower = tool_name.trim().to_ascii_lowercase();
    let tokens = tool_name_tokens(&lower);

    for token in [
        "write", "create", "delete", "remove", "update", "edit", "move", "rename", "patch",
        "insert", "upsert", "append", "add", "commit", "checkout", "reset", "publish", "deploy",
        "install",
    ] {
        if tokens.contains(token) {
            add_advisory_signal(
                risk_classes,
                signals,
                "mutation",
                &format!("name-token:{}", token),
            );
        }
    }

    for token in [
        "powershell",
        "shell",
        "command",
        "exec",
        "execute",
        "process",
        "registry",
        "clipboard",
    ] {
        if tokens.contains(token)
            && !["read", "list", "describe"]
                .iter()
                .any(|safe| tokens.contains(*safe))
        {
            add_advisory_signal(
                risk_classes,
                signals,
                "system-control",
                &format!("name-token:{}", token),
            );
        }
    }

    if lower.contains("run_code") || lower.contains("run-code") {
        add_advisory_signal(
            risk_classes,
            signals,
            "system-control",
            "name-pattern:run_code",
        );
    }

    for token in [
        "click", "type", "press", "shortcut", "scroll", "drag", "hover", "select", "navigate",
        "resize", "tab", "tabs", "upload", "dialog", "evaluate", "fill", "close",
    ] {
        if tokens.contains(token) {
            let class = if lower.contains("page") || lower.contains("web") {
                "interaction-control"
            } else {
                "desktop-control"
            };
            add_advisory_signal(
                risk_classes,
                signals,
                class,
                &format!("name-token:{}", token),
            );
        }
    }

    for token in [
        "javascript",
        "cdp",
        "permission",
        "permissions",
        "downloads",
        "files",
        "action",
        "clear",
        "slider",
    ] {
        if tokens.contains(token) {
            add_advisory_signal(
                risk_classes,
                signals,
                "interaction-control",
                &format!("name-token:{}", token),
            );
        }
    }

    for token in ["screenshot", "snapshot", "scrape", "screen"] {
        if tokens.contains(token) {
            let class = if lower.contains("page") || lower.contains("web") {
                "interaction-observation"
            } else {
                "desktop-observation"
            };
            add_advisory_signal(
                risk_classes,
                signals,
                class,
                &format!("name-token:{}", token),
            );
        }
    }

    for token in ["fetch", "search", "http", "url", "web", "request"] {
        if tokens.contains(token) {
            add_advisory_signal(
                risk_classes,
                signals,
                "open-world",
                &format!("name-token:{}", token),
            );
        }
    }
}

fn tool_name_tokens(lower_name: &str) -> BTreeSet<String> {
    lower_name
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn risk_class_recommends_policy(risk_class: &str) -> bool {
    matches!(
        risk_class,
        "mutation"
            | "not-readonly"
            | "interaction-control"
            | "interaction-observation"
            | "desktop-control"
            | "desktop-observation"
            | "system-control"
            | "metadata-injection"
    )
}

fn audit_recommendation(
    policy_status: &str,
    guard_recommended: bool,
    policy_covered: bool,
    unknown_semantics: bool,
) -> &'static str {
    if guard_recommended && !policy_covered {
        "Add an explicit mcpace.config.json toolPolicies entry before using this tool routinely; keep runtime enforcement declarative instead of hardcoding this tool in Rust."
    } else if policy_status == "unprotected-advisory-risk" {
        "Review the upstream tool semantics and add a toolPolicies guard if it can mutate local, remote, interactive, or host state."
    } else if unknown_semantics {
        "No MCP annotations or MCPace policy describe this tool; inspect the upstream server documentation before relying on parallel or unattended calls."
    } else if policy_covered {
        "Covered by declarative MCPace policy; callers must use the configured allow argument or risk-class opt-in for guarded calls."
    } else {
        "No guard is currently recommended from annotations or generic name heuristics."
    }
}

fn tool_annotation_keys(tool: &JsonValue) -> Vec<String> {
    json_helpers::object_at_path(tool, &["annotations"])
        .map(|object| object.keys().cloned().collect())
        .unwrap_or_default()
}

pub(super) fn tool_policy_summaries(policies: &[ToolRiskPolicy]) -> Vec<JsonValue> {
    policies.iter().map(tool_policy_summary).collect()
}

fn tool_policy_summary(policy: &ToolRiskPolicy) -> JsonValue {
    JsonValue::object([
        (
            "tools",
            JsonValue::array(policy.tools.iter().cloned().map(JsonValue::string)),
        ),
        (
            "riskClass",
            policy
                .risk_class
                .as_ref()
                .map(JsonValue::string)
                .unwrap_or(JsonValue::Null),
        ),
        (
            "allowArgument",
            policy
                .allow_argument
                .as_ref()
                .map(JsonValue::string)
                .unwrap_or(JsonValue::Null),
        ),
        (
            "description",
            policy
                .description
                .as_ref()
                .map(JsonValue::string)
                .unwrap_or(JsonValue::Null),
        ),
    ])
}
