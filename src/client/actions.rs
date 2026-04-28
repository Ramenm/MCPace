use super::args::ParsedArgs;
use super::context::resolve_context;
use super::metadata::load_metadata;
use super::pathing::stable_hash_hex;
use super::plan::build_plan;
use super::render::{count_static, join_count_map, join_or_none, write_text_plan};
use crate::client_catalog::{
    self, client_install_support_summary as catalog_client_install_support_summary,
    client_install_support_summary_for_targets, ClientInstallKindRecord as ClientInstallKind,
    ClientTargetRecord as ClientTarget, JsonMcpServerShapeRecord as JsonMcpServerShape,
};
use crate::doctor;
use crate::json::{parse_str, JsonValue};
use crate::json_helpers;
use crate::runtimepaths;
use crate::server;
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn local_mcp_url() -> String {
    runtimepaths::default_local_mcp_url()
}

pub(super) fn client_install_support_summary() -> String {
    catalog_client_install_support_summary()
}

pub(super) fn run_list(
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

pub(super) fn run_plan(
    parsed: ParsedArgs,
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let root_path = parsed.root_override.clone().or(default_root);
    let Some(root_path) = root_path else {
        let _ = writeln!(stderr, "mcpace root not found; expected mcpace.config.json");
        return 1;
    };

    let json = match build_plan_json(parsed.clone(), &root_path) {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 1;
        }
    };

    if parsed.json_output {
        let _ = writeln!(stdout, "{}", json.to_pretty_string());
        return 0;
    }

    let plan = match build_plan_struct(parsed, &root_path) {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 1;
        }
    };
    write_text_plan(&plan, stdout);
    0
}

pub(super) fn build_plan_json(parsed: ParsedArgs, root_path: &Path) -> Result<JsonValue, String> {
    build_plan_struct(parsed, root_path).map(|plan| plan.to_json_value())
}

fn build_plan_struct(
    parsed: ParsedArgs,
    root_path: &Path,
) -> Result<super::model::ClientPlan, String> {
    let server_records = server::load_server_records(root_path)?;
    let registry = client_catalog::load_registry(Some(root_path))?;
    let metadata = load_metadata(&parsed)?;

    let mut context = resolve_context(&parsed, &metadata);
    let client_target = client_catalog::find_in_targets(&registry.targets, &context.client_id);
    if let Some(client_target) = client_target {
        prefer_local_http_when_supported(&parsed, &mut context, client_target, "serve-default");
    }
    Ok(build_plan(
        root_path.display().to_string(),
        doctor::read_config_version(root_path),
        read_client_key_name(root_path),
        context,
        client_target,
        &server_records,
    ))
}

pub(super) fn run_export(
    parsed: ParsedArgs,
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let root_path = parsed.root_override.clone().or(default_root);
    let Some(root_path) = root_path else {
        let _ = writeln!(stderr, "mcpace root not found; expected mcpace.config.json");
        return 1;
    };

    let registry = match client_catalog::load_registry(Some(&root_path)) {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 1;
        }
    };

    let metadata = match load_metadata(&parsed) {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 2;
        }
    };
    let mut context = resolve_context(&parsed, &metadata);
    if context.project_root.is_none() {
        context.project_root = Some(sanitize_path_for_display(&canonicalize_or_original(
            &root_path,
        )));
        context.project_root_source = "export-root".to_string();
    }
    let client_target = match client_catalog::find_in_targets(&registry.targets, &context.client_id)
    {
        Some(value) => value,
        None => {
            let _ = writeln!(
                stderr,
                "unknown client target '{}'; use 'mcpace client list' to inspect supported surfaces",
                context.client_id
            );
            return 2;
        }
    };
    prefer_local_http_when_supported(&parsed, &mut context, client_target, "serve-default");

    let server_records = match server::load_server_records(&root_path) {
        Ok(records) => records,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 1;
        }
    };

    let plan = build_plan(
        root_path.display().to_string(),
        doctor::read_config_version(&root_path),
        read_client_key_name(&root_path),
        context,
        Some(client_target),
        &server_records,
    );

    let preview = ClientExportPreview::from_plan(client_target, &plan);
    if parsed.json_output {
        let _ = writeln!(stdout, "{}", preview.to_json_value().to_pretty_string());
        return 0;
    }

    preview.write_text(stdout);
    0
}

pub(super) fn run_install(
    parsed: ParsedArgs,
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let root_path = parsed.root_override.clone().or(default_root);
    let Some(root_path) = root_path else {
        let _ = writeln!(stderr, "mcpace root not found; expected mcpace.config.json");
        return 1;
    };

    let registry = match client_catalog::load_registry(Some(&root_path)) {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 1;
        }
    };

    if parsed
        .client_id
        .as_deref()
        .map(|value| value.eq_ignore_ascii_case("all"))
        .unwrap_or(false)
    {
        return run_install_all(parsed, root_path, &registry, stdout, stderr);
    }

    let metadata = match load_metadata(&parsed) {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 2;
        }
    };
    let mut context = resolve_context(&parsed, &metadata);
    if context.project_root.is_none() {
        context.project_root = Some(sanitize_path_for_display(&canonicalize_or_original(
            &root_path,
        )));
        context.project_root_source = "install-root".to_string();
    }
    let client_target = match client_catalog::find_in_targets(&registry.targets, &context.client_id)
    {
        Some(value) => value,
        None => {
            let _ = writeln!(
                stderr,
                "unknown client target '{}'; use 'mcpace client list' to inspect supported surfaces",
                context.client_id
            );
            return 2;
        }
    };
    prefer_local_http_when_supported(&parsed, &mut context, client_target, "serve-default");

    let server_records = match server::load_server_records(&root_path) {
        Ok(records) => records,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 1;
        }
    };

    let plan = build_plan(
        root_path.display().to_string(),
        doctor::read_config_version(&root_path),
        read_client_key_name(&root_path),
        context,
        Some(client_target),
        &server_records,
    );

    let install = match ClientInstallPlan::from_plan(&root_path, client_target, &plan) {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 1;
        }
    };

    let options = ClientInstallRunOptions::from_args(&parsed);
    let result = match install.write_with_options(&options) {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 1;
        }
    };

    if parsed.json_output {
        let _ = writeln!(stdout, "{}", result.to_json_value().to_pretty_string());
        return 0;
    }

    result.write_text(stdout);
    0
}

pub(super) fn run_restore(
    parsed: ParsedArgs,
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let root_path = parsed.root_override.clone().or(default_root);
    let Some(root_path) = root_path else {
        let _ = writeln!(stderr, "mcpace root not found; expected mcpace.config.json");
        return 1;
    };
    let Some(client_id) = parsed.client_id.as_deref() else {
        let _ = writeln!(
            stderr,
            "client restore requires a client target id, for example: mcpace client restore codex"
        );
        return 2;
    };

    let selector = parsed.backup.as_deref().unwrap_or("latest");
    if client_id.eq_ignore_ascii_case("all") {
        return run_restore_all(&root_path, selector, parsed.json_output, stdout, stderr);
    }
    let result = match restore_client_install_backup(&root_path, client_id, selector) {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 1;
        }
    };

    if parsed.json_output {
        let _ = writeln!(stdout, "{}", result.to_json_value().to_pretty_string());
        return 0;
    }

    result.write_text(stdout);
    0
}

fn run_restore_all(
    root_path: &Path,
    selector: &str,
    json_output: bool,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    if !selector.trim().is_empty() && !selector.eq_ignore_ascii_case("latest") {
        let _ = writeln!(
            stderr,
            "client restore all supports only --backup latest because backup ids are per-client"
        );
        return 2;
    }

    let backup_root = install_backup_root(root_path);
    let entries = match fs::read_dir(&backup_root) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let _ = writeln!(
                stderr,
                "no client install backups found under '{}'",
                backup_root.display()
            );
            return 1;
        }
        Err(error) => {
            let _ = writeln!(
                stderr,
                "failed to read client install backups '{}': {}",
                backup_root.display(),
                error
            );
            return 1;
        }
    };

    let mut restored = Vec::new();
    let mut failed = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(value) => value,
            Err(error) => {
                failed.push((
                    "unknown".to_string(),
                    format!("failed to inspect backup entry: {}", error),
                ));
                continue;
            }
        };
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let client_id = entry.file_name().to_string_lossy().to_string();
        match restore_client_install_backup(root_path, &client_id, "latest") {
            Ok(value) => restored.push(value),
            Err(error) => failed.push((client_id, error)),
        }
    }

    if json_output {
        let json = JsonValue::object([
            ("mode", JsonValue::string("restored-all")),
            ("backupSelector", JsonValue::string("latest")),
            (
                "restored",
                JsonValue::array(restored.iter().map(ClientRestoreResult::to_json_value)),
            ),
            (
                "failed",
                JsonValue::array(failed.iter().map(|(client_id, error)| {
                    JsonValue::object([
                        ("clientTargetId", JsonValue::string(client_id.clone())),
                        ("error", JsonValue::string(error.clone())),
                    ])
                })),
            ),
        ]);
        let _ = writeln!(stdout, "{}", json.to_pretty_string());
    } else {
        let _ = writeln!(stdout, "Client restore all complete");
        for result in &restored {
            result.write_text(stdout);
        }
        for (client_id, error) in &failed {
            let _ = writeln!(stderr, "{}: {}", client_id, error);
        }
    }

    if failed.is_empty() {
        0
    } else {
        1
    }
}

