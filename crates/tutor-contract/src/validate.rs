//! Bounds + cross-field validation for parsed lesson results
//! (07-tutor-contract.md §2–4). Pure; returns issues, never panics.

use crate::{CardMutation, LessonResult, MutationOperation, SCHEMA_VERSION, bounds};

/// One validation problem, addressable in the import preview (08-local-rpc.md §7).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ValidationIssue {
    /// Stable machine code, e.g. `too_long`, `missing_field`.
    pub code: &'static str,
    /// Slash path into the document, e.g. `card_mutations/3/card/prompt`.
    pub path: String,
    /// Human-readable explanation.
    pub message: String,
}

fn issue(issues: &mut Vec<ValidationIssue>, code: &'static str, path: String, message: String) {
    issues.push(ValidationIssue {
        code,
        path,
        message,
    });
}

/// Validates a parsed lesson result. Empty result = valid.
#[must_use]
pub fn validate(result: &LessonResult) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    header(result, &mut issues);
    collections(result, &mut issues);
    texts(result, &mut issues);
    item_references(result, &mut issues);
    measurement_coverage(result, &mut issues);
    interventions(result, &mut issues);
    mutations(&result.card_mutations, &mut issues);
    metrics(result, &mut issues);
    issues
}

fn header(r: &LessonResult, issues: &mut Vec<ValidationIssue>) {
    if r.schema_version != SCHEMA_VERSION {
        issue(
            issues,
            "unsupported_schema_version",
            "schema_version".to_owned(),
            format!("expected '{SCHEMA_VERSION}', got '{}'", r.schema_version),
        );
    }
    for (path, value) in [
        ("client_operation_id", &r.client_operation_id),
        ("language_profile_id", &r.language_profile_id),
        ("language_pack_id", &r.language_pack_id),
        ("language_pack_version", &r.language_pack_version),
    ] {
        if value.trim().is_empty() {
            issue(
                issues,
                "missing_field",
                path.to_owned(),
                "must be non-empty".to_owned(),
            );
        }
        if value.len() > bounds::MAX_ID {
            issue(
                issues,
                "too_long",
                path.to_owned(),
                format!("over {} chars", bounds::MAX_ID),
            );
        }
    }
    if r.lesson.scenario.trim().is_empty() {
        issue(
            issues,
            "missing_field",
            "lesson/scenario".to_owned(),
            "must be non-empty".to_owned(),
        );
    }
}

fn collections(r: &LessonResult, issues: &mut Vec<ValidationIssue>) {
    let caps = [
        (
            "new_learning_items",
            r.new_learning_items.len(),
            bounds::MAX_NEW_ITEMS,
        ),
        ("evidence", r.evidence.len(), bounds::MAX_EVIDENCE),
        (
            "interventions",
            r.interventions.len(),
            bounds::MAX_INTERVENTIONS,
        ),
        (
            "error_occurrences",
            r.error_occurrences.len(),
            bounds::MAX_ERROR_OCCURRENCES,
        ),
        (
            "listening_attempts",
            r.listening_attempts.len(),
            bounds::MAX_LISTENING_ATTEMPTS,
        ),
        (
            "mastery_changes",
            r.mastery_changes.len(),
            bounds::MAX_MASTERY_CHANGES,
        ),
        (
            "card_mutations",
            r.card_mutations.len(),
            bounds::MAX_CARD_MUTATIONS,
        ),
    ];
    for (path, len, max) in caps {
        if len > max {
            issue(
                issues,
                "over_limit",
                path.to_owned(),
                format!("{len} entries, max {max}"),
            );
        }
    }
}

