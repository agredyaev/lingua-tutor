//! Table-driven recommendation rules (07-tutor-contract.md §7): explicit Rust,
//! deterministic, unit-tested. Rules emit recommendations — they never
//! mutate content (05-review-engine.md §10 actions are proposals).

/// Bounded per-card signal window assembled by the local runtime (newest first).
#[derive(Debug, Clone, Default)]
pub struct CardSignals {
    /// Card under evaluation.
    pub card_id: String,
    /// Source learning item, when linked.
    pub learning_item_id: Option<String>,
    /// Last grades, newest first, window ≤ 10.
    pub recent_grades: Vec<u8>,
    /// Mean answer latency over the window.
    pub avg_response_millis: Option<u32>,
    /// Failed spontaneous-production evidence on the source item.
    pub spontaneous_failures: u32,
    /// Successful spontaneous-production evidence on the source item.
    pub spontaneous_successes: u32,
    /// Successful prompted-production evidence on the source item.
    pub prompted_successes: u32,
    /// Failed listening evidence on the source item.
    pub listening_failures: u32,
    /// Total graded reviews of this card (lifetime).
    pub review_count: u32,
}

/// One emitted recommendation (persisted to `recommendations`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recommendation {
    /// `recommendations.subject_type` value.
    pub subject_type: &'static str,
    /// Subject row ID.
    pub subject_id: String,
    /// Stable action code.
    pub action: &'static str,
    /// Human-readable rationale.
    pub detail: String,
}

/// Window size the local runtime must respect when building [`CardSignals`].
pub const GRADE_WINDOW: usize = 10;
/// `Again` count in the window that marks a redesign candidate (05 §10).
pub const REDESIGN_AGAIN_THRESHOLD: usize = 3;
/// `Hard` count + latency floor marking a struggling card.
pub const REDESIGN_HARD_THRESHOLD: usize = 3;
/// Latency considered "high" (ms).
pub const HIGH_LATENCY_MILLIS: u32 = 10_000;
/// Minimum graded reviews before evaluating the low-utility rule.
pub const RETIREMENT_MIN_REVIEWS: u32 = 10;

const AGAIN: u8 = 1;
const HARD: u8 = 2;

