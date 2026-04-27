use crate::json::{self, JsonValue};
use crate::json_helpers;
use std::collections::BTreeSet;
use std::env;

use super::args::ParsedArgs;
use super::model::MetadataEnvelope;
use super::pathing::{normalize_path, normalize_transport};

pub(super) fn load_metadata(parsed: &ParsedArgs) -> Result<MetadataEnvelope, String> {
    let raw = parsed
        .metadata_json
        .clone()
        .or_else(|| env::var("MCPACE_CLIENT_METADATA_JSON").ok())
        .unwrap_or_default();

    if raw.trim().is_empty() {
        return Ok(MetadataEnvelope::default());
    }

    let json = json::parse_str(&raw)
        .map_err(|error| format!("failed to parse client metadata JSON: {}", error))?;
    let documents = metadata_documents(&json);
    let context_hints = metadata_context_hints(&documents);

    Ok(MetadataEnvelope {
        client_id: first_string_at_any_path(
            &documents,
            &[&["client", "id"], &["clientId"], &["clientInfo", "name"]],
        )
        .or_else(|| {
            first_string_at_any_path(&context_hints, &[&["clientProfileId"], &["clientId"]])
        }),
        session_id: first_string_at_any_path(
            &documents,
            &[&["session", "id"], &["sessionId"], &["externalSessionId"]],
        )
        .or_else(|| {
            first_string_at_any_path(&context_hints, &[&["sessionId"], &["externalSessionId"]])
        }),
        conversation_id: first_string_at_any_path(
            &documents,
            &[&["conversation", "id"], &["conversationId"], &["chatId"]],
        )
        .or_else(|| first_string_at_any_path(&context_hints, &[&["conversationId"]])),
        client_instance_id: first_string_at_any_path(
            &documents,
            &[&["clientInstanceId"], &["workspace", "id"], &["windowId"]],
        )
        .or_else(|| first_string_at_any_path(&context_hints, &[&["clientInstanceId"]])),
        transport_session_id: first_string_at_any_path(
            &documents,
            &[
                &["transportSessionId"],
                &["mcpSessionId"],
                &["headers", "Mcp-Session-Id"],
                &["headers", "mcp-session-id"],
            ],
        )
        .or_else(|| {
            first_string_at_any_path(
                &context_hints,
                &[&["transportSessionId"], &["mcpSessionId"]],
            )
        }),
        credential_profile_id: first_string_at_any_path(
            &documents,
            &[
                &["credentialProfileId"],
                &["credential", "profileId"],
                &["auth", "profileId"],
            ],
        )
        .or_else(|| {
            first_string_at_any_path(
                &context_hints,
                &[&["credentialProfileId"], &["credential", "profileId"]],
            )
        }),
        workspace_roots: metadata_workspace_roots(&documents, &context_hints),
        cwd: first_string_at_any_path(&documents, &[&["cwd"], &["workingDirectory"]])
            .map(|value| normalize_path(&value))
            .or_else(|| {
                first_string_at_any_path(&context_hints, &[&["cwd"]])
                    .map(|value| normalize_path(&value))
            }),
        transport: first_string_at_any_path(
            &documents,
            &[&["transport"], &["ingress"], &["transportPreference"]],
        )
        .map(|value| normalize_transport(&value))
        .or_else(|| {
            first_string_at_any_path(&context_hints, &[&["transport"], &["ingress"]])
                .map(|value| normalize_transport(&value))
        }),
    })
}

fn metadata_documents<'a>(json: &'a JsonValue) -> Vec<&'a JsonValue> {
    let mut documents = vec![json];
    if let Some(params) = json.get("params") {
        documents.push(params);
    }
    if let Some(payload) = json.get("payload") {
        documents.push(payload);
        if let Some(params) = payload.get("params") {
            documents.push(params);
        }
    }
    documents
}

fn metadata_context_hints<'a>(documents: &[&'a JsonValue]) -> Vec<&'a JsonValue> {
    let mut hints = Vec::new();
    for document in documents {
        if let Some(meta) = document.get("_meta") {
            if let Some(hint) = meta.get("com.mcpace/context") {
                hints.push(hint);
                continue;
            }
            if let Some(hint) = meta.get("com.mcpace.context") {
                hints.push(hint);
                continue;
            }
            if let Some(hint) = meta.get("mcpaceContext") {
                hints.push(hint);
            }
        }
    }
    hints
}

