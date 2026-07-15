use super::{build_legacy_launcher, ServiceConfig, ServiceConfigError, APP_NAME, LEGACY_APP_NAME};
#[cfg(windows)]
use super::{windows_delete_hklm_run_value, windows_run_value_hkcu, windows_run_value_hklm};

// Legacy autostart support is intentionally quarantined here.
// New installs must use APP_NAME plus the hidden launcher/plan flow; this module only removes
// stale MCPace entries that may still exist on user machines from older releases.

pub(super) struct LegacyCleanup {
    pub(super) ok: bool,
    pub(super) warnings: Vec<String>,
}

pub(super) fn cleanup_legacy_autostart(config: &ServiceConfig, dry_run: bool) -> LegacyCleanup {
    match legacy_autostart_present(config) {
        Ok(false) => LegacyCleanup {
            ok: true,
            warnings: Vec::new(),
        },
        Ok(true) if dry_run => LegacyCleanup {
            ok: true,
            warnings: vec![format!(
                "Legacy '{}' autostart entry is present; dry run did not remove it.",
                LEGACY_APP_NAME
            )],
        },
        Ok(true) => match build_legacy_launcher(config) {
            Ok(legacy_launcher) => match legacy_launcher.disable() {
                Ok(()) => match legacy_autostart_present(config) {
                    Ok(false) => LegacyCleanup {
                        ok: true,
                        warnings: vec![format!(
                            "Removed legacy '{}' autostart entry so only '{}' launches at login.",
                            LEGACY_APP_NAME, APP_NAME
                        )],
                    },
                    Ok(true) => LegacyCleanup {
                        ok: false,
                        warnings: vec![format!(
                            "Legacy '{}' autostart entry is still present after cleanup; an elevated shell may be required to remove a machine-wide Run entry.",
                            LEGACY_APP_NAME
                        )],
                    },
                    Err(error) => LegacyCleanup {
                        ok: false,
                        warnings: vec![format!(
                            "Removed legacy '{}' autostart entry but failed to verify cleanup: {}",
                            LEGACY_APP_NAME, error
                        )],
                    },
                },
                Err(error) => LegacyCleanup {
                    ok: false,
                    warnings: vec![format!(
                        "Failed to remove legacy '{}' autostart entry: {}",
                        LEGACY_APP_NAME, error
                    )],
                },
            },
            Err(error) => LegacyCleanup {
                ok: false,
                warnings: vec![format!(
                    "Failed to build legacy '{}' autostart cleanup handle: {}",
                    LEGACY_APP_NAME, error
                )],
            },
        },
        Err(error) => LegacyCleanup {
            ok: false,
            warnings: vec![format!(
                "Failed to inspect legacy '{}' autostart entry: {}",
                LEGACY_APP_NAME, error
            )],
        },
    }
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
                "legacy '{}' autostart entry is absent; '{}' is the only MCPace login entry",
                LEGACY_APP_NAME, APP_NAME
            ),
        ),
        Ok(true) => (
            false,
            format!(
                "legacy '{}' autostart entry is still present and should be removed",
                LEGACY_APP_NAME
            ),
        ),
        Err(error) => (
            false,
            format!(
                "failed to inspect legacy '{}' autostart entry: {}",
                LEGACY_APP_NAME, error
            ),
        ),
    }
}

#[cfg(windows)]
fn legacy_autostart_present(_config: &ServiceConfig) -> Result<bool, ServiceConfigError> {
    Ok(windows_run_value_hkcu(LEGACY_APP_NAME)?.is_some()
        || windows_run_value_hklm(LEGACY_APP_NAME)?.is_some())
}

#[cfg(not(windows))]
fn legacy_autostart_present(config: &ServiceConfig) -> Result<bool, ServiceConfigError> {
    build_legacy_launcher(config)
        .map_err(|error| ServiceConfigError::Autostart(error.to_string()))?
        .is_enabled()
        .map_err(|error| ServiceConfigError::Autostart(error.to_string()))
}