fn texts(r: &LessonResult, issues: &mut Vec<ValidationIssue>) {
    for (i, item) in r.new_learning_items.iter().enumerate() {
        if item.target_text.trim().is_empty() {
            issue(
                issues,
                "missing_field",
                format!("new_learning_items/{i}/target_text"),
                "must be non-empty".to_owned(),
            );
        }
        short(
            issues,
            format!("new_learning_items/{i}/target_text"),
            &item.target_text,
        );
    }
    for (i, e) in r.error_occurrences.iter().enumerate() {
        short(
            issues,
            format!("error_occurrences/{i}/original_production"),
            &e.original_production,
        );
        short(
            issues,
            format!("error_occurrences/{i}/corrected_production"),
            &e.corrected_production,
        );
    }
    for (path, text) in [
        ("lesson/tutor_summary", &r.lesson.tutor_summary),
        ("lesson/next_goal", &r.lesson.next_goal),
    ] {
        if let Some(text) = text
            && text.len() > bounds::MAX_LONG_TEXT
        {
            issue(
                issues,
                "too_long",
                path.to_owned(),
                format!("over {} chars", bounds::MAX_LONG_TEXT),
            );
        }
    }
}

fn short(issues: &mut Vec<ValidationIssue>, path: String, text: &str) {
    if text.len() > bounds::MAX_SHORT_TEXT {
        issue(
            issues,
            "too_long",
            path,
            format!("over {} chars", bounds::MAX_SHORT_TEXT),
        );
    }
}

fn listening_metadata(
    index: usize,
    attempt: &crate::ListeningAttemptEntry,
    issues: &mut Vec<ValidationIssue>,
) {
    for (field, value) in [
        ("source_variant_id", attempt.source_variant_id.as_deref()),
        ("talker_key", attempt.talker_key.as_deref()),
        ("context_tag", attempt.context_tag.as_deref()),
    ] {
        if let Some(value) = value
            && (value.trim().is_empty() || value.len() > bounds::MAX_ID)
        {
            issue(
                issues,
                "invalid_field",
                format!("listening_attempts/{index}/{field}"),
                "must be non-empty and bounded when present".to_owned(),
            );
        }
    }
}

fn item_references(r: &LessonResult, issues: &mut Vec<ValidationIssue>) {
    let keys = collect_item_keys(r, issues);
    for (i, evidence) in r.evidence.iter().enumerate() {
        evidence_reference(i, evidence, &keys, issues);
        evidence_semantics(i, evidence, issues);
        evidence_measurements(i, evidence, issues);
    }
    for (i, attempt) in r.listening_attempts.iter().enumerate() {
        listening_metadata(i, attempt, issues);
        listening_reference(i, attempt, &keys, issues);
        listening_measurements(i, attempt, issues);
    }
}

fn collect_item_keys<'a>(r: &'a LessonResult, issues: &mut Vec<ValidationIssue>) -> Vec<&'a str> {
    let mut keys = Vec::new();
    for (i, item) in r.new_learning_items.iter().enumerate() {
        let Some(key) = item.client_item_key.as_deref() else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() || key.len() > bounds::MAX_ID {
            issue(
                issues,
                "invalid_field",
                format!("new_learning_items/{i}/client_item_key"),
                "must be non-empty and bounded".to_owned(),
            );
        } else if keys.contains(&key) {
            issue(
                issues,
                "duplicate_reference",
                format!("new_learning_items/{i}/client_item_key"),
                format!("duplicate client_item_key '{key}'"),
            );
        } else {
            keys.push(key);
        }
    }
    keys
}

fn evidence_reference(
    i: usize,
    evidence: &crate::EvidenceEntry,
    keys: &[&str],
    issues: &mut Vec<ValidationIssue>,
) {
    let id = evidence
        .learning_item_id
        .as_deref()
        .filter(|value| !value.trim().is_empty());
    let key = evidence
        .client_item_key
        .as_deref()
        .filter(|value| !value.trim().is_empty());
    if id.is_some() == key.is_some() {
        issue(
            issues,
            "invalid_reference",
            format!("evidence/{i}"),
            "exactly one of learning_item_id or client_item_key is required".to_owned(),
        );
    }
    if let Some(id) = id
        && id.len() > bounds::MAX_ID
    {
        issue(
            issues,
            "too_long",
            format!("evidence/{i}/learning_item_id"),
            format!("over {} chars", bounds::MAX_ID),
        );
    }
    if let Some(key) = key {
        if key.len() > bounds::MAX_ID {
            issue(
                issues,
                "too_long",
                format!("evidence/{i}/client_item_key"),
                format!("over {} chars", bounds::MAX_ID),
            );
        } else if !keys.contains(&key) {
            issue(
                issues,
                "unknown_reference",
                format!("evidence/{i}/client_item_key"),
                format!("unknown client_item_key '{key}'"),
            );
        }
    }
}