/// Evaluates the 07 §7 card rules over one signal window. Deterministic:
/// same signals → same recommendations, stable order.
#[must_use]
pub fn evaluate_card_rules(signals: &CardSignals) -> Vec<Recommendation> {
    let mut out = Vec::new();
    let again_count = count(signals, AGAIN);
    let hard_count = count(signals, HARD);
    let last_failed = signals.recent_grades.first().copied() == Some(AGAIN);
    let successes = signals.recent_grades.iter().filter(|g| **g > AGAIN).count();
    let success_rate = if signals.recent_grades.is_empty() {
        0.0
    } else {
        #[allow(clippy::cast_precision_loss)] // window ≤ 10
        let rate = successes as f64 / signals.recent_grades.len() as f64;
        rate
    };

    // Rule: failed production review → phrase into next speaking session.
    if last_failed && let Some(item) = &signals.learning_item_id {
        out.push(Recommendation {
            subject_type: "learning_item",
            subject_id: item.clone(),
            action: "practice_in_lesson",
            detail: "failed production review — bring the phrase into the next speaking session"
                .to_owned(),
        });
    }

    // Rule: review success + spontaneous speaking failure → role-play
    // transfer, not more reviews.
    if success_rate >= 0.8
        && signals.spontaneous_failures > 0
        && let Some(item) = &signals.learning_item_id
    {
        out.push(Recommendation {
            subject_type: "learning_item",
            subject_id: item.clone(),
            action: "schedule_transfer_practice",
            detail: "review passes but spontaneous speech fails — schedule role-play transfer"
                .to_owned(),
        });
    }

    // Rule: repeated listening failure on a known phrase → listening card.
    if signals.listening_failures >= 2
        && let Some(item) = &signals.learning_item_id
    {
        out.push(Recommendation {
            subject_type: "listening_item",
            subject_id: item.clone(),
            action: "create_listening_card",
            detail: "repeated listening failures — create/activate a listening card".to_owned(),
        });
    }

    // Rule: prompted success + spontaneous failure → transfer target.
    if signals.prompted_successes >= 2
        && signals.spontaneous_successes == 0
        && let Some(item) = &signals.learning_item_id
    {
        out.push(Recommendation {
            subject_type: "learning_item",
            subject_id: item.clone(),
            action: "create_transfer_target",
            detail: "produced only with hints — create a transfer target".to_owned(),
        });
    }

    // Redesign candidates (05 §10): repeated Again, or high latency + Hard.
    if again_count >= REDESIGN_AGAIN_THRESHOLD {
        out.push(redesign(
            signals,
            "repeated Again grades in the recent window",
        ));
    } else if hard_count >= REDESIGN_HARD_THRESHOLD
        && signals.avg_response_millis.unwrap_or(0) >= HIGH_LATENCY_MILLIS
    {
        out.push(redesign(signals, "frequent Hard with high response time"));
    } else if success_rate >= 0.8 && signals.spontaneous_failures > 0 {
        out.push(redesign(
            signals,
            "review success but conversational failure",
        ));
    }

    // Rule: high review success + no real-life usage → consider retiring.
    if signals.review_count >= RETIREMENT_MIN_REVIEWS
        && success_rate >= 0.9
        && signals.spontaneous_successes == 0
        && signals.spontaneous_failures == 0
    {
        out.push(Recommendation {
            subject_type: "card",
            subject_id: signals.card_id.clone(),
            action: "consider_retirement",
            detail: "high review success but never used in conversation — low utility".to_owned(),
        });
    }

    out
}

fn count(signals: &CardSignals, grade: u8) -> usize {
    signals
        .recent_grades
        .iter()
        .filter(|g| **g == grade)
        .count()
}

fn redesign(signals: &CardSignals, reason: &str) -> Recommendation {
    Recommendation {
        subject_type: "card",
        subject_id: signals.card_id.clone(),
        action: "redesign_card",
        detail: format!("redesign candidate: {reason}"),
    }
}

/// Escalation recommendations for error targets (03 §7, 07 §7 rows 5–6).
#[must_use]
pub fn error_escalation(target_id: &str, occurrence_count: u32) -> Vec<Recommendation> {
    let mut out = Vec::new();
    if occurrence_count >= learning_core_thresholds::METHOD_CHANGE {
        out.push(Recommendation {
            subject_type: "error_target",
            subject_id: target_id.to_owned(),
            action: "require_method_change",
            detail: format!("{occurrence_count} unresolved occurrences — change the method"),
        });
    } else if occurrence_count >= learning_core_thresholds::ESCALATION {
        out.push(Recommendation {
            subject_type: "error_target",
            subject_id: target_id.to_owned(),
            action: "activate_mini_target",
            detail: format!(
                "{occurrence_count} occurrences — activate a mini-target and a contrast card"
            ),
        });
    }
    out
}

/// Failed 1/3/7 recheck → target back to active practice (07 §7 row 8).
#[must_use]
pub fn recheck_failed(target_id: &str, offset_days: u32) -> Recommendation {
    Recommendation {
        subject_type: "error_target",
        subject_id: target_id.to_owned(),
        action: "practice_again",
        detail: format!("failed the {offset_days}-day recheck — back to active practice"),
    }
}

/// Threshold mirror (adaptation must not depend on learning-core: it sits
/// beside it in the graph, 02 §3). Values pinned by cross-crate test.
mod learning_core_thresholds {
    pub const ESCALATION: u32 = 3;
    pub const METHOD_CHANGE: u32 = 5;
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn base() -> CardSignals {
        CardSignals {
            card_id: "crd-1".to_owned(),
            learning_item_id: Some("li-1".to_owned()),
            ..CardSignals::default()
        }
    }

