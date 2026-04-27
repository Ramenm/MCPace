#[derive(Debug, Default)]
pub(super) struct MetadataEnvelope {
    pub(super) client_id: Option<String>,
    pub(super) session_id: Option<String>,
    pub(super) conversation_id: Option<String>,
    pub(super) client_instance_id: Option<String>,
    pub(super) transport_session_id: Option<String>,
    pub(super) credential_profile_id: Option<String>,
    pub(super) workspace_roots: Vec<String>,
    pub(super) cwd: Option<String>,
    pub(super) transport: Option<String>,
}

#[derive(Debug, Clone)]
pub(super) struct ResolvedContext {
    pub(super) client_id: String,
    pub(super) client_id_source: String,
    pub(super) session_id: Option<String>,
    pub(super) session_id_source: String,
    pub(super) session_lease_id: String,
    pub(super) session_lease_source: String,
    pub(super) conversation_id: Option<String>,
    pub(super) conversation_id_source: String,
    pub(super) client_instance_id: Option<String>,
    pub(super) client_instance_id_source: String,
    pub(super) transport_session_id: Option<String>,
    pub(super) transport_session_id_source: String,
    pub(super) credential_profile_id: Option<String>,
    pub(super) credential_profile_id_source: String,
    pub(super) project_root: Option<String>,
    pub(super) project_root_source: String,
    pub(super) workspace_roots: Vec<String>,
    pub(super) cwd: Option<String>,
    pub(super) cwd_source: String,
    pub(super) preferred_ingress: String,
    pub(super) preferred_ingress_source: String,
    pub(super) warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub(super) struct ClientPlan {
    pub(super) root_path: String,
    pub(super) config_version: Option<String>,
    pub(super) configured_client_key_name: Option<String>,
    pub(super) client_target_id: Option<String>,
    pub(super) client_target_family_id: Option<String>,
    pub(super) client_target_maturity: String,
    pub(super) client_target_surface_class: String,
    pub(super) client_target_surface_kind: String,
    pub(super) client_target_documented_features: Vec<String>,
    pub(super) client_target_documented_constraints: Vec<String>,
    pub(super) entrypoint_mode: String,
    pub(super) launcher_command: String,
    pub(super) current_grouped_action: String,
    pub(super) preferred_ingress: String,
    pub(super) preferred_ingress_source: String,
    pub(super) supported_ingresses: Vec<String>,
    pub(super) hub_lifecycle_implemented: bool,
    pub(super) client_install_implemented: bool,
    pub(super) client_export_implemented: bool,
    pub(super) context: ResolvedContext,
    pub(super) session_binding_key: String,
    pub(super) requires_hub_owned_stdio: bool,
    pub(super) parallel_safe_servers: usize,
    pub(super) serialized_servers: usize,
    pub(super) exclusive_servers: usize,
    pub(super) warnings: Vec<String>,
    pub(super) servers: Vec<ServerCoordinationPlan>,
}

#[derive(Debug, Clone)]
pub(super) struct ServerCoordinationPlan {
    pub(super) name: String,
    pub(super) admission_state: String,
    pub(super) scope_class: String,
    pub(super) concurrency_policy: String,
    pub(super) upstream_transport: String,
    pub(super) process_partition: String,
    pub(super) process_scope_key: String,
    pub(super) project_binding_key: Option<String>,
    pub(super) worktree_binding_key: Option<String>,
    pub(super) conflict_domain: String,
    pub(super) host_lock_key: Option<String>,
    pub(super) browser_profile_key: Option<String>,
    pub(super) parallelism_limit: usize,
    pub(super) scheduler_lane: String,
    pub(super) startup_strategy: String,
    pub(super) request_strategy: String,
    pub(super) request_mutex_key: Option<String>,
    pub(super) session_affinity_key: Option<String>,
    pub(super) warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub(super) struct ScopeResolution {
    pub(super) process_partition: String,
    pub(super) process_scope_key: String,
    pub(super) project_binding_key: Option<String>,
    pub(super) worktree_binding_key: Option<String>,
    pub(super) conflict_domain: String,
    pub(super) host_lock_key: Option<String>,
    pub(super) browser_profile_key: Option<String>,
    pub(super) parallelism_limit: usize,
    pub(super) scheduler_lane: String,
    pub(super) startup_strategy: String,
    pub(super) session_affinity_key: Option<String>,
    pub(super) warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub(super) struct RequestStrategy {
    pub(super) name: String,
    pub(super) mutex_key: Option<String>,
    pub(super) scheduler_lane: String,
    pub(super) parallelism_limit: usize,
    pub(super) warnings: Vec<String>,
}
