use super::analysis::{build_gap_list, build_next_steps, capability_index, readiness_counts};
use super::model::{CapabilityGap, CapabilityRecord, ScenarioAssessment};
use crate::client_catalog;
use crate::json::JsonValue;
use std::collections::BTreeMap;
use std::io::Write;

pub(super) fn render_list(
    assessments: &[ScenarioAssessment],
    json_output: bool,
    stdout: &mut dyn Write,
) -> i32 {
    if json_output {
        let _ = writeln!(
            stdout,
            "{}",
            JsonValue::array(assessments.iter().map(ScenarioAssessment::to_json_value))
                .to_pretty_string()
        );
        return 0;
    }

    let _ = writeln!(stdout, "Runtime lab scenarios: {}", assessments.len());
    for assessment in assessments {
        let _ = writeln!(
            stdout,
            "- {} [{} / {} / {}] {}",
            assessment.record.id,
            assessment.record.suite,
            assessment.record.category,
            assessment.readiness,
            assessment.record.title
        );
    }
    0
}

pub(super) fn render_matrix(
    assessments: &[ScenarioAssessment],
    capabilities: &[CapabilityRecord],
    json_output: bool,
    stdout: &mut dyn Write,
) -> i32 {
    let mut category_counts = BTreeMap::<String, usize>::new();
    let mut suite_counts = BTreeMap::<String, usize>::new();
    let mut proof_layer_counts = BTreeMap::<String, usize>::new();
    let mut readiness_counts = BTreeMap::<String, usize>::new();
    let mut held_out_count = 0usize;

    for assessment in assessments {
        *category_counts
            .entry(assessment.record.category.clone())
            .or_default() += 1;
        *suite_counts
            .entry(assessment.record.suite.clone())
            .or_default() += 1;
        *proof_layer_counts
            .entry(assessment.record.proof_layer.clone())
            .or_default() += 1;
        *readiness_counts
            .entry(assessment.readiness.clone())
            .or_default() += 1;
        if assessment.record.held_out {
            held_out_count += 1;
        }
    }

    let implemented_capabilities = capabilities
        .iter()
        .filter(|record| record.status.eq_ignore_ascii_case("implemented"))
        .count();
    let planned_capabilities = capabilities
        .iter()
        .filter(|record| !record.status.eq_ignore_ascii_case("implemented"))
        .count();

    if json_output {
        let _ = writeln!(
            stdout,
            "{}",
            JsonValue::object([
                ("scenarioCount", JsonValue::number(assessments.len())),
                ("heldOutCount", JsonValue::number(held_out_count)),
                (
                    "capabilityInventory",
                    JsonValue::object([
                        ("featureCount", JsonValue::number(capabilities.len())),
                        ("implemented", JsonValue::number(implemented_capabilities)),
                        ("plannedOrMissing", JsonValue::number(planned_capabilities)),
                    ]),
                ),
                (
                    "categoryCounts",
                    JsonValue::object(
                        category_counts
                            .into_iter()
                            .map(|(key, value)| (key, JsonValue::number(value))),
                    ),
                ),
                (
                    "suiteCounts",
                    JsonValue::object(
                        suite_counts
                            .into_iter()
                            .map(|(key, value)| (key, JsonValue::number(value))),
                    ),
                ),
                (
                    "proofLayerCounts",
                    JsonValue::object(
                        proof_layer_counts
                            .into_iter()
                            .map(|(key, value)| (key, JsonValue::number(value))),
                    ),
                ),
                (
                    "readinessCounts",
                    JsonValue::object(
                        readiness_counts
                            .into_iter()
                            .map(|(key, value)| (key, JsonValue::number(value))),
                    ),
                ),
            ])
            .to_pretty_string()
        );
        return 0;
    }

    let _ = writeln!(stdout, "Runtime lab matrix");
    let _ = writeln!(stdout, "  scenarios: {}", assessments.len());
    let _ = writeln!(stdout, "  held-out: {}", held_out_count);
    let _ = writeln!(
        stdout,
        "  capability inventory: {} total, {} implemented, {} planned/missing",
        capabilities.len(),
        implemented_capabilities,
        planned_capabilities
    );
    let _ = writeln!(stdout, "  by readiness:");
    for (readiness, count) in readiness_counts {
        let _ = writeln!(stdout, "    - {}: {}", readiness, count);
    }
    let _ = writeln!(stdout, "  by proof layer:");
    for (layer, count) in proof_layer_counts {
        let _ = writeln!(stdout, "    - {}: {}", layer, count);
    }
    let _ = writeln!(stdout, "  by category:");
    for (category, count) in category_counts {
        let _ = writeln!(stdout, "    - {}: {}", category, count);
    }
    let _ = writeln!(stdout, "  by suite:");
    for (suite, count) in suite_counts {
        let _ = writeln!(stdout, "    - {}: {}", suite, count);
    }
    0
}

