use std::env;

use super::args::ParsedArgs;
use super::model::{MetadataEnvelope, ResolvedContext};
use super::pathing::{
    normalize_path, normalize_transport, path_is_within, sanitize_key, stable_hash_hex,
};

pub(super) fn resolve_context(parsed: &ParsedArgs, metadata: &MetadataEnvelope) -> ResolvedContext {
    let mut warnings = Vec::new();

    let (client_id, client_id_source) = resolve_string([
        (parsed.client_id.clone(), "flag"),
        (env::var("MCPACE_CLIENT_ID").ok(), "env"),
        (metadata.client_id.clone(), "metadata"),
        (Some("unknown-client".to_string()), "fallback"),
    ]);

    let (session_id, session_id_source) = resolve_string([
        (parsed.session_id.clone(), "flag"),
        (env::var("MCPACE_SESSION_ID").ok(), "env"),
        (metadata.session_id.clone(), "metadata"),
        (None, "none"),
    ]);

    let (conversation_id, conversation_id_source) = resolve_string([
        (env::var("MCPACE_CONVERSATION_ID").ok(), "env"),
        (None, "env"),
        (metadata.conversation_id.clone(), "metadata"),
        (None, "none"),
    ]);

    let (client_instance_id, client_instance_id_source) = resolve_string([
        (env::var("MCPACE_CLIENT_INSTANCE_ID").ok(), "env"),
        (None, "env"),
        (metadata.client_instance_id.clone(), "metadata"),
        (None, "none"),
    ]);

    let (transport_session_id, transport_session_id_source) = resolve_string([
        (env::var("MCPACE_TRANSPORT_SESSION_ID").ok(), "env"),
        (None, "env"),
        (metadata.transport_session_id.clone(), "metadata"),
        (None, "none"),
    ]);

    let (credential_profile_id, credential_profile_id_source) = resolve_string([
        (env::var("MCPACE_CREDENTIAL_PROFILE_ID").ok(), "env"),
        (None, "env"),
        (metadata.credential_profile_id.clone(), "metadata"),
        (None, "none"),
    ]);

    let (cwd, cwd_source) = resolve_string([
        (
            env::var("MCPACE_CWD")
                .ok()
                .map(|value| normalize_path(&value)),
            "env",
        ),
        (None, "env"),
        (metadata.cwd.clone(), "metadata"),
        (None, "none"),
    ]);

    let (project_root, project_root_source) =
        resolve_project_root(parsed, metadata, cwd.as_deref(), &mut warnings);

    let (preferred_ingress, preferred_ingress_source) = resolve_string([
        (
            parsed
                .transport
                .clone()
                .map(|value| normalize_transport(&value)),
            "flag",
        ),
        (
            env::var("MCPACE_CLIENT_TRANSPORT")
                .ok()
                .or_else(|| env::var("MCPACE_CLIENT_INGRESS").ok())
                .map(|value| normalize_transport(&value)),
            "env",
        ),
        (
            metadata
                .transport
                .clone()
                .map(|value| normalize_transport(&value)),
            "metadata",
        ),
        (Some("stdio".to_string()), "default-local"),
    ]);

    let client_id = client_id.unwrap_or_else(|| "unknown-client".to_string());
    let preferred_ingress = preferred_ingress.unwrap_or_else(|| "stdio".to_string());
    let session_lease = resolve_session_lease(SessionLeaseInput {
        client_id: &client_id,
        session_id: session_id.as_deref(),
        conversation_id: conversation_id.as_deref(),
        client_instance_id: client_instance_id.as_deref(),
        transport_session_id: transport_session_id.as_deref(),
        project_root: project_root.as_deref(),
        cwd: cwd.as_deref(),
        preferred_ingress: &preferred_ingress,
    });

    if session_id.is_none() {
        warnings.push(format!(
            "No external session id was resolved; the plan derived an internal session lease '{}' from {} and the future hub must keep that lease sticky for the life of the connection.",
            session_lease.0, session_lease.1
        ));
    }
    if session_id.is_none()
        && conversation_id.is_none()
        && client_instance_id.is_none()
        && transport_session_id.is_none()
    {
        warnings.push(
            "No external session, conversation, client-instance, or transport-session id was resolved; multiple live instances of the same client in the same project can share the derived planned lease. Pass --session-id, MCPACE_SESSION_ID, MCPACE_CLIENT_INSTANCE_ID, or _meta.com.mcpace/context metadata for strict multi-client isolation.".to_string(),
        );
    }
    if metadata.workspace_roots.len() > 1 && project_root.is_none() {
        warnings.push(
            "Multiple workspace roots were provided but no explicit project root or cwd selected a single project; project-local routing stays unresolved.".to_string(),
        );
    }

    warnings.sort();
    warnings.dedup();

    ResolvedContext {
        client_id,
        client_id_source,
        session_id,
        session_id_source,
        session_lease_id: session_lease.0,
        session_lease_source: session_lease.1,
        conversation_id,
        conversation_id_source,
        client_instance_id,
        client_instance_id_source,
        transport_session_id,
        transport_session_id_source,
        credential_profile_id,
        credential_profile_id_source,
        project_root,
        project_root_source,
        workspace_roots: metadata.workspace_roots.clone(),
        cwd,
        cwd_source,
        preferred_ingress,
        preferred_ingress_source,
        warnings,
    }
}

