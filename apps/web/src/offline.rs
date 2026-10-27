//! Bounded offline support (09-frontend.md §7, ADR-0005): pending review
//! events + prefetched queue in localStorage; ordered idempotent replay;
//! 409 → drop local schedule copy, keep the event for audit.

use serde::{Deserialize, Serialize};
use shared_schema::{ApiError, QueueDto};

const EVENTS_KEY: &str = "lt-offline-events";
const CONFLICTS_KEY: &str = "lt-conflict-events";
const QUEUE_KEY: &str = "lt-prefetch-queue";

/// Hard bound on queued events (mirrors `max_review_batch`).
pub const MAX_PENDING_EVENTS: usize = 50;

/// One grade recorded while offline; replayed verbatim (same
/// `client_operation_id` + `prior_state_hash` → server-side idempotency).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingEvent {
    /// Target session.
    pub session_id: String,
    /// Idempotency key.
    pub client_operation_id: String,
    /// Graded card.
    pub card_id: String,
    /// Grade 1..=4.
    pub grade: u8,
    /// Presentation instant (RFC 3339).
    pub presented_at: String,
    /// Answer instant (RFC 3339).
    pub answered_at: String,
    /// Measured latency.
    pub response_millis: u32,
    /// Hash the server must see.
    pub prior_state_hash: String,
}

/// Pure queue ops — unit-tested on host; storage IO stays in the two
/// thin wrappers below.
pub mod pure {
    use super::{MAX_PENDING_EVENTS, PendingEvent};

    /// Parses the stored queue; corrupt text = empty queue (never panic).
    #[must_use]
    pub fn parse_queue(raw: Option<String>) -> Vec<PendingEvent> {
        raw.and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Appends preserving order and the hard bound: full queue rejects the
    /// event (review must pause offline rather than silently drop history).
    ///
    /// # Errors
    ///
    /// `Err(())` when the queue is at [`MAX_PENDING_EVENTS`].
    pub fn append(
        mut queue: Vec<PendingEvent>,
        event: PendingEvent,
    ) -> Result<Vec<PendingEvent>, ()> {
        if queue.len() >= MAX_PENDING_EVENTS {
            return Err(());
        }
        queue.push(event);
        Ok(queue)
    }

    /// Serializes for storage.
    #[must_use]
    pub fn serialize(queue: &[PendingEvent]) -> String {
        serde_json::to_string(queue).unwrap_or_else(|_| "[]".to_owned())
    }
}

fn storage() -> Option<web_sys::Storage> {
    leptos::prelude::window().local_storage().ok().flatten()
}

fn read(key: &str) -> Option<String> {
    storage().and_then(|s| s.get_item(key).ok().flatten())
}

fn write(key: &str, value: &str) -> bool {
    storage().is_some_and(|s| s.set_item(key, value).is_ok())
}

/// Currently queued offline events, oldest first.
#[must_use]
pub fn pending_events() -> Vec<PendingEvent> {
    pure::parse_queue(read(EVENTS_KEY))
}

/// Queues one event. `false` = queue full; caller must stop the session.
#[must_use]
pub fn queue_event(event: PendingEvent) -> bool {
    match pure::append(pending_events(), event) {
        Ok(queue) => write(EVENTS_KEY, &pure::serialize(&queue)),
        Err(()) => false,
    }
}

/// Stores the prefetched due batch for offline review.
pub fn cache_queue(queue: &QueueDto) {
    if let Ok(text) = serde_json::to_string(queue) {
        write(QUEUE_KEY, &text);
    }
}

/// Last prefetched due batch, if any.
#[must_use]
pub fn cached_queue() -> Option<QueueDto> {
    read(QUEUE_KEY).and_then(|text| serde_json::from_str(&text).ok())
}

/// Replay outcome for the caller's UI.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ReplayReport {
    /// Events accepted (or already known) by the server.
    pub synced: u32,
    /// Events rejected with `conflict_stale_state`, kept for audit.
    pub conflicts: u32,
    /// True when replay stopped on a network failure (still offline).
    pub interrupted: bool,
}

/// Replays pending events in order (09 §7): success/duplicate → drop from
/// queue; 409 stale → move to the conflict audit list (local schedule copy
/// is already server-side truth after refetch); network error → stop.
pub async fn replay() -> ReplayReport {
    let mut report = ReplayReport::default();
    let mut queue = pending_events();
    while let Some(event) = queue.first().cloned() {
        let body = serde_json::json!({
            "session_id": event.session_id,
            "client_operation_id": event.client_operation_id,
            "card_id": event.card_id,
            "grade": event.grade,
            "presented_at": event.presented_at,
            "answered_at": event.answered_at,
            "response_millis": event.response_millis,
            "answer_revealed": true,
            "prior_state_hash": event.prior_state_hash,
            "source": "offline",
        });
        match crate::api_client::call::<_, serde_json::Value>("review_card", &body).await {
            Ok(_) => {
                queue.remove(0);
                report.synced += 1;
            }
            Err(ApiError::Api { code, .. }) if code == "conflict_stale_state" => {
                let mut conflicts = pure::parse_queue(read(CONFLICTS_KEY));
                conflicts.push(queue.remove(0));
                if !write(CONFLICTS_KEY, &pure::serialize(&conflicts)) {
                    report.interrupted = true;
                    break;
                }
                report.conflicts += 1;
            }
            Err(ApiError::Api { .. } | ApiError::Decode(_)) => {
                // Rejected for another reason (e.g. duplicate returns 200 —
                // handled above; validation errors): keep for audit, move on.
                let mut conflicts = pure::parse_queue(read(CONFLICTS_KEY));
                conflicts.push(queue.remove(0));
                if !write(CONFLICTS_KEY, &pure::serialize(&conflicts)) {
                    report.interrupted = true;
                    break;
                }
                report.conflicts += 1;
            }
            Err(ApiError::Network(_)) => {
                report.interrupted = true;
                break;
            }
        }
        write(EVENTS_KEY, &pure::serialize(&queue));
    }
    write(EVENTS_KEY, &pure::serialize(&queue));
    report
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::pure::{append, parse_queue, serialize};
    use super::{MAX_PENDING_EVENTS, PendingEvent};

    fn event(n: usize) -> PendingEvent {
        PendingEvent {
            session_id: "rs-1".to_owned(),
            client_operation_id: format!("op-{n}"),
            card_id: "crd-1".to_owned(),
            grade: 3,
            presented_at: "2026-07-12T10:00:00Z".to_owned(),
            answered_at: "2026-07-12T10:00:05Z".to_owned(),
            response_millis: 5000,
            prior_state_hash: format!("h-{n}"),
        }
    }

    #[test]
    fn round_trip_preserves_order() {
        let queue = vec![event(1), event(2), event(3)];
        let parsed = parse_queue(Some(serialize(&queue)));
        assert_eq!(parsed, queue);
    }

    #[test]
    fn corrupt_or_missing_storage_is_empty_queue() {
        assert!(parse_queue(None).is_empty());
        assert!(parse_queue(Some("{broken".to_owned())).is_empty());
    }

    #[test]
    fn append_enforces_hard_bound() {
        let mut queue = Vec::new();
        for n in 0..MAX_PENDING_EVENTS {
            queue = append(queue, event(n)).unwrap();
        }
        assert!(
            append(queue.clone(), event(999)).is_err(),
            "queue full must reject"
        );
        assert_eq!(queue.len(), MAX_PENDING_EVENTS);
    }
}
