use super::model::{CapabilityRecord, LabScenarioRecord};
use crate::json::JsonValue;
use crate::json_helpers;
use std::fs;
use std::path::Path;

pub(super) fn load_runtime_scenarios(root_path: &Path) -> Result<Vec<LabScenarioRecord>, String> {
    let fixture_dir = root_path.join("eval").join("fixtures").join("runtime");
    if !fixture_dir.exists() {
        return Ok(Vec::new());
    }

    let mut paths = Vec::new();
    let entries = fs::read_dir(&fixture_dir)
        .map_err(|error| format!("failed to read {}: {}", fixture_dir.display(), error))?;
    for entry in entries {
        let entry = entry
            .map_err(|error| format!("failed to inspect {}: {}", fixture_dir.display(), error))?;
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
        let json = json_helpers::read_json_file(&path)?;
        let record = normalize_scenario_record(&json).ok_or_else(|| {
            format!(
                "failed to parse lab scenario {}: required fields are missing or invalid",
                path.display()
            )
        })?;
        records.push(record);
    }

    records.sort_by(|left, right| {
        left.id
            .to_ascii_lowercase()
            .cmp(&right.id.to_ascii_lowercase())
    });
    Ok(records)
}

pub(super) fn load_runtime_capabilities(root_path: &Path) -> Result<Vec<CapabilityRecord>, String> {
    let path = root_path.join("eval").join("runtime-capabilities.json");
    if !path.exists() {
        return Ok(Vec::new());
    }

    let json = json_helpers::read_json_file(&path)?;
    let mut records = Vec::new();
    for entry in json_helpers::array_at_path(&json, &["features"]).unwrap_or(&[]) {
        let record = normalize_capability_record(entry).ok_or_else(|| {
            format!(
                "failed to parse runtime capability inventory {}: required fields are missing or invalid",
                path.display()
            )
        })?;
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
