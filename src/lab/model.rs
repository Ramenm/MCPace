use crate::client_catalog;
use crate::json::JsonValue;

#[derive(Debug, Clone)]
pub(super) struct LabScenarioRecord {
    pub(super) id: String,
    pub(super) suite: String,
    pub(super) category: String,
    pub(super) proof_layer: String,
    pub(super) held_out: bool,
    pub(super) title: String,
    pub(super) objective: String,
    pub(super) client_archetype: String,
    pub(super) server_archetype: String,
    pub(super) expected_runtime_type: String,
    pub(super) expected_state_class: String,
    pub(super) expected_effect_class: String,
    pub(super) expected_concurrency_policy: String,
    pub(super) expected_auto_action: String,
    pub(super) confidence: String,
    pub(super) trust_boundary: String,
    pub(super) safe_probe_mode: String,
    pub(super) evidence_sources: Vec<String>,
    pub(super) metadata_layers: Vec<String>,
    pub(super) decision_trace: Vec<String>,
    pub(super) server_policies: Vec<String>,
    pub(super) signals: Vec<String>,
    pub(super) checks: Vec<String>,
    pub(super) requires: Vec<String>,
}

#[derive(Debug, Clone)]
pub(super) struct CapabilityRecord {
    pub(super) id: String,
    pub(super) area: String,
    pub(super) title: String,
    pub(super) status: String,
    pub(super) priority: String,
    pub(super) summary: String,
    pub(super) evidence: Vec<String>,
    pub(super) next_step: String,
}

#[derive(Debug, Clone)]
pub(super) struct ScenarioAssessment {
    pub(super) record: LabScenarioRecord,
    pub(super) readiness: String,
    pub(super) satisfied: Vec<String>,
    pub(super) outstanding: Vec<String>,
}

#[derive(Debug, Clone)]
pub(super) struct CapabilityGap {
    pub(super) capability_id: String,
    pub(super) area: String,
    pub(super) title: String,
    pub(super) status: String,
    pub(super) priority: String,
    pub(super) summary: String,
    pub(super) next_step: String,
    pub(super) evidence: Vec<String>,
    pub(super) impacted_scenarios: Vec<String>,
    pub(super) impacted_client_archetypes: Vec<String>,
    pub(super) impacted_surface_classes: Vec<String>,
    pub(super) impacted_surface_kinds: Vec<String>,
}

impl LabScenarioRecord {
    pub(super) fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("id", JsonValue::string(self.id.clone())),
            ("suite", JsonValue::string(self.suite.clone())),
            ("category", JsonValue::string(self.category.clone())),
            ("proofLayer", JsonValue::string(self.proof_layer.clone())),
            ("heldOut", JsonValue::bool(self.held_out)),
            ("title", JsonValue::string(self.title.clone())),
            ("objective", JsonValue::string(self.objective.clone())),
            (
                "traffic",
                JsonValue::object([
                    (
                        "clientArchetype",
                        JsonValue::string(self.client_archetype.clone()),
                    ),
                    (
                        "serverArchetype",
                        JsonValue::string(self.server_archetype.clone()),
                    ),
                    (
                        "serverPolicies",
                        JsonValue::array(
                            self.server_policies.iter().cloned().map(JsonValue::string),
                        ),
                    ),
                    (
                        "signals",
                        JsonValue::array(self.signals.iter().cloned().map(JsonValue::string)),
                    ),
                ]),
            ),
            (
                "expected",
                JsonValue::object([
                    (
                        "runtimeType",
                        JsonValue::string(self.expected_runtime_type.clone()),
                    ),
                    (
                        "stateClass",
                        JsonValue::string(self.expected_state_class.clone()),
                    ),
                    (
                        "effectClass",
                        JsonValue::string(self.expected_effect_class.clone()),
                    ),
                    (
                        "concurrencyPolicy",
                        JsonValue::string(self.expected_concurrency_policy.clone()),
                    ),
                    (
                        "autoAction",
                        JsonValue::string(self.expected_auto_action.clone()),
                    ),
                    ("confidence", JsonValue::string(self.confidence.clone())),
                    (
                        "trustBoundary",
                        JsonValue::string(self.trust_boundary.clone()),
                    ),
                    (
                        "safeProbeMode",
                        JsonValue::string(self.safe_probe_mode.clone()),
                    ),
                ]),
            ),
            (
                "evidenceSources",
                JsonValue::array(self.evidence_sources.iter().cloned().map(JsonValue::string)),
            ),
            (
                "metadataLayers",
                JsonValue::array(self.metadata_layers.iter().cloned().map(JsonValue::string)),
            ),
            (
                "decisionTrace",
                JsonValue::array(self.decision_trace.iter().cloned().map(JsonValue::string)),
            ),
            (
                "checks",
                JsonValue::array(self.checks.iter().cloned().map(JsonValue::string)),
            ),
            (
                "requires",
                JsonValue::array(self.requires.iter().cloned().map(JsonValue::string)),
            ),
        ])
    }
}

impl ScenarioAssessment {
    pub(super) fn to_json_value(&self) -> JsonValue {
        let client_target = client_catalog::find(&self.record.client_archetype);
        JsonValue::object([
            ("scenario", self.record.to_json_value()),
            (
                "clientTarget",
                match client_target {
                    Some(target) => target.to_json_value(),
                    None => JsonValue::Null,
                },
            ),
            ("readiness", JsonValue::string(self.readiness.clone())),
            (
                "satisfied",
                JsonValue::array(self.satisfied.iter().cloned().map(JsonValue::string)),
            ),
            (
                "outstanding",
                JsonValue::array(self.outstanding.iter().cloned().map(JsonValue::string)),
            ),
        ])
    }
}

impl CapabilityGap {
    pub(super) fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            (
                "capabilityId",
                JsonValue::string(self.capability_id.clone()),
            ),
            ("area", JsonValue::string(self.area.clone())),
            ("title", JsonValue::string(self.title.clone())),
            ("status", JsonValue::string(self.status.clone())),
            ("priority", JsonValue::string(self.priority.clone())),
            ("summary", JsonValue::string(self.summary.clone())),
            ("nextStep", JsonValue::string(self.next_step.clone())),
            (
                "evidence",
                JsonValue::array(self.evidence.iter().cloned().map(JsonValue::string)),
            ),
            (
                "impactedScenarios",
                JsonValue::array(
                    self.impacted_scenarios
                        .iter()
                        .cloned()
                        .map(JsonValue::string),
                ),
            ),
            (
                "impactedClientArchetypes",
                JsonValue::array(
                    self.impacted_client_archetypes
                        .iter()
                        .cloned()
                        .map(JsonValue::string),
                ),
            ),
            (
                "impactedSurfaceClasses",
                JsonValue::array(
                    self.impacted_surface_classes
                        .iter()
                        .cloned()
                        .map(JsonValue::string),
                ),
            ),
            (
                "impactedSurfaceKinds",
                JsonValue::array(
                    self.impacted_surface_kinds
                        .iter()
                        .cloned()
                        .map(JsonValue::string),
                ),
            ),
        ])
    }
}
