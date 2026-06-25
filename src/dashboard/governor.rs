use crate::json::JsonValue;
use crate::resources;
use std::convert::TryFrom;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug)]
pub(super) struct GlobalResourceGovernor {
    active_requests: AtomicUsize,
    completed_requests: AtomicUsize,
    rejected_requests: AtomicUsize,
    max_active_requests: AtomicUsize,
    active_request_limit: usize,
    rss_soft_bytes: Option<u64>,
    fd_soft_limit: Option<u64>,
    thread_soft_limit: Option<u64>,
}

impl Default for GlobalResourceGovernor {
    fn default() -> Self {
        Self {
            active_requests: AtomicUsize::new(0),
            completed_requests: AtomicUsize::new(0),
            rejected_requests: AtomicUsize::new(0),
            max_active_requests: AtomicUsize::new(0),
            active_request_limit: resources::default_global_active_request_limit(),
            rss_soft_bytes: resources::default_process_rss_soft_bytes(),
            fd_soft_limit: resources::default_process_fd_soft_limit(),
            thread_soft_limit: resources::default_process_thread_soft_limit(),
        }
    }
}

impl GlobalResourceGovernor {
    pub(super) fn try_enter_request(&self) -> Result<GlobalResourcePermit<'_>, ResourceRejection> {
        loop {
            let current = self.active_requests.load(Ordering::Acquire);
            if current >= self.active_request_limit {
                self.rejected_requests.fetch_add(1, Ordering::Relaxed);
                return Err(ResourceRejection {
                    reason: "active request budget exhausted",
                    retry_after_ms: 1_000,
                });
            }
            let next = current.saturating_add(1);
            if self
                .active_requests
                .compare_exchange(current, next, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                self.record_max_active(next);
                return Ok(GlobalResourcePermit { governor: self });
            }
        }
    }

    pub(super) fn snapshot_json(&self, process_resource: &JsonValue) -> JsonValue {
        let pressure = self.pressure_json(process_resource);
        JsonValue::object([
            (
                "schema",
                JsonValue::string("mcpace.globalResourceGovernor.v1"),
            ),
            (
                "activeRequestLimit",
                JsonValue::number(self.active_request_limit),
            ),
            (
                "activeRequests",
                JsonValue::number(self.active_requests.load(Ordering::Relaxed)),
            ),
            (
                "maxActiveRequests",
                JsonValue::number(self.max_active_requests.load(Ordering::Relaxed)),
            ),
            (
                "completedRequests",
                JsonValue::number(self.completed_requests.load(Ordering::Relaxed)),
            ),
            (
                "rejectedRequests",
                JsonValue::number(self.rejected_requests.load(Ordering::Relaxed)),
            ),
            (
                "rssSoftBytes",
                self.rss_soft_bytes
                    .map(JsonValue::number)
                    .unwrap_or(JsonValue::Null),
            ),
            (
                "fdSoftLimit",
                self.fd_soft_limit
                    .map(JsonValue::number)
                    .unwrap_or(JsonValue::Null),
            ),
            (
                "threadSoftLimit",
                self.thread_soft_limit
                    .map(JsonValue::number)
                    .unwrap_or(JsonValue::Null),
            ),
            ("pressure", pressure),
            (
                "otelAliases",
                JsonValue::object([
                    (
                        "http.server.active_requests",
                        JsonValue::string("runtime.http.resourceGovernor.activeRequests"),
                    ),
                    (
                        "http.server.request.duration",
                        JsonValue::string("runtime.http.latency.byRoute[*].totalMs"),
                    ),
                ]),
            ),
        ])
    }

    fn pressure_json(&self, process_resource: &JsonValue) -> JsonValue {
        let rss = json_u64(process_resource, "rssBytes");
        let fds = json_u64(process_resource, "fdCount");
        let threads = json_u64(process_resource, "threads");
        let mut items = Vec::new();
        push_pressure_item(&mut items, "rssBytes", rss, self.rss_soft_bytes);
        push_pressure_item(&mut items, "fdCount", fds, self.fd_soft_limit);
        push_pressure_item(&mut items, "threads", threads, self.thread_soft_limit);
        let over_limit = items.iter().any(|item| {
            item.get("overLimit")
                .and_then(JsonValue::as_bool)
                .unwrap_or(false)
        });
        JsonValue::object([
            ("ok", JsonValue::bool(!over_limit)),
            ("items", JsonValue::array(items)),
        ])
    }

    fn record_max_active(&self, active: usize) {
        let mut observed = self.max_active_requests.load(Ordering::Relaxed);
        while active > observed {
            match self.max_active_requests.compare_exchange_weak(
                observed,
                active,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(value) => observed = value,
            }
        }
    }

    fn leave_request(&self) {
        self.active_requests.fetch_sub(1, Ordering::AcqRel);
        self.completed_requests.fetch_add(1, Ordering::Relaxed);
    }
}

fn push_pressure_item(
    items: &mut Vec<JsonValue>,
    name: &'static str,
    observed: Option<u64>,
    soft_limit: Option<u64>,
) {
    let over_limit = match (observed, soft_limit) {
        (Some(value), Some(limit)) => value > limit,
        _ => false,
    };
    items.push(JsonValue::object([
        ("name", JsonValue::string(name)),
        (
            "observed",
            observed.map(JsonValue::number).unwrap_or(JsonValue::Null),
        ),
        (
            "softLimit",
            soft_limit.map(JsonValue::number).unwrap_or(JsonValue::Null),
        ),
        ("overLimit", JsonValue::bool(over_limit)),
    ]));
}

fn json_u64(value: &JsonValue, key: &str) -> Option<u64> {
    value
        .get(key)
        .and_then(JsonValue::as_i64)
        .and_then(|value| u64::try_from(value).ok())
}

pub(super) struct ResourceRejection {
    pub(super) reason: &'static str,
    pub(super) retry_after_ms: u64,
}

pub(super) struct GlobalResourcePermit<'a> {
    governor: &'a GlobalResourceGovernor,
}

impl Drop for GlobalResourcePermit<'_> {
    fn drop(&mut self) {
        self.governor.leave_request();
    }
}
