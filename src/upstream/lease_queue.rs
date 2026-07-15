//! Process-local FIFO admission queue for upstream leases.
//!
//! The durable lease store remains the cross-process source of truth. This
//! queue prevents callers in one MCPace process from racing that store, adds
//! bounded waiting/cancellation, and wakes the head waiter when a lease is
//! released.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

const MAX_QUEUE_LANES: usize = 4_096;
const MAX_WAIT_SLICE: Duration = Duration::from_millis(250);

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum LeaseQueueError {
    Poisoned,
    LaneLimit { maximum: usize },
    Full { lane: String, maximum: usize },
    Timeout { lane: String, ticket: u64 },
}

impl fmt::Display for LeaseQueueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Poisoned => formatter.write_str("upstream lease queue lock was poisoned"),
            Self::LaneLimit { maximum } => write!(
                formatter,
                "upstream lease queue exceeded its {}-lane safety limit",
                maximum
            ),
            Self::Full { lane, maximum } => write!(
                formatter,
                "upstream lease queue '{}' is full (maximum depth {})",
                lane, maximum
            ),
            Self::Timeout { lane, ticket } => write!(
                formatter,
                "upstream lease queue '{}' timed out while waiting for ticket {}",
                lane, ticket
            ),
        }
    }
}

impl std::error::Error for LeaseQueueError {}

#[derive(Debug, Default)]
struct QueueRegistry {
    lanes: BTreeMap<String, Arc<QueueLane>>,
}

#[derive(Debug, Default)]
struct QueueLane {
    state: Mutex<QueueLaneState>,
    changed: Condvar,
}

#[derive(Debug, Default)]
struct QueueLaneState {
    next_ticket: u64,
    serving_ticket: u64,
    waiting: usize,
    cancelled: BTreeSet<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct LeaseQueuePosition {
    pub(super) ticket: u64,
    pub(super) depth_at_enqueue: usize,
    pub(super) ahead_at_enqueue: usize,
}

pub(super) struct LeaseQueueTicket {
    lane_key: String,
    lane: Arc<QueueLane>,
    position: LeaseQueuePosition,
    completed: bool,
}

static QUEUE_REGISTRY: OnceLock<Mutex<QueueRegistry>> = OnceLock::new();

fn registry() -> &'static Mutex<QueueRegistry> {
    QUEUE_REGISTRY.get_or_init(|| Mutex::new(QueueRegistry::default()))
}

pub(super) fn enqueue(
    lane_key: impl Into<String>,
    max_depth: usize,
) -> Result<LeaseQueueTicket, LeaseQueueError> {
    let lane_key = lane_key.into();
    let max_depth = max_depth.max(1);
    let lane = {
        let mut registry = registry().lock().map_err(|_| LeaseQueueError::Poisoned)?;
        prune_empty_lanes(&mut registry)?;
        if let Some(lane) = registry.lanes.get(&lane_key) {
            Arc::clone(lane)
        } else {
            if registry.lanes.len() >= MAX_QUEUE_LANES {
                return Err(LeaseQueueError::LaneLimit {
                    maximum: MAX_QUEUE_LANES,
                });
            }
            let lane = Arc::new(QueueLane::default());
            registry.lanes.insert(lane_key.clone(), Arc::clone(&lane));
            lane
        }
    };

    let position = {
        let mut state = lane.state.lock().map_err(|_| LeaseQueueError::Poisoned)?;
        if state.waiting >= max_depth {
            return Err(LeaseQueueError::Full {
                lane: lane_key,
                maximum: max_depth,
            });
        }
        advance_cancelled(&mut state);
        let ticket = state.next_ticket;
        state.next_ticket = state.next_ticket.saturating_add(1);
        let ahead_at_enqueue = ticket.saturating_sub(state.serving_ticket) as usize;
        state.waiting = state.waiting.saturating_add(1);
        LeaseQueuePosition {
            ticket,
            depth_at_enqueue: state.waiting,
            ahead_at_enqueue,
        }
    };

    Ok(LeaseQueueTicket {
        lane_key,
        lane,
        position,
        completed: false,
    })
}

impl LeaseQueueTicket {
    pub(super) fn position(&self) -> &LeaseQueuePosition {
        &self.position
    }

    pub(super) fn wait_until_head(&self, deadline: Instant) -> Result<(), LeaseQueueError> {
        let mut state = self
            .lane
            .state
            .lock()
            .map_err(|_| LeaseQueueError::Poisoned)?;
        loop {
            advance_cancelled(&mut state);
            if state.serving_ticket == self.position.ticket {
                return Ok(());
            }
            let now = Instant::now();
            if now >= deadline {
                return Err(self.timeout_error());
            }
            let wait = deadline.saturating_duration_since(now).min(MAX_WAIT_SLICE);
            let (next_state, wait_result) = self
                .lane
                .changed
                .wait_timeout(state, wait)
                .map_err(|_| LeaseQueueError::Poisoned)?;
            state = next_state;
            if wait_result.timed_out() && Instant::now() >= deadline {
                return Err(self.timeout_error());
            }
        }
    }

