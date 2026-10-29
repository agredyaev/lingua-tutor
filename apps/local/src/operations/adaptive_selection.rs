//! Bounded task/focus selection shared by active lessons and read-only tutor output.
//! One request-local owner for text retained across individual item projections.
use crate::Result;
use serde::Serialize;

const MAX_TASKS: usize = 24;
const MAX_FOCUS: usize = 32;
const MAX_CONTEXTS: usize = adaptation::MAX_CAPABILITY_OBSERVATIONS;

#[derive(Clone, Copy, Default)]
struct TextRef {
    start: usize,
    len: usize,
}

fn push_text(text: &mut String, value: &str) -> TextRef {
    let start = text.len();
    text.push_str(value);
    TextRef {
        start,
        len: value.len(),
    }
}

fn text_at(text: &str, reference: TextRef) -> &str {
    &text[reference.start..reference.start + reference.len]
}

struct CandidateColumns {
    len: usize,
    task_type: [&'static str; MAX_TASKS],
    item_key: [u32; MAX_TASKS],
    learning_item_id: [TextRef; MAX_TASKS],
    target_text: [TextRef; MAX_TASKS],
    score: [u8; MAX_TASKS],
    rationale: [&'static str; MAX_TASKS],
    expected_evidence: [&'static str; MAX_TASKS],
}

fn new_candidate_columns() -> CandidateColumns {
    CandidateColumns {
        len: 0,
        task_type: [""; MAX_TASKS],
        item_key: [0; MAX_TASKS],
        learning_item_id: [TextRef::default(); MAX_TASKS],
        target_text: [TextRef::default(); MAX_TASKS],
        score: [0; MAX_TASKS],
        rationale: [""; MAX_TASKS],
        expected_evidence: [""; MAX_TASKS],
    }
}

fn copy_candidate_columns(data: &mut CandidateColumns, from: usize, to: usize) {
    data.task_type[to] = data.task_type[from];
    data.item_key[to] = data.item_key[from];
    data.learning_item_id[to] = data.learning_item_id[from];
    data.target_text[to] = data.target_text[from];
    data.score[to] = data.score[from];
    data.rationale[to] = data.rationale[from];
    data.expected_evidence[to] = data.expected_evidence[from];
}

struct FocusColumns {
    len: usize,
    item_key: [u32; MAX_FOCUS],
    learning_item_id: [TextRef; MAX_FOCUS],
    target_text: [TextRef; MAX_FOCUS],
    capability: [adaptation::CapabilityKind; MAX_FOCUS],
    stage: [u8; MAX_FOCUS],
    due_at_unix: [i64; MAX_FOCUS],
    due_now: [bool; MAX_FOCUS],
    context_count: [u8; MAX_FOCUS],
    family_count: [u8; MAX_FOCUS],
    contexts: [TextRef; MAX_FOCUS * MAX_CONTEXTS],
    families: [TextRef; MAX_FOCUS * MAX_CONTEXTS],
    weighted_success_rate: [Option<f64>; MAX_FOCUS],
    evidence_strength: [f64; MAX_FOCUS],
    distinct_contexts: [u16; MAX_FOCUS],
    distinct_families: [u16; MAX_FOCUS],
}

fn new_focus_columns() -> FocusColumns {
    FocusColumns {
        len: 0,
        item_key: [0; MAX_FOCUS],
        learning_item_id: [TextRef::default(); MAX_FOCUS],
        target_text: [TextRef::default(); MAX_FOCUS],
        capability: [adaptation::CapabilityKind::Recognition; MAX_FOCUS],
        stage: [0; MAX_FOCUS],
        due_at_unix: [0; MAX_FOCUS],
        due_now: [false; MAX_FOCUS],
        context_count: [0; MAX_FOCUS],
        family_count: [0; MAX_FOCUS],
        contexts: [TextRef::default(); MAX_FOCUS * MAX_CONTEXTS],
        families: [TextRef::default(); MAX_FOCUS * MAX_CONTEXTS],
        weighted_success_rate: [None; MAX_FOCUS],
        evidence_strength: [0.0; MAX_FOCUS],
        distinct_contexts: [0; MAX_FOCUS],
        distinct_families: [0; MAX_FOCUS],
    }
}

fn copy_focus_columns(data: &mut FocusColumns, from: usize, to: usize) {
    data.item_key[to] = data.item_key[from];
    data.learning_item_id[to] = data.learning_item_id[from];
    data.target_text[to] = data.target_text[from];
    data.capability[to] = data.capability[from];
    data.stage[to] = data.stage[from];
    data.due_at_unix[to] = data.due_at_unix[from];
    data.due_now[to] = data.due_now[from];
    data.context_count[to] = data.context_count[from];
    data.family_count[to] = data.family_count[from];
    data.weighted_success_rate[to] = data.weighted_success_rate[from];
    data.evidence_strength[to] = data.evidence_strength[from];
    data.distinct_contexts[to] = data.distinct_contexts[from];
    data.distinct_families[to] = data.distinct_families[from];
    for offset in 0..MAX_CONTEXTS {
        data.contexts[to * MAX_CONTEXTS + offset] = data.contexts[from * MAX_CONTEXTS + offset];
        data.families[to * MAX_CONTEXTS + offset] = data.families[from * MAX_CONTEXTS + offset];
    }
}

fn candidate_precedes(
    candidate: adaptation::TaskCandidateView<'_>,
    columns: &CandidateColumns,
    index: usize,
    text: &str,
) -> bool {
    candidate
        .score
        .cmp(&columns.score[index])
        .reverse()
        .then_with(|| {
            candidate
                .learning_item_id
                .cmp(text_at(text, columns.learning_item_id[index]))
        })
        .then_with(|| candidate.task_type.cmp(columns.task_type[index]))
        .is_lt()
}

fn insert_candidate(
    columns: &mut CandidateColumns,
    text: &mut String,
    item_key: u32,
    candidate: adaptation::TaskCandidateView<'_>,
    limit: usize,
) {
    let mut position = columns.len.min(limit);
    for index in 0..columns.len.min(limit) {
        if candidate_precedes(candidate, columns, index, text) {
            position = index;
            break;
        }
    }
    if columns.len == limit && position == limit {
        return;
    }

    let end = if columns.len < limit {
        columns.len
    } else {
        limit - 1
    };
    for index in (position..end).rev() {
        copy_candidate_columns(columns, index, index + 1);
    }

    columns.task_type[position] = candidate.task_type;
    columns.item_key[position] = item_key;
    columns.learning_item_id[position] = push_text(text, candidate.learning_item_id);
    columns.target_text[position] = push_text(text, candidate.target_text);
    columns.score[position] = candidate.score;
    columns.rationale[position] = candidate.rationale;
    columns.expected_evidence[position] = candidate.expected_evidence;
    if columns.len < limit {
        columns.len += 1;
    }
}

fn focus_precedes(
    thread: &adaptation::TutorFocusDecisionView<'_>,
    columns: &FocusColumns,
    index: usize,
    text: &str,
) -> bool {
    columns.due_now[index]
        .cmp(&thread.due_now)
        .then_with(|| thread.due_at_unix.cmp(&columns.due_at_unix[index]))
        .then_with(|| {
            thread
                .learning_item_id
                .cmp(text_at(text, columns.learning_item_id[index]))
        })
        .then_with(|| thread.capability.cmp(&columns.capability[index]))
        .is_lt()
}

fn insert_focus(
    columns: &mut FocusColumns,
    text: &mut String,
    item_key: u32,
    thread: adaptation::TutorFocusDecisionView<'_>,
    limit: usize,
) {
    let mut position = columns.len.min(limit);
    for index in 0..columns.len.min(limit) {
        if focus_precedes(&thread, columns, index, text) {
            position = index;
            break;
        }
    }
    if columns.len == limit && position == limit {
        return;
    }

    let end = if columns.len < limit {
        columns.len
    } else {
        limit - 1
    };
    for index in (position..end).rev() {
        copy_focus_columns(columns, index, index + 1);
    }

    columns.item_key[position] = item_key;
    columns.learning_item_id[position] = push_text(text, thread.learning_item_id);
    columns.target_text[position] = push_text(text, thread.target_text);
    columns.capability[position] = thread.capability;
    columns.stage[position] = thread.stage;
    columns.due_at_unix[position] = thread.due_at_unix;
    columns.due_now[position] = thread.due_now;

    let context_base = position * MAX_CONTEXTS;
    let mut context_count = 0usize;
    for index in 0..thread.estimate.successful_contexts.len().min(MAX_CONTEXTS) {
        if let Some(value) = thread.estimate.successful_contexts.get(index) {
            columns.contexts[context_base + context_count] = push_text(text, value);
            context_count += 1;
        }
    }
    columns.context_count[position] = u8::try_from(context_count).unwrap_or(u8::MAX);

    let mut family_count = 0usize;
    for index in 0..thread
        .estimate
        .successful_context_families
        .len()
        .min(MAX_CONTEXTS)
    {
        if let Some(value) = thread.estimate.successful_context_families.get(index) {
            columns.families[context_base + family_count] = push_text(text, value);
            family_count += 1;
        }
    }
    columns.family_count[position] = u8::try_from(family_count).unwrap_or(u8::MAX);
    columns.weighted_success_rate[position] = thread.estimate.weighted_success_rate;
    columns.evidence_strength[position] = thread.estimate.evidence_strength;
    columns.distinct_contexts[position] = thread.estimate.distinct_success_contexts;
    columns.distinct_families[position] = thread.estimate.distinct_success_families;

    if columns.len < limit {
        columns.len += 1;
    }
}

fn serialize_candidates(columns: &CandidateColumns, text: &str) -> Result<String> {
    #[derive(Serialize)]
    struct Candidate<'a> {
        task_type: &'static str,
        learning_item_id: &'a str,
        target_text: &'a str,
        score: u8,
        rationale: &'static str,
        expected_evidence: &'static str,
    }

    let mut output = String::from("[");
    for index in 0..columns.len {
        if index > 0 {
            output.push(',');
        }
        let value = Candidate {
            task_type: columns.task_type[index],
            learning_item_id: text_at(text, columns.learning_item_id[index]),
            target_text: text_at(text, columns.target_text[index]),
            score: columns.score[index],
            rationale: columns.rationale[index],
            expected_evidence: columns.expected_evidence[index],
        };
        output.push_str(&serde_json::to_string(&value)?);
    }
    output.push(']');
    Ok(output)
}

fn format_selected_focus(columns: &FocusColumns, text: &str, index: usize) -> (String, String) {
    let base = index * MAX_CONTEXTS;
    let mut contexts = [""; MAX_CONTEXTS];
    let mut families = [""; MAX_CONTEXTS];
    for (offset, slot) in contexts
        .iter_mut()
        .enumerate()
        .take(usize::from(columns.context_count[index]))
    {
        *slot = text_at(text, columns.contexts[base + offset]);
    }
    for (offset, slot) in families
        .iter_mut()
        .enumerate()
        .take(usize::from(columns.family_count[index]))
    {
        *slot = text_at(text, columns.families[base + offset]);
    }
    let policy = adaptation::format_focus_policy(
        columns.capability[index],
        &contexts[..usize::from(columns.context_count[index])],
        &families[..usize::from(columns.family_count[index])],
    );
    let rationale = adaptation::format_focus_rationale(
        columns.capability[index],
        columns.weighted_success_rate[index],
        columns.evidence_strength[index],
        columns.stage[index],
        columns.distinct_contexts[index],
        columns.distinct_families[index],
    );
    (policy, rationale)
}

fn serialize_focus(columns: &FocusColumns, text: &str) -> Result<String> {
    #[derive(Serialize)]
    struct Thread<'a> {
        learning_item_id: &'a str,
        target_text: &'a str,
        capability: adaptation::CapabilityKind,
        stage: u8,
        due_at_unix: i64,
        due_now: bool,
        successful_contexts: &'a [&'a str],
        successful_context_families: &'a [&'a str],
        next_context_policy: &'a str,
        rationale: &'a str,
    }

    let mut output = String::from("[");
    for index in 0..columns.len {
        if index > 0 {
            output.push(',');
        }
        let base = index * MAX_CONTEXTS;
        let context_count = usize::from(columns.context_count[index]);
        let family_count = usize::from(columns.family_count[index]);

        let mut contexts = [""; MAX_CONTEXTS];
        for (offset, slot) in contexts.iter_mut().enumerate().take(context_count) {
            *slot = text_at(text, columns.contexts[base + offset]);
        }
        let mut families = [""; MAX_CONTEXTS];
        for (offset, slot) in families.iter_mut().enumerate().take(family_count) {
            *slot = text_at(text, columns.families[base + offset]);
        }

        let (policy, rationale) = format_selected_focus(columns, text, index);
        let value = Thread {
            learning_item_id: text_at(text, columns.learning_item_id[index]),
            target_text: text_at(text, columns.target_text[index]),
            capability: columns.capability[index],
            stage: columns.stage[index],
            due_at_unix: columns.due_at_unix[index],
            due_now: columns.due_now[index],
            successful_contexts: &contexts[..context_count],
            successful_context_families: &families[..family_count],
            next_context_policy: &policy,
            rationale: &rationale,
        };
        output.push_str(&serde_json::to_string(&value)?);
    }
    output.push(']');
    Ok(output)
}