fn resolve_project_root(
    parsed: &ParsedArgs,
    metadata: &MetadataEnvelope,
    cwd: Option<&str>,
    warnings: &mut Vec<String>,
) -> (Option<String>, String) {
    if let Some(value) = clean_optional_string(
        parsed
            .project_root
            .clone()
            .map(|value| normalize_path(&value)),
    ) {
        return (Some(value), "flag".to_string());
    }
    if let Some(value) = clean_optional_string(
        env::var("MCPACE_PROJECT_ROOT")
            .ok()
            .map(|value| normalize_path(&value)),
    ) {
        return (Some(value), "env".to_string());
    }

    if metadata.workspace_roots.len() == 1 {
        return (
            metadata.workspace_roots.first().cloned(),
            "metadata-single-root".to_string(),
        );
    }

    if metadata.workspace_roots.len() > 1 {
        if let Some(cwd) = cwd {
            if let Some(best) = best_matching_root(&metadata.workspace_roots, cwd) {
                return (Some(best.to_string()), "metadata-roots+cwd".to_string());
            }
            warnings.push(format!(
                "{} workspace roots were provided, but cwd '{}' did not match any of them.",
                metadata.workspace_roots.len(),
                cwd
            ));
        }
        return (None, "ambiguous-workspace-roots".to_string());
    }

    if let Some(cwd) = cwd {
        return (Some(cwd.to_string()), "metadata-cwd".to_string());
    }

    (None, "unresolved".to_string())
}

fn best_matching_root<'a>(roots: &'a [String], cwd: &str) -> Option<&'a str> {
    let mut best: Option<&str> = None;
    for root in roots {
        if path_is_within(cwd, root) {
            match best {
                Some(current) if current.len() >= root.len() => {}
                _ => best = Some(root.as_str()),
            }
        }
    }
    best
}

fn resolve_string(candidates: [(Option<String>, &str); 4]) -> (Option<String>, String) {
    let fallback_source = candidates[3].1.to_string();
    for (candidate, source) in candidates {
        if let Some(value) = clean_optional_string(candidate) {
            return (Some(value), source.to_string());
        }
    }
    (None, fallback_source)
}

fn clean_optional_string(value: Option<String>) -> Option<String> {
    let value = value?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

struct SessionLeaseInput<'a> {
    client_id: &'a str,
    session_id: Option<&'a str>,
    conversation_id: Option<&'a str>,
    client_instance_id: Option<&'a str>,
    transport_session_id: Option<&'a str>,
    project_root: Option<&'a str>,
    cwd: Option<&'a str>,
    preferred_ingress: &'a str,
}

fn resolve_session_lease(input: SessionLeaseInput<'_>) -> (String, String) {
    if let Some(external_session_id) = clean_optional_string(input.session_id.map(str::to_string)) {
        return (
            format!("external:{}", sanitize_key(&external_session_id)),
            "external-session-id".to_string(),
        );
    }

    let mut seed = Vec::new();
    extend_seed(&mut seed, "client", Some(input.client_id));
    extend_seed(&mut seed, "conversation", input.conversation_id);
    extend_seed(&mut seed, "client-instance", input.client_instance_id);
    extend_seed(&mut seed, "transport-session", input.transport_session_id);
    extend_seed(&mut seed, "project-root", input.project_root);
    extend_seed(&mut seed, "cwd", input.cwd);
    extend_seed(&mut seed, "ingress", Some(input.preferred_ingress));

    if seed.is_empty() {
        seed.push("anonymous".to_string());
    }

    (
        format!("planned:{}", stable_hash_hex(&seed.join("|"))),
        "planned-fallback".to_string(),
    )
}

fn extend_seed(seed: &mut Vec<String>, key: &str, value: Option<&str>) {
    if let Some(value) = clean_optional_string(value.map(str::to_string)) {
        seed.push(format!("{}:{}", key, sanitize_key(&value)));
    }
}

#[cfg(test)]
mod tests {
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
}
