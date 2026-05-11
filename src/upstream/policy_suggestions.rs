use crate::json::JsonValue;
use crate::json_helpers;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn value_or_null(value: &JsonValue, path: &[&str]) -> JsonValue {
    json_helpers::value_at_path(value, path)
        .cloned()
        .unwrap_or(JsonValue::Null)
}

#[derive(Default)]
struct PolicySuggestionBucket {
    server: String,
    risk_class: String,
    allow_argument: String,
    tools: BTreeSet<String>,
    evidence: BTreeSet<String>,
    confidence_score: u8,
}

pub(super) fn report(audit: &JsonValue) -> JsonValue {
    let mut buckets: BTreeMap<(String, String), PolicySuggestionBucket> = BTreeMap::new();
    let mut unknown_by_server: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for server in json_helpers::array_at_path(audit, &["servers"]).unwrap_or(&[]) {
        let Some(server_name) = json_helpers::string_at_path(server, &["name"])
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        for tool in json_helpers::array_at_path(server, &["tools"]).unwrap_or(&[]) {
            let Some(tool_name) = json_helpers::string_at_path(tool, &["name"])
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                continue;
            };

            if json_helpers::string_at_path(tool, &["policyStatus"])
                == Some("review-unknown-semantics")
            {
                unknown_by_server
                    .entry(server_name.to_string())
                    .or_default()
                    .insert(tool_name.to_string());
            }

            let guard_recommended =
                json_helpers::bool_at_path(tool, &["guardRecommended"]).unwrap_or(false);
            let policy_covered =
                json_helpers::bool_at_path(tool, &["policyCovered"]).unwrap_or(false);
            if !guard_recommended || policy_covered {
                continue;
            }

            let classes = json_helpers::strings_from_array(json_helpers::array_at_path(
                tool,
                &["advisoryRiskClasses"],
            ));
            let Some(risk_class) = suggested_policy_risk_class(server_name, &classes) else {
                continue;
            };
            let key = (server_name.to_string(), risk_class.clone());
            let bucket = buckets
                .entry(key)
                .or_insert_with(|| PolicySuggestionBucket {
                    server: server_name.to_string(),
                    allow_argument: allow_argument_for_risk_class(&risk_class),
                    risk_class,
                    ..Default::default()
                });
            bucket.tools.insert(tool_name.to_string());
            for signal in json_helpers::strings_from_array(json_helpers::array_at_path(
                tool,
                &["advisorySignals"],
            )) {
                bucket.evidence.insert(signal.clone());
                bucket.confidence_score = bucket
                    .confidence_score
                    .max(policy_suggestion_signal_score(&signal));
            }
            if bucket.confidence_score == 0 {
                bucket.confidence_score = 1;
            }
        }
    }

    let suggestions = buckets
        .values()
        .map(policy_suggestion_to_json)
        .collect::<Vec<_>>();
    let suggested_tool_count = buckets
        .values()
        .map(|bucket| bucket.tools.len())
        .sum::<usize>();
    let unknown_review_tool_count = unknown_by_server.values().map(BTreeSet::len).sum::<usize>();
    let servers = policy_suggestion_servers(&buckets, &unknown_by_server);

    JsonValue::object([
        ("suggestedPolicyCount", JsonValue::number(suggestions.len())),
        (
            "suggestedToolCount",
            JsonValue::number(suggested_tool_count),
        ),
        (
            "unknownReviewToolCount",
            JsonValue::number(unknown_review_tool_count),
        ),
        ("suggestions", JsonValue::array(suggestions)),
        ("servers", JsonValue::array(servers)),
    ])
}