pub(super) fn render_coverage(
    assessments: &[ScenarioAssessment],
    json_output: bool,
    stdout: &mut dyn Write,
) -> i32 {
    let mut client_counts = BTreeMap::<String, usize>::new();
    let mut family_counts = BTreeMap::<String, usize>::new();
    let mut surface_class_counts = BTreeMap::<String, usize>::new();
    let mut surface_kind_counts = BTreeMap::<String, usize>::new();
    let mut constraint_counts = BTreeMap::<String, usize>::new();
    let mut unknown_client_archetypes = Vec::<String>::new();
    let mut signal_counts = BTreeMap::<String, usize>::new();
    let mut policy_counts = BTreeMap::<String, usize>::new();
    let mut check_counts = BTreeMap::<String, usize>::new();
    let mut requirement_counts = BTreeMap::<String, usize>::new();

    for assessment in assessments {
        *client_counts
            .entry(assessment.record.client_archetype.clone())
            .or_default() += 1;
        if let Some(target) = client_catalog::find(&assessment.record.client_archetype) {
            *family_counts
                .entry(target.family_id.to_string())
                .or_default() += 1;
            *surface_class_counts
                .entry(target.surface_class.to_string())
                .or_default() += 1;
            *surface_kind_counts
                .entry(target.surface_kind.to_string())
                .or_default() += 1;
            for value in target.documented_constraints {
                *constraint_counts.entry((*value).to_string()).or_default() += 1;
            }
        } else if !unknown_client_archetypes
            .iter()
            .any(|value| value.eq_ignore_ascii_case(&assessment.record.client_archetype))
        {
            unknown_client_archetypes.push(assessment.record.client_archetype.clone());
        }
        for value in &assessment.record.signals {
            *signal_counts.entry(value.clone()).or_default() += 1;
        }
        for value in &assessment.record.server_policies {
            *policy_counts.entry(value.clone()).or_default() += 1;
        }
        for value in &assessment.record.checks {
            *check_counts.entry(value.clone()).or_default() += 1;
        }
        for value in &assessment.record.requires {
            *requirement_counts.entry(value.clone()).or_default() += 1;
        }
    }

    unknown_client_archetypes.sort();

    if json_output {
        let _ = writeln!(
            stdout,
            "{}",
            JsonValue::object([
                (
                    "clientArchetypes",
                    JsonValue::object(
                        client_counts
                            .into_iter()
                            .map(|(key, value)| (key, JsonValue::number(value))),
                    ),
                ),
                (
                    "clientFamilies",
                    JsonValue::object(
                        family_counts
                            .into_iter()
                            .map(|(key, value)| (key, JsonValue::number(value))),
                    ),
                ),
                (
                    "surfaceClasses",
                    JsonValue::object(
                        surface_class_counts
                            .into_iter()
                            .map(|(key, value)| (key, JsonValue::number(value))),
                    ),
                ),
                (
                    "surfaceKinds",
                    JsonValue::object(
                        surface_kind_counts
                            .into_iter()
                            .map(|(key, value)| (key, JsonValue::number(value))),
                    ),
                ),
                (
                    "documentedConstraints",
                    JsonValue::object(
                        constraint_counts
                            .into_iter()
                            .map(|(key, value)| (key, JsonValue::number(value))),
                    ),
                ),
                (
                    "unknownClientArchetypes",
                    JsonValue::array(
                        unknown_client_archetypes
                            .iter()
                            .cloned()
                            .map(JsonValue::string)
                    ),
                ),
                (
                    "signals",
                    JsonValue::object(
                        signal_counts
                            .into_iter()
                            .map(|(key, value)| (key, JsonValue::number(value))),
                    ),
                ),
                (
                    "serverPolicies",
                    JsonValue::object(
                        policy_counts
                            .into_iter()
                            .map(|(key, value)| (key, JsonValue::number(value))),
                    ),
                ),
                (
                    "checks",
                    JsonValue::object(
                        check_counts
                            .into_iter()
                            .map(|(key, value)| (key, JsonValue::number(value))),
                    ),
                ),
                (
                    "requirements",
                    JsonValue::object(
                        requirement_counts
                            .into_iter()
                            .map(|(key, value)| (key, JsonValue::number(value))),
                    ),
                ),
            ])
            .to_pretty_string()
        );
        return 0;
    }

    let _ = writeln!(stdout, "Runtime lab coverage");
    let _ = writeln!(stdout, "  client archetypes:");
    for (name, count) in client_counts {
        let _ = writeln!(stdout, "    - {}: {}", name, count);
    }
    let _ = writeln!(stdout, "  client families:");
    for (name, count) in family_counts {
        let _ = writeln!(stdout, "    - {}: {}", name, count);
    }
    let _ = writeln!(stdout, "  surface classes:");
    for (name, count) in surface_class_counts {
        let _ = writeln!(stdout, "    - {}: {}", name, count);
    }
    let _ = writeln!(stdout, "  surface kinds:");
    for (name, count) in surface_kind_counts {
        let _ = writeln!(stdout, "    - {}: {}", name, count);
    }
    let _ = writeln!(stdout, "  documented constraints seen in fixtures:");
    for (name, count) in constraint_counts {
        let _ = writeln!(stdout, "    - {}: {}", name, count);
    }
    let _ = writeln!(
        stdout,
        "  unknown client archetypes: {}",
        join_or_none(&unknown_client_archetypes)
    );
    let _ = writeln!(stdout, "  signals:");
    for (name, count) in signal_counts {
        let _ = writeln!(stdout, "    - {}: {}", name, count);
    }
    let _ = writeln!(stdout, "  server policies:");
    for (name, count) in policy_counts {
        let _ = writeln!(stdout, "    - {}: {}", name, count);
    }
    let _ = writeln!(stdout, "  checks:");
    for (name, count) in check_counts {
        let _ = writeln!(stdout, "    - {}: {}", name, count);
    }
    let _ = writeln!(stdout, "  required capabilities:");
    for (name, count) in requirement_counts {
        let _ = writeln!(stdout, "    - {}: {}", name, count);
    }
    0
}

