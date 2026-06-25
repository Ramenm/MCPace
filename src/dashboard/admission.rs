use crate::json::JsonValue;
use crate::resources;
// DEFAULT_HEAVY_ACTION_CONCURRENCY is applied through resources::default_heavy_action_concurrency().
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HttpAdmissionKind {
    HeavyAction,
    OverviewRefresh,
}

#[derive(Debug)]
pub(crate) struct HttpAdmissionController {
    heavy_action_active: AtomicUsize,
    heavy_action_rejected: AtomicUsize,
    heavy_action_completed: AtomicUsize,
    overview_refresh_active: AtomicUsize,
    overview_refresh_rejected: AtomicUsize,
    overview_refresh_completed: AtomicUsize,
    heavy_action_limit: usize,
    overview_refresh_limit: usize,
}

impl Default for HttpAdmissionController {
    fn default() -> Self {
        Self {
            heavy_action_active: AtomicUsize::new(0),
            heavy_action_rejected: AtomicUsize::new(0),
            heavy_action_completed: AtomicUsize::new(0),
            overview_refresh_active: AtomicUsize::new(0),
            overview_refresh_rejected: AtomicUsize::new(0),
            overview_refresh_completed: AtomicUsize::new(0),
            heavy_action_limit: resources::default_heavy_action_concurrency(),
            overview_refresh_limit: 1,
        }
    }
}

impl HttpAdmissionController {
    pub(crate) fn try_enter(&self, kind: HttpAdmissionKind) -> Option<HttpAdmissionPermit<'_>> {
        let (active, rejected, limit) = self.parts(kind);
        loop {
            let current = active.load(Ordering::Acquire);
            if current >= limit {
                rejected.fetch_add(1, Ordering::Relaxed);
                return None;
            }
            if active
                .compare_exchange(
                    current,
                    current.saturating_add(1),
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok()
            {
                return Some(HttpAdmissionPermit {
                    controller: self,
                    kind,
                });
            }
        }
    }

    pub(crate) fn snapshot_json(&self) -> JsonValue {
        JsonValue::object([
            ("schema", JsonValue::string("mcpace.httpAdmission.v1")),
            (
                "heavyActions",
                self.kind_snapshot_json(HttpAdmissionKind::HeavyAction, self.heavy_action_limit),
            ),
            (
                "overviewRefresh",
                self.kind_snapshot_json(
                    HttpAdmissionKind::OverviewRefresh,
                    self.overview_refresh_limit,
                ),
            ),
        ])
    }

    fn kind_snapshot_json(&self, kind: HttpAdmissionKind, limit: usize) -> JsonValue {
        let (active, rejected, _) = self.parts(kind);
        let completed = match kind {
            HttpAdmissionKind::HeavyAction => &self.heavy_action_completed,
            HttpAdmissionKind::OverviewRefresh => &self.overview_refresh_completed,
        };
        JsonValue::object([
            ("limit", JsonValue::number(limit)),
            ("active", JsonValue::number(active.load(Ordering::Relaxed))),
            (
                "completed",
                JsonValue::number(completed.load(Ordering::Relaxed)),
            ),
            (
                "rejected",
                JsonValue::number(rejected.load(Ordering::Relaxed)),
            ),
        ])
    }

    fn parts(&self, kind: HttpAdmissionKind) -> (&AtomicUsize, &AtomicUsize, usize) {
        match kind {
            HttpAdmissionKind::HeavyAction => (
                &self.heavy_action_active,
                &self.heavy_action_rejected,
                self.heavy_action_limit,
            ),
            HttpAdmissionKind::OverviewRefresh => (
                &self.overview_refresh_active,
                &self.overview_refresh_rejected,
                self.overview_refresh_limit,
            ),
        }
    }

    fn leave(&self, kind: HttpAdmissionKind) {
        let (active, _, _) = self.parts(kind);
        active.fetch_sub(1, Ordering::AcqRel);
        match kind {
            HttpAdmissionKind::HeavyAction => &self.heavy_action_completed,
            HttpAdmissionKind::OverviewRefresh => &self.overview_refresh_completed,
        }
        .fetch_add(1, Ordering::Relaxed);
    }
}

pub(crate) struct HttpAdmissionPermit<'a> {
    controller: &'a HttpAdmissionController,
    kind: HttpAdmissionKind,
}

impl Drop for HttpAdmissionPermit<'_> {
    fn drop(&mut self) {
        self.controller.leave(self.kind);
    }
}
