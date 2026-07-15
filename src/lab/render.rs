use super::analysis::{build_gap_list, build_next_steps, capability_index, readiness_counts};
use super::model::{CapabilityGap, CapabilityRecord, ScenarioAssessment};
use crate::client_catalog;
use crate::diagnostics;
use crate::json::JsonValue;
use crate::text_utils::join_or_none;
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
            "- {} [{} / {} / {}] {} -> {} / {} / {}",
            assessment.record.id,
            assessment.record.suite,
            assessment.record.category,
            assessment.readiness,
            assessment.record.title,
            assessment.record.expected_runtime_type,
            assessment.record.expected_state_class,
            assessment.record.expected_concurrency_policy
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
    let mut server_archetype_counts = BTreeMap::<String, usize>::new();
    let mut runtime_type_counts = BTreeMap::<String, usize>::new();
    let mut state_class_counts = BTreeMap::<String, usize>::new();
    let mut effect_class_counts = BTreeMap::<String, usize>::new();
    let mut auto_action_counts = BTreeMap::<String, usize>::new();
    let mut signal_counts = BTreeMap::<String, usize>::new();
    let mut policy_counts = BTreeMap::<String, usize>::new();
    let mut check_counts = BTreeMap::<String, usize>::new();
    let mut requirement_counts = BTreeMap::<String, usize>::new();
    let mut metadata_layer_counts = BTreeMap::<String, usize>::new();
    let mut confidence_counts = BTreeMap::<String, usize>::new();
    let mut trust_boundary_counts = BTreeMap::<String, usize>::new();
    let mut safe_probe_mode_counts = BTreeMap::<String, usize>::new();

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
        *server_archetype_counts
            .entry(assessment.record.server_archetype.clone())
            .or_default() += 1;
        *runtime_type_counts
            .entry(assessment.record.expected_runtime_type.clone())
            .or_default() += 1;
        *state_class_counts
            .entry(assessment.record.expected_state_class.clone())
            .or_default() += 1;
        *effect_class_counts
            .entry(assessment.record.expected_effect_class.clone())
            .or_default() += 1;
        *auto_action_counts
            .entry(assessment.record.expected_auto_action.clone())
            .or_default() += 1;
        *confidence_counts
            .entry(assessment.record.confidence.clone())
            .or_default() += 1;
        *trust_boundary_counts
            .entry(assessment.record.trust_boundary.clone())
            .or_default() += 1;
        *safe_probe_mode_counts
            .entry(assessment.record.safe_probe_mode.clone())
            .or_default() += 1;
        for value in &assessment.record.metadata_layers {
            *metadata_layer_counts.entry(value.clone()).or_default() += 1;
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
                    "serverArchetypes",
                    JsonValue::object(
                        server_archetype_counts
                            .into_iter()
                            .map(|(key, value)| (key, JsonValue::number(value))),
                    ),
                ),
                (
                    "expectedRuntimeTypes",
                    JsonValue::object(
                        runtime_type_counts
                            .into_iter()
                            .map(|(key, value)| (key, JsonValue::number(value))),
                    ),
                ),
                (
                    "expectedStateClasses",
                    JsonValue::object(
                        state_class_counts
                            .into_iter()
                            .map(|(key, value)| (key, JsonValue::number(value))),
                    ),
                ),
                (
                    "expectedEffectClasses",
                    JsonValue::object(
                        effect_class_counts
                            .into_iter()
                            .map(|(key, value)| (key, JsonValue::number(value))),
                    ),
                ),
                (
                    "expectedAutoActions",
                    JsonValue::object(
                        auto_action_counts
                            .into_iter()
                            .map(|(key, value)| (key, JsonValue::number(value))),
                    ),
                ),
                (
                    "confidence",
                    JsonValue::object(
                        confidence_counts
                            .into_iter()
                            .map(|(key, value)| (key, JsonValue::number(value))),
                    ),
                ),
                (
                    "trustBoundaries",
                    JsonValue::object(
                        trust_boundary_counts
                            .into_iter()
                            .map(|(key, value)| (key, JsonValue::number(value))),
                    ),
                ),
                (
                    "safeProbeModes",
                    JsonValue::object(
                        safe_probe_mode_counts
                            .into_iter()
                            .map(|(key, value)| (key, JsonValue::number(value))),
                    ),
                ),
                (
                    "metadataLayers",
                    JsonValue::object(
                        metadata_layer_counts
                            .into_iter()
                            .map(|(key, value)| (key, JsonValue::number(value))),
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
    let _ = writeln!(stdout, "  server archetypes:");
    for (name, count) in server_archetype_counts {
        let _ = writeln!(stdout, "    - {}: {}", name, count);
    }
    let _ = writeln!(stdout, "  expected runtime types:");
    for (name, count) in runtime_type_counts {
        let _ = writeln!(stdout, "    - {}: {}", name, count);
    }
    let _ = writeln!(stdout, "  expected state classes:");
    for (name, count) in state_class_counts {
        let _ = writeln!(stdout, "    - {}: {}", name, count);
    }
    let _ = writeln!(stdout, "  expected effect classes:");
    for (name, count) in effect_class_counts {
        let _ = writeln!(stdout, "    - {}: {}", name, count);
    }
    let _ = writeln!(stdout, "  expected auto actions:");
    for (name, count) in auto_action_counts {
        let _ = writeln!(stdout, "    - {}: {}", name, count);
    }
    let _ = writeln!(stdout, "  evidence confidence:");
    for (name, count) in confidence_counts {
        let _ = writeln!(stdout, "    - {}: {}", name, count);
    }
    let _ = writeln!(stdout, "  trust boundaries:");
    for (name, count) in trust_boundary_counts {
        let _ = writeln!(stdout, "    - {}: {}", name, count);
    }
    let _ = writeln!(stdout, "  safe probe modes:");
    for (name, count) in safe_probe_mode_counts {
        let _ = writeln!(stdout, "    - {}: {}", name, count);
    }
    let _ = writeln!(stdout, "  metadata layers:");
    for (name, count) in metadata_layer_counts {
        let _ = writeln!(stdout, "    - {}: {}", name, count);
    }
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
    let _ = writeln!(
        stdout,
        "  proof: server -> evidence -> runtimeType/stateClass/effectClass -> concurrencyPolicy"
    );
    for (key, value) in &readiness_counts {
        let _ = writeln!(stdout, "  {}: {}", key, value);
    }

    let _ = writeln!(stdout);
    let _ = writeln!(stdout, "Evidence matrix sample:");
    for assessment in assessments.iter().take(12) {
        let _ = writeln!(
            stdout,
            "  - {} => {} / {} / {} / {} ({}, confidence={}, probe={})",
            assessment.record.server_archetype,
            assessment.record.expected_runtime_type,
            assessment.record.expected_state_class,
            assessment.record.expected_effect_class,
            assessment.record.expected_concurrency_policy,
            assessment.record.expected_auto_action,
            assessment.record.confidence,
            assessment.record.safe_probe_mode
        );
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
        diagnostics::stderr_line(stderr, format_args!("lab show requires --id <scenario>"));
        return 2;
    };

    let Some(assessment) = assessments
        .iter()
        .find(|assessment| assessment.record.id.eq_ignore_ascii_case(id_filter.trim()))
    else {
        diagnostics::stderr_line(
            stderr,
            format_args!("lab scenario not found: {}", id_filter),
        );
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
    let _ = writeln!(
        stdout,
        "Server archetype: {}",
        assessment.record.server_archetype
    );
    let _ = writeln!(
        stdout,
        "Expected: runtimeType={} stateClass={} effectClass={} concurrencyPolicy={} autoAction={} confidence={} trustBoundary={} safeProbeMode={}",
        assessment.record.expected_runtime_type,
        assessment.record.expected_state_class,
        assessment.record.expected_effect_class,
        assessment.record.expected_concurrency_policy,
        assessment.record.expected_auto_action,
        assessment.record.confidence,
        assessment.record.trust_boundary,
        assessment.record.safe_probe_mode
    );
    if let Some(target) = client_catalog::find(&assessment.record.client_archetype) {
        let _ = writeln!(
            stdout,
            "Client surface: family={} class={} kind={} constraints={}",
            target.family_id,
            target.surface_class,
            target.surface_kind,
            join_or_none(target.documented_constraints)
        );
    }
    let _ = writeln!(
        stdout,
        "Evidence sources: {}",
        join_or_none(&assessment.record.evidence_sources)
    );
    let _ = writeln!(
        stdout,
        "Metadata layers: {}",
        join_or_none(&assessment.record.metadata_layers)
    );
    let _ = writeln!(
        stdout,
        "Decision trace: {}",
        join_or_none(&assessment.record.decision_trace)
    );
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

fn blank_to_none(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        "none".to_string()
    } else {
        trimmed.to_string()
    }
}

pub(super) fn render_probe(value: &JsonValue, json_output: bool, stdout: &mut dyn Write) -> i32 {
    if json_output {
        let _ = writeln!(stdout, "{}", value.to_pretty_string());
        return 0;
    }

    let server_count = json_number_text(value, &["serverCount"]).unwrap_or_else(|| "0".to_string());
    let ok_count = json_number_text(value, &["okCount"]).unwrap_or_else(|| "0".to_string());
    let failed_count = json_number_text(value, &["failedCount"]).unwrap_or_else(|| "0".to_string());
    let skipped_count =
        json_number_text(value, &["skippedCount"]).unwrap_or_else(|| "0".to_string());
    let cache_hit_count =
        json_number_text(value, &["cacheHitCount"]).unwrap_or_else(|| "0".to_string());
    let cache_miss_count =
        json_number_text(value, &["cacheMissCount"]).unwrap_or_else(|| "0".to_string());

    let _ = writeln!(stdout, "Live safe probe");
    let _ = writeln!(
        stdout,
        "  method: initialize + notifications/initialized + tools/list only"
    );
    let _ = writeln!(stdout, "  tools/call: not executed");
    let _ = writeln!(
        stdout,
        "  servers: {} ok={} failed={} skipped={}",
        server_count, ok_count, failed_count, skipped_count
    );
    let _ = writeln!(
        stdout,
        "  cache: hit={} miss={}",
        cache_hit_count, cache_miss_count
    );

    if let Some(results) = json_array_at_path(value, &["results"]) {
        for result in results.iter().take(20) {
            let name = json_string_text(result, &["name"]).unwrap_or_else(|| "unknown".to_string());
            let status =
                json_string_text(result, &["status"]).unwrap_or_else(|| "unknown".to_string());
            let tool_count =
                json_number_text(result, &["toolCount"]).unwrap_or_else(|| "?".to_string());
            let ok = json_bool_text(result, &["ok"]).unwrap_or_else(|| "false".to_string());
            let _ = writeln!(
                stdout,
                "  - {}: ok={} status={} tools={}",
                name, ok, status, tool_count
            );
        }
        if results.len() > 20 {
            let _ = writeln!(stdout, "  ... {} more", results.len() - 20);
        }
    }
    0
}

fn json_value_at_path<'a>(value: &'a JsonValue, path: &[&str]) -> Option<&'a JsonValue> {
    let mut current = value;
    for segment in path {
        let JsonValue::Object(object) = current else {
            return None;
        };
        current = object.get(*segment)?;
    }
    Some(current)
}

fn json_string_text(value: &JsonValue, path: &[&str]) -> Option<String> {
    json_value_at_path(value, path)
        .and_then(JsonValue::as_str)
        .map(str::to_string)
}

fn json_number_text(value: &JsonValue, path: &[&str]) -> Option<String> {
    match json_value_at_path(value, path)? {
        JsonValue::Number(value) => Some(value.clone()),
        JsonValue::Null => None,
        _ => None,
    }
}

fn json_bool_text(value: &JsonValue, path: &[&str]) -> Option<String> {
    match json_value_at_path(value, path)? {
        JsonValue::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn json_array_at_path<'a>(value: &'a JsonValue, path: &[&str]) -> Option<&'a [JsonValue]> {
    json_value_at_path(value, path).and_then(JsonValue::as_array)
}