fn evidence_semantics(
    i: usize,
    evidence: &crate::EvidenceEntry,
    issues: &mut Vec<ValidationIssue>,
) {
    if let Some(context) = evidence.context.as_deref() {
        short(issues, format!("evidence/{i}/context"), context);
    }
    if let Some(family) = evidence.context_family.as_deref() {
        if family.trim().is_empty() {
            issue(
                issues,
                "invalid_field",
                format!("evidence/{i}/context_family"),
                "must be non-empty when provided".to_owned(),
            );
        }
        short(issues, format!("evidence/{i}/context_family"), family);
    }
    if matches!(evidence.kind, crate::EvidenceKind::Transfer)
        && evidence
            .context_family
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
    {
        issue(
            issues,
            "missing_field",
            format!("evidence/{i}/context_family"),
            "transfer evidence requires a semantic context_family".to_owned(),
        );
    }
    if matches!(
        evidence.kind,
        crate::EvidenceKind::SpontaneousProduction | crate::EvidenceKind::Transfer
    ) && evidence.assistance_level.is_some_and(|level| level > 0)
    {
        issue(
            issues,
            "inconsistent_evidence",
            format!("evidence/{i}/assistance_level"),
            "spontaneous/transfer evidence cannot use assisted production".to_owned(),
        );
    }
    if matches!(evidence.kind, crate::EvidenceKind::PromptedProduction)
        && evidence.assistance_level == Some(0)
    {
        issue(
            issues,
            "inconsistent_evidence",
            format!("evidence/{i}/assistance_level"),
            "prompted_production must not be marked unaided".to_owned(),
        );
    }
}

fn evidence_measurements(
    i: usize,
    evidence: &crate::EvidenceEntry,
    issues: &mut Vec<ValidationIssue>,
) {
    if evidence.assistance_level.is_some_and(|level| level > 3) {
        issue(
            issues,
            "invalid_field",
            format!("evidence/{i}/assistance_level"),
            "must be 0..=3".to_owned(),
        );
    }
    let measured_probe = !matches!(evidence.kind, crate::EvidenceKind::Correction);
    if measured_probe && evidence.assistance_level.is_none() {
        issue(
            issues,
            "missing_metric",
            format!("evidence/{i}/assistance_level"),
            "selected probes require observed assistance_level".to_owned(),
        );
    }
    if measured_probe && evidence.response_millis.is_none() {
        issue(
            issues,
            "missing_metric",
            format!("evidence/{i}/response_millis"),
            "selected probes require observed response_millis".to_owned(),
        );
    }
    if measured_probe && evidence.probe_phase.is_none() {
        issue(
            issues,
            "missing_metric",
            format!("evidence/{i}/probe_phase"),
            "selected probes require probe_phase".to_owned(),
        );
    }
    if measured_probe
        && evidence
            .context
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
    {
        issue(
            issues,
            "missing_field",
            format!("evidence/{i}/context"),
            "selected probes require an explicit context".to_owned(),
        );
    }
    pronunciation_measurements(i, evidence, issues);
    if evidence
        .response_millis
        .is_some_and(|value| value > 10 * 60 * 1000)
    {
        issue(
            issues,
            "invalid_field",
            format!("evidence/{i}/response_millis"),
            "must be <= 600000 ms".to_owned(),
        );
    }
}