pub(super) fn render_gaps(
    assessments: &[ScenarioAssessment],
    capabilities: &[CapabilityRecord],
    json_output: bool,
    stdout: &mut dyn Write,
) -> i32 {
    let gaps = build_gap_list(assessments, capabilities);

    if json_output {
        let _ = writeln!(
            stdout,
            "{}",
            JsonValue::array(gaps.iter().map(CapabilityGap::to_json_value)).to_pretty_string()
        );
        return 0;
    }

    let _ = writeln!(stdout, "Runtime lab gaps");
    if gaps.is_empty() {
        let _ = writeln!(stdout, "  no gaps detected by the current fixture set");
        return 0;
    }

    for gap in &gaps {
        let _ = writeln!(
            stdout,
            "- {} [{} / {} / {}] impacts {} scenario(s)",
            gap.capability_id,
            gap.area,
            gap.status,
            gap.priority,
            gap.impacted_scenarios.len()
        );
        let _ = writeln!(stdout, "    title={}", gap.title);
        let _ = writeln!(stdout, "    summary={}", blank_to_none(&gap.summary));
        let _ = writeln!(stdout, "    nextStep={}", blank_to_none(&gap.next_step));
        let _ = writeln!(
            stdout,
            "    impacted={}",
            join_or_none(&gap.impacted_scenarios)
        );
        let _ = writeln!(
            stdout,
            "    clientArchetypes={}",
            join_or_none(&gap.impacted_client_archetypes)
        );
        let _ = writeln!(
            stdout,
            "    surfaceClasses={}",
            join_or_none(&gap.impacted_surface_classes)
        );
    }
    0
}

pub(super) fn render_report(
    assessments: &[ScenarioAssessment],
    capabilities: &[CapabilityRecord],
    json_output: bool,
    stdout: &mut dyn Write,
) -> i32 {
    let gaps = build_gap_list(assessments, capabilities);
    let next_steps = build_next_steps(&gaps);
    let readiness_counts = readiness_counts(assessments);

    if json_output {
        let _ = writeln!(
            stdout,
            "{}",
            JsonValue::object([
                (
                    "matrix",
                    JsonValue::object([
                        ("scenarioCount", JsonValue::number(assessments.len())),
                        (
                            "readinessCounts",
                            JsonValue::object(
                                readiness_counts
                                    .clone()
                                    .into_iter()
                                    .map(|(key, value)| (key, JsonValue::number(value))),
                            ),
                        ),
                    ]),
                ),
                (
                    "scenarios",
                    JsonValue::array(assessments.iter().map(ScenarioAssessment::to_json_value)),
                ),
                (
                    "gaps",
                    JsonValue::array(gaps.iter().map(CapabilityGap::to_json_value)),
                ),
                (
                    "nextSteps",
                    JsonValue::array(next_steps.iter().cloned().map(JsonValue::string)),
                ),
            ])
            .to_pretty_string()
        );
        return 0;
    }

    let _ = writeln!(stdout, "Runtime lab report");
    let _ = writeln!(stdout, "  scenarios: {}", assessments.len());
    for (key, value) in &readiness_counts {
        let _ = writeln!(stdout, "  {}: {}", key, value);
    }

    let _ = writeln!(stdout);
    let _ = writeln!(stdout, "Priority gaps:");
    if gaps.is_empty() {
        let _ = writeln!(stdout, "  none");
    } else {
        for gap in gaps.iter().take(8) {
            let _ = writeln!(
                stdout,
                "  - {} [{} / {} / {}] -> {} scenario(s)",
                gap.capability_id,
                gap.area,
                gap.status,
                gap.priority,
                gap.impacted_scenarios.len()
            );
            let _ = writeln!(
                stdout,
                "    clientArchetypes={} surfaceClasses={}",
                join_or_none(&gap.impacted_client_archetypes),
                join_or_none(&gap.impacted_surface_classes)
            );
            let _ = writeln!(stdout, "    {}", blank_to_none(&gap.next_step));
        }
    }

    let _ = writeln!(stdout);
    let _ = writeln!(stdout, "Recommended next steps:");
    if next_steps.is_empty() {
        let _ = writeln!(stdout, "  none");
    } else {
        for (index, step) in next_steps.iter().enumerate() {
            let _ = writeln!(stdout, "  {}. {}", index + 1, step);
        }
    }

    let _ = writeln!(stdout);
    let _ = writeln!(stdout, "Scenarios still blocked:");
    for assessment in assessments
        .iter()
        .filter(|assessment| assessment.readiness == "blocked")
    {
        let _ = writeln!(
            stdout,
            "  - {} [{}] outstanding={}",
            assessment.record.id,
            assessment.record.proof_layer,
            join_or_none(&assessment.outstanding)
        );
    }
    0
}