fn run_install_all(
    parsed: ParsedArgs,
    root_path: PathBuf,
    registry: &client_catalog::ClientRegistry,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let server_records = match server::load_server_records(&root_path) {
        Ok(records) => records,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 1;
        }
    };
    let mut installed = Vec::new();
    let mut failed = Vec::new();
    let mut skipped = Vec::new();
    let options = ClientInstallRunOptions::from_args(&parsed);

    for target in registry
        .targets
        .iter()
        .filter(|target| client_catalog::normalize(&target.surface_class) == "local")
    {
        if !target.supports_client_install() {
            skipped.push(format!("{}: manual/no patcher", target.id));
            continue;
        }

        let mut target_args = parsed.clone();
        target_args.client_id = Some(target.id.clone());
        let metadata = match load_metadata(&target_args) {
            Ok(value) => value,
            Err(error) => {
                failed.push((target.id.clone(), error));
                continue;
            }
        };
        let mut context = resolve_context(&target_args, &metadata);
        if context.project_root.is_none() {
            context.project_root = Some(sanitize_path_for_display(&canonicalize_or_original(
                &root_path,
            )));
            context.project_root_source = "install-root".to_string();
        }
        prefer_local_http_when_supported(&target_args, &mut context, target, "serve-default");

        let plan = build_plan(
            root_path.display().to_string(),
            doctor::read_config_version(&root_path),
            read_client_key_name(&root_path),
            context,
            Some(target),
            &server_records,
        );

        let install = match ClientInstallPlan::from_plan(&root_path, target, &plan) {
            Ok(value) => value,
            Err(error) => {
                failed.push((target.id.clone(), error));
                continue;
            }
        };
        match install.write_with_options(&options) {
            Ok(value) => installed.push(value),
            Err(error) => failed.push((target.id.clone(), error)),
        }
    }

    if parsed.json_output {
        let json = JsonValue::object([
            (
                "mode",
                JsonValue::string(if parsed.dry_run {
                    "install-preview-all"
                } else {
                    "installed-all"
                }),
            ),
            ("dryRun", JsonValue::bool(parsed.dry_run)),
            ("diffRequested", JsonValue::bool(parsed.diff)),
            (
                "installed",
                JsonValue::array(installed.iter().map(ClientInstallResult::to_json_value)),
            ),
            (
                "skipped",
                JsonValue::array(skipped.iter().cloned().map(JsonValue::string)),
            ),
            (
                "failed",
                JsonValue::array(failed.iter().map(|(target, error)| {
                    JsonValue::object([
                        ("clientTargetId", JsonValue::string(target.clone())),
                        ("error", JsonValue::string(error.clone())),
                    ])
                })),
            ),
        ]);
        let _ = writeln!(stdout, "{}", json.to_pretty_string());
    } else {
        let _ = writeln!(
            stdout,
            "{}",
            if parsed.dry_run {
                "Client install all dry-run complete"
            } else {
                "Client install all complete"
            }
        );
        for result in &installed {
            result.write_text(stdout);
        }
        if !skipped.is_empty() {
            let _ = writeln!(stdout, "Skipped: {}", join_or_none(&skipped));
        }
        for (target, error) in &failed {
            let _ = writeln!(stderr, "{}: {}", target, error);
        }
    }

    if failed.is_empty() {
        0
    } else {
        1
    }
}

fn restore_client_install_backup(
    root_path: &Path,
    client_id: &str,
    selector: &str,
) -> Result<ClientRestoreResult, String> {
    let backup_path = resolve_backup_path(root_path, client_id, selector)?;
    let manifest_path = backup_path.join("manifest.json");
    let manifest = json_helpers::read_json_file(&manifest_path)?;
    let schema = json_helpers::string_at_path(&manifest, &["schema"]).unwrap_or("");
    if schema != "mcpace.clientInstallBackup.v1" {
        return Err(format!(
            "client install backup '{}' has unsupported schema '{}'",
            manifest_path.display(),
            schema
        ));
    }

    let backup_id = required_manifest_string(&manifest, &manifest_path, "backupId")?;
    let manifest_client_id = required_manifest_string(&manifest, &manifest_path, "clientTargetId")?;
    if manifest_client_id != client_id {
        return Err(format!(
            "client install backup '{}' belongs to '{}' not '{}'",
            manifest_path.display(),
            manifest_client_id,
            client_id
        ));
    }
    let config_path_text = required_manifest_string(&manifest, &manifest_path, "configPath")?;
    let expected_hash = required_manifest_string(&manifest, &manifest_path, "configPathHash")?;
    let actual_hash = stable_hash_hex(&config_path_text);
    if actual_hash != expected_hash {
        return Err(format!(
            "client install backup '{}' failed config path integrity check",
            manifest_path.display()
        ));
    }
    let config_existed =
        json_helpers::bool_at_path(&manifest, &["configExisted"]).ok_or_else(|| {
            format!(
                "client install backup '{}' is missing boolean field configExisted",
                manifest_path.display()
            )
        })?;
    let config_path = PathBuf::from(&config_path_text);

    let mut removed_config_file = false;
    let mut wrote_config_file = false;
    if config_existed {
        let content_path = backup_path.join("config.before");
        let contents = fs::read_to_string(&content_path).map_err(|error| {
            format!(
                "failed to read client install backup content '{}': {}",
                content_path.display(),
                error
            )
        })?;
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "failed to create restored client config directory '{}': {}",
                    parent.display(),
                    error
                )
            })?;
        }
        fs::write(&config_path, contents).map_err(|error| {
            format!(
                "failed to restore client config '{}': {}",
                config_path.display(),
                error
            )
        })?;
        wrote_config_file = true;
    } else if config_path.is_file() {
        fs::remove_file(&config_path).map_err(|error| {
            format!(
                "failed to remove client config '{}' during restore: {}",
                config_path.display(),
                error
            )
        })?;
        removed_config_file = true;
    }

    Ok(ClientRestoreResult {
        client_target_id: client_id.to_string(),
        backup_id,
        backup_path: sanitize_path_for_display(&backup_path),
        config_path: sanitize_path_for_display(&config_path),
        restored_existing_config: config_existed,
        removed_config_file,
        wrote_config_file,
    })
}

fn resolve_backup_path(
    root_path: &Path,
    client_id: &str,
    selector: &str,
) -> Result<PathBuf, String> {
    let client_backup_root = install_backup_root(root_path).join(safe_file_segment(client_id));
    let normalized_selector = selector.trim();
    if normalized_selector.is_empty() || normalized_selector.eq_ignore_ascii_case("latest") {
        return latest_backup_path(&client_backup_root, client_id);
    }
    if !is_safe_backup_id(normalized_selector) {
        return Err(format!(
            "client restore backup selector '{}' is invalid; use a backup id or 'latest'",
            selector
        ));
    }
    let path = client_backup_root.join(normalized_selector);
    if !path.join("manifest.json").is_file() {
        return Err(format!(
            "client restore backup '{}' was not found for '{}'",
            normalized_selector, client_id
        ));
    }
    Ok(path)
}

fn latest_backup_path(client_backup_root: &Path, client_id: &str) -> Result<PathBuf, String> {
    let entries = fs::read_dir(client_backup_root).map_err(|error| {
        format!(
            "failed to read client install backups for '{}': {}",
            client_id, error
        )
    })?;
    let mut candidates = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            format!(
                "failed to inspect client install backups for '{}': {}",
                client_id, error
            )
        })?;
        let path = entry.path();
        if path.join("manifest.json").is_file() {
            candidates.push(path);
        }
    }
    candidates.sort_by(|left, right| left.file_name().cmp(&right.file_name()));
    candidates.pop().ok_or_else(|| {
        format!(
            "no client install backups found for '{}'; run client install first",
            client_id
        )
    })
}

