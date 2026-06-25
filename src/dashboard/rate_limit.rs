use crate::json::JsonValue;
use crate::resources;
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

const MAX_TRACKED_RATE_LIMIT_CLIENTS: usize = 1024;
const RECENT_RATE_LIMIT_EXPORT_LIMIT: usize = 25;

#[derive(Clone, Debug)]
struct ClientWindow {
    requests: VecDeque<Instant>,
    last_seen: Instant,
}

#[derive(Clone, Debug)]
struct RateLimitedEvent {
    client_key: String,
    retry_after: Duration,
}

#[derive(Clone, Debug)]
pub(super) struct RateLimitDecision {
    pub allowed: bool,
    pub retry_after: Duration,
    pub client_key: String,
}

#[derive(Debug)]
pub(super) struct HttpRateLimiter {
    window: Duration,
    max_requests: usize,
    max_clients: usize,
    clients: HashMap<String, ClientWindow>,
    allowed_total: u64,
    limited_total: u64,
    evicted_clients: u64,
    recent_limited: VecDeque<RateLimitedEvent>,
}

impl Default for HttpRateLimiter {
    fn default() -> Self {
        Self::new(
            Duration::from_millis(resources::default_http_rate_limit_window_ms()),
            resources::default_http_rate_limit_max_requests(),
        )
    }
}

impl HttpRateLimiter {
    fn new(window: Duration, max_requests: usize) -> Self {
        Self {
            window: window.max(Duration::from_millis(1)),
            max_requests: max_requests.max(1),
            max_clients: MAX_TRACKED_RATE_LIMIT_CLIENTS,
            clients: HashMap::new(),
            allowed_total: 0,
            limited_total: 0,
            evicted_clients: 0,
            recent_limited: VecDeque::with_capacity(RECENT_RATE_LIMIT_EXPORT_LIMIT),
        }
    }

    pub fn check(&mut self, client_key: &str, now: Instant) -> RateLimitDecision {
        self.prune_idle(now);
        if !self.clients.contains_key(client_key) && self.clients.len() >= self.max_clients {
            self.evict_oldest_client();
        }

        let window = self.window;
        let max_requests = self.max_requests;
        let retry_after = {
            let window_start = now.checked_sub(window).unwrap_or(now);
            let client = self
                .clients
                .entry(client_key.to_string())
                .or_insert_with(|| ClientWindow {
                    requests: VecDeque::new(),
                    last_seen: now,
                });
            client.last_seen = now;
            prune_window(&mut client.requests, window_start);

            if client.requests.len() >= max_requests {
                Some(
                    client
                        .requests
                        .front()
                        .map(|oldest| window.saturating_sub(now.saturating_duration_since(*oldest)))
                        .unwrap_or(window)
                        .max(Duration::from_secs(1)),
                )
            } else {
                client.requests.push_back(now);
                None
            }
        };

        if let Some(retry_after) = retry_after {
            self.limited_total = self.limited_total.saturating_add(1);
            self.record_limited(client_key, retry_after);
            return RateLimitDecision {
                allowed: false,
                retry_after,
                client_key: client_key.to_string(),
            };
        }

        self.allowed_total = self.allowed_total.saturating_add(1);
        RateLimitDecision {
            allowed: true,
            retry_after: Duration::from_secs(0),
            client_key: client_key.to_string(),
        }
    }

    pub fn snapshot_json(&self) -> JsonValue {
        let active_clients = self.clients.len();
        let retained_requests = self
            .clients
            .values()
            .map(|client| client.requests.len())
            .sum::<usize>();
        let mut recent = self
            .recent_limited
            .iter()
            .rev()
            .take(RECENT_RATE_LIMIT_EXPORT_LIMIT)
            .map(|event| {
                JsonValue::object([
                    ("clientKey", JsonValue::string(event.client_key.as_str())),
                    (
                        "retryAfterMs",
                        JsonValue::number(event.retry_after.as_millis()),
                    ),
                ])
            })
            .collect::<Vec<_>>();
        recent.reverse();

        JsonValue::object([
            ("schema", JsonValue::string("mcpace.httpRateLimit.v1")),
            ("windowMs", JsonValue::number(self.window.as_millis())),
            ("maxRequests", JsonValue::number(self.max_requests)),
            ("maxTrackedClients", JsonValue::number(self.max_clients)),
            ("activeClients", JsonValue::number(active_clients)),
            ("retainedRequests", JsonValue::number(retained_requests)),
            ("allowedTotal", JsonValue::number(self.allowed_total)),
            ("limitedTotal", JsonValue::number(self.limited_total)),
            ("evictedClients", JsonValue::number(self.evicted_clients)),
            ("recentLimited", JsonValue::array(recent)),
        ])
    }

    fn prune_idle(&mut self, now: Instant) {
        let window_start = now.checked_sub(self.window).unwrap_or(now);
        for client in self.clients.values_mut() {
            prune_window(&mut client.requests, window_start);
        }
        let idle_window = self.window.checked_mul(2).unwrap_or(self.window);
        let idle_cutoff = now.checked_sub(idle_window).unwrap_or(now);
        let before = self.clients.len();
        self.clients
            .retain(|_, client| client.last_seen > idle_cutoff || !client.requests.is_empty());
        self.evicted_clients = self
            .evicted_clients
            .saturating_add(before.saturating_sub(self.clients.len()) as u64);
    }

    fn evict_oldest_client(&mut self) {
        let Some(oldest_key) = self
            .clients
            .iter()
            .min_by_key(|(_, client)| client.last_seen)
            .map(|(key, _)| key.clone())
        else {
            return;
        };
        self.clients.remove(&oldest_key);
        self.evicted_clients = self.evicted_clients.saturating_add(1);
    }

    fn record_limited(&mut self, client_key: &str, retry_after: Duration) {
        if self.recent_limited.len() >= RECENT_RATE_LIMIT_EXPORT_LIMIT {
            self.recent_limited.pop_front();
        }
        self.recent_limited.push_back(RateLimitedEvent {
            client_key: client_key.to_string(),
            retry_after,
        });
    }
}

fn prune_window(requests: &mut VecDeque<Instant>, window_start: Instant) {
    while requests
        .front()
        .map(|instant| *instant <= window_start)
        .unwrap_or(false)
    {
        requests.pop_front();
    }
}