/// Retains only the top-K task/focus values required beyond one item callback.
pub(crate) struct Selection {
    text: String,
    candidates: CandidateColumns,
    focus: FocusColumns,
    candidate_limit: usize,
    focus_limit: usize,
    next_item_key: u32,
}
pub(crate) fn new_selection(candidate_limit: usize, focus_limit: usize) -> Selection {
    assert!(candidate_limit <= MAX_TASKS && focus_limit <= MAX_FOCUS);
    Selection {
        text: String::new(),
        candidates: new_candidate_columns(),
        focus: new_focus_columns(),
        candidate_limit,
        focus_limit,
        next_item_key: 0,
    }
}

pub(crate) fn observe_selection(
    selection: &mut Selection,
    view: adaptation::ItemCapabilityView<'_>,
    primary_objective: &str,
    now: i64,
) {
    let item_key = selection.next_item_key;
    selection.next_item_key = selection.next_item_key.saturating_add(1);
    adaptation::visit_task_candidates(view, primary_objective, |candidate| {
        insert_candidate(
            &mut selection.candidates,
            &mut selection.text,
            item_key,
            candidate,
            selection.candidate_limit,
        );
    });
    adaptation::visit_tutor_focus_decisions(view, now, |thread| {
        insert_focus(
            &mut selection.focus,
            &mut selection.text,
            item_key,
            thread,
            selection.focus_limit,
        );
    });
}