fn required_manifest_string(
    manifest: &JsonValue,
    manifest_path: &Path,
    key: &str,
) -> Result<String, String> {
    json_helpers::string_at_path(manifest, &[key])
        .map(|value| value.to_string())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            format!(
                "client install backup '{}' is missing string field {}",
                manifest_path.display(),
                key
            )
        })
}

fn is_safe_backup_id(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
}

fn install_backup_root(root_path: &Path) -> PathBuf {
    runtimepaths::resolve_state_root(root_path)
        .join("data")
        .join("client-install-backups")
}

fn safe_file_segment(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    if sanitized.trim_matches(['.', '_', '-']).is_empty() {
        "unknown".to_string()
    } else {
        sanitized
    }
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

fn read_client_key_name(root_path: &Path) -> Option<String> {
    let config_path = root_path.join("mcpace.config.json");
    let config = json_helpers::read_json_file(&config_path).ok()?;
    json_helpers::string_at_path(&config, &["client", "keyName"])
        .map(|value| value.trim().to_string())
}

fn prefer_local_http_when_supported(
    parsed: &ParsedArgs,
    context: &mut super::model::ResolvedContext,
    client_target: &ClientTarget,
    source: &str,
) {
    if parsed.transport.is_none()
        && !client_target.has_constraint("public-http-only")
        && client_target.supports_ingress("streamable-http")
    {
        context.preferred_ingress = "streamable-http".to_string();
        context.preferred_ingress_source = source.to_string();
    }
}

struct ClientExportPreview {
    client_target_id: String,
    display_name: String,
    adapter_key_name: String,
    config_format: String,
    config_paths: Vec<String>,
    config_precedence: Vec<String>,
    native_scopes: Vec<String>,
    preferred_ingress: String,
    export_mode: String,
    entrypoint_mode: String,
    launcher_command: String,
    root_path: String,
    mode: String,
    can_connect_today: bool,
    writes_config: bool,
    recommended_install_scope: Option<String>,
    recommended_install_path: Option<String>,
    adapter_contract: AdapterContractPreview,
    blockers: Vec<String>,
    warnings: Vec<String>,
    next_actions: Vec<String>,
}

struct AdapterContractPreview {
    kind: String,
    command: Option<String>,
    args: Vec<String>,
    url_template: Option<String>,
    metadata_carrier: String,
    session_model: String,
    notes: Vec<String>,
}

enum ClientInstallConfig {
    TomlManagedTable,
    JsonMcpServers { server_config: JsonValue },
    YamlMcpServers,
}

struct ClientInstallPlan {
    client_target_id: String,
    display_name: String,
    adapter_key_name: String,
    config_path: PathBuf,
    backup_root: PathBuf,
    config_scope: String,
    server_url: String,
    config: ClientInstallConfig,
    warnings: Vec<String>,
}

#[derive(Clone, Copy)]
struct ClientInstallRunOptions {
    dry_run: bool,
    diff: bool,
}

struct ClientInstallResult {
    client_target_id: String,
    display_name: String,
    adapter_key_name: String,
    config_path: String,
    config_scope: String,
    transport: String,
    server_url: String,
    changed: bool,
    would_change: bool,
    dry_run: bool,
    diff_requested: bool,
    diff: Option<String>,
    persisted: bool,
    backup_created: bool,
    backup_id: Option<String>,
    backup_path: Option<String>,
    backup_manifest_path: Option<String>,
    restore_command: Option<String>,
    replaced_existing_block: bool,
    created_config_dir: bool,
    created_config_file: bool,
    would_create_config_dir: bool,
    would_create_config_file: bool,
    warnings: Vec<String>,
}

struct ClientInstallBackup {
    id: String,
    path: PathBuf,
    manifest_path: PathBuf,
    restore_command: String,
}

struct ClientRestoreResult {
    client_target_id: String,
    backup_id: String,
    backup_path: String,
    config_path: String,
    restored_existing_config: bool,
    removed_config_file: bool,
    wrote_config_file: bool,
}

impl ClientExportPreview {
    fn from_plan(target: &ClientTarget, plan: &super::model::ClientPlan) -> Self {
        let adapter_key_name = plan
            .configured_client_key_name
            .clone()
            .unwrap_or_else(|| format!("{}-adapter", target.family_id));
        let export_mode = resolve_export_mode(target, plan);
        let recommended_install_scope = target
            .preferred_install_scope()
            .map(|value| value.to_string());
        let recommended_install_path = target
            .preferred_install_config_path()
            .map(|value| value.to_string());

        let blockers = match export_mode.as_str() {
            "local-stdio-launcher" | "local-streamable-http" => Vec::new(),
            "public-http-connector" => vec![
                "This client surface needs a public HTTP MCP endpoint or relay, and MCPace does not ship that lane yet in this repo.".to_string(),
            ],
            _ => vec![
                "MCPace does not yet have a verified ingress lane for this client surface, so export stays preview-only.".to_string(),
            ],
        };

        let next_actions = match export_mode.as_str() {
            "local-streamable-http" if target.supports_client_install() => {
                let scope = recommended_install_scope
                    .as_deref()
                    .unwrap_or("recommended");
                let path = recommended_install_path
                    .as_deref()
                    .unwrap_or("documented install path");
                vec![
                format!(
                    "Run 'mcpace client install {} --root <path>' to patch the MCPace entry in {} automatically using the shared {} scope.",
                    target.id,
                    path,
                    scope
                ),
                format!(
                    "MCPace defaults to the shared {} scope for {} so one localhost MCPace server can be reused across projects.",
                    scope,
                    target.display_name
                ),
                format!(
                    "Keep one MCPace server running on port {} so {} always points at the same localhost MCP URL.",
                    runtimepaths::DEFAULT_LOCAL_MCP_PORT,
                    target.display_name
                ),
            ]
            }
            "local-streamable-http" => vec![
                format!(
                    "Run 'mcpace serve --port {}' and point this client at the one MCPace URL.",
                    runtimepaths::DEFAULT_LOCAL_MCP_PORT
                ),
                "Keep export as the source of truth for the HTTP MCPace contract until that client gets a dedicated config patcher.".to_string(),
            ],
            "local-stdio-launcher" => vec![
                "This client surface still needs the stdio MCPace fallback today.".to_string(),
                "Keep stdio surfaces as internal compatibility lanes while HTTP-first clients move to the one-port MCPace server.".to_string(),
            ],
            "public-http-connector" => vec![
                "Ship a public HTTP / relay lane for cloud connectors before claiming this surface works through MCPace.".to_string(),
                "Add credential bootstrap and real hosted compatibility traces for this connector class.".to_string(),
            ],
            _ => vec![
                "Add a verified ingress lane for this client surface before turning preview output into a real config patch.".to_string(),
            ],
        };

        ClientExportPreview {
            client_target_id: target.id.clone(),
            display_name: target.display_name.clone(),
            adapter_key_name,
            config_format: target.config_format.clone(),
            config_paths: target.config_paths.clone(),
            config_precedence: target.config_precedence.clone(),
            native_scopes: target.native_scopes.clone(),
            preferred_ingress: plan.preferred_ingress.clone(),
            export_mode: export_mode.clone(),
            entrypoint_mode: plan.entrypoint_mode.clone(),
            launcher_command: plan.launcher_command.clone(),
            root_path: plan.root_path.clone(),
            mode: if matches!(
                export_mode.as_str(),
                "local-stdio-launcher" | "local-streamable-http"
            ) {
                "connectable-preview".to_string()
            } else {
                "preview-only".to_string()
            },
            can_connect_today: matches!(
                export_mode.as_str(),
                "local-stdio-launcher" | "local-streamable-http"
            ),
            writes_config: false,
            recommended_install_scope,
            recommended_install_path,
            adapter_contract: build_adapter_contract(target, plan, &export_mode),
            blockers,
            warnings: plan.warnings.clone(),
            next_actions,
        }
    }

    fn to_json_value(&self) -> JsonValue {
        let mut map = BTreeMap::new();
        map.insert("mode".to_string(), JsonValue::string(self.mode.clone()));
        map.insert(
            "clientTargetId".to_string(),
            JsonValue::string(self.client_target_id.clone()),
        );
        map.insert(
            "displayName".to_string(),
            JsonValue::string(self.display_name.clone()),
        );
        map.insert(
            "adapterKeyName".to_string(),
            JsonValue::string(self.adapter_key_name.clone()),
        );
        map.insert(
            "configFormat".to_string(),
            JsonValue::string(self.config_format.clone()),
        );
        map.insert(
            "configPaths".to_string(),
            JsonValue::array(self.config_paths.iter().cloned().map(JsonValue::string)),
        );
        map.insert(
            "configPrecedence".to_string(),
            JsonValue::array(
                self.config_precedence
                    .iter()
                    .cloned()
                    .map(JsonValue::string),
            ),
        );
        map.insert(
            "nativeScopes".to_string(),
            JsonValue::array(self.native_scopes.iter().cloned().map(JsonValue::string)),
        );
        map.insert(
            "preferredIngress".to_string(),
            JsonValue::string(self.preferred_ingress.clone()),
        );
        map.insert(
            "exportMode".to_string(),
            JsonValue::string(self.export_mode.clone()),
        );
        map.insert(
            "entrypointMode".to_string(),
            JsonValue::string(self.entrypoint_mode.clone()),
        );
        map.insert(
            "launcherCommand".to_string(),
            JsonValue::string(self.launcher_command.clone()),
        );
        map.insert(
            "rootPath".to_string(),
            JsonValue::string(self.root_path.clone()),
        );
        map.insert(
            "canConnectToday".to_string(),
            JsonValue::bool(self.can_connect_today),
        );
        map.insert(
            "writesConfig".to_string(),
            JsonValue::bool(self.writes_config),
        );
        match &self.recommended_install_scope {
            Some(value) => {
                map.insert(
                    "recommendedInstallScope".to_string(),
                    JsonValue::string(value.clone()),
                );
            }
            None => {
                map.insert("recommendedInstallScope".to_string(), JsonValue::Null);
            }
        }
        match &self.recommended_install_path {
            Some(value) => {
                map.insert(
                    "recommendedInstallPath".to_string(),
                    JsonValue::string(value.clone()),
                );
            }
            None => {
                map.insert("recommendedInstallPath".to_string(), JsonValue::Null);
            }
        }
        map.insert(
            "adapterContract".to_string(),
            self.adapter_contract.to_json_value(),
        );
        map.insert(
            "blockers".to_string(),
            JsonValue::array(self.blockers.iter().cloned().map(JsonValue::string)),
        );
        map.insert(
            "warnings".to_string(),
            JsonValue::array(self.warnings.iter().cloned().map(JsonValue::string)),
        );
        map.insert(
            "nextActions".to_string(),
            JsonValue::array(self.next_actions.iter().cloned().map(JsonValue::string)),
        );
        JsonValue::Object(map)
    }

    fn write_text(&self, stdout: &mut dyn Write) {
        let _ = writeln!(
            stdout,
            "Client export {}",
            if self.can_connect_today {
                "connectable preview"
            } else {
                "preview only"
            }
        );
        let _ = writeln!(
            stdout,
            "Client target: {} ({})",
            self.client_target_id, self.display_name
        );
        let _ = writeln!(stdout, "Adapter key: {}", self.adapter_key_name);
        let _ = writeln!(stdout, "Config format: {}", self.config_format);
        let _ = writeln!(stdout, "Config paths: {}", join_or_none(&self.config_paths));
        let _ = writeln!(
            stdout,
            "Config precedence: {}",
            join_or_none(&self.config_precedence)
        );
        let _ = writeln!(
            stdout,
            "Native scopes: {}",
            join_or_none(&self.native_scopes)
        );
        let _ = writeln!(stdout, "Preferred ingress: {}", self.preferred_ingress);
        let _ = writeln!(stdout, "Export mode: {}", self.export_mode);
        let _ = writeln!(stdout, "Entrypoint mode: {}", self.entrypoint_mode);
        let _ = writeln!(stdout, "Launcher command: {}", self.launcher_command);
        let _ = writeln!(
            stdout,
            "Can connect today: {}",
            yes_no(self.can_connect_today)
        );
        let _ = writeln!(stdout, "Writes config: {}", yes_no(self.writes_config));
        let _ = writeln!(
            stdout,
            "Recommended install scope: {}",
            self.recommended_install_scope.as_deref().unwrap_or("none")
        );
        let _ = writeln!(
            stdout,
            "Recommended install path: {}",
            self.recommended_install_path.as_deref().unwrap_or("none")
        );
        self.adapter_contract.write_text(stdout);
        let _ = writeln!(stdout, "Blockers: {}", join_or_none(&self.blockers));
        let _ = writeln!(stdout, "Warnings: {}", join_or_none(&self.warnings));
        let _ = writeln!(stdout, "Next actions: {}", join_or_none(&self.next_actions));
    }
}

impl ClientInstallRunOptions {
    fn from_args(parsed: &ParsedArgs) -> Self {
        Self {
            dry_run: parsed.dry_run,
            diff: parsed.diff,
        }
    }
}

impl ClientInstallPlan {
    fn from_plan(
        root_path: &Path,
        target: &ClientTarget,
        plan: &super::model::ClientPlan,
    ) -> Result<Self, String> {
        let adapter_key_name = plan
            .configured_client_key_name
            .clone()
            .unwrap_or_else(|| "MCPace".to_string());
        let Some(install_support) = target.install_support() else {
            return Err(format!(
                "client install currently supports {}; '{}' remains manual for now",
                client_install_support_summary(),
                target.id,
            ));
        };

        let config_path = resolve_install_path(&install_support.preferred_config_path)?;
        let config_scope = install_support.preferred_scope.clone();
        let config = install_config_for_target(target)?;

        Ok(Self {
            client_target_id: target.id.clone(),
            display_name: target.display_name.clone(),
            adapter_key_name,
            config_path,
            backup_root: install_backup_root(root_path),
            config_scope,
            server_url: local_mcp_url(),
            config,
            warnings: install_warnings_from_plan(plan, target),
        })
    }

    fn write_with_options(
        &self,
        options: &ClientInstallRunOptions,
    ) -> Result<ClientInstallResult, String> {
        let config_dir = self
            .config_path
            .parent()
            .ok_or_else(|| "failed to resolve the target client config directory".to_string())?;
        let would_create_config_dir = !config_dir.is_dir();
        let created_config_dir = if would_create_config_dir && !options.dry_run {
            fs::create_dir_all(config_dir).map_err(|error| {
                format!(
                    "failed to create client config directory '{}': {}",
                    config_dir.display(),
                    error
                )
            })?;
            true
        } else {
            false
        };

        let config_file_existed = self.config_path.is_file();
        let existing = match fs::read_to_string(&self.config_path) {
            Ok(value) => value,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => {
                return Err(format!(
                    "failed to read client config '{}': {}",
                    self.config_path.display(),
                    error
                ))
            }
        };
        let would_create_config_file = existing.is_empty() && !config_file_existed;
        let created_config_file = would_create_config_file && !options.dry_run;
        let update = match &self.config {
            ClientInstallConfig::TomlManagedTable => {
                let newline = detect_newline(&existing);
                let managed_block =
                    build_toml_managed_block(&self.adapter_key_name, &self.server_url, newline);
                upsert_toml_managed_block(
                    &existing,
                    &self.adapter_key_name,
                    &managed_block,
                    &self.config_path,
                )?
            }
            ClientInstallConfig::JsonMcpServers { server_config } => upsert_json_mcp_server(
                &existing,
                &self.adapter_key_name,
                server_config.clone(),
                &self.config_path,
            )?,
            ClientInstallConfig::YamlMcpServers => upsert_yaml_mcp_server(
                &existing,
                &self.adapter_key_name,
                &self.server_url,
                &self.config_path,
            )?,
        };

        let would_change = update.contents != existing;
        let changed = would_change && !options.dry_run;
        let backup = if changed {
            Some(self.create_backup(&existing, config_file_existed)?)
        } else {
            None
        };
        if changed {
            fs::write(&self.config_path, &update.contents).map_err(|error| {
                format!(
                    "failed to write client config '{}': {}",
                    self.config_path.display(),
                    error
                )
            })?;
        }
        let diff = if options.diff {
            Some(build_unified_config_diff(
                &self.config_path,
                &existing,
                &update.contents,
            ))
        } else {
            None
        };

        Ok(ClientInstallResult {
            client_target_id: self.client_target_id.clone(),
            display_name: self.display_name.clone(),
            adapter_key_name: self.adapter_key_name.clone(),
            config_path: sanitize_path_for_display(&self.config_path),
            config_scope: self.config_scope.clone(),
            transport: "streamable-http".to_string(),
            server_url: self.server_url.clone(),
            changed,
            would_change,
            dry_run: options.dry_run,
            diff_requested: options.diff,
            diff,
            persisted: changed,
            backup_created: backup.is_some(),
            backup_id: backup.as_ref().map(|value| value.id.clone()),
            backup_path: backup
                .as_ref()
                .map(|value| sanitize_path_for_display(&value.path)),
            backup_manifest_path: backup
                .as_ref()
                .map(|value| sanitize_path_for_display(&value.manifest_path)),
            restore_command: backup.as_ref().map(|value| value.restore_command.clone()),
            replaced_existing_block: update.replaced_existing_block,
            created_config_dir,
            created_config_file,
            would_create_config_dir,
            would_create_config_file,
            warnings: self.warnings.clone(),
        })
    }

    fn create_backup(
        &self,
        existing: &str,
        config_file_existed: bool,
    ) -> Result<ClientInstallBackup, String> {
        let now_ms = now_ms();
        let config_path_text = self.config_path.display().to_string();
        let config_path_hash = stable_hash_hex(&config_path_text);
        let id_hash = stable_hash_hex(&format!(
            "{}|{}|{}|{}",
            self.client_target_id, self.adapter_key_name, config_path_text, now_ms
        ));
        let id = format!("{}-{}", now_ms, &id_hash[..8]);
        let backup_path = self
            .backup_root
            .join(safe_file_segment(&self.client_target_id))
            .join(&id);
        fs::create_dir_all(&backup_path).map_err(|error| {
            format!(
                "failed to create client install backup directory '{}': {}",
                backup_path.display(),
                error
            )
        })?;

        let content_path = backup_path.join("config.before");
        if config_file_existed {
            fs::write(&content_path, existing).map_err(|error| {
                format!(
                    "failed to write client install backup '{}': {}",
                    content_path.display(),
                    error
                )
            })?;
        }

        let manifest_path = backup_path.join("manifest.json");
        let restore_command = format!(
            "mcpace client restore {} --backup {} --root <mcpace-root>",
            self.client_target_id, id
        );
        let manifest = JsonValue::object([
            ("schema", JsonValue::string("mcpace.clientInstallBackup.v1")),
            ("backupId", JsonValue::string(id.clone())),
            ("createdAtMs", JsonValue::number(now_ms)),
            (
                "clientTargetId",
                JsonValue::string(self.client_target_id.clone()),
            ),
            ("displayName", JsonValue::string(self.display_name.clone())),
            (
                "adapterKeyName",
                JsonValue::string(self.adapter_key_name.clone()),
            ),
            ("configPath", JsonValue::string(config_path_text)),
            ("configPathHash", JsonValue::string(config_path_hash)),
            ("configExisted", JsonValue::bool(config_file_existed)),
            (
                "contentPath",
                if config_file_existed {
                    JsonValue::string("config.before")
                } else {
                    JsonValue::Null
                },
            ),
            ("restoreCommand", JsonValue::string(restore_command.clone())),
        ]);
        fs::write(&manifest_path, manifest.to_pretty_string()).map_err(|error| {
            format!(
                "failed to write client install backup manifest '{}': {}",
                manifest_path.display(),
                error
            )
        })?;

        Ok(ClientInstallBackup {
            id,
            path: backup_path,
            manifest_path,
            restore_command,
        })
    }
}

fn build_unified_config_diff(path: &Path, before: &str, after: &str) -> String {
    if before == after {
        return String::new();
    }

    let display_path = sanitize_path_for_display(path);
    let mut diff = Vec::new();
    diff.push(format!("--- {} (current)", display_path));
    diff.push(format!("+++ {} (candidate)", display_path));

    let mut before_state = DiffSanitizeState::default();
    if !before.is_empty() {
        for line in before.lines() {
            diff.push(format!(
                "-{}",
                sanitize_config_diff_line(line, &mut before_state)
            ));
        }
    }
    let mut after_state = DiffSanitizeState::default();
    if !after.is_empty() {
        for line in after.lines() {
            diff.push(format!(
                "+{}",
                sanitize_config_diff_line(line, &mut after_state)
            ));
        }
    }

    diff.join("\n")
}

#[derive(Default)]
struct DiffSanitizeState {
    in_sensitive_multiline_value: bool,
    close_marker: Option<&'static str>,
}

fn sanitize_config_diff_line(line: &str, state: &mut DiffSanitizeState) -> String {
    let escaped = escape_diff_control_chars(line);
    if state.in_sensitive_multiline_value {
        if let Some(marker) = state.close_marker {
            if escaped.contains(marker) {
                state.in_sensitive_multiline_value = false;
                state.close_marker = None;
            }
            return "[REDACTED]".to_string();
        }
        if is_top_level_config_boundary(&escaped) {
            state.in_sensitive_multiline_value = false;
        } else {
            return "[REDACTED]".to_string();
        }
    }

    let separator_index = match (escaped.find('='), escaped.find(':')) {
        (Some(equal), Some(colon)) => Some(equal.min(colon)),
        (Some(equal), None) => Some(equal),
        (None, Some(colon)) => Some(colon),
        (None, None) => None,
    };
    let lower_line = escaped.to_ascii_lowercase();
    let key_area = separator_index
        .map(|index| &escaped[..index])
        .unwrap_or(&escaped)
        .to_ascii_lowercase();
    let sensitive_keys = [
        "token",
        "api_key",
        "apikey",
        "api-key",
        "private_key",
        "private-key",
        "secret",
        "password",
        "passwd",
        "auth",
        "authorization",
        "credential",
    ];
    if !sensitive_keys
        .iter()
        .any(|sensitive_key| lower_line.contains(sensitive_key))
    {
        return escaped;
    }

    state.in_sensitive_multiline_value = true;
    state.close_marker = sensitive_multiline_close_marker(&escaped);

    if !sensitive_keys
        .iter()
        .any(|sensitive_key| key_area.contains(sensitive_key))
    {
        return "[REDACTED]".to_string();
    }

    let Some(separator_index) = separator_index else {
        return "[REDACTED]".to_string();
    };
    let prefix = escaped[..=separator_index].trim_end();
    let suffix = if escaped.trim_end().ends_with(',') {
        ","
    } else {
        ""
    };
    format!("{} \"[redacted]\"{}", prefix, suffix)
}

fn sensitive_multiline_close_marker(line: &str) -> Option<&'static str> {
    if line.matches("\"\"\"").count() % 2 == 1 {
        return Some("\"\"\"");
    }
    if line.matches("'''").count() % 2 == 1 {
        return Some("'''");
    }
    None
}

