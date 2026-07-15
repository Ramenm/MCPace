use super::{
    load_metadata, metadata_context_hints, metadata_documents, metadata_workspace_roots, ParsedArgs,
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