pub(crate) fn selection_serialize_candidates(selection: &Selection) -> Result<String> {
    serialize_candidates(&selection.candidates, &selection.text)
}
pub(crate) fn selection_serialize_focus(selection: &Selection) -> Result<String> {
    serialize_focus(&selection.focus, &selection.text)
}

#[derive(Clone, Copy)]
struct TaskChoice {
    focus: Option<usize>,
    candidate: Option<usize>,
    priority: u8,
    due_now: bool,
    item_key: u32,
    item_id: TextRef,
    capability: adaptation::CapabilityKind,
}
impl Default for TaskChoice {
    fn default() -> Self {
        Self {
            focus: None,
            candidate: None,
            priority: 0,
            due_now: false,
            item_key: 0,
            item_id: TextRef::default(),
            capability: adaptation::CapabilityKind::Recognition,
        }
    }
}

/// Build only the final tutor actions; avoid owned intermediate item,
/// candidate, and focus collections. Selection scratch remains index-only.
pub(crate) fn selection_tutor_tasks(
    selection: &Selection,
    limit: usize,
) -> Vec<adaptation::TutorTask> {
    let mut entries = [TaskChoice::default(); MAX_TASKS + MAX_FOCUS];
    let mut len = 0usize;
    for fi in 0..selection.focus.len {
        let item_key = selection.focus.item_key[fi];
        let capability = selection.focus.capability[fi];
        let mut best = None;
        for ci in 0..selection.candidates.len {
            if selection.candidates.item_key[ci] == item_key
                && adaptation::evidence_capability(selection.candidates.expected_evidence[ci])
                    == Some(capability)
                && best.is_none_or(|previous| {
                    selection.candidates.score[ci] >= selection.candidates.score[previous]
                })
            {
                best = Some(ci);
            }
        }
        let priority = best
            .map_or(adaptation::default_focus_score(capability), |index| {
                selection.candidates.score[index]
            })
            .saturating_add(if selection.focus.due_now[fi] { 8 } else { 0 })
            .min(100);

        entries[len] = TaskChoice {
            focus: Some(fi),
            candidate: best,
            priority,
            due_now: selection.focus.due_now[fi],
            item_key,
            item_id: selection.focus.learning_item_id[fi],
            capability,
        };
        len += 1;
    }

    for ci in 0..selection.candidates.len {
        let Some(capability) =
            adaptation::evidence_capability(selection.candidates.expected_evidence[ci])
        else {
            continue;
        };
        let item_key = selection.candidates.item_key[ci];
        if (0..selection.focus.len).any(|fi| {
            selection.focus.capability[fi] == capability && selection.focus.item_key[fi] == item_key
        }) {
            continue;
        }
        entries[len] = TaskChoice {
            focus: None,
            candidate: Some(ci),
            priority: selection.candidates.score[ci],
            due_now: false,
            item_key,
            item_id: selection.candidates.learning_item_id[ci],
            capability,
        };
        len += 1;
    }

    entries[..len].sort_by(|left, right| {
        right
            .due_now
            .cmp(&left.due_now)
            .then_with(|| right.priority.cmp(&left.priority))
            .then_with(|| left.item_key.cmp(&right.item_key))
            .then_with(|| left.capability.cmp(&right.capability))
            .then_with(|| {
                text_at(&selection.text, left.item_id).cmp(text_at(&selection.text, right.item_id))
            })
    });

    let mut output = Vec::with_capacity(len.min(limit));
    for entry in entries[..len.min(limit)].iter().copied() {
        let (action, id, target, rationale, due_at_unix, stage, contexts, families, policy) =
            if let Some(fi) = entry.focus {
                let (policy, rationale) =
                    format_selected_focus(&selection.focus, &selection.text, fi);
                let (action, reason) = entry.candidate.map_or_else(
                    || {
                        (
                            adaptation::default_action(entry.capability).to_owned(),
                            rationale.to_owned(),
                        )
                    },
                    |ci| {
                        (
                            selection.candidates.task_type[ci].to_owned(),
                            format!("{}; {}", rationale, selection.candidates.rationale[ci]),
                        )
                    },
                );
                let base = fi * MAX_CONTEXTS;
                let contexts = (0..usize::from(selection.focus.context_count[fi]))
                    .map(|offset| {
                        text_at(&selection.text, selection.focus.contexts[base + offset]).to_owned()
                    })
                    .collect();
                let families = (0..usize::from(selection.focus.family_count[fi]))
                    .map(|offset| {
                        text_at(&selection.text, selection.focus.families[base + offset]).to_owned()
                    })
                    .collect();
                (
                    action,
                    text_at(&selection.text, selection.focus.learning_item_id[fi]).to_owned(),
                    text_at(&selection.text, selection.focus.target_text[fi]).to_owned(),
                    reason,
                    Some(selection.focus.due_at_unix[fi]),
                    Some(selection.focus.stage[fi]),
                    contexts,
                    families,
                    Some(policy),
                )
            } else {
                let Some(ci) = entry.candidate else {
                    continue;
                };
                (
                    selection.candidates.task_type[ci].to_owned(),
                    text_at(&selection.text, selection.candidates.learning_item_id[ci]).to_owned(),
                    text_at(&selection.text, selection.candidates.target_text[ci]).to_owned(),
                    selection.candidates.rationale[ci].to_owned(),
                    None,
                    None,
                    Vec::new(),
                    Vec::new(),
                    None,
                )
            };
        output.push(adaptation::TutorTask {
            action,
            learning_item_id: id,
            target_text: target,
            capability: entry.capability,
            priority: entry.priority,
            due_now: entry.due_now,
            due_at_unix,
            stage,
            successful_contexts: contexts,
            successful_context_families: families,
            next_context_policy: policy,
            rationale,
            success_criterion: adaptation::criterion(entry.capability).to_owned(),
        });
    }
    output
}