fn is_top_level_config_boundary(line: &str) -> bool {
    let trimmed = line.trim_start();
    if trimmed.is_empty() || line.len() != trimmed.len() {
        return false;
    }
    if trimmed.starts_with('[') {
        return true;
    }
    let Some(separator_index) = trimmed.find('=').or_else(|| trimmed.find(':')) else {
        return false;
    };
    let key = trimmed[..separator_index].trim();
    !key.is_empty()
        && key
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | '"' | '\''))
}

fn escape_diff_control_chars(line: &str) -> String {
    let mut escaped = String::new();
    for ch in line.chars() {
        if ch == '\x1b' {
            escaped.push_str("\\x1b");
        } else if ch.is_control() && ch != '\t' {
            escaped.push_str(&format!("\\u{{{:x}}}", ch as u32));
        } else {
            escaped.push(ch);
        }
    }
    escaped
}

impl ClientInstallResult {
    fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            (
                "mode",
                JsonValue::string(if self.dry_run {
                    "install-preview"
                } else {
                    "installed"
                }),
            ),
            (
                "clientTargetId",
                JsonValue::string(self.client_target_id.clone()),
            ),
            ("displayName", JsonValue::string(self.display_name.clone())),
            (
                "adapterKeyName",
                JsonValue::string(self.adapter_key_name.clone()),
            ),
            ("configPath", JsonValue::string(self.config_path.clone())),
            ("configScope", JsonValue::string(self.config_scope.clone())),
            ("transport", JsonValue::string(self.transport.clone())),
            ("url", JsonValue::string(self.server_url.clone())),
            ("writesConfig", JsonValue::bool(!self.dry_run)),
            ("dryRun", JsonValue::bool(self.dry_run)),
            ("persisted", JsonValue::bool(self.persisted)),
            ("backupCreated", JsonValue::bool(self.backup_created)),
            (
                "backupId",
                self.backup_id
                    .as_ref()
                    .map(|value| JsonValue::string(value.clone()))
                    .unwrap_or(JsonValue::Null),
            ),
            (
                "backupPath",
                self.backup_path
                    .as_ref()
                    .map(|value| JsonValue::string(value.clone()))
                    .unwrap_or(JsonValue::Null),
            ),
            (
                "backupManifestPath",
                self.backup_manifest_path
                    .as_ref()
                    .map(|value| JsonValue::string(value.clone()))
                    .unwrap_or(JsonValue::Null),
            ),
            (
                "restoreCommand",
                self.restore_command
                    .as_ref()
                    .map(|value| JsonValue::string(value.clone()))
                    .unwrap_or(JsonValue::Null),
            ),
            ("changed", JsonValue::bool(self.changed)),
            ("wouldChange", JsonValue::bool(self.would_change)),
            ("diffRequested", JsonValue::bool(self.diff_requested)),
            (
                "diff",
                self.diff
                    .as_ref()
                    .map(|value| JsonValue::string(value.clone()))
                    .unwrap_or(JsonValue::Null),
            ),
            (
                "replacedExistingBlock",
                JsonValue::bool(self.replaced_existing_block),
            ),
            ("createdConfigDir", JsonValue::bool(self.created_config_dir)),
            (
                "createdConfigFile",
                JsonValue::bool(self.created_config_file),
            ),
            (
                "wouldCreateConfigDir",
                JsonValue::bool(self.would_create_config_dir),
            ),
            (
                "wouldCreateConfigFile",
                JsonValue::bool(self.would_create_config_file),
            ),
            (
                "warnings",
                JsonValue::array(self.warnings.iter().cloned().map(JsonValue::string)),
            ),
        ])
    }

    fn write_text(&self, stdout: &mut dyn Write) {
        let _ = writeln!(
            stdout,
            "{}",
            if self.dry_run {
                "Client install dry-run complete"
            } else {
                "Client install complete"
            }
        );
        let _ = writeln!(
            stdout,
            "Client target: {} ({})",
            self.client_target_id, self.display_name
        );
        let _ = writeln!(stdout, "Adapter key: {}", self.adapter_key_name);
        let _ = writeln!(stdout, "Config path: {}", self.config_path);
        let _ = writeln!(stdout, "Config scope: {}", self.config_scope);
        let _ = writeln!(stdout, "Transport: {}", self.transport);
        let _ = writeln!(stdout, "URL: {}", self.server_url);
        let _ = writeln!(stdout, "Changed config: {}", yes_no(self.changed));
        let _ = writeln!(stdout, "Would change config: {}", yes_no(self.would_change));
        let _ = writeln!(stdout, "Persisted: {}", yes_no(self.persisted));
        let _ = writeln!(stdout, "Backup created: {}", yes_no(self.backup_created));
        if let Some(backup_path) = &self.backup_path {
            let _ = writeln!(stdout, "Backup path: {}", backup_path);
        }
        if let Some(restore_command) = &self.restore_command {
            let _ = writeln!(stdout, "Restore command: {}", restore_command);
        }
        let _ = writeln!(
            stdout,
            "Replaced existing block: {}",
            yes_no(self.replaced_existing_block)
        );
        let _ = writeln!(
            stdout,
            "Created config directory: {}",
            yes_no(self.created_config_dir)
        );
        let _ = writeln!(
            stdout,
            "Created config file: {}",
            yes_no(self.created_config_file)
        );
        let _ = writeln!(
            stdout,
            "Would create config directory: {}",
            yes_no(self.would_create_config_dir)
        );
        let _ = writeln!(
            stdout,
            "Would create config file: {}",
            yes_no(self.would_create_config_file)
        );
        if let Some(diff) = &self.diff {
            if !diff.is_empty() {
                let _ = writeln!(stdout, "Diff:\n{}", diff);
            }
        }
        let _ = writeln!(stdout, "Warnings: {}", join_or_none(&self.warnings));
    }
}