fn pronunciation_measurements(
    i: usize,
    evidence: &crate::EvidenceEntry,
    issues: &mut Vec<ValidationIssue>,
) {
    let production_probe = matches!(
        evidence.kind,
        crate::EvidenceKind::PromptedProduction
            | crate::EvidenceKind::SpontaneousProduction
            | crate::EvidenceKind::Transfer
    );
    if production_probe && evidence.pronunciation_intelligible.is_none() {
        issue(
            issues,
            "missing_metric",
            format!("evidence/{i}/pronunciation_intelligible"),
            "production probes require pronunciation intelligibility".to_owned(),
        );
    }
    if production_probe && evidence.pronunciation_repeat_required.is_none() {
        issue(
            issues,
            "missing_metric",
            format!("evidence/{i}/pronunciation_repeat_required"),
            "production probes require whether pronunciation caused a repeat".to_owned(),
        );
    }
    if evidence.pronunciation_intelligible == Some(true)
        && evidence.pronunciation_repeat_required == Some(true)
    {
        issue(
            issues,
            "inconsistent_metric",
            format!("evidence/{i}/pronunciation_repeat_required"),
            "intelligible-without-repeat cannot also require a pronunciation repeat".to_owned(),
        );
    }
}

fn listening_reference(
    i: usize,
    attempt: &crate::ListeningAttemptEntry,
    keys: &[&str],
    issues: &mut Vec<ValidationIssue>,
) {
    let id = attempt
        .learning_item_id
        .as_deref()
        .filter(|value| !value.trim().is_empty());
    let key = attempt
        .client_item_key
        .as_deref()
        .filter(|value| !value.trim().is_empty());
    if id.is_some() && key.is_some() {
        issue(
            issues,
            "invalid_reference",
            format!("listening_attempts/{i}"),
            "use learning_item_id or client_item_key, not both".to_owned(),
        );
    }
    if let Some(key) = key
        && !keys.contains(&key)
    {
        issue(
            issues,
            "unknown_reference",
            format!("listening_attempts/{i}/client_item_key"),
            format!("unknown client_item_key '{key}'"),
        );
    }
}

fn listening_measurements(
    i: usize,
    attempt: &crate::ListeningAttemptEntry,
    issues: &mut Vec<ValidationIssue>,
) {
    if attempt.assistance_level.is_none() {
        issue(
            issues,
            "missing_metric",
            format!("listening_attempts/{i}/assistance_level"),
            "listening probes require observed assistance_level".to_owned(),
        );
    }
    if attempt.assistance_level.is_some_and(|level| level > 3) {
        issue(
            issues,
            "invalid_field",
            format!("listening_attempts/{i}/assistance_level"),
            "must be 0..=3".to_owned(),
        );
    }
    if attempt.response_millis.is_none() {
        issue(
            issues,
            "missing_metric",
            format!("listening_attempts/{i}/response_millis"),
            "listening probes require response_millis".to_owned(),
        );
    }
    if attempt
        .response_millis
        .is_some_and(|value| value > 10 * 60 * 1000)
    {
        issue(
            issues,
            "invalid_field",
            format!("listening_attempts/{i}/response_millis"),
            "must be <= 600000 ms".to_owned(),
        );
    }
    if attempt.probe_phase.is_none() {
        issue(
            issues,
            "missing_metric",
            format!("listening_attempts/{i}/probe_phase"),
            "listening probes require probe_phase".to_owned(),
        );
    }
    if attempt
        .context_tag
        .as_deref()
        .is_none_or(|value| value.trim().is_empty())
    {
        issue(
            issues,
            "missing_field",
            format!("listening_attempts/{i}/context_tag"),
            "listening probes require an explicit context_tag".to_owned(),
        );
    }
}

fn measurement_coverage(r: &LessonResult, issues: &mut Vec<ValidationIssue>) {
    let diagnostic_count = r
        .evidence
        .iter()
        .filter(|entry| entry.probe_phase == Some(crate::ProbePhase::Diagnostic))
        .count()
        + r.listening_attempts
            .iter()
            .filter(|entry| entry.probe_phase == Some(crate::ProbePhase::Diagnostic))
            .count();
    if diagnostic_count < 2 {
        issue(
            issues,
            "insufficient_measurement_coverage",
            "evidence".to_owned(),
            format!(
                "lesson completion requires at least 2 measured diagnostic probes, got {diagnostic_count}"
            ),
        );
    }
}