pub(super) fn render_show(
    assessments: &[ScenarioAssessment],
    capabilities: &[CapabilityRecord],
    id_filter: Option<&str>,
    json_output: bool,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let Some(id_filter) = id_filter else {
        let _ = writeln!(stderr, "lab show requires --id <scenario>");
        return 2;
    };

    let Some(assessment) = assessments
        .iter()
        .find(|assessment| assessment.record.id.eq_ignore_ascii_case(id_filter.trim()))
    else {
        let _ = writeln!(stderr, "lab scenario not found: {}", id_filter);
        return 1;
    };

    if json_output {
        let _ = writeln!(stdout, "{}", assessment.to_json_value().to_pretty_string());
        return 0;
    }

    let _ = writeln!(stdout, "Scenario: {}", assessment.record.id);
    let _ = writeln!(stdout, "Suite: {}", assessment.record.suite);
    let _ = writeln!(stdout, "Category: {}", assessment.record.category);
    let _ = writeln!(stdout, "Proof layer: {}", assessment.record.proof_layer);
    let _ = writeln!(
        stdout,
        "Held out: {}",
        if assessment.record.held_out {
            "yes"
        } else {
            "no"
        }
    );
    let _ = writeln!(stdout, "Readiness: {}", assessment.readiness);
    let _ = writeln!(stdout, "Title: {}", assessment.record.title);
    let _ = writeln!(stdout, "Objective: {}", assessment.record.objective);
    let _ = writeln!(
        stdout,
        "Client archetype: {}",
        assessment.record.client_archetype
    );
    if let Some(target) = client_catalog::find(&assessment.record.client_archetype) {
        let _ = writeln!(
            stdout,
            "Client surface: family={} class={} kind={} constraints={}",
            target.family_id,
            target.surface_class,
            target.surface_kind,
            join_static_or_none(target.documented_constraints)
        );
    }
    let _ = writeln!(
        stdout,
        "Signals: {}",
        join_or_none(&assessment.record.signals)
    );
    let _ = writeln!(
        stdout,
        "Server policies: {}",
        join_or_none(&assessment.record.server_policies)
    );
    let _ = writeln!(
        stdout,
        "Checks: {}",
        join_or_none(&assessment.record.checks)
    );
    let _ = writeln!(
        stdout,
        "Requires: {}",
        join_or_none(&assessment.record.requires)
    );
    let _ = writeln!(stdout, "Satisfied: {}", join_or_none(&assessment.satisfied));
    let _ = writeln!(
        stdout,
        "Outstanding: {}",
        join_or_none(&assessment.outstanding)
    );

    if !assessment.outstanding.is_empty() {
        let gap_index = capability_index(capabilities);
        let _ = writeln!(stdout, "Outstanding details:");
        for capability_id in &assessment.outstanding {
            if let Some(record) = gap_index.get(capability_id) {
                let _ = writeln!(
                    stdout,
                    "  - {} [{} / {} / {}] {}",
                    record.id, record.area, record.status, record.priority, record.title
                );
                let _ = writeln!(stdout, "    nextStep={}", blank_to_none(&record.next_step));
            } else {
                let _ = writeln!(stdout, "  - {} [unknown]", capability_id);
            }
        }
    }

    0
}

fn join_or_none(values: &[String]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values.join(", ")
    }
}

fn join_static_or_none(values: &[&str]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values.join(", ")
    }
}

fn blank_to_none(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        "none".to_string()
    } else {
        trimmed.to_string()
    }
}