impl ClientRestoreResult {
    fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("mode", JsonValue::string("restored")),
            (
                "clientTargetId",
                JsonValue::string(self.client_target_id.clone()),
            ),
            ("backupId", JsonValue::string(self.backup_id.clone())),
            ("backupPath", JsonValue::string(self.backup_path.clone())),
            ("configPath", JsonValue::string(self.config_path.clone())),
            (
                "restoredExistingConfig",
                JsonValue::bool(self.restored_existing_config),
            ),
            (
                "removedConfigFile",
                JsonValue::bool(self.removed_config_file),
            ),
            ("wroteConfigFile", JsonValue::bool(self.wrote_config_file)),
        ])
    }

    fn write_text(&self, stdout: &mut dyn Write) {
        let _ = writeln!(stdout, "Client config restore complete");
        let _ = writeln!(stdout, "Client target: {}", self.client_target_id);
        let _ = writeln!(stdout, "Backup id: {}", self.backup_id);
        let _ = writeln!(stdout, "Backup path: {}", self.backup_path);
        let _ = writeln!(stdout, "Config path: {}", self.config_path);
        let _ = writeln!(
            stdout,
            "Restored existing config: {}",
            yes_no(self.restored_existing_config)
        );
        let _ = writeln!(
            stdout,
            "Removed config file: {}",
            yes_no(self.removed_config_file)
        );
        let _ = writeln!(
            stdout,
            "Wrote config file: {}",
            yes_no(self.wrote_config_file)
        );
    }
}