fn interventions(r: &LessonResult, issues: &mut Vec<ValidationIssue>) {
    let known_keys: Vec<&str> = r
        .new_learning_items
        .iter()
        .filter_map(|item| item.client_item_key.as_deref())
        .collect();

    for (i, intervention) in r.interventions.iter().enumerate() {
        let base = format!("interventions/{i}");
        let id = intervention
            .learning_item_id
            .as_deref()
            .filter(|value| !value.trim().is_empty());
        let key = intervention
            .client_item_key
            .as_deref()
            .filter(|value| !value.trim().is_empty());

        if intervention.action.trim().is_empty() || intervention.action.len() > bounds::MAX_ID {
            issue(
                issues,
                "invalid_field",
                format!("{base}/action"),
                "must be non-empty and bounded".to_owned(),
            );
        }
        if id.is_some() == key.is_some() {
            issue(
                issues,
                "invalid_reference",
                base.clone(),
                "exactly one of learning_item_id or client_item_key is required".to_owned(),
            );
        }
        if let Some(id) = id
            && id.len() > bounds::MAX_ID
        {
            issue(
                issues,
                "too_long",
                format!("{base}/learning_item_id"),
                format!("over {} chars", bounds::MAX_ID),
            );
        }
        if let Some(key) = key {
            if key.len() > bounds::MAX_ID {
                issue(
                    issues,
                    "too_long",
                    format!("{base}/client_item_key"),
                    format!("over {} chars", bounds::MAX_ID),
                );
            } else if !known_keys.contains(&key) {
                issue(
                    issues,
                    "unknown_reference",
                    format!("{base}/client_item_key"),
                    format!("unknown client_item_key '{key}'"),
                );
            }
        }
        if matches!(
            intervention.target_capability,
            crate::EvidenceKind::Correction
        ) {
            issue(
                issues,
                "invalid_field",
                format!("{base}/target_capability"),
                "correction is not an adaptive capability".to_owned(),
            );
        }
        if intervention.evidence_index.is_some() && intervention.listening_attempt_index.is_some() {
            issue(
                issues,
                "invalid_reference",
                base.clone(),
                "use at most one immediate outcome reference".to_owned(),
            );
            continue;
        }
        if let Some(index) = intervention.evidence_index.map(usize::from) {
            let Some(evidence) = r.evidence.get(index) else {
                issue(
                    issues,
                    "invalid_reference",
                    format!("{base}/evidence_index"),
                    "evidence_index is out of bounds".to_owned(),
                );
                continue;
            };
            if intervention.target_capability != evidence.kind {
                issue(
                    issues,
                    "inconsistent_intervention",
                    format!("{base}/target_capability"),
                    "target_capability must match referenced evidence kind".to_owned(),
                );
            }
            if evidence.learning_item_id.as_deref() != id
                || evidence.client_item_key.as_deref() != key
            {
                issue(
                    issues,
                    "inconsistent_intervention",
                    base.clone(),
                    "intervention item must match referenced evidence".to_owned(),
                );
            }
        }
        if let Some(index) = intervention.listening_attempt_index.map(usize::from) {
            let Some(attempt) = r.listening_attempts.get(index) else {
                issue(
                    issues,
                    "invalid_reference",
                    format!("{base}/listening_attempt_index"),
                    "listening_attempt_index is out of bounds".to_owned(),
                );
                continue;
            };
            if intervention.target_capability != crate::EvidenceKind::Listening {
                issue(
                    issues,
                    "inconsistent_intervention",
                    format!("{base}/target_capability"),
                    "listening outcome requires target_capability=listening".to_owned(),
                );
            }
            if attempt.learning_item_id.as_deref() != id
                || attempt.client_item_key.as_deref() != key
            {
                issue(
                    issues,
                    "inconsistent_intervention",
                    base,
                    "intervention item must match referenced listening attempt".to_owned(),
                );
            }
        }
    }
}

