use super::model::{CapabilityGap, CapabilityRecord, LabScenarioRecord, ScenarioAssessment};
use crate::client_catalog;
use std::collections::BTreeMap;

pub(super) fn assess_scenarios(
    records: &[LabScenarioRecord],
    capabilities: &[CapabilityRecord],
) -> Vec<ScenarioAssessment> {
    let capability_index: BTreeMap<String, CapabilityRecord> = capabilities
        .iter()
        .cloned()
        .map(|record| (record.id.clone(), record))
        .collect();

    records
        .iter()
        .cloned()
        .map(|record| {
            let mut satisfied = Vec::new();
            let mut outstanding = Vec::new();
            for required in &record.requires {
                match capability_index.get(required) {
                    Some(capability) if capability.status.eq_ignore_ascii_case("implemented") => {
                        satisfied.push(required.clone())
                    }
                    Some(_) => outstanding.push(required.clone()),
                    None => outstanding.push(required.clone()),
                }
            }

            let readiness = if outstanding.is_empty() {
                "covered-now"
            } else if satisfied.is_empty() {
                "blocked"
            } else {
                "partial"
            }
            .to_string();

            ScenarioAssessment {
                record,
                readiness,
                satisfied,
                outstanding,
            }
        })
        .collect()
}

pub(super) fn capability_index(
    capabilities: &[CapabilityRecord],
) -> BTreeMap<String, CapabilityRecord> {
    capabilities
        .iter()
        .cloned()
        .map(|record| (record.id.clone(), record))
        .collect()
}

pub(super) fn build_gap_list(
    assessments: &[ScenarioAssessment],
    capabilities: &[CapabilityRecord],
) -> Vec<CapabilityGap> {
    let capability_index = capability_index(capabilities);
    let mut gaps = BTreeMap::<String, CapabilityGap>::new();

    for assessment in assessments {
        for capability_id in &assessment.outstanding {
            let capability = capability_index.get(capability_id);
            let entry = gaps.entry(capability_id.clone()).or_insert_with(|| CapabilityGap {
                capability_id: capability_id.clone(),
                area: capability
                    .map(|record| record.area.clone())
                    .unwrap_or_else(|| "unknown".to_string()),
                title: capability
                    .map(|record| record.title.clone())
                    .unwrap_or_else(|| "Capability not mapped in inventory".to_string()),
                status: capability
                    .map(|record| record.status.clone())
                    .unwrap_or_else(|| "missing".to_string()),
                priority: capability
                    .map(|record| record.priority.clone())
                    .unwrap_or_else(|| "p0".to_string()),
                summary: capability
                    .map(|record| record.summary.clone())
                    .unwrap_or_default(),
                next_step: capability
                    .map(|record| record.next_step.clone())
                    .unwrap_or_else(|| "Add this capability to eval/runtime-capabilities.json and decide whether to implement it or drop the scenario.".to_string()),
                evidence: capability
                    .map(|record| record.evidence.clone())
                    .unwrap_or_default(),
                impacted_scenarios: Vec::new(),
                impacted_client_archetypes: Vec::new(),
                impacted_surface_classes: Vec::new(),
                impacted_surface_kinds: Vec::new(),
            });
            entry.impacted_scenarios.push(assessment.record.id.clone());
            entry
                .impacted_client_archetypes
                .push(assessment.record.client_archetype.clone());
            if let Some(target) = client_catalog::find(&assessment.record.client_archetype) {
                entry
                    .impacted_surface_classes
                    .push(target.surface_class.to_string());
                entry
                    .impacted_surface_kinds
                    .push(target.surface_kind.to_string());
            }
        }
    }

    let mut values: Vec<CapabilityGap> = gaps.into_values().collect();
    for gap in &mut values {
        gap.impacted_scenarios.sort();
        gap.impacted_scenarios.dedup();
        gap.impacted_client_archetypes.sort();
        gap.impacted_client_archetypes.dedup();
        gap.impacted_surface_classes.sort();
        gap.impacted_surface_classes.dedup();
        gap.impacted_surface_kinds.sort();
        gap.impacted_surface_kinds.dedup();
    }
    values.sort_by(compare_gap_priority);
    values
}

fn compare_gap_priority(left: &CapabilityGap, right: &CapabilityGap) -> std::cmp::Ordering {
    right
        .impacted_scenarios
        .len()
        .cmp(&left.impacted_scenarios.len())
        .then_with(|| priority_rank(&left.priority).cmp(&priority_rank(&right.priority)))
        .then_with(|| status_rank(&left.status).cmp(&status_rank(&right.status)))
        .then_with(|| left.area.cmp(&right.area))
        .then_with(|| left.capability_id.cmp(&right.capability_id))
}

pub(super) fn build_next_steps(gaps: &[CapabilityGap]) -> Vec<String> {
    let mut steps = Vec::new();
    for gap in gaps {
        let text = if gap.next_step.trim().is_empty() {
            format!(
                "Close capability gap '{}' before expanding the scenario set.",
                gap.capability_id
            )
        } else {
            gap.next_step.trim().to_string()
        };
        if !steps.iter().any(|value| value == &text) {
            steps.push(text);
        }
    }
    steps.truncate(6);
    steps
}

pub(super) fn readiness_counts(assessments: &[ScenarioAssessment]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::<String, usize>::new();
    for assessment in assessments {
        *counts.entry(assessment.readiness.clone()).or_default() += 1;
    }
    counts
}

fn priority_rank(value: &str) -> usize {
    match value.trim().to_ascii_lowercase().as_str() {
        "p0" => 0,
        "p1" => 1,
        "p2" => 2,
        _ => 9,
    }
}

fn status_rank(value: &str) -> usize {
    match value.trim().to_ascii_lowercase().as_str() {
        "missing" => 0,
        "planned" => 1,
        "partial" => 2,
        "implemented" => 3,
        _ => 9,
    }
}