impl AdapterContractPreview {
    fn to_json_value(&self) -> JsonValue {
        let mut map = BTreeMap::new();
        map.insert("type".to_string(), JsonValue::string(self.kind.clone()));
        match &self.command {
            Some(value) => {
                map.insert("command".to_string(), JsonValue::string(value.clone()));
            }
            None => {
                map.insert("command".to_string(), JsonValue::Null);
            }
        }
        map.insert(
            "args".to_string(),
            JsonValue::array(self.args.iter().cloned().map(JsonValue::string)),
        );
        match &self.url_template {
            Some(value) => {
                map.insert("urlTemplate".to_string(), JsonValue::string(value.clone()));
            }
            None => {
                map.insert("urlTemplate".to_string(), JsonValue::Null);
            }
        }
        map.insert(
            "metadataCarrier".to_string(),
            JsonValue::string(self.metadata_carrier.clone()),
        );
        map.insert(
            "sessionModel".to_string(),
            JsonValue::string(self.session_model.clone()),
        );
        map.insert(
            "notes".to_string(),
            JsonValue::array(self.notes.iter().cloned().map(JsonValue::string)),
        );
        JsonValue::Object(map)
    }

    fn write_text(&self, stdout: &mut dyn Write) {
        let _ = writeln!(stdout, "Adapter contract type: {}", self.kind);
        let _ = writeln!(
            stdout,
            "Adapter command: {}",
            self.command.as_deref().unwrap_or("none")
        );
        let _ = writeln!(stdout, "Adapter args: {}", join_or_none(&self.args));
        let _ = writeln!(
            stdout,
            "Adapter URL template: {}",
            self.url_template.as_deref().unwrap_or("none")
        );
        let _ = writeln!(stdout, "Metadata carrier: {}", self.metadata_carrier);
        let _ = writeln!(stdout, "Session model: {}", self.session_model);
        let _ = writeln!(stdout, "Adapter notes: {}", join_or_none(&self.notes));
    }
}

fn resolve_export_mode(target: &ClientTarget, plan: &super::model::ClientPlan) -> String {
    if target.has_constraint("public-http-only") || target.surface_class == "cloud" {
        return "public-http-connector".to_string();
    }
    if plan.preferred_ingress == "streamable-http" || target.supports_ingress("streamable-http") {
        return "local-streamable-http".to_string();
    }
    "local-stdio-launcher".to_string()
}

fn preferred_install_config_path(target: &ClientTarget) -> &str {
    target
        .preferred_install_config_path()
        .unwrap_or("client config")
}

fn preferred_install_scope(target: &ClientTarget) -> &str {
    target.preferred_install_scope().unwrap_or("project")
}

