use crate::diagnostics;
use crate::text_utils::yes_no;
mod backup;
#[cfg(test)]
mod compatibility_tests;
mod config_update;
mod list;
mod removal;
#[cfg(test)]
mod removal_tests;
mod render_models;

pub(super) use self::list::run_list;

pub(super) fn remove_owned_client_integrations(
    root_path: &Path,
    dry_run: bool,
) -> super::ClientIntegrationResult<JsonValue> {
    removal::remove_owned_client_integrations(root_path, dry_run)
}

use self::backup::{
    install_backup_root, now_ms, restore_client_install_backup, safe_file_segment,
    ClientInstallBackup,
};
use self::config_update::build_unified_config_diff;
use self::render_models::{
    AdapterContractPreview, ClientExportPreview, ClientInstallResult, ClientRestoreResult,
};
use super::args::ParsedArgs;
use super::context::resolve_context;
use super::metadata::load_metadata;
use super::pathing::stable_hash_hex;
use super::plan::build_plan;
use super::render::{join_semicolon_or_none, write_text_plan};
use crate::client_catalog::{
    self, client_install_support_summary as catalog_client_install_support_summary,
    ClientInstallKindRecord as ClientInstallKind, ClientTargetRecord as ClientTarget,
    JsonMcpServerShapeRecord as JsonMcpServerShape,
};
use crate::config_edit::{
    apply_json_mcp_server_entry, apply_toml_mcp_server_block, apply_yaml_mcp_server_entry,
    build_toml_mcp_server_block, detect_missing_stdio_command_warnings, detect_newline,
};
use crate::doctor;
use crate::json::JsonValue;
use crate::json_helpers;
use crate::runtimepaths;
use crate::server;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

fn local_mcp_url(root_path: &Path) -> String {
    runtimepaths::configured_mcp_url(root_path)
}

fn public_mcp_url_or_placeholder(root_path: &Path) -> String {
    runtimepaths::public_mcp_url_or_placeholder(Some(root_path))
}

pub(super) fn client_install_support_summary() -> String {
    catalog_client_install_support_summary()
}