fn mutations(mutations: &[CardMutation], issues: &mut Vec<ValidationIssue>) {
    for (i, m) in mutations.iter().enumerate() {
        let base = format!("card_mutations/{i}");
        if m.client_operation_id.trim().is_empty() || m.client_operation_id.len() > bounds::MAX_ID {
            issue(
                issues,
                "invalid_field",
                format!("{base}/client_operation_id"),
                "must be non-empty and bounded".to_owned(),
            );
        }
        let needs_card = matches!(m.operation, MutationOperation::Create);
        let needs_card_id = !matches!(m.operation, MutationOperation::Create);
        if needs_card && m.card.is_none() {
            issue(
                issues,
                "missing_field",
                format!("{base}/card"),
                "create requires card".to_owned(),
            );
        }
        if needs_card_id && m.card_id.is_none() {
            issue(
                issues,
                "missing_field",
                format!("{base}/card_id"),
                format!("{:?} requires card_id", m.operation),
            );
        }
        if matches!(
            m.operation,
            MutationOperation::AddAcceptedAnswer | MutationOperation::RemoveAcceptedAnswer
        ) && m
            .accepted_answer
            .as_deref()
            .is_none_or(|a| a.trim().is_empty())
        {
            issue(
                issues,
                "missing_field",
                format!("{base}/accepted_answer"),
                "accepted-answer operations require accepted_answer".to_owned(),
            );
        }
        if let Some(card) = &m.card {
            card_payload(&base, card, issues);
        }
    }
}

fn card_payload(base: &str, card: &crate::CardPayload, issues: &mut Vec<ValidationIssue>) {
    for (field, value) in [
        ("prompt", &card.prompt),
        ("answer", &card.answer),
        ("creation_reason", &card.creation_reason),
    ] {
        if value.trim().is_empty() {
            issue(
                issues,
                "missing_field",
                format!("{base}/card/{field}"),
                "quality rules require non-empty value".to_owned(),
            );
        }
        short(issues, format!("{base}/card/{field}"), value);
    }
    if card.accepted_answers.len() > bounds::MAX_ACCEPTED_ANSWERS {
        issue(
            issues,
            "over_limit",
            format!("{base}/card/accepted_answers"),
            format!("max {}", bounds::MAX_ACCEPTED_ANSWERS),
        );
    }
    if card.scenario_tags.len() > bounds::MAX_TAGS || card.skill_tags.len() > bounds::MAX_TAGS {
        issue(
            issues,
            "over_limit",
            format!("{base}/card/tags"),
            format!("max {} per tag list", bounds::MAX_TAGS),
        );
    }
    if let Some(score) = card.priority_score
        && !(0.0..=1.0).contains(&score)
    {
        issue(
            issues,
            "invalid_field",
            format!("{base}/card/priority_score"),
            "must be within 0.0..=1.0".to_owned(),
        );
    }
}

fn metrics(r: &LessonResult, issues: &mut Vec<ValidationIssue>) {
    if let Some(metrics) = &r.metrics {
        if metrics.task_success_count > metrics.task_attempt_count {
            issue(
                issues,
                "invalid_field",
                "metrics/task_success_count".to_owned(),
                "cannot exceed task_attempt_count".to_owned(),
            );
        }
        match (metrics.task_attempt_count, metrics.task_success_rubric) {
            (0, Some(_)) => issue(
                issues,
                "invalid_field",
                "metrics/task_success_rubric".to_owned(),
                "must be absent when task_attempt_count is zero".to_owned(),
            ),
            (1.., None) => issue(
                issues,
                "missing_field",
                "metrics/task_success_rubric".to_owned(),
                "required when task_attempt_count is non-zero".to_owned(),
            ),
            _ => {}
        }
    }

    if let Some(m) = &r.metrics {
        for (path, value) in [
            ("metrics/listening_score", m.listening_score),
            ("metrics/confidence", m.confidence),
        ] {
            if let Some(v) = value
                && !(0.0..=1.0).contains(&v)
            {
                issue(
                    issues,
                    "invalid_field",
                    path.to_owned(),
                    "must be within 0.0..=1.0".to_owned(),
                );
            }
        }
    }
}