#[cfg(test)]
// Test fixtures intentionally fail fast and use compact tuple snapshots.
#[allow(clippy::unwrap_used, clippy::type_complexity)]
mod tests {
    use super::*;

    fn item<'a>(id: &'a str, strength: f64) -> adaptation::ItemCapabilityStateFixed<'a> {
        let estimate = adaptation::CapabilityEstimateFixed {
            attempts: 2,
            successes: 2,
            weighted_success_rate: Some(0.90),
            unaided_success_rate: Some(0.90),
            latency_weighted_millis: Some(2000),
            qualified_success_streak: 1,
            distinct_success_contexts: 1,
            distinct_success_families: 1,
            evidence_strength: strength,
            last_observed_at_unix: Some(86_400),
            last_observed_success: Some(true),
            last_observed_qualified_success: Some(true),
            ..adaptation::CapabilityEstimateFixed::default()
        };
        adaptation::ItemCapabilityStateFixed {
            learning_item_id: id,
            target_text: "target",
            status: "learning",
            recognition: adaptation::CapabilityEstimateFixed::default(),
            listening: estimate,
            prompted_production: estimate,
            spontaneous_production: estimate,
            transfer: estimate,
        }
    }

    #[test]
    fn bounded_selection_is_deterministic_and_ranked() {
        let items = [item("li-z", 0.30), item("li-a", 0.70), item("li-b", 0.55)];
        let now = 2 * 86_400;
        let mut selection = new_selection(24, 32);
        for item in &items {
            observe_selection(
                &mut selection,
                adaptation::item_capability_view(item),
                "reduce_prompt_dependence",
                now,
            );
        }

        let candidates: serde_json::Value =
            serde_json::from_str(&selection_serialize_candidates(&selection).unwrap()).unwrap();
        let focuses: serde_json::Value =
            serde_json::from_str(&selection_serialize_focus(&selection).unwrap()).unwrap();
        let tasks = selection_tutor_tasks(&selection, 16);

        assert!(candidates.as_array().is_some_and(|rows| !rows.is_empty()));
        assert!(focuses.as_array().is_some_and(|rows| !rows.is_empty()));
        assert!(!tasks.is_empty());
        assert!(tasks.windows(2).all(|pair| {
            (pair[0].due_now && !pair[1].due_now)
                || (pair[0].due_now == pair[1].due_now && pair[0].priority >= pair[1].priority)
        }));

        let mut repeated = new_selection(24, 32);
        for item in &items {
            observe_selection(
                &mut repeated,
                adaptation::item_capability_view(item),
                "reduce_prompt_dependence",
                now,
            );
        }
        assert_eq!(
            selection_serialize_candidates(&selection).unwrap(),
            selection_serialize_candidates(&repeated).unwrap()
        );
        assert_eq!(
            selection_serialize_focus(&selection).unwrap(),
            selection_serialize_focus(&repeated).unwrap()
        );
        assert_eq!(tasks, selection_tutor_tasks(&repeated, 16));
    }
}