pub(super) fn run_plan(
    parsed: ParsedArgs,
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let root_path = parsed.root_override.clone().or(default_root);
    let Some(root_path) = root_path else {
        diagnostics::stderr_line(
            stderr,
            format_args!("mcpace root not found; expected mcpace.config.json"),
        );
        return 1;
    };

    let json = match build_plan_json(parsed.clone(), &root_path) {
        Ok(value) => value,
        Err(error) => {
            diagnostics::stderr_line(stderr, format_args!("{}", error));
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
            diagnostics::stderr_line(stderr, format_args!("{}", error));
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
        diagnostics::stderr_line(
            stderr,
            format_args!("mcpace root not found; expected mcpace.config.json"),
        );
        return 1;
    };

    let registry = match client_catalog::load_registry(Some(&root_path)) {
        Ok(value) => value,
        Err(error) => {
            diagnostics::stderr_line(stderr, format_args!("{}", error));
            return 1;
        }
    };

    let metadata = match load_metadata(&parsed) {
        Ok(value) => value,
        Err(error) => {
            diagnostics::stderr_line(stderr, format_args!("{}", error));
            return 2;
        }
    };
    let mut context = resolve_context(&parsed, &metadata);
    if context.project_root.is_none() {
        context.project_root = Some(sanitize_path_for_display(
            &runtimepaths::canonicalize_or_original(&root_path),
        ));
        context.project_root_source = "export-root".to_string();
    }
    let client_target = match client_catalog::find_in_targets(&registry.targets, &context.client_id)
    {
        Some(value) => value,
        None => {
            diagnostics::stderr_line(stderr, format_args!("unknown client target '{}'; use 'mcpace advanced client list' to inspect supported surfaces",
                context.client_id));
            return 2;
        }
    };
    prefer_local_http_when_supported(&parsed, &mut context, client_target, "serve-default");

    let server_records = match server::load_server_records(&root_path) {
        Ok(records) => records,
        Err(error) => {
            diagnostics::stderr_line(stderr, format_args!("{}", error));
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
        diagnostics::stderr_line(
            stderr,
            format_args!("mcpace root not found; expected mcpace.config.json"),
        );
        return 1;
    };

    let registry = match client_catalog::load_registry(Some(&root_path)) {
        Ok(value) => value,
        Err(error) => {
            diagnostics::stderr_line(stderr, format_args!("{}", error));
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
            diagnostics::stderr_line(stderr, format_args!("{}", error));
            return 2;
        }
    };
    let mut context = resolve_context(&parsed, &metadata);
    if context.project_root.is_none() {
        context.project_root = Some(sanitize_path_for_display(
            &runtimepaths::canonicalize_or_original(&root_path),
        ));
        context.project_root_source = "install-root".to_string();
    }
    let client_target = match client_catalog::find_in_targets(&registry.targets, &context.client_id)
    {
        Some(value) => value,
        None => {
            diagnostics::stderr_line(stderr, format_args!("unknown client target '{}'; use 'mcpace advanced client list' to inspect supported surfaces",
                context.client_id));
            return 2;
        }
    };
    prefer_local_http_when_supported(&parsed, &mut context, client_target, "serve-default");

    let server_records = match server::load_server_records(&root_path) {
        Ok(records) => records,
        Err(error) => {
            diagnostics::stderr_line(stderr, format_args!("{}", error));
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
            diagnostics::stderr_line(stderr, format_args!("{}", error));
            return 1;
        }
    };

    let options = ClientInstallRunOptions::from_args(&parsed);
    let result = match install.write_with_options(&options) {
        Ok(value) => value,
        Err(error) => {
            diagnostics::stderr_line(stderr, format_args!("{}", error));
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
        diagnostics::stderr_line(
            stderr,
            format_args!("mcpace root not found; expected mcpace.config.json"),
        );
        return 1;
    };
    let Some(client_id) = parsed.client_id.as_deref() else {
        diagnostics::stderr_line(stderr, format_args!("client restore requires a client target id, for example: mcpace advanced client restore codex"));
        return 2;
    };

    let selector = parsed.backup.as_deref().unwrap_or("latest");
    if client_id.eq_ignore_ascii_case("all") {
        return run_restore_all(&root_path, selector, parsed.json_output, stdout, stderr);
    }
    let result = match restore_client_install_backup(&root_path, client_id, selector) {
        Ok(value) => value,
        Err(error) => {
            diagnostics::stderr_line(stderr, format_args!("{}", error));
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
        diagnostics::stderr_line(stderr, format_args!("client restore all supports only --backup latest because backup ids are per-client"));
        return 2;
    }

    let backup_root = install_backup_root(root_path);
    let entries = match fs::read_dir(&backup_root) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            diagnostics::stderr_line(
                stderr,
                format_args!(
                    "no client install backups found under '{}'",
                    backup_root.display()
                ),
            );
            return 1;
        }
        Err(error) => {
            diagnostics::stderr_line(
                stderr,
                format_args!(
                    "failed to read client install backups '{}': {}",
                    backup_root.display(),
                    error
                ),
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
            Err(error) => failed.push((client_id, error.to_string())),
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
            diagnostics::stderr_line(stderr, format_args!("{}: {}", client_id, error));
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
            diagnostics::stderr_line(stderr, format_args!("{}", error));
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
                failed.push((target.id.clone(), error.to_string()));
                continue;
            }
        };
        let mut context = resolve_context(&target_args, &metadata);
        if context.project_root.is_none() {
            context.project_root = Some(sanitize_path_for_display(
                &runtimepaths::canonicalize_or_original(&root_path),
            ));
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
                failed.push((target.id.clone(), error.to_string()));
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
            let _ = writeln!(stdout, "Skipped: {}", join_semicolon_or_none(&skipped));
        }
        for (target, error) in &failed {
            diagnostics::stderr_line(stderr, format_args!("{}: {}", target, error));
        }
    }

    if failed.is_empty() {
        0
    } else {
        1
    }
}

pub(super) fn read_client_key_name(root_path: &Path) -> Option<String> {
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

enum ClientInstallConfig {
    TomlManagedTable,
    JsonMcpServers {
        servers_object_key: String,
        server_config: JsonValue,
    },
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

        let config_path = resolve_install_path(
            platform_install_config_path(target)
                .unwrap_or(install_support.preferred_config_path.as_str()),
        )?;
        let config_scope = install_support.preferred_scope.clone();
        let server_url = local_mcp_url(root_path);
        let config = install_config_for_target(target, &server_url)?;

        Ok(Self {
            client_target_id: target.id.clone(),
            display_name: target.display_name.clone(),
            adapter_key_name,
            config_path,
            backup_root: install_backup_root(root_path),
            config_scope,
            server_url,
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
        let _config_lock = if options.dry_run {
            None
        } else {
            Some(runtimepaths::acquire_exclusive_file_lock(
                &self.config_path,
                "client config install",
            )?)
        };
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
                    build_toml_mcp_server_block(&self.adapter_key_name, &self.server_url, newline);
                apply_toml_mcp_server_block(
                    &existing,
                    &self.adapter_key_name,
                    &managed_block,
                    &self.config_path,
                )
                .map_err(|error| error.to_string())?
            }
            ClientInstallConfig::JsonMcpServers {
                servers_object_key,
                server_config,
            } => apply_json_mcp_server_entry(
                &existing,
                &self.adapter_key_name,
                servers_object_key,
                server_config.clone(),
                &self.config_path,
            )
            .map_err(|error| error.to_string())?,
            ClientInstallConfig::YamlMcpServers => apply_yaml_mcp_server_entry(
                &existing,
                &self.adapter_key_name,
                &self.server_url,
                &self.config_path,
            )
            .map_err(|error| error.to_string())?,
        };

        let would_change = update.contents != existing;
        let changed = would_change && !options.dry_run;
        let mut warnings = self.warnings.clone();
        if matches!(self.config, ClientInstallConfig::TomlManagedTable) {
            warnings.extend(detect_missing_stdio_command_warnings(
                &existing,
                &self.adapter_key_name,
                &self.config_path,
            ));
        }
        let backup = if changed {
            Some(self.create_backup(&existing, config_file_existed)?)
        } else {
            None
        };
        if changed {
            runtimepaths::write_text_atomic(&self.config_path, &update.contents)?;
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
            warnings,
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
            runtimepaths::write_text_atomic(&content_path, existing)?;
        }

        let manifest_path = backup_path.join("manifest.json");
        let restore_command = format!(
            "mcpace advanced client restore {} --backup {} --root <mcpace-root>",
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
        runtimepaths::write_text_atomic(&manifest_path, &manifest.to_pretty_string())?;

        Ok(ClientInstallBackup {
            id,
            path: backup_path,
            manifest_path,
            restore_command,
        })
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

pub(super) fn platform_install_config_path(target: &ClientTarget) -> Option<&str> {
    let preferred = target.preferred_install_config_path();
    let platform_marker = if cfg!(windows) {
        Some("~/appdata/")
    } else if cfg!(target_os = "macos") {
        Some("~/library/application support/")
    } else {
        Some("~/.config/")
    };
    let platform_path = platform_marker.and_then(|marker| {
        target.config_paths.iter().find_map(|path| {
            path.trim()
                .to_ascii_lowercase()
                .starts_with(marker)
                .then_some(path.as_str())
        })
    });
    platform_path.or(preferred)
}

fn preferred_install_config_path(target: &ClientTarget) -> &str {
    platform_install_config_path(target).unwrap_or("client config")
}

fn preferred_install_scope(target: &ClientTarget) -> &str {
    target.preferred_install_scope().unwrap_or("project")
}

fn build_json_install_server_config(shape: JsonMcpServerShape, server_url: &str) -> JsonValue {
    let mut entries = Vec::new();
    if shape.include_type_http {
        entries.push(("type".to_string(), JsonValue::string("http")));
    }
    entries.push((
        shape.url_field.to_string(),
        JsonValue::string(server_url.to_string()),
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

fn install_config_for_target(
    target: &ClientTarget,
    server_url: &str,
) -> Result<ClientInstallConfig, String> {
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
            servers_object_key: shape.servers_object_key.clone(),
            server_config: build_json_install_server_config(shape, server_url),
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
            url_template: Some(public_mcp_url_or_placeholder(Path::new(&plan.root_path))),
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
            url_template: Some(runtimepaths::configured_mcp_url(Path::new(&plan.root_path))),
            metadata_carrier: "localhost HTTP request metadata plus session headers".to_string(),
            session_model: "connectable localhost HTTP endpoint; upstream wrapper calls derive sticky session affinity from explicit args, metadata, or MCP session headers, while durable cross-process session ownership remains preview".to_string(),
            notes: vec![
                "Use one localhost MCPace URL so the client can target a single MCPace endpoint without loading every upstream tool schema at startup.".to_string(),
                "Request-time upstream session reuse and lease-gated ownership are implemented for explicit wrapper calls; protocol-level durable session lifecycle across MCPace restarts remains preview.".to_string(),
            ],
        },
        _ => AdapterContractPreview {
            kind: "stdio-launcher".to_string(),
            command: Some(plan.launcher_command.clone()),
            args: stdio_launcher_args(&plan.root_path, &target.id),
            url_template: None,
            metadata_carrier:
                "MCP initialize params, roots, cwd, and optional _meta context hints".to_string(),
            session_model: "live stdio launcher contract; lease and session context are derived from initialize metadata and explicit CLI hints".to_string(),
            notes: vec![
                format!(
                    "{} should see one MCPace launcher entry instead of one config block per upstream MCP server.",
                    target.display_name
                ),
                "Use one stable MCPace stdio launcher command; upstream access stays brokered through MCPace instead of one client config block per upstream server.".to_string(),
            ],
        },
    }
}

fn stdio_launcher_args(root_path: &str, client_id: &str) -> Vec<String> {
    vec![
        "stdio".to_string(),
        "--root".to_string(),
        sanitize_launcher_root_path(root_path),
        "--client-id".to_string(),
        client_id.to_string(),
    ]
}

fn sanitize_launcher_root_path(root_path: &str) -> String {
    crate::runtimepaths::strip_windows_extended_path_prefix(root_path)
}

fn sanitize_path_for_display(path: &Path) -> String {
    sanitize_launcher_root_path(&path.display().to_string())
}

fn resolve_install_path(default_config_path: &str) -> Result<PathBuf, String> {
    if !default_config_path.starts_with("~/") && !default_config_path.starts_with("~\\") {
        return Err(format!(
            "client install default config path '{}' is not user-home based yet",
            default_config_path
        ));
    }
    runtimepaths::resolve_user_config_path_expression(default_config_path).ok_or_else(|| {
        format!(
            "client install default config path '{}' is invalid or escapes the user config directory",
            default_config_path
        )
    })
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
        "MCPace upserts only the '{}' entry for {} at {}; existing MCP server entries in that client config are preserved, and a backup is written before changes.",
        plan.configured_client_key_name.as_deref().unwrap_or("MCPace"),
        target.display_name,
        preferred_install_config_path(target)
    ));
    warnings.push(format!(
        "MCPace installs {} into the shared {} scope by default so one localhost MCPace server can be reused across projects.",
        target.display_name,
        preferred_install_scope(target)
    ));
    warnings.sort();
    warnings.dedup();
    warnings
}
