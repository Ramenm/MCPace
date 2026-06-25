use crate::json::JsonValue;
use std::collections::{BTreeMap, VecDeque};
use std::time::Duration;

const DEFAULT_RECENT_LATENCY_WINDOW: usize = 2_048;
const RECENT_SAMPLE_EXPORT_LIMIT: usize = 20;

#[derive(Clone, Debug)]
pub(super) struct RequestLatencyObservation {
    pub method: String,
    pub route: String,
    pub path: String,
    pub request_body_bytes: usize,
    pub request_header_bytes: usize,
    pub parse_duration: Duration,
    pub body_read_duration: Duration,
    pub dispatch_duration: Duration,
    pub total_duration: Duration,
    pub failed: bool,
}

#[derive(Debug)]
pub(super) struct RequestLatencyTracker {
    sample_limit: usize,
    recorded_total: u64,
    observations: VecDeque<RequestLatencyObservation>,
}

impl Default for RequestLatencyTracker {
    fn default() -> Self {
        Self {
            sample_limit: DEFAULT_RECENT_LATENCY_WINDOW,
            recorded_total: 0,
            observations: VecDeque::with_capacity(DEFAULT_RECENT_LATENCY_WINDOW),
        }
    }
}

impl RequestLatencyTracker {
    pub fn record(&mut self, observation: RequestLatencyObservation) {
        self.recorded_total = self.recorded_total.saturating_add(1);
        if self.observations.len() >= self.sample_limit {
            self.observations.pop_front();
        }
        self.observations.push_back(observation);
    }

    pub fn snapshot_json(&self) -> JsonValue {
        let samples = self.observations.iter().collect::<Vec<_>>();
        let mut by_route = BTreeMap::<String, Vec<&RequestLatencyObservation>>::new();
        for observation in &samples {
            by_route
                .entry(format!("{} {}", observation.method, observation.route))
                .or_default()
                .push(*observation);
        }

        let mut route_items = Vec::new();
        for (route, observations) in by_route {
            let mut summary = latency_summary_json(&observations);
            if let JsonValue::Object(map) = &mut summary {
                map.insert("route".to_string(), JsonValue::string(route));
            }
            route_items.push(summary);
        }

        let mut recent = self
            .observations
            .iter()
            .rev()
            .take(RECENT_SAMPLE_EXPORT_LIMIT)
            .map(observation_json)
            .collect::<Vec<_>>();
        recent.reverse();

        JsonValue::object([
            ("schema", JsonValue::string("mcpace.httpLatency.v1")),
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
            ("overall", latency_summary_json(&samples)),
            ("byRoute", JsonValue::array(route_items)),
            ("recent", JsonValue::array(recent)),
            (
                "otelAliases",
                JsonValue::object([
                    (
                        "http.server.request.duration",
                        JsonValue::string("runtime.http.latency.byRoute[*].totalMs"),
                    ),
                    (
                        "http.server.request.body.size",
                        JsonValue::string("runtime.http.latency.byRoute[*].requestBodyBytesTotal"),
                    ),
                    (
                        "http.request.header.size",
                        JsonValue::string(
                            "runtime.http.latency.byRoute[*].requestHeaderBytesTotal",
                        ),
                    ),
                ]),
            ),
        ])
    }
}

fn latency_summary_json(observations: &[&RequestLatencyObservation]) -> JsonValue {
    let count = observations.len();
    let failed_count = observations.iter().filter(|item| item.failed).count();
    let request_body_bytes_total = observations
        .iter()
        .map(|item| item.request_body_bytes as u128)
        .sum::<u128>();
    let request_header_bytes_total = observations
        .iter()
        .map(|item| item.request_header_bytes as u128)
        .sum::<u128>();

    JsonValue::object([
        ("count", JsonValue::number(count)),
        ("failed", JsonValue::number(failed_count)),
        (
            "requestBodyBytesTotal",
            JsonValue::number(request_body_bytes_total),
        ),
        (
            "requestHeaderBytesTotal",
            JsonValue::number(request_header_bytes_total),
        ),
        (
            "totalMs",
            duration_distribution_json(observations, |item| item.total_duration),
        ),
        (
            "parseMs",
            duration_distribution_json(observations, |item| item.parse_duration),
        ),
        (
            "bodyReadMs",
            duration_distribution_json(observations, |item| item.body_read_duration),
        ),
        (
            "dispatchMs",
            duration_distribution_json(observations, |item| item.dispatch_duration),
        ),
    ])
}

fn duration_distribution_json<F>(observations: &[&RequestLatencyObservation], pick: F) -> JsonValue
where
    F: Fn(&RequestLatencyObservation) -> Duration,
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

fn observation_json(observation: &RequestLatencyObservation) -> JsonValue {
    JsonValue::object([
        ("method", JsonValue::string(observation.method.as_str())),
        ("route", JsonValue::string(observation.route.as_str())),
        ("path", JsonValue::string(observation.path.as_str())),
        (
            "requestBodyBytes",
            JsonValue::number(observation.request_body_bytes),
        ),
        (
            "requestHeaderBytes",
            JsonValue::number(observation.request_header_bytes),
        ),
        (
            "parseMs",
            JsonValue::number(micros_to_millis(observation.parse_duration.as_micros())),
        ),
        (
            "bodyReadMs",
            JsonValue::number(micros_to_millis(observation.body_read_duration.as_micros())),
        ),
        (
            "dispatchMs",
            JsonValue::number(micros_to_millis(observation.dispatch_duration.as_micros())),
        ),
        (
            "totalMs",
            JsonValue::number(micros_to_millis(observation.total_duration.as_micros())),
        ),
        ("failed", JsonValue::bool(observation.failed)),
    ])
}
