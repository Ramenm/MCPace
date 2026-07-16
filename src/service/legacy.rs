#[cfg(not(target_os = "linux"))]
use super::build_named_launcher;
#[cfg(windows)]
use super::{windows_delete_hklm_run_value, windows_run_value_hkcu, windows_run_value_hklm};
use super::{ServiceConfig, ServiceConfigError, APP_NAME, LEGACY_APP_NAME};
#[cfg(target_os = "linux")]
use auto_launch::{AutoLaunch, AutoLaunchBuilder, LinuxLaunchMode};
#[cfg(windows)]
use std::path::PathBuf;

// Legacy autostart support is intentionally quarantined here.
// New installs must use APP_NAME plus the hidden launcher/plan flow; this module only removes
// stale MCPace entries that may still exist on user machines from older releases.

pub(super) struct LegacyCleanup {
    pub(super) ok: bool,
    pub(super) warnings: Vec<String>,
}

pub(super) fn cleanup_legacy_autostart(config: &ServiceConfig, dry_run: bool) -> LegacyCleanup {
    let launchers = match legacy_launchers(config) {
        Ok(value) => value,
        Err(error) => {
            return LegacyCleanup {
                ok: false,
                warnings: vec![format!(
                    "Failed to build obsolete autostart cleanup handles: {}",
                    error
                )],
            };
        }
    };
    let mut ok = true;
    let mut warnings = Vec::new();
    for launcher in launchers {
        let name = launcher.get_app_name().to_string();
        match obsolete_launcher_present(&launcher, config) {
            Ok(false) => {}
            Ok(true) if dry_run => warnings.push(format!(
                "Obsolete '{}' autostart entry is present; dry run did not remove it.",
                name
            )),
            Ok(true) => match launcher.disable() {
                Ok(()) => warnings.push(format!(
                    "Removed obsolete '{}' autostart entry so only '{}' owns login startup.",
                    name, APP_NAME
                )),
                Err(error) => {
                    ok = false;
                    warnings.push(format!(
                        "Failed to remove obsolete '{}' autostart entry: {}",
                        name, error
                    ));
                }
            },
            Err(error) => {
                ok = false;
                warnings.push(format!(
                    "Failed to inspect obsolete '{}' autostart entry: {}",
                    name, error
                ));
            }
        }
    }
    let startup_files = cleanup_legacy_startup_files(dry_run);
    ok &= startup_files.ok;
    warnings.extend(startup_files.warnings);
    if ok && !dry_run {
        match legacy_autostart_present(config) {
            Ok(false) => {}
            Ok(true) => {
                ok = false;
                warnings
                    .push("An obsolete MCPace autostart entry remains after cleanup.".to_string());
            }
            Err(error) => {
                ok = false;
                warnings.push(format!(
                    "Obsolete autostart entries were removed but cleanup verification failed: {}",
                    error
                ));
            }
        }
    }
    LegacyCleanup { ok, warnings }
}

pub(super) fn cleanup_machine_wide_autostart(
    config: &ServiceConfig,
    dry_run: bool,
) -> LegacyCleanup {
    cleanup_machine_wide_autostart_impl(config, dry_run)
}

#[cfg(windows)]
fn cleanup_machine_wide_autostart_impl(_config: &ServiceConfig, dry_run: bool) -> LegacyCleanup {
    match windows_run_value_hklm(APP_NAME) {
        Ok(None) => LegacyCleanup {
            ok: true,
            warnings: Vec::new(),
        },
        Ok(Some(_)) if dry_run => LegacyCleanup {
            ok: true,
            warnings: vec![format!(
                "Machine-wide '{}' Run entry is present; dry run did not remove it.",
                APP_NAME
            )],
        },
        Ok(Some(_)) => match windows_delete_hklm_run_value(APP_NAME) {
            Ok(()) => LegacyCleanup {
                ok: true,
                warnings: vec![format!(
                    "Removed machine-wide '{}' Run entry so the current-user hidden launcher is the only login entry.",
                    APP_NAME
                )],
            },
            Err(error) => LegacyCleanup {
                ok: false,
                warnings: vec![format!(
                    "Machine-wide '{}' Run entry is still present and may launch a stale console startup; remove it from an elevated shell: {}",
                    APP_NAME, error
                )],
            },
        },
        Err(error) => LegacyCleanup {
            ok: false,
            warnings: vec![format!(
                "Failed to inspect machine-wide '{}' Run entry: {}",
                APP_NAME, error
            )],
        },
    }
}

#[cfg(not(windows))]
fn cleanup_machine_wide_autostart_impl(_config: &ServiceConfig, _dry_run: bool) -> LegacyCleanup {
    LegacyCleanup {
        ok: true,
        warnings: Vec::new(),
    }
}

