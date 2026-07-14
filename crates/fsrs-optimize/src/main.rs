//! Offline FSRS parameter optimizer (WBS 12.3; 05-review-engine.md §11).
//!
//! Usage: `fsrs-optimize export-full.json` (or `-` for stdin), where the file
//! is the `GET /api/v1/export/full` document. Prints optimized parameters as
//! JSON. Data gate: refuses to run under 400 effective review events — below
//! that the optimizer overfits and default parameters serve better.
//!
//! Never runs in the Worker: training is native-only compute; the Worker just
//! stores the resulting parameter set (scheduler_parameter_sets).

use std::io::Read as _;

const MIN_EVENTS: usize = 400;

fn fail(message: &str) -> ! {
    eprintln!("fsrs-optimize: {message}");
    std::process::exit(1)
}

fn main() {
    let arg = std::env::args().nth(1).unwrap_or_else(|| "-".to_owned());
    let text = if arg == "-" {
        let mut buffer = String::new();
        if std::io::stdin().read_to_string(&mut buffer).is_err() {
            fail("cannot read stdin");
        }
        buffer
    } else {
        match std::fs::read_to_string(&arg) {
            Ok(text) => text,
            Err(e) => fail(&format!("cannot read {arg}: {e}")),
        }
    };
    let Ok(export) = serde_json::from_str::<serde_json::Value>(&text) else {
        fail("input is not valid JSON (expected export/full document)");
    };
    let Some(events) = export["review_events"].as_array() else {
        fail("no review_events section — is this an export/full document?");
    };

    // Effective events only: drop undo compensations and their targets,
    // mirroring the Worker's replay semantics.
    let compensated: std::collections::BTreeSet<&str> = events
        .iter()
        .filter_map(|e| e["compensates_event_id"].as_str())
        .collect();
    #[derive(Clone)]
    struct Event {
        card_id: String,
        rating: u32,
        at: i64,
    }
    let mut effective: Vec<Event> = events
        .iter()
        .filter(|e| e["compensates_event_id"].is_null())
        .filter(|e| e["id"].as_str().is_some_and(|id| !compensated.contains(id)))
        .filter_map(|e| {
            Some(Event {
                card_id: e["card_id"].as_str()?.to_owned(),
                rating: u32::try_from(e["grade"].as_i64()?).ok()?,
                at: chrono::DateTime::parse_from_rfc3339(e["answered_at"].as_str()?)
                    .ok()?
                    .timestamp(),
            })
        })
        .collect();
    if effective.len() < MIN_EVENTS {
        fail(&format!(
            "data gate: {} effective events < {MIN_EVENTS} — keep default parameters for now",
            effective.len()
        ));
    }
    effective.sort_by(|a, b| a.card_id.cmp(&b.card_id).then(a.at.cmp(&b.at)));

    // Build cumulative per-card items: FSRS trains on every review prefix.
    let mut train_set: Vec<fsrs::FSRSItem> = Vec::new();
    let mut card_ids: Vec<i64> = Vec::new();
    let mut reviews: Vec<fsrs::FSRSReview> = Vec::new();
    let mut current_card: Option<(&str, i64, i64)> = None; // (card, first index, last at)
    let mut card_index: i64 = 0;
    for event in &effective {
        let (last_at, is_new_card) = match current_card {
            Some((card, _, last)) if card == event.card_id => (last, false),
            _ => (event.at, true),
        };
        if is_new_card {
            reviews.clear();
            card_index += 1;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let delta_t = if is_new_card {
            0
        } else {
            (((event.at - last_at).max(0)) / 86_400) as u32
        };
        reviews.push(fsrs::FSRSReview {
            rating: event.rating.clamp(1, 4),
            delta_t,
        });
        // A training item per prefix that ends in a long-term review.
        if reviews.len() > 1 {
            train_set.push(fsrs::FSRSItem {
                reviews: reviews.clone(),
            });
            card_ids.push(card_index);
        }
        current_card = Some((event.card_id.as_str(), 0, event.at));
    }
    if train_set.is_empty() {
        fail("no multi-review cards yet — nothing to optimize");
    }

    let parameters = match fsrs::compute_parameters(fsrs::ComputeParametersInput {
        train_set,
        card_ids: Some(card_ids),
        progress: None,
        enable_short_term: true,
        num_relearning_steps: None,
        training_config: None,
    }) {
        Ok(parameters) => parameters,
        Err(e) => fail(&format!("training failed: {e}")),
    };
    println!(
        "{}",
        serde_json::json!({
            "algorithm": "fsrs",
            "effective_events": effective.len(),
            "parameters": parameters,
            "apply": "PATCH /api/v1/language-profiles/{id} scheduler parameters \
                      (05-review-engine.md §11: bump parameter_set, never rewrite history)",
        })
    );
}