fn first_string_at_any_path(documents: &[&JsonValue], paths: &[&[&str]]) -> Option<String> {
    for document in documents {
        for path in paths {
            if let Some(value) = json_helpers::string_at_path(document, path) {
                let trimmed = value.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
    }
    None
}

fn metadata_workspace_roots(documents: &[&JsonValue], context_hints: &[&JsonValue]) -> Vec<String> {
    let mut roots = Vec::new();

    for document in documents {
        roots.extend(root_strings_from_entries(
            document.get("workspaceRoots").and_then(JsonValue::as_array),
        ));
        roots.extend(
            json_helpers::array_at_path(document, &["workspace", "roots"])
                .map(|items| root_strings_from_entries(Some(items)))
                .unwrap_or_default(),
        );
        roots.extend(root_strings_from_entries(
            document.get("workspaces").and_then(JsonValue::as_array),
        ));
        roots.extend(root_strings_from_entries(
            document.get("roots").and_then(JsonValue::as_array),
        ));
        roots.extend(
            json_helpers::array_at_path(document, &["result", "roots"])
                .map(|items| root_strings_from_entries(Some(items)))
                .unwrap_or_default(),
        );
    }

    for hint in context_hints {
        roots.extend(root_strings_from_entries(
            hint.get("workspaceRoots").and_then(JsonValue::as_array),
        ));
        roots.extend(root_strings_from_entries(
            hint.get("workspaceRootsFallback")
                .and_then(JsonValue::as_array),
        ));
    }

    let mut unique = BTreeSet::new();
    for root in roots {
        if !root.is_empty() {
            unique.insert(root);
        }
    }
    unique.into_iter().collect()
}

fn root_strings_from_entries(value: Option<&[JsonValue]>) -> Vec<String> {
    value
        .unwrap_or(&[])
        .iter()
        .filter_map(|entry| match entry {
            JsonValue::String(text) => Some(text.to_string()),
            JsonValue::Object(map) => map
                .get("uri")
                .and_then(JsonValue::as_str)
                .map(|value| value.to_string())
                .or_else(|| {
                    map.get("path")
                        .and_then(JsonValue::as_str)
                        .map(|value| value.to_string())
                })
                .or_else(|| {
                    map.get("root")
                        .and_then(JsonValue::as_str)
                        .map(|value| value.to_string())
                }),
            _ => None,
        })
        .map(|value| normalize_path(&value))
        .filter(|value| !value.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        load_metadata, metadata_context_hints, metadata_documents, metadata_workspace_roots,
        ParsedArgs,
    };
    use crate::json::JsonValue;

    #[test]
    fn load_metadata_merges_context_hints_across_multiple_depths() {
        let json = metadata_json([
            Some(JsonValue::object([
                ("clientId", JsonValue::string("root-client")),
                (
                    "workspaceRoots",
                    JsonValue::array([JsonValue::string("/work/a")]),
                ),
            ])),
            Some(JsonValue::object([(
                "sessionId",
                JsonValue::string("params-session"),
            )])),
            Some(JsonValue::object([
                (
                    "credentialProfileId",
                    JsonValue::string("payload-credential"),
                ),
                (
                    "workspaceRootsFallback",
                    JsonValue::array([JsonValue::string("file:///work/b")]),
                ),
            ])),
            Some(JsonValue::object([
                ("cwd", JsonValue::string("file:///work/project")),
                ("transport", JsonValue::string("HTTP")),
                (
                    "workspaceRoots",
                    JsonValue::array([
                        JsonValue::string("/work/a"),
                        JsonValue::object([("path", JsonValue::string("/work/c"))]),
                    ]),
                ),
            ])),
        ]);

        let envelope = load_metadata(&parsed_with_metadata(json)).expect("load metadata");

        assert_eq!(envelope.client_id.as_deref(), Some("root-client"));
        assert_eq!(envelope.session_id.as_deref(), Some("params-session"));
        assert_eq!(
            envelope.credential_profile_id.as_deref(),
            Some("payload-credential")
        );
        assert_eq!(envelope.cwd.as_deref(), Some("/work/project"));
        assert_eq!(envelope.transport.as_deref(), Some("streamable-http"));
        assert_eq!(
            envelope.workspace_roots,
            vec![
                "/work/a".to_string(),
                "/work/b".to_string(),
                "/work/c".to_string(),
            ]
        );
    }

    #[test]
    fn load_metadata_prefers_the_earliest_context_hint_across_all_four_depths() {
        let labels = ["root", "params", "payload", "payload-params"];

        for mask in 1u8..16 {
            let json = metadata_json([
                ((mask & 0b0001) != 0)
                    .then(|| JsonValue::object([("sessionId", JsonValue::string(labels[0]))])),
                ((mask & 0b0010) != 0)
                    .then(|| JsonValue::object([("sessionId", JsonValue::string(labels[1]))])),
                ((mask & 0b0100) != 0)
                    .then(|| JsonValue::object([("sessionId", JsonValue::string(labels[2]))])),
                ((mask & 0b1000) != 0)
                    .then(|| JsonValue::object([("sessionId", JsonValue::string(labels[3]))])),
            ]);

            let documents = metadata_documents(&json);
            let hints = metadata_context_hints(&documents);
            assert_eq!(hints.len(), mask.count_ones() as usize, "mask={mask:04b}");

            let envelope = load_metadata(&parsed_with_metadata(json)).expect("load metadata");
            let expected = labels[usize::from(mask.trailing_zeros() as u8)];
            assert_eq!(
                envelope.session_id.as_deref(),
                Some(expected),
                "mask={mask:04b}"
            );
        }
    }

    #[test]
    fn metadata_workspace_roots_collect_from_all_hint_depths_and_dedup() {
        let json = metadata_json([
            Some(JsonValue::object([(
                "workspaceRoots",
                JsonValue::array([JsonValue::string("/work/a")]),
            )])),
            Some(JsonValue::object([(
                "workspaceRootsFallback",
                JsonValue::array([JsonValue::string("file:///work/b")]),
            )])),
            Some(JsonValue::object([(
                "workspaceRoots",
                JsonValue::array([
                    JsonValue::string("/work/a"),
                    JsonValue::object([("uri", JsonValue::string("file:///work/c"))]),
                ]),
            )])),
            Some(JsonValue::object([(
                "workspaceRootsFallback",
                JsonValue::array([JsonValue::object([("root", JsonValue::string("/work/d"))])]),
            )])),
        ]);

        let documents = metadata_documents(&json);
        let hints = metadata_context_hints(&documents);
        let roots = metadata_workspace_roots(&documents, &hints);

        assert_eq!(
            roots,
            vec![
                "/work/a".to_string(),
                "/work/b".to_string(),
                "/work/c".to_string(),
                "/work/d".to_string(),
            ]
        );
    }

    fn parsed_with_metadata(json: JsonValue) -> ParsedArgs {
        ParsedArgs {
            metadata_json: Some(json.to_compact_string()),
            ..ParsedArgs::default()
        }
    }

    fn metadata_json(mut hints: [Option<JsonValue>; 4]) -> JsonValue {
        let mut root_entries = Vec::new();

        if let Some(hint) = hints[0].take() {
            root_entries.push(("_meta", meta_wrapper(hint)));
        }

        let mut params_entries = Vec::new();
        if let Some(hint) = hints[1].take() {
            params_entries.push(("_meta", meta_wrapper(hint)));
        }
        if !params_entries.is_empty() {
            root_entries.push(("params", JsonValue::object(params_entries)));
        }

        let mut payload_entries = Vec::new();
        if let Some(hint) = hints[2].take() {
            payload_entries.push(("_meta", meta_wrapper(hint)));
        }

        let mut payload_params_entries = Vec::new();
        if let Some(hint) = hints[3].take() {
            payload_params_entries.push(("_meta", meta_wrapper(hint)));
        }
        if !payload_params_entries.is_empty() {
            payload_entries.push(("params", JsonValue::object(payload_params_entries)));
        }
        if !payload_entries.is_empty() {
            root_entries.push(("payload", JsonValue::object(payload_entries)));
        }

        JsonValue::object(root_entries)
    }

    fn meta_wrapper(hint: JsonValue) -> JsonValue {
        JsonValue::object([("com.mcpace/context", hint)])
    }
}
