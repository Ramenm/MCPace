use crate::json::JsonValue;
use std::collections::{BTreeMap, VecDeque};
use std::time::Duration;

const DEFAULT_RECENT_OPERATION_TRACE_WINDOW: usize = 512;
const RECENT_OPERATION_TRACE_EXPORT_LIMIT: usize = 25;

#[derive(Clone, Debug)]
pub(super) struct OperationTraceObservation {
    pub name: String,
    pub route: String,
    pub duration: Duration,
    pub failed: bool,
    pub attributes: Vec<(String, String)>,
}

#[derive(Debug)]
pub(super) struct OperationTraceTracker {
    sample_limit: usize,
    recorded_total: u64,
    observations: VecDeque<OperationTraceObservation>,
}

impl Default for OperationTraceTracker {
    fn default() -> Self {
        Self {
            sample_limit: DEFAULT_RECENT_OPERATION_TRACE_WINDOW,
            recorded_total: 0,
            observations: VecDeque::with_capacity(DEFAULT_RECENT_OPERATION_TRACE_WINDOW),
        }
    }
}

impl OperationTraceTracker {
    pub fn record(&mut self, observation: OperationTraceObservation) {
        self.recorded_total = self.recorded_total.saturating_add(1);
        if self.observations.len() >= self.sample_limit {
            self.observations.pop_front();
        }
        self.observations.push_back(observation);
    }

    pub fn snapshot_json(&self) -> JsonValue {
        let samples = self.observations.iter().collect::<Vec<_>>();
        let mut by_name = BTreeMap::<String, Vec<&OperationTraceObservation>>::new();
        let mut by_route = BTreeMap::<String, Vec<&OperationTraceObservation>>::new();

        for observation in &samples {
            by_name
                .entry(observation.name.clone())
                .or_default()
                .push(*observation);
            by_route
                .entry(observation.route.clone())
                .or_default()
                .push(*observation);
        }

        let name_items = grouped_operation_rows(by_name, "name");
        let route_items = grouped_operation_rows(by_route, "route");
        let mut recent = self
            .observations
            .iter()
            .rev()
            .take(RECENT_OPERATION_TRACE_EXPORT_LIMIT)
            .map(operation_observation_json)
            .collect::<Vec<_>>();
        recent.reverse();

        JsonValue::object([
            ("schema", JsonValue::string("mcpace.operationTrace.v1")),
            ("sampleLimit", JsonValue::number(self.sample_limit)),
            ("recordedTotal", JsonValue::number(self.recorded_total)),
            (
                "retainedSamples",
                JsonValue::number(self.observations.len()),
            ),
            (
                "droppedSamples",
                JsonValue::number(
                    self.recorded_total
                        .saturating_sub(self.observations.len() as u64),
                ),
            ),
            ("overall", operation_summary_json(&samples)),
            ("byName", JsonValue::array(name_items)),
            ("byRoute", JsonValue::array(route_items)),
            ("recent", JsonValue::array(recent)),
        ])
    }
}

fn grouped_operation_rows(
    groups: BTreeMap<String, Vec<&OperationTraceObservation>>,
    key_name: &'static str,
) -> Vec<JsonValue> {
    let mut items = Vec::new();
    for (key, observations) in groups {
        let mut summary = operation_summary_json(&observations);
        if let JsonValue::Object(map) = &mut summary {
            map.insert(key_name.to_string(), JsonValue::string(key));
        }
        items.push(summary);
    }
    items
}

fn operation_summary_json(observations: &[&OperationTraceObservation]) -> JsonValue {
    let count = observations.len();
    let failed_count = observations.iter().filter(|item| item.failed).count();

    JsonValue::object([
        ("count", JsonValue::number(count)),
        ("failed", JsonValue::number(failed_count)),
        (
            "durationMs",
            operation_duration_distribution_json(observations, |item| item.duration),
        ),
    ])
}

fn operation_duration_distribution_json<F>(
    observations: &[&OperationTraceObservation],
    pick: F,
) -> JsonValue
where
    F: Fn(&OperationTraceObservation) -> Duration,
{
    let mut values = observations
        .iter()
        .map(|item| pick(item).as_micros())
        .collect::<Vec<_>>();
    if values.is_empty() {
        return JsonValue::object([
            ("min", JsonValue::number(0)),
            ("avg", JsonValue::number(0)),
            ("p50", JsonValue::number(0)),
            ("p95", JsonValue::number(0)),
            ("p99", JsonValue::number(0)),
            ("max", JsonValue::number(0)),
        ]);
    }
    values.sort_unstable();
    let sum = values.iter().copied().sum::<u128>();
    JsonValue::object([
        ("min", JsonValue::number(micros_to_millis(values[0]))),
        (
            "avg",
            JsonValue::number(micros_to_millis(sum / values.len() as u128)),
        ),
        (
            "p50",
            JsonValue::number(micros_to_millis(percentile_micros(&values, 50))),
        ),
        (
            "p95",
            JsonValue::number(micros_to_millis(percentile_micros(&values, 95))),
        ),
        (
            "p99",
            JsonValue::number(micros_to_millis(percentile_micros(&values, 99))),
        ),
        (
            "max",
            JsonValue::number(micros_to_millis(*values.last().unwrap_or(&0))),
        ),
    ])
}

fn percentile_micros(sorted_values: &[u128], percentile: usize) -> u128 {
    if sorted_values.is_empty() {
        return 0;
    }
    let rank = sorted_values
        .len()
        .saturating_mul(percentile)
        .saturating_add(99)
        / 100;
    let index = rank.saturating_sub(1).min(sorted_values.len() - 1);
    sorted_values[index]
}

fn micros_to_millis(micros: u128) -> String {
    let whole = micros / 1_000;
    let fraction = micros % 1_000;
    format!("{}.{:03}", whole, fraction)
}

fn operation_observation_json(observation: &OperationTraceObservation) -> JsonValue {
    JsonValue::object([
        ("name", JsonValue::string(observation.name.as_str())),
        ("route", JsonValue::string(observation.route.as_str())),
        (
            "durationMs",
            JsonValue::number(micros_to_millis(observation.duration.as_micros())),
        ),
        ("failed", JsonValue::bool(observation.failed)),
        (
            "attributes",
            JsonValue::object(
                observation
                    .attributes
                    .iter()
                    .map(|(key, value)| (key.clone(), JsonValue::string(value.as_str())))
                    .collect::<Vec<_>>(),
            ),
        ),
    ])
}