    /// Waits while this ticket remains the head. A release notification wakes
    /// the caller immediately; the timeout slice also covers releases made by a
    /// different MCPace process, which cannot signal this process-local queue.
    pub(super) fn wait_for_retry(
        &self,
        retry_after: Duration,
        deadline: Instant,
    ) -> Result<(), LeaseQueueError> {
        let now = Instant::now();
        if now >= deadline {
            return Err(self.timeout_error());
        }
        let wait = retry_after
            .min(deadline.saturating_duration_since(now))
            .min(MAX_WAIT_SLICE);
        let state = self
            .lane
            .state
            .lock()
            .map_err(|_| LeaseQueueError::Poisoned)?;
        let (_state, wait_result) = self
            .lane
            .changed
            .wait_timeout(state, wait)
            .map_err(|_| LeaseQueueError::Poisoned)?;
        if wait_result.timed_out() && Instant::now() >= deadline {
            return Err(self.timeout_error());
        }
        Ok(())
    }

    pub(super) fn complete(mut self) {
        self.finish(false);
    }

    fn timeout_error(&self) -> LeaseQueueError {
        LeaseQueueError::Timeout {
            lane: self.lane_key.clone(),
            ticket: self.position.ticket,
        }
    }

    fn finish(&mut self, cancelled: bool) {
        if self.completed {
            return;
        }
        let (mut state, recovered) = match self.lane.state.lock() {
            Ok(state) => (state, false),
            Err(poisoned) => (poisoned.into_inner(), true),
        };
        if recovered {
            repair_lane_state(&mut state);
            self.lane.state.clear_poison();
            record_poison_recovery("lane state");
        }
        state.waiting = state.waiting.saturating_sub(1);
        if self.position.ticket == state.serving_ticket {
            state.serving_ticket = state.serving_ticket.saturating_add(1);
            advance_cancelled(&mut state);
        } else if cancelled && self.position.ticket > state.serving_ticket {
            state.cancelled.insert(self.position.ticket);
        }
        self.lane.changed.notify_all();
        self.completed = true;
    }
}

impl Drop for LeaseQueueTicket {
    fn drop(&mut self) {
        self.finish(true);
    }
}

pub(super) fn notify_all_lanes() {
    let registry_mutex = registry();
    let (registry_guard, recovered) = match registry_mutex.lock() {
        Ok(registry_guard) => (registry_guard, false),
        Err(poisoned) => (poisoned.into_inner(), true),
    };
    let lanes = registry_guard.lanes.values().cloned().collect::<Vec<_>>();
    drop(registry_guard);
    if recovered {
        registry_mutex.clear_poison();
        record_poison_recovery("registry");
    }
    for lane in lanes {
        lane.changed.notify_all();
    }
}

fn advance_cancelled(state: &mut QueueLaneState) {
    while state.cancelled.remove(&state.serving_ticket) {
        state.serving_ticket = state.serving_ticket.saturating_add(1);
    }
}

fn repair_lane_state(state: &mut QueueLaneState) {
    state.next_ticket = state.next_ticket.max(state.serving_ticket);
    state
        .cancelled
        .retain(|ticket| *ticket >= state.serving_ticket && *ticket < state.next_ticket);
    advance_cancelled(state);
    let outstanding = state.next_ticket.saturating_sub(state.serving_ticket);
    state.waiting = usize::try_from(outstanding)
        .unwrap_or(usize::MAX)
        .saturating_sub(state.cancelled.len());
}

fn record_poison_recovery(scope: &str) {
    let mut stderr = std::io::stderr().lock();
    crate::diagnostics::stderr_line(
        &mut stderr,
        format_args!("recovered poisoned upstream lease queue {}", scope),
    );
}

fn prune_empty_lanes(registry: &mut QueueRegistry) -> Result<(), LeaseQueueError> {
    let keys = registry
        .lanes
        .iter()
        .filter_map(|(key, lane)| {
            let (mut state, recovered) = match lane.state.lock() {
                Ok(state) => (state, false),
                Err(poisoned) => (poisoned.into_inner(), true),
            };
            if recovered {
                repair_lane_state(&mut state);
                lane.state.clear_poison();
                record_poison_recovery("lane state during pruning");
            }
            (state.waiting == 0 && Arc::strong_count(lane) == 1).then(|| key.clone())
        })
        .collect::<Vec<_>>();
    for key in keys {
        registry.lanes.remove(&key);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