fn build_json_install_server_config(shape: JsonMcpServerShape) -> JsonValue {
    let mut entries = Vec::new();
    if shape.include_type_http {
        entries.push(("type".to_string(), JsonValue::string("http")));
    }
    entries.push((
        shape.url_field.to_string(),
        JsonValue::string(local_mcp_url()),
    ));
    if shape.include_tools_star {
        entries.push((
            "tools".to_string(),
            JsonValue::array([JsonValue::string("*")]),
        ));
    }
    if shape.include_disabled_false {
        entries.push(("disabled".to_string(), JsonValue::bool(false)));
    }
    JsonValue::object(entries)
}

fn install_config_for_target(target: &ClientTarget) -> Result<ClientInstallConfig, String> {
    let Some(install_support) = target.install_support() else {
        return Err(format!(
            "client install currently supports {}; '{}' remains manual for now",
            client_install_support_summary(),
            target.id,
        ));
    };

    match install_support.kind {
        ClientInstallKind::TomlMcpServersManagedTable => Ok(ClientInstallConfig::TomlManagedTable),
        ClientInstallKind::JsonMcpServers(shape) => Ok(ClientInstallConfig::JsonMcpServers {
            server_config: build_json_install_server_config(shape),
        }),
        ClientInstallKind::YamlMcpServersManagedSection => Ok(ClientInstallConfig::YamlMcpServers),
    }
}

fn build_adapter_contract(
    target: &ClientTarget,
    plan: &super::model::ClientPlan,
    export_mode: &str,
) -> AdapterContractPreview {
    match export_mode {
        "public-http-connector" => AdapterContractPreview {
            kind: "public-http-connector".to_string(),
            command: None,
            args: Vec::new(),
            url_template: Some("https://YOUR-MCPACE-RELAY/mcp".to_string()),
            metadata_carrier: "public HTTP request metadata plus MCP session headers".to_string(),
            session_model: "planned public HTTP session plus relay-owned auth context; real hosted proof still required".to_string(),
            notes: vec![
                format!(
                    "{} only reaches public HTTP MCP servers, so MCPace needs a relay/public ingress instead of a local launcher.",
                    target.display_name
                ),
                "The cloud/API connector path keeps one visible MCPace URL as the goal, but relay/auth/runtime proof is still pending for this lane.".to_string(),
            ],
        },
        "local-streamable-http" => AdapterContractPreview {
            kind: "local-streamable-http".to_string(),
            command: None,
            args: Vec::new(),
            url_template: Some(local_mcp_url()),
            metadata_carrier: "localhost HTTP request metadata plus session headers".to_string(),
            session_model: "connectable localhost HTTP endpoint; sticky session ownership is still preview until runtime proof lands".to_string(),
            notes: vec![
                "Use one localhost MCPace URL so the client can target a single MCPace endpoint while upstream routing proof is still in progress.".to_string(),
                "Today this lane proves connectability to /mcp; full session reuse, ownership, and upstream-runtime guarantees remain preview.".to_string(),
            ],
        },
        _ => AdapterContractPreview {
            kind: "stdio-launcher".to_string(),
            command: Some(plan.launcher_command.clone()),
            args: vec![
                "mcp-server".to_string(),
                "--root".to_string(),
                sanitize_launcher_root_path(&plan.root_path),
                "--client-id".to_string(),
                target.id.clone(),
            ],
            url_template: None,
            metadata_carrier:
                "MCP initialize params, roots, cwd, and optional _meta context hints".to_string(),
            session_model: "preview stdio launcher contract; live lease/session forwarding is still pending runtime proof".to_string(),
            notes: vec![
                format!(
                    "{} should see one MCPace launcher entry instead of one config block per upstream MCP server.",
                    target.display_name
                ),
                "This preview keeps one stable stdio launcher command, but bootstrap-only proof should not be treated as completed live forwarding yet.".to_string(),
            ],
        },
    }
}

