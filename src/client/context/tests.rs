use super::{
    best_matching_root, clean_optional_string, resolve_session_lease, resolve_string,
    SessionLeaseInput,
};

const ABSENT: u8 = 0;
const BLANK: u8 = 1;
const VALUE: u8 = 2;

#[test]
fn resolve_string_prefers_flag_then_env_then_metadata_then_fallback() {
    let (value, source) = resolve_string([
        (Some("  flag-value  ".to_string()), "flag"),
        (Some("env-value".to_string()), "env"),
        (Some("metadata-value".to_string()), "metadata"),
        (Some("fallback-value".to_string()), "fallback"),
    ]);
    assert_eq!(value.as_deref(), Some("flag-value"));
    assert_eq!(source, "flag");

    let (value, source) = resolve_string([
        (Some("   ".to_string()), "flag"),
        (Some("env-value".to_string()), "env"),
        (Some("metadata-value".to_string()), "metadata"),
        (Some("fallback-value".to_string()), "fallback"),
    ]);
    assert_eq!(value.as_deref(), Some("env-value"));
    assert_eq!(source, "env");

    let (value, source) = resolve_string([
        (None, "flag"),
        (None, "env"),
        (Some("metadata-value".to_string()), "metadata"),
        (Some("fallback-value".to_string()), "fallback"),
    ]);
    assert_eq!(value.as_deref(), Some("metadata-value"));
    assert_eq!(source, "metadata");

    let (value, source) = resolve_string([
        (None, "flag"),
        (None, "env"),
        (None, "metadata"),
        (Some("fallback-value".to_string()), "fallback"),
    ]);
    assert_eq!(value.as_deref(), Some("fallback-value"));
    assert_eq!(source, "fallback");
}

#[test]
fn clean_optional_string_drops_blank_values() {
    assert_eq!(clean_optional_string(None), None);
    assert_eq!(clean_optional_string(Some("   ".to_string())), None);
    assert_eq!(
        clean_optional_string(Some("  useful-value  ".to_string())).as_deref(),
        Some("useful-value")
    );
}

#[test]
fn resolve_session_lease_keeps_external_session_ids() {
    let (lease_id, source) = resolve_session_lease(SessionLeaseInput {
        client_id: "codex",
        session_id: Some(" sess-42 "),
        conversation_id: Some("conv-1"),
        client_instance_id: Some("client-1"),
        transport_session_id: Some("transport-1"),
        project_root: Some("/work/project"),
        cwd: Some("/work/project"),
        preferred_ingress: "stdio",
    });
    assert_eq!(lease_id, "external:sess-42");
    assert_eq!(source, "external-session-id");
}

#[test]
fn resolve_session_lease_derives_stable_fallback_from_context() {
    let left = resolve_session_lease(SessionLeaseInput {
        client_id: "claude-code",
        session_id: None,
        conversation_id: Some("conv-1"),
        client_instance_id: Some("client-1"),
        transport_session_id: Some("transport-1"),
        project_root: Some("/work/project-b"),
        cwd: Some("/work/project-b"),
        preferred_ingress: "streamable-http",
    });
    let right = resolve_session_lease(SessionLeaseInput {
        client_id: "claude-code",
        session_id: None,
        conversation_id: Some("conv-1"),
        client_instance_id: Some("client-1"),
        transport_session_id: Some("transport-1"),
        project_root: Some("/work/project-b"),
        cwd: Some("/work/project-b"),
        preferred_ingress: "streamable-http",
    });
    let different = resolve_session_lease(SessionLeaseInput {
        client_id: "claude-code",
        session_id: None,
        conversation_id: Some("conv-1"),
        client_instance_id: Some("client-1"),
        transport_session_id: Some("transport-1"),
        project_root: Some("/work/project-c"),
        cwd: Some("/work/project-c"),
        preferred_ingress: "streamable-http",
    });

    assert_eq!(left.1, "planned-fallback");
    assert_eq!(left, right);
    assert_ne!(left.0, different.0);
    assert!(left.0.starts_with("planned:"));
}

#[test]
fn best_matching_root_prefers_the_most_specific_matching_root() {
    let roots = vec![
        "/work".to_string(),
        "/work/project".to_string(),
        "/work/project/nested".to_string(),
    ];

    assert_eq!(
        best_matching_root(&roots, "/work/project/nested/src"),
        Some("/work/project/nested")
    );
    assert_eq!(best_matching_root(&roots, "/elsewhere"), None);
}

#[test]
fn resolve_string_covers_all_four_source_combinations_including_blank_values() {
    let source_names = ["primary", "secondary", "tertiary", "fallback"];

    for mask in 0usize..81 {
        let states = [
            (mask % 3) as u8,
            ((mask / 3) % 3) as u8,
            ((mask / 9) % 3) as u8,
            ((mask / 27) % 3) as u8,
        ];
        let values = states
            .iter()
            .enumerate()
            .map(|(index, state)| match *state {
                ABSENT => None,
                BLANK => Some("   ".to_string()),
                VALUE => Some(format!("  {}-value  ", source_names[index])),
                _ => unreachable!("unexpected state"),
            })
            .collect::<Vec<_>>();

        let (value, source) = resolve_string([
            (values[0].clone(), source_names[0]),
            (values[1].clone(), source_names[1]),
            (values[2].clone(), source_names[2]),
            (values[3].clone(), source_names[3]),
        ]);

        if let Some(index) = states.iter().position(|state| *state == VALUE) {
            let expected_value = format!("{}-value", source_names[index]);
            assert_eq!(
                value.as_deref(),
                Some(expected_value.as_str()),
                "mask={mask:04o}"
            );
            assert_eq!(source, source_names[index], "mask={mask:04o}");
        } else {
            assert_eq!(value, None, "mask={mask:04o}");
            assert_eq!(source, source_names[3], "mask={mask:04o}");
        }
    }
}

#[test]
fn best_matching_root_is_order_independent_across_four_root_permutations() {
    let roots = vec![
        "/work".to_string(),
        "/work/project".to_string(),
        "/work/project/nested".to_string(),
        "/work/project/nested/src".to_string(),
    ];
    let permutations = permute_roots(&roots);
    assert_eq!(permutations.len(), 24);

    for permutation in permutations {
        assert_eq!(
            best_matching_root(&permutation, "/work/project/nested/src/lib.rs"),
            Some("/work/project/nested/src")
        );
    }
}

fn permute_roots(roots: &[String]) -> Vec<Vec<String>> {
    if roots.is_empty() {
        return vec![Vec::new()];
    }

    let mut permutations = Vec::new();
    for index in 0..roots.len() {
        let current = roots[index].clone();
        let mut remainder = roots.to_vec();
        remainder.remove(index);
        for mut permutation in permute_roots(&remainder) {
            let mut next = vec![current.clone()];
            next.append(&mut permutation);
            permutations.push(next);
        }
    }
    permutations
}
