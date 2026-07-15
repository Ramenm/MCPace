use super::model::{CapabilityRecord, LabScenarioRecord};
use crate::json::JsonValue;
use crate::json_helpers;
use std::fmt;
use std::fs;
use std::path::Path;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum LabLoaderError {
    ReadFailed { path: String, reason: String },
    ParseFailed { path: String, kind: &'static str },
}

pub(super) type LabLoaderResult<T> = std::result::Result<T, LabLoaderError>;

impl LabLoaderError {
    fn read_failed(path: &Path, reason: impl Into<String>) -> Self {
        Self::ReadFailed {
            path: path.display().to_string(),
            reason: reason.into(),
        }
    }

    fn parse_failed(path: &Path, kind: &'static str) -> Self {
        Self::ParseFailed {
            path: path.display().to_string(),
            kind,
        }
    }
}

impl fmt::Display for LabLoaderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReadFailed { path, reason } => {
                write!(formatter, "failed to read {}: {}", path, reason)
            }
            Self::ParseFailed { path, kind } => write!(
                formatter,
                "failed to parse lab {} {}: required fields are missing or invalid",
                kind, path
            ),
        }
    }
}

impl std::error::Error for LabLoaderError {}

impl From<String> for LabLoaderError {
    fn from(message: String) -> Self {
        Self::ReadFailed {
            path: "lab input".to_string(),
            reason: message,
        }
    }
}

impl From<LabLoaderError> for String {
    fn from(error: LabLoaderError) -> Self {
        error.to_string()
    }
}

pub(super) fn load_runtime_scenarios(root_path: &Path) -> LabLoaderResult<Vec<LabScenarioRecord>> {
    let fixture_dir = root_path.join("eval").join("fixtures").join("runtime");
    if !fixture_dir.exists() {
        return Ok(Vec::new());
    }

    let mut paths = Vec::new();
    let entries = fs::read_dir(&fixture_dir)
        .map_err(|error| LabLoaderError::read_failed(&fixture_dir, error.to_string()))?;
    for entry in entries {
        let entry =
            entry.map_err(|error| LabLoaderError::read_failed(&fixture_dir, error.to_string()))?;
        let path = entry.path();
        if path
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.eq_ignore_ascii_case("json"))
            .unwrap_or(false)
        {
            paths.push(path);
        }
    }
    paths.sort();

    let mut records = Vec::new();
    for path in paths {
        let json = json_helpers::read_json_file(&path)
            .map_err(|error| LabLoaderError::read_failed(&path, error.to_string()))?;
        let record = normalize_scenario_record(&json)
            .ok_or_else(|| LabLoaderError::parse_failed(&path, "scenario"))?;
        records.push(record);
    }

    records.sort_by(|left, right| {
        left.id
            .to_ascii_lowercase()
            .cmp(&right.id.to_ascii_lowercase())
    });
    Ok(records)
}

pub(super) fn load_runtime_capabilities(
    root_path: &Path,
) -> LabLoaderResult<Vec<CapabilityRecord>> {
    let path = root_path.join("eval").join("runtime-capabilities.json");
    if !path.exists() {
        return Ok(Vec::new());
    }

    let json = json_helpers::read_json_file(&path)
        .map_err(|error| LabLoaderError::read_failed(&path, error.to_string()))?;
    let mut records = Vec::new();
    for entry in json_helpers::array_at_path(&json, &["features"]).unwrap_or(&[]) {
        let record = normalize_capability_record(entry)
            .ok_or_else(|| LabLoaderError::parse_failed(&path, "runtime capability inventory"))?;
        records.push(record);
    }
    records.sort_by(|left, right| {
        left.id
            .to_ascii_lowercase()
            .cmp(&right.id.to_ascii_lowercase())
    });
    Ok(records)
}