fn sanitize_launcher_root_path(root_path: &str) -> String {
    root_path
        .strip_prefix(r"\\?\")
        .unwrap_or(root_path)
        .to_string()
}

fn sanitize_path_for_display(path: &Path) -> String {
    sanitize_launcher_root_path(&path.display().to_string())
}

fn canonicalize_or_original(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn resolve_install_path(default_config_path: &str) -> Result<PathBuf, String> {
    let home = user_home_dir().ok_or_else(|| {
        "failed to resolve the current user's home directory for user-scoped client config"
            .to_string()
    })?;

    let relative = default_config_path
        .strip_prefix("~/")
        .or_else(|| default_config_path.strip_prefix("~\\"))
        .ok_or_else(|| {
            format!(
                "client install default config path '{}' is not user-home based yet",
                default_config_path
            )
        })?;

    let mut path = home;
    for segment in relative.split(['/', '\\']) {
        if !segment.is_empty() {
            path.push(segment);
        }
    }
    Ok(path)
}

fn user_home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn detect_newline(existing: &str) -> &'static str {
    if existing.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

fn empty_json_object() -> JsonValue {
    JsonValue::object::<String, Vec<(String, JsonValue)>>(Vec::new())
}

fn build_toml_managed_block(adapter_key_name: &str, server_url: &str, newline: &str) -> String {
    let lines = [
        format!("# BEGIN MCPACE MANAGED BLOCK: {}", adapter_key_name),
        "# This block is managed by `mcpace client install`.".to_string(),
        format!("[mcp_servers.{}]", format_toml_table_key(adapter_key_name)),
        format!("url = {}", toml_basic_string(server_url)),
        "enabled = true".to_string(),
        "startup_timeout_sec = 20".to_string(),
        format!("# END MCPACE MANAGED BLOCK: {}", adapter_key_name),
        String::new(),
    ];
    lines.join(newline)
}

fn build_yaml_mcp_servers_entry_block(
    adapter_key_name: &str,
    server_url: &str,
    newline: &str,
) -> String {
    let lines = [
        format!("  # BEGIN MCPACE MANAGED BLOCK: {}", adapter_key_name),
        format!("  {}:", adapter_key_name),
        format!("    url: {}", yaml_double_quoted_string(server_url)),
        "    enabled: true".to_string(),
        "    timeout: 120".to_string(),
        "    connect_timeout: 60".to_string(),
        format!("  # END MCPACE MANAGED BLOCK: {}", adapter_key_name),
        String::new(),
    ];
    lines.join(newline)
}

struct ClientConfigUpdate {
    contents: String,
    replaced_existing_block: bool,
}

fn upsert_json_mcp_server(
    existing: &str,
    adapter_key_name: &str,
    server_config: JsonValue,
    config_path: &Path,
) -> Result<ClientConfigUpdate, String> {
    let mut root = if existing.trim().is_empty() {
        JsonValue::object::<String, Vec<(String, JsonValue)>>(Vec::new())
    } else {
        parse_str(existing).map_err(|error| {
            format!(
                "failed to parse JSON client config '{}': {}",
                config_path.display(),
                error
            )
        })?
    };

    let root_object = match &mut root {
        JsonValue::Object(map) => map,
        _ => {
            return Err(format!(
                "JSON client config '{}' must be a top-level object",
                config_path.display()
            ))
        }
    };

    let servers_value = root_object
        .entry("mcpServers".to_string())
        .or_insert_with(empty_json_object);
    let servers_object = match servers_value {
        JsonValue::Object(map) => map,
        _ => {
            return Err(format!(
                "JSON client config '{}' has a non-object mcpServers field",
                config_path.display()
            ))
        }
    };

    let replaced_existing_block = servers_object.contains_key(adapter_key_name);
    servers_object.insert(adapter_key_name.to_string(), server_config);

    Ok(ClientConfigUpdate {
        contents: root.to_pretty_string(),
        replaced_existing_block,
    })
}

fn upsert_toml_managed_block(
    existing: &str,
    adapter_key_name: &str,
    managed_block: &str,
    config_path: &Path,
) -> Result<ClientConfigUpdate, String> {
    if let Some((start, end)) = find_managed_block(existing, adapter_key_name, config_path)? {
        let mut updated = String::new();
        updated.push_str(&existing[..start]);
        updated.push_str(managed_block);
        updated.push_str(&existing[end..]);
        return Ok(ClientConfigUpdate {
            contents: updated,
            replaced_existing_block: true,
        });
    }

    if let Some((start, end)) = find_toml_mcp_servers_table_block(existing, adapter_key_name) {
        let mut updated = String::new();
        updated.push_str(&existing[..start]);
        updated.push_str(managed_block);
        updated.push_str(&existing[end..]);
        return Ok(ClientConfigUpdate {
            contents: updated,
            replaced_existing_block: true,
        });
    }

    let newline = detect_newline(existing);
    let mut updated = existing.to_string();
    if !updated.is_empty() {
        if !updated.ends_with('\n') {
            updated.push_str(newline);
        }
        if !updated.ends_with(&(newline.to_string() + newline)) {
            updated.push_str(newline);
        }
    }
    updated.push_str(managed_block);
    Ok(ClientConfigUpdate {
        contents: updated,
        replaced_existing_block: false,
    })
}

fn upsert_yaml_mcp_server(
    existing: &str,
    adapter_key_name: &str,
    server_url: &str,
    config_path: &Path,
) -> Result<ClientConfigUpdate, String> {
    let newline = detect_newline(existing);
    let entry_block = build_yaml_mcp_servers_entry_block(adapter_key_name, server_url, newline);

    if let Some((start, end)) = find_managed_block(existing, adapter_key_name, config_path)? {
        let mut updated = String::new();
        updated.push_str(&existing[..start]);
        updated.push_str(&entry_block);
        updated.push_str(&existing[end..]);
        return Ok(ClientConfigUpdate {
            contents: updated,
            replaced_existing_block: true,
        });
    }

    if let Some((section_start, section_body_start, section_end)) =
        find_yaml_top_level_section(existing, "mcp_servers")
    {
        if let Some((entry_start, entry_end)) =
            find_yaml_section_entry(existing, section_body_start, section_end, adapter_key_name)
        {
            let mut updated = String::new();
            updated.push_str(&existing[..entry_start]);
            updated.push_str(&entry_block);
            updated.push_str(&existing[entry_end..]);
            return Ok(ClientConfigUpdate {
                contents: updated,
                replaced_existing_block: true,
            });
        }

        let mut updated = String::new();
        updated.push_str(&existing[..section_end]);
        if section_end > section_start && !existing[..section_end].ends_with('\n') {
            updated.push_str(newline);
        }
        updated.push_str(&entry_block);
        updated.push_str(&existing[section_end..]);
        return Ok(ClientConfigUpdate {
            contents: updated,
            replaced_existing_block: false,
        });
    }

    let mut updated = existing.to_string();
    if !updated.is_empty() {
        if !updated.ends_with('\n') {
            updated.push_str(newline);
        }
        if !updated.ends_with(&(newline.to_string() + newline)) {
            updated.push_str(newline);
        }
    }
    updated.push_str("mcp_servers:");
    updated.push_str(newline);
    updated.push_str(&entry_block);
    Ok(ClientConfigUpdate {
        contents: updated,
        replaced_existing_block: false,
    })
}

fn find_managed_block(
    existing: &str,
    adapter_key_name: &str,
    config_path: &Path,
) -> Result<Option<(usize, usize)>, String> {
    let begin_marker = format!("# BEGIN MCPACE MANAGED BLOCK: {}", adapter_key_name);
    let end_marker = format!("# END MCPACE MANAGED BLOCK: {}", adapter_key_name);
    let Some(marker_start) = existing.find(&begin_marker) else {
        return Ok(None);
    };
    let start = existing[..marker_start]
        .rfind('\n')
        .map(|index| index + 1)
        .unwrap_or(0);
    let Some(relative_end) = existing[marker_start..].find(&end_marker) else {
        return Err(format!(
            "Client config '{}' contains an MCPace begin marker without a matching end marker for '{}'",
            config_path.display(),
            adapter_key_name
        ));
    };

    let mut end = marker_start + relative_end + end_marker.len();
    if existing[end..].starts_with("\r\n") {
        end += 2;
    } else if existing[end..].starts_with('\n') {
        end += 1;
    }
    Ok(Some((start, end)))
}

fn find_toml_mcp_servers_table_block(
    existing: &str,
    adapter_key_name: &str,
) -> Option<(usize, usize)> {
    let candidates = table_header_candidates(adapter_key_name);
    let mut start = None;
    let mut offset = 0usize;

    for line in existing.split_inclusive('\n') {
        let trimmed = trim_toml_line(line);
        if start.is_none() {
            if candidates.iter().any(|candidate| trimmed == candidate) {
                start = Some(offset);
            }
        } else if looks_like_toml_table_header(trimmed) {
            return Some((start.unwrap_or_default(), offset));
        }
        offset += line.len();
    }

    start.map(|value| (value, existing.len()))
}

fn table_header_candidates(adapter_key_name: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    if is_bare_toml_key(adapter_key_name) {
        candidates.push(format!("[mcp_servers.{}]", adapter_key_name));
    }
    candidates.push(format!(
        "[mcp_servers.{}]",
        toml_basic_string(adapter_key_name)
    ));
    candidates
}

fn trim_toml_line(line: &str) -> &str {
    let trimmed = line.trim();
    match trimmed.find('#') {
        Some(index) => trimmed[..index].trim_end(),
        None => trimmed,
    }
}

fn looks_like_toml_table_header(trimmed_line: &str) -> bool {
    trimmed_line.starts_with('[') && trimmed_line.ends_with(']')
}

fn find_yaml_top_level_section(existing: &str, key: &str) -> Option<(usize, usize, usize)> {
    let mut start = None;
    let mut body_start = 0usize;
    let mut offset = 0usize;

    for line in existing.split_inclusive('\n') {
        if let Some((indent, line_key)) = parse_yaml_mapping_key(line) {
            if start.is_none() {
                if indent == 0 && line_key == key {
                    start = Some(offset);
                    body_start = offset + line.len();
                }
            } else if indent == 0 {
                return Some((start.unwrap_or_default(), body_start, offset));
            }
        }
        offset += line.len();
    }

    start.map(|value| (value, body_start, existing.len()))
}

fn find_yaml_section_entry(
    existing: &str,
    section_body_start: usize,
    section_end: usize,
    adapter_key_name: &str,
) -> Option<(usize, usize)> {
    let section = &existing[section_body_start..section_end];
    let mut start = None;
    let mut entry_indent = 0usize;
    let mut offset = 0usize;

    for line in section.split_inclusive('\n') {
        if let Some((indent, line_key)) = parse_yaml_mapping_key(line) {
            if start.is_none() {
                if indent > 0 && line_key == adapter_key_name {
                    start = Some(section_body_start + offset);
                    entry_indent = indent;
                }
            } else if indent == entry_indent {
                return Some((start.unwrap_or_default(), section_body_start + offset));
            }
        }
        offset += line.len();
    }

    start.map(|value| (value, section_end))
}

fn parse_yaml_mapping_key(line: &str) -> Option<(usize, String)> {
    let without_newline = line.trim_end_matches(['\r', '\n']);
    if without_newline.trim().is_empty() {
        return None;
    }
    let indent = without_newline.len() - without_newline.trim_start().len();
    let content = without_newline.trim_start();
    if content.starts_with('#') || content.starts_with('-') {
        return None;
    }
    let colon_index = content.find(':')?;
    let key = content[..colon_index].trim();
    if key.is_empty() {
        return None;
    }
    Some((indent, trim_yaml_key_quotes(key).to_string()))
}

fn trim_yaml_key_quotes(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|trimmed| trimmed.strip_suffix('"'))
        .or_else(|| {
            value
                .strip_prefix('\'')
                .and_then(|trimmed| trimmed.strip_suffix('\''))
        })
        .unwrap_or(value)
}

fn format_toml_table_key(value: &str) -> String {
    if is_bare_toml_key(value) {
        value.to_string()
    } else {
        toml_basic_string(value)
    }
}

fn is_bare_toml_key(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
}

fn toml_basic_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            _ => escaped.push(ch),
        }
    }
    escaped.push('"');
    escaped
}

fn yaml_double_quoted_string(value: &str) -> String {
    toml_basic_string(value)
}

fn install_warnings_from_plan(
    plan: &super::model::ClientPlan,
    target: &ClientTarget,
) -> Vec<String> {
    let mut warnings = plan
        .warnings
        .iter()
        .filter(|warning| {
            warning.contains("At least one routed server uses stdio")
                || warning.contains("Client surface")
                || warning.contains("public HTTP")
                || warning.contains("cannot consume MCPace")
        })
        .cloned()
        .collect::<Vec<_>>();
    warnings.push(format!(
        "MCPace installs {} into the shared {} scope by default at {} so one localhost MCPace server can be reused across projects.",
        target.display_name,
        preferred_install_scope(target),
        preferred_install_config_path(target)
    ));
    warnings.sort();
    warnings.dedup();
    warnings
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}
