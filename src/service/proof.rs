#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
use super::config::{
    start_platform_user_supervisor, stop_runtime_before_supervisor_start, supervisor_endpoint,
    supervisor_endpoint_ready, supervisor_runtime_ready, wait_for_supervisor_runtime,
};
use super::{report, ServiceConfig, ServiceConfigError, ServiceConfigResult};
use crate::json::JsonValue;
use auto_launch::AutoLaunch;

#[derive(Debug, Clone, Copy)]
pub(super) struct SupervisorProof {
    pub(super) dry_run: bool,
    pub(super) initial_runtime_active: bool,
    pub(super) activation_attempted: bool,
    pub(super) endpoint_verified: bool,
    pub(super) supervisor_verified: bool,
    pub(super) restored_initial_state: bool,
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
fn prove_user_supervisor(
    config: &ServiceConfig,
    dry_run: bool,
) -> ServiceConfigResult<SupervisorProof> {
    let endpoint = supervisor_endpoint(config)?;
    let initial_runtime_active = supervisor_endpoint_ready(&endpoint);
    let initial_supervisor_active = supervisor_runtime_ready(&endpoint);
    if dry_run {
        return Ok(SupervisorProof {
            dry_run: true,
            initial_runtime_active,
            activation_attempted: false,
            endpoint_verified: initial_runtime_active,
            supervisor_verified: initial_supervisor_active,
            restored_initial_state: true,
        });
    }

    stop_runtime_before_supervisor_start(config)?;
    let activation = start_platform_user_supervisor(config)
        .and_then(|()| wait_for_supervisor_runtime(&endpoint));
    if let Err(error) = activation {
        let restoration = if initial_runtime_active {
            start_platform_user_supervisor(config)
                .and_then(|()| wait_for_supervisor_runtime(&endpoint))
        } else {
            stop_runtime_before_supervisor_start(config)
        };
        return Err(ServiceConfigError::Autostart(match restoration {
            Ok(()) => format!("autostart activation proof failed: {}", error),
            Err(restore_error) => format!(
                "autostart activation proof failed: {}; restoring the initial runtime state also failed: {}",
                error, restore_error
            ),
        }));
    }

    let endpoint_verified = supervisor_endpoint_ready(&endpoint);
    let supervisor_verified = supervisor_runtime_ready(&endpoint);
    let restored_initial_state = if initial_runtime_active {
        supervisor_verified
    } else {
        stop_runtime_before_supervisor_start(config)?;
        !supervisor_runtime_ready(&endpoint)
    };
    if !endpoint_verified || !supervisor_verified || !restored_initial_state {
        return Err(ServiceConfigError::Autostart(
            "autostart activation proof did not verify the endpoint and restore its initial running state"
                .to_string(),
        ));
    }

    Ok(SupervisorProof {
        dry_run: false,
        initial_runtime_active,
        activation_attempted: true,
        endpoint_verified,
        supervisor_verified,
        restored_initial_state,
    })
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
fn prove_user_supervisor(
    _config: &ServiceConfig,
    dry_run: bool,
) -> ServiceConfigResult<SupervisorProof> {
    Err(ServiceConfigError::Autostart(format!(
        "autostart activation proof is unsupported on this platform (dry-run={})",
        dry_run
    )))
}

pub(super) fn service_prove(
    launcher: &AutoLaunch,
    config: &ServiceConfig,
    dry_run: bool,
) -> JsonValue {
    let enabled = match launcher.is_enabled() {
        Ok(value) => value,
        Err(error) => {
            return report(
                "prove",
                false,
                false,
                config,
                config.warnings.clone(),
                Some(format!(
                    "failed to inspect login-startup registration: {}",
                    error
                )),
            )
        }
    };
    if !enabled {
        return report(
            "prove",
            false,
            false,
            config,
            config.warnings.clone(),
            Some(
                "login startup is disabled; run `mcpace up` before proving activation".to_string(),
            ),
        );
    }

    match prove_user_supervisor(config, dry_run) {
        Ok(proof) => report_with_supervisor_proof(config, enabled, proof),
        Err(error) => report(
            "prove",
            false,
            enabled,
            config,
            config.warnings.clone(),
            Some(error.to_string()),
        ),
    }
}

pub(super) fn report_with_supervisor_proof(
    config: &ServiceConfig,
    enabled: bool,
    proof: SupervisorProof,
) -> JsonValue {
    let mut value = report(
        "prove",
        proof.endpoint_verified || proof.dry_run,
        enabled,
        config,
        config.warnings.clone(),
        None,
    );
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "proof".to_string(),
            JsonValue::object([
                ("schema", JsonValue::string("mcpace.autostartProof.v1")),
                ("dryRun", JsonValue::bool(proof.dry_run)),
                (
                    "initialRuntimeActive",
                    JsonValue::bool(proof.initial_runtime_active),
                ),
                (
                    "activationAttempted",
                    JsonValue::bool(proof.activation_attempted),
                ),
                ("endpointVerified", JsonValue::bool(proof.endpoint_verified)),
                (
                    "supervisorVerified",
                    JsonValue::bool(proof.supervisor_verified),
                ),
                (
                    "restoredInitialState",
                    JsonValue::bool(proof.restored_initial_state),
                ),
            ]),
        );
    }
    value
}