fn normalize_scenario_record(value: &JsonValue) -> Option<LabScenarioRecord> {
    Some(LabScenarioRecord {
        id: clean_string(value.get("id").and_then(JsonValue::as_str))?,
        suite: clean_string(value.get("suite").and_then(JsonValue::as_str))?,
        category: clean_string(value.get("category").and_then(JsonValue::as_str))?,
        proof_layer: clean_string(value.get("proofLayer").and_then(JsonValue::as_str))
            .unwrap_or_else(|| "planner".to_string()),
        held_out: value
            .get("heldOut")
            .and_then(JsonValue::as_bool)
            .unwrap_or(false),
        title: clean_string(value.get("title").and_then(JsonValue::as_str))?,
        objective: clean_string(value.get("objective").and_then(JsonValue::as_str))
            .unwrap_or_default(),
        client_archetype: clean_string(json_helpers::string_at_path(
            value,
            &["traffic", "clientArchetype"],
        ))
        .unwrap_or_else(|| "unknown".to_string()),
        server_archetype: clean_string(json_helpers::string_at_path(
            value,
            &["traffic", "serverArchetype"],
        ))
        .unwrap_or_else(|| "unknown-mcp-server".to_string()),
        expected_runtime_type: clean_string(json_helpers::string_at_path(
            value,
            &["expected", "runtimeType"],
        ))
        .unwrap_or_else(|| "unknown".to_string()),
        expected_state_class: clean_string(json_helpers::string_at_path(
            value,
            &["expected", "stateClass"],
        ))
        .unwrap_or_else(|| "unknown-conservative".to_string()),
        expected_effect_class: clean_string(json_helpers::string_at_path(
            value,
            &["expected", "effectClass"],
        ))
        .unwrap_or_else(|| "unknown".to_string()),
        expected_concurrency_policy: clean_string(json_helpers::string_at_path(
            value,
            &["expected", "concurrencyPolicy"],
        ))
        .unwrap_or_else(|| "single-writer".to_string()),
        expected_auto_action: clean_string(json_helpers::string_at_path(
            value,
            &["expected", "autoAction"],
        ))
        .unwrap_or_else(|| "plan-only".to_string()),
        confidence: clean_string(json_helpers::string_at_path(
            value,
            &["expected", "confidence"],
        ))
        .unwrap_or_else(|| "metadata-only".to_string()),
        trust_boundary: clean_string(json_helpers::string_at_path(
            value,
            &["expected", "trustBoundary"],
        ))
        .unwrap_or_else(|| "unknown-code-is-plan-only".to_string()),
        safe_probe_mode: clean_string(json_helpers::string_at_path(
            value,
            &["expected", "safeProbeMode"],
        ))
        .unwrap_or_else(|| "metadata-only-no-exec".to_string()),
        evidence_sources: json_helpers::strings_from_array(
            value.get("evidenceSources").and_then(JsonValue::as_array),
        ),
        metadata_layers: json_helpers::strings_from_array(
            value.get("metadataLayers").and_then(JsonValue::as_array),
        ),
        decision_trace: json_helpers::strings_from_array(
            value.get("decisionTrace").and_then(JsonValue::as_array),
        ),
        server_policies: json_helpers::strings_from_array(json_helpers::array_at_path(
            value,
            &["traffic", "serverPolicies"],
        )),
        signals: json_helpers::strings_from_array(json_helpers::array_at_path(
            value,
            &["traffic", "signals"],
        )),
        checks: json_helpers::strings_from_array(value.get("checks").and_then(JsonValue::as_array)),
        requires: json_helpers::strings_from_array(
            value.get("requires").and_then(JsonValue::as_array),
        ),
    })
}

fn normalize_capability_record(value: &JsonValue) -> Option<CapabilityRecord> {
    Some(CapabilityRecord {
        id: clean_string(value.get("id").and_then(JsonValue::as_str))?,
        area: clean_string(value.get("area").and_then(JsonValue::as_str))?,
        title: clean_string(value.get("title").and_then(JsonValue::as_str))?,
        status: clean_string(value.get("status").and_then(JsonValue::as_str))?,
        priority: clean_string(value.get("priority").and_then(JsonValue::as_str))?
            .to_ascii_lowercase(),
        summary: clean_string(value.get("summary").and_then(JsonValue::as_str)).unwrap_or_default(),
        evidence: json_helpers::strings_from_array(
            value.get("evidence").and_then(JsonValue::as_array),
        ),
        next_step: clean_string(value.get("nextStep").and_then(JsonValue::as_str))
            .unwrap_or_default(),
    })
}

fn clean_string(value: Option<&str>) -> Option<String> {
    value
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}