    #[test]
    fn failed_review_sends_phrase_to_next_lesson() {
        let signals = CardSignals {
            recent_grades: vec![1, 3, 3],
            ..base()
        };
        let recs = evaluate_card_rules(&signals);
        assert!(recs.iter().any(|r| r.action == "practice_in_lesson"));
    }

    #[test]
    fn review_pass_conversation_fail_yields_transfer_and_redesign() {
        let signals = CardSignals {
            recent_grades: vec![3, 3, 4, 3, 3],
            spontaneous_failures: 2,
            ..base()
        };
        let actions: Vec<_> = evaluate_card_rules(&signals)
            .into_iter()
            .map(|r| r.action)
            .collect();
        assert!(actions.contains(&"schedule_transfer_practice"));
        assert!(actions.contains(&"redesign_card"));
    }

    #[test]
    fn repeated_again_marks_redesign() {
        let signals = CardSignals {
            recent_grades: vec![1, 1, 3, 1, 3],
            ..base()
        };
        let recs = evaluate_card_rules(&signals);
        assert!(
            recs.iter()
                .any(|r| r.action == "redesign_card" && r.detail.contains("repeated Again"))
        );
    }

    #[test]
    fn hard_plus_latency_marks_redesign_but_hard_alone_does_not() {
        let slow = CardSignals {
            recent_grades: vec![2, 2, 3, 2],
            avg_response_millis: Some(12_000),
            ..base()
        };
        assert!(
            evaluate_card_rules(&slow)
                .iter()
                .any(|r| r.action == "redesign_card")
        );
        let fast = CardSignals {
            recent_grades: vec![2, 2, 3, 2],
            avg_response_millis: Some(3_000),
            ..base()
        };
        assert!(
            !evaluate_card_rules(&fast)
                .iter()
                .any(|r| r.action == "redesign_card")
        );
    }

    #[test]
    fn listening_failures_and_prompted_only_production() {
        let signals = CardSignals {
            recent_grades: vec![3],
            listening_failures: 2,
            prompted_successes: 3,
            spontaneous_successes: 0,
            ..base()
        };
        let actions: Vec<_> = evaluate_card_rules(&signals)
            .into_iter()
            .map(|r| r.action)
            .collect();
        assert!(actions.contains(&"create_listening_card"));
        assert!(actions.contains(&"create_transfer_target"));
    }

    #[test]
    fn low_utility_retirement_needs_volume_and_silence() {
        let signals = CardSignals {
            recent_grades: vec![3, 3, 4, 3, 3, 4, 3, 3, 4, 3],
            review_count: 12,
            ..base()
        };
        assert!(
            evaluate_card_rules(&signals)
                .iter()
                .any(|r| r.action == "consider_retirement")
        );
        let used = CardSignals {
            spontaneous_successes: 1,
            ..signals
        };
        assert!(
            !evaluate_card_rules(&used)
                .iter()
                .any(|r| r.action == "consider_retirement")
        );
    }

    #[test]
    fn escalation_thresholds_and_recheck() {
        assert!(error_escalation("et-1", 2).is_empty());
        assert_eq!(
            error_escalation("et-1", 3)[0].action,
            "activate_mini_target"
        );
        assert_eq!(
            error_escalation("et-1", 5)[0].action,
            "require_method_change"
        );
        assert_eq!(recheck_failed("et-1", 3).action, "practice_again");
    }

    #[test]
    fn deterministic_and_no_signals_no_noise() {
        let signals = CardSignals {
            recent_grades: vec![3, 3],
            ..base()
        };
        assert_eq!(evaluate_card_rules(&signals), evaluate_card_rules(&signals));
        assert!(evaluate_card_rules(&CardSignals::default()).is_empty());
    }
}
