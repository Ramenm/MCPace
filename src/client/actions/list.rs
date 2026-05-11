use super::super::args::ParsedArgs;
use super::super::render::{count_static, join_count_map, join_or_none};
use super::read_client_key_name;
use crate::client_catalog::{
    self, client_install_support_summary_for_targets, ClientTargetRecord as ClientTarget,
};
use crate::doctor;
use crate::json::JsonValue;
use std::io::Write;
use std::path::PathBuf;

// Re-exported by actions as `pub(super) use self::list::run_list`; keep this
// function no wider than the client command module boundary.
pub(in crate::client) fn run_list(
    parsed: ParsedArgs,
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let root_path = parsed.root_override.clone().or(default_root);
    let config_version = root_path.as_deref().and_then(doctor::read_config_version);
    let configured_client_key_name = root_path.as_deref().and_then(read_client_key_name);
    let registry = match client_catalog::load_registry(root_path.as_deref()) {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 1;
        }
    };

    let family_counts = count_static(
        registry
            .targets
            .iter()
            .map(|target| target.family_id.clone()),
    );
    let surface_class_counts = count_static(
        registry
            .targets
            .iter()
            .map(|target| target.surface_class.clone()),
    );
    let proof_tier_counts = count_static(
        registry
            .targets
            .iter()
            .map(|target| target.proof_tier.clone()),
    );
    let install_supported_target_ids = registry
        .targets
        .iter()
        .filter(|target| target.supports_client_install())
        .map(|target| target.id.clone())
        .collect::<Vec<_>>();

    if parsed.json_output {
        let json = JsonValue::object([
            (
                "configVersion",
                match config_version {
                    Some(ref value) => JsonValue::string(value.clone()),
                    None => JsonValue::Null,
                },
            ),
            (
                "configuredClientKeyName",
                match configured_client_key_name {
                    Some(ref value) => JsonValue::string(value.clone()),
                    None => JsonValue::Null,
                },
            ),
            (
                "catalogSources",
                JsonValue::array(registry.sources.iter().cloned().map(JsonValue::string)),
            ),
            (
                "catalogWarnings",
                JsonValue::array(registry.warnings.iter().cloned().map(JsonValue::string)),
            ),
            (
                "familyCounts",
                JsonValue::object(
                    family_counts
                        .iter()
                        .map(|(key, value)| (key.clone(), JsonValue::number(*value))),
                ),
            ),
            (
                "surfaceClassCounts",
                JsonValue::object(
                    surface_class_counts
                        .iter()
                        .map(|(key, value)| (key.clone(), JsonValue::number(*value))),
                ),
            ),
            (
                "proofTierCounts",
                JsonValue::object(
                    proof_tier_counts
                        .iter()
                        .map(|(key, value)| (key.clone(), JsonValue::number(*value))),
                ),
            ),
            (
                "installSupportedTargetIds",
                JsonValue::array(
                    install_supported_target_ids
                        .iter()
                        .cloned()
                        .map(JsonValue::string),
                ),
            ),
            (
                "targets",
                JsonValue::array(registry.targets.iter().map(ClientTarget::to_json_value)),
            ),
        ]);
        let _ = writeln!(stdout, "{}", json.to_pretty_string());
        return 0;
    }

    let _ = writeln!(stdout, "Known client targets: {}", registry.targets.len());
    let _ = writeln!(
        stdout,
        "Configured adapter key name: {}",
        configured_client_key_name.as_deref().unwrap_or("none")
    );
    let _ = writeln!(
        stdout,
        "Catalog sources: {}",
        join_or_none(&registry.sources)
    );
    let _ = writeln!(
        stdout,
        "Catalog warnings: {}",
        join_or_none(&registry.warnings)
    );
    let _ = writeln!(stdout, "Families: {}", join_count_map(&family_counts));
    let _ = writeln!(
        stdout,
        "Surface classes: {}",
        join_count_map(&surface_class_counts)
    );
    let _ = writeln!(
        stdout,
        "Proof tiers: {}",
        join_count_map(&proof_tier_counts)
    );
    let _ = writeln!(
        stdout,
        "Install patchers: {}",
        client_install_support_summary_for_targets(&registry.targets)
    );
    for target in &registry.targets {
        let install_label = target
            .install_support()
            .map(|support| support.kind.as_str())
            .unwrap_or("manual");
        let _ = writeln!(
            stdout,
            "- {} [{} / {} / {} / {}] format={} ingress={} scopes={} install={}",
            target.id,
            target.maturity,
            target.surface_class,
            target.surface_kind,
            target.proof_tier(),
            target.config_format,
            join_or_none(&target.supported_ingresses),
            join_or_none(&target.native_scopes),
            install_label,
        );
        let _ = writeln!(
            stdout,
            "    family={} proofTier={} source={} installSupport={} paths={}",
            target.family_id,
            target.proof_tier(),
            target.source,
            target
                .install_support()
                .map(|value| value.kind.as_str())
                .unwrap_or("manual"),
            join_or_none(&target.config_paths)
        );
        let _ = writeln!(
            stdout,
            "    precedence={}",
            join_or_none(&target.config_precedence)
        );
        let _ = writeln!(
            stdout,
            "    features={}",
            join_or_none(&target.documented_features)
        );
        let _ = writeln!(
            stdout,
            "    constraints={}",
            join_or_none(&target.documented_constraints)
        );
        let _ = writeln!(stdout, "    notes={}", join_or_none(&target.notes));
    }
    0
}