fn policy_suggestion_to_json(bucket: &PolicySuggestionBucket) -> JsonValue {
    let policy = JsonValue::object([
        (
            "tools",
            JsonValue::array(bucket.tools.iter().cloned().map(JsonValue::string)),
        ),
        ("riskClass", JsonValue::string(&bucket.risk_class)),
        ("allowArgument", JsonValue::string(&bucket.allow_argument)),
        (
            "description",
            JsonValue::string(policy_suggestion_description(
                &bucket.server,
                &bucket.risk_class,
            )),
        ),
    ]);
    JsonValue::object([
        ("server", JsonValue::string(&bucket.server)),
        (
            "applyPath",
            JsonValue::string(format!("servers.{}.toolPolicies", bucket.server)),
        ),
        (
            "confidence",
            JsonValue::string(policy_suggestion_confidence(bucket.confidence_score)),
        ),
        (
            "evidence",
            JsonValue::array(bucket.evidence.iter().cloned().map(JsonValue::string)),
        ),
        ("policy", policy),
    ])
}

fn policy_suggestion_servers(
    buckets: &BTreeMap<(String, String), PolicySuggestionBucket>,
    unknown_by_server: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<JsonValue> {
    let mut server_names = BTreeSet::new();
    server_names.extend(buckets.keys().map(|(server, _)| server.clone()));
    server_names.extend(unknown_by_server.keys().cloned());

    server_names
        .into_iter()
        .map(|server| {
            let suggestions = buckets
                .values()
                .filter(|bucket| bucket.server == server)
                .map(policy_suggestion_to_json)
                .collect::<Vec<_>>();
            let suggested_tool_count = suggestions
                .iter()
                .filter_map(|suggestion| {
                    json_helpers::array_at_path(suggestion, &["policy", "tools"])
                })
                .map(<[JsonValue]>::len)
                .sum::<usize>();
            let unknown_tools = unknown_by_server
                .get(&server)
                .map(|tools| {
                    tools
                        .iter()
                        .cloned()
                        .map(JsonValue::string)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            JsonValue::object([
                ("name", JsonValue::string(&server)),
                ("suggestedPolicyCount", JsonValue::number(suggestions.len())),
                (
                    "suggestedToolCount",
                    JsonValue::number(suggested_tool_count),
                ),
                (
                    "unknownReviewToolCount",
                    JsonValue::number(unknown_tools.len()),
                ),
                ("suggestions", JsonValue::array(suggestions)),
                ("unknownReviewTools", JsonValue::array(unknown_tools)),
            ])
        })
        .collect()
}

fn suggested_policy_risk_class(server_name: &str, classes: &[String]) -> Option<String> {
    for stable in [
        "interaction-control",
        "interaction-observation",
        "desktop-control",
        "desktop-observation",
        "system-control",
    ] {
        if classes.iter().any(|class| class == stable) {
            return Some(stable.to_string());
        }
    }

    if classes
        .iter()
        .any(|class| class == "mutation" || class == "not-readonly")
    {
        return Some(format!("{}-mutation", policy_slug(server_name)));
    }

    None
}

fn policy_slug(value: &str) -> String {
    let parts = value
        .trim()
        .to_ascii_lowercase()
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if parts.is_empty() {
        "upstream".to_string()
    } else {
        parts.join("-")
    }
}

fn allow_argument_for_risk_class(risk_class: &str) -> String {
    let mut output = String::from("allow");
    for part in risk_class
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
    {
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            output.push(first.to_ascii_uppercase());
            for character in chars {
                output.push(character.to_ascii_lowercase());
            }
        }
    }
    if output == "allow" {
        output.push_str("UpstreamRisk");
    }
    output
}

fn policy_suggestion_signal_score(signal: &str) -> u8 {
    if signal.starts_with("mcp.destructiveHint") || signal.starts_with("mcp.readOnlyHint=false") {
        3
    } else if signal.starts_with("name-token:") || signal.starts_with("name-pattern:") {
        2
    } else {
        1
    }
}

fn policy_suggestion_confidence(score: u8) -> &'static str {
    match score {
        3..=u8::MAX => "high",
        2 => "medium",
        _ => "low",
    }
}

fn policy_suggestion_description(server_name: &str, risk_class: &str) -> String {
    format!(
        "Suggested by upstream_policy_suggest from live tools/list annotations and name signals for server '{}'; review semantics, then keep this declarative '{}' guard if these tools mutate or control state.",
        server_name, risk_class
    )
}
