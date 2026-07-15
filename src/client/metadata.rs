use crate::json::{self, JsonValue};
use crate::json_helpers;
use std::collections::BTreeSet;
use std::env;
use std::fmt;

use super::args::ParsedArgs;
use super::model::MetadataEnvelope;
use super::pathing::{normalize_path, normalize_transport};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ClientMetadataError {
    InvalidJson { reason: String },
}

pub(super) type ClientMetadataResult<T> = Result<T, ClientMetadataError>;

impl fmt::Display for ClientMetadataError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidJson { reason } => {
                write!(
                    formatter,
                    "failed to parse client metadata JSON: {}",
                    reason
                )
            }
        }
    }
}

impl std::error::Error for ClientMetadataError {}

impl From<ClientMetadataError> for String {
    fn from(error: ClientMetadataError) -> Self {
        error.to_string()
    }
}

pub(super) fn load_metadata(parsed: &ParsedArgs) -> ClientMetadataResult<MetadataEnvelope> {
    let raw = parsed
        .metadata_json
        .clone()
        .or_else(|| env::var("MCPACE_CLIENT_METADATA_JSON").ok())
        .unwrap_or_default();

    if raw.trim().is_empty() {
        return Ok(MetadataEnvelope::default());
    }

    let json = json::parse_str(&raw).map_err(|error| ClientMetadataError::InvalidJson {
        reason: error.to_string(),
    })?;
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

fn metadata_documents(json: &JsonValue) -> Vec<&JsonValue> {
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
mod tests;