pub(super) fn legacy_autostart_absent_detail(config: &ServiceConfig) -> (bool, String) {
    match legacy_autostart_present(config) {
        Ok(false) => (
            true,
            format!(
                "obsolete MCPace autostart entries are absent; '{}' is the only login owner",
                APP_NAME
            ),
        ),
        Ok(true) => (
            false,
            "an obsolete MCPace autostart entry is still present and should be removed".to_string(),
        ),
        Err(error) => (
            false,
            format!(
                "failed to inspect obsolete MCPace autostart entries: {}",
                error
            ),
        ),
    }
}

#[cfg(windows)]
fn obsolete_launcher_present(
    _launcher: &auto_launch::AutoLaunch,
    config: &ServiceConfig,
) -> Result<bool, ServiceConfigError> {
    legacy_autostart_present(config)
}

#[cfg(not(windows))]
fn obsolete_launcher_present(
    launcher: &auto_launch::AutoLaunch,
    _config: &ServiceConfig,
) -> Result<bool, ServiceConfigError> {
    launcher
        .is_enabled()
        .map_err(|error| ServiceConfigError::Autostart(error.to_string()))
}

#[cfg(target_os = "linux")]
fn legacy_launchers(config: &ServiceConfig) -> Result<Vec<AutoLaunch>, ServiceConfigError> {
    [APP_NAME, LEGACY_APP_NAME]
        .into_iter()
        .map(|name| {
            let mut builder = AutoLaunchBuilder::new();
            builder
                .set_app_name(name)
                .set_app_path(&config.app_path)
                .set_args(&config.args)
                .set_linux_launch_mode(LinuxLaunchMode::XdgAutostart);
            builder
                .build()
                .map_err(|error| ServiceConfigError::Autostart(error.to_string()))
        })
        .collect()
}

#[cfg(not(target_os = "linux"))]
fn legacy_launchers(
    config: &ServiceConfig,
) -> Result<Vec<auto_launch::AutoLaunch>, ServiceConfigError> {
    build_named_launcher(LEGACY_APP_NAME, config)
        .map(|launcher| vec![launcher])
        .map_err(|error| ServiceConfigError::Autostart(error.to_string()))
}

#[cfg(windows)]
fn legacy_autostart_present(_config: &ServiceConfig) -> Result<bool, ServiceConfigError> {
    Ok(windows_run_value_hkcu(LEGACY_APP_NAME)?.is_some()
        || windows_run_value_hklm(LEGACY_APP_NAME)?.is_some()
        || legacy_windows_startup_files()
            .iter()
            .any(|path| path.is_file()))
}

#[cfg(not(windows))]
fn legacy_autostart_present(config: &ServiceConfig) -> Result<bool, ServiceConfigError> {
    for launcher in legacy_launchers(config)? {
        if launcher
            .is_enabled()
            .map_err(|error| ServiceConfigError::Autostart(error.to_string()))?
        {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(windows)]
fn cleanup_legacy_startup_files(dry_run: bool) -> LegacyCleanup {
    let mut ok = true;
    let mut warnings = Vec::new();
    for path in legacy_windows_startup_files() {
        if !path.is_file() {
            continue;
        }
        if dry_run {
            warnings.push(format!(
                "Legacy Windows Startup launcher is present; dry run did not remove '{}'.",
                path.display()
            ));
            continue;
        }
        match std::fs::remove_file(&path) {
            Ok(()) => warnings.push(format!(
                "Removed legacy Windows Startup launcher '{}'.",
                path.display()
            )),
            Err(error) => {
                ok = false;
                warnings.push(format!(
                    "Failed to remove legacy Windows Startup launcher '{}': {}",
                    path.display(),
                    error
                ));
            }
        }
    }
    LegacyCleanup { ok, warnings }
}

#[cfg(not(windows))]
fn cleanup_legacy_startup_files(_dry_run: bool) -> LegacyCleanup {
    LegacyCleanup {
        ok: true,
        warnings: Vec::new(),
    }
}

#[cfg(windows)]
fn legacy_windows_startup_files() -> Vec<PathBuf> {
    let roaming = std::env::var_os("APPDATA").map(PathBuf::from).or_else(|| {
        std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .map(|home| home.join("AppData").join("Roaming"))
    });
    let Some(roaming) = roaming else {
        return Vec::new();
    };
    let startup = roaming
        .join("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs")
        .join("Startup");
    [APP_NAME, LEGACY_APP_NAME]
        .into_iter()
        .map(|name| startup.join(format!("{}.cmd", name)))
        .collect()
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn legacy_windows_startup_cleanup_targets_only_owned_names() {
        let names = legacy_windows_startup_files()
            .into_iter()
            .filter_map(|path| {
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["MCPace Agent.cmd", "MCPace.cmd"]);
    }
}
