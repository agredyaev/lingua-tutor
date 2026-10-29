//! Shared bounded card mutation passes used by lesson completion and standalone upsert.

use crate::Result;
use crate::repositories::to_rfc3339;
use rusqlite::{Connection, params};

use super::card_plan::{
    CardEffectKind, CardPlan, CardPlanError, CardPlanState, CardVerdict, add_existing_card_json,
    card_accepted_at, card_accepted_len, card_answer, card_hash_at, card_intent, card_plan_note,
    card_plan_source_index, card_plan_status, card_prompt, find_card, mark_existing_card_hash,
    new_card_plan_state, plan_card_mutations, set_card_content_hash,
};
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

use super::card_write::{
    insert_card, insert_card_mutation, insert_card_tag, update_accepted_answers,
    update_card_content, update_card_status, upsert_schedule_state,
};

const MAX_MUTATIONS: usize = tutor_contract::bounds::MAX_CARD_MUTATIONS;
const MAX_ACCEPTED_PLUS_ONE: usize = tutor_contract::bounds::MAX_ACCEPTED_ANSWERS + 1;

pub(crate) fn prepare_card_mutations(
    database: &Connection,
    profile_id: &str,
    mutations: &[tutor_contract::CardMutation],
    now: learning_core::Timestamp,
) -> Result<(CardPlanState, CardPlan)> {
    let remaining_activation_budget = super::card_budget::remaining_budget(
        database,
        profile_id,
        now,
        crate::repositories::cards::COUNT_ACTIVATED_TODAY,
    )?;

    let mut state = new_card_plan_state(remaining_activation_budget);
    for (index, mutation) in mutations.iter().enumerate() {
        if let Some(payload) = mutation.card.as_ref() {
            let hash = learning_core::card_content_hash(
                payload.intent,
                &payload.prompt,
                &payload.answer,
                &payload.accepted_answers,
            );
            set_card_content_hash(&mut state, index, &hash, false).map_err(card_plan_error)?;
        }
    }

    let hashes: Vec<&str> = (0..mutations.len())
        .filter_map(|index| card_hash_at(&state, index))
        .collect();
    if !hashes.is_empty() {
        let encoded = serde_json::to_string(&hashes)?;
        let mut statement = database.prepare(
            "SELECT DISTINCT content_hash FROM cards
             WHERE learner_language_profile_id=?1 AND status!='retired'
               AND content_hash IN (SELECT value FROM json_each(?2))",
        )?;
        let mut rows = statement.query(params![profile_id, encoded])?;
        while let Some(row) = rows.next()? {
            let hash: String = row.get(0)?;
            mark_existing_card_hash(&mut state, &hash);
        }
    }

    // Resolve all referenced cards in one read, scoped to the active profile.
    // A card from a different learner must never enter the mutation state.
    let ids: Vec<&str> = mutations
        .iter()
        .filter_map(|mutation| mutation.card_id.as_deref())
        .collect();
    if !ids.is_empty() {
        let encoded = serde_json::to_string(&ids)?;
        let mut statement = database.prepare(
            "SELECT id,status,intent,prompt,answer,accepted_answers_json
             FROM cards
             WHERE learner_language_profile_id=?1
               AND id IN (SELECT value FROM json_each(?2))
             ORDER BY id",
        )?;
        let mut rows = statement.query(params![profile_id, encoded])?;
        while let Some(row) = rows.next()? {
            let field = |index| -> Result<&str> {
                row.get_ref(index)?
                    .as_str()
                    .map_err(|error| crate::Error::RustError(error.to_string()))
            };
            add_existing_card_json(
                &mut state,
                field(0)?,
                field(1)?,
                field(2)?,
                field(3)?,
                field(4)?,
                field(5)?,
            )
            .map_err(card_plan_error)?;
        }
    }

    let plan = plan_card_mutations(mutations, &state).map_err(card_plan_error)?;
    Ok((state, plan))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_card_mutations(
    database: &Connection,
    state: &CardPlanState,
    plan: &CardPlan,
    mutations: &[tutor_contract::CardMutation],
    profile_id: &str,
    pack_id: &str,
    pack_version: &str,
    source_language: &str,
    target_locale: &str,
    source_lesson_id: Option<&str>,
    default_reason: &str,
    now: learning_core::Timestamp,
    at: &str,
) -> Result<Vec<serde_json::Value>> {
    let mut created_text = String::new();
    let mut created_ids = [TextRef::default(); MAX_MUTATIONS];
    let mut created_present = [false; MAX_MUTATIONS];
    let mut summary = Vec::with_capacity(plan.len);

    for i in 0..plan.len {
        let source_index = card_plan_source_index(plan, i);
        let source = &mutations[source_index];

        match plan.effect[i] {
            CardEffectKind::Create => {
                let payload = source
                    .card
                    .as_ref()
                    .ok_or_else(|| crate::Error::RustError("create without payload".to_owned()))?;
                let card_id = crate::api::new_id("crd");
                let hash = card_hash_at(state, source_index).ok_or_else(|| {
                    crate::Error::RustError(
                        "create payload hash missing after admission".to_owned(),
                    )
                })?;
                let accepted_json = serde_json::to_string(&payload.accepted_answers)?;

                insert_card(
                    database,
                    &card_id,
                    profile_id,
                    pack_id,
                    pack_version,
                    crate::repositories::card_intent_db(&payload.intent),
                    card_plan_status(plan, i),
                    &payload.prompt,
                    payload
                        .prompt_language
                        .as_deref()
                        .unwrap_or(source_language),
                    &payload.answer,
                    target_locale,
                    &accepted_json,
                    "exact",
                    payload.explanation.as_deref(),
                    None,
                    payload.source_learning_item_id.as_deref(),
                    payload.source_error_target_id.as_deref(),
                    source_lesson_id,
                    &payload.creation_reason,
                    payload.priority_score,
                    hash,
                    1,
                    None,
                    None,
                    "tutor",
                    at,
                )?;

                for tag in &payload.scenario_tags {
                    let normalized = tag.trim().to_lowercase();
                    insert_card_tag(database, &card_id, "scenario", &normalized)?;
                }
                for tag in &payload.skill_tags {
                    let normalized = tag.trim().to_lowercase();
                    insert_card_tag(database, &card_id, "skill", &normalized)?;
                }

                if card_plan_status(plan, i) == "new" {
                    let schedule = review_engine::initial_state(
                        review_engine::FSRS_ID,
                        review_engine::Timestamp(now.0),
                    )
                    .map_err(|error| crate::Error::RustError(error.to_string()))?;
                    let last_review_at = schedule
                        .last_review_at
                        .map(|value| to_rfc3339(learning_core::Timestamp(value.0)));
                    let due_at = schedule
                        .due_at
                        .map(|value| to_rfc3339(learning_core::Timestamp(value.0)));
                    let state_hash = review_engine::state_hash(&schedule);

                    upsert_schedule_state(
                        database,
                        &card_id,
                        &schedule.algorithm,
                        &schedule.algorithm_version,
                        schedule.parameter_set_id.as_deref(),
                        schedule.difficulty,
                        schedule.stability,
                        last_review_at.as_deref(),
                        due_at.as_deref(),
                        schedule.scheduled_interval_seconds,
                        schedule.lapse_count,
                        schedule.review_count,
                        schedule.state_version,
                        &state_hash,
                        at,
                    )?;
                }

                created_ids[i] = push_text(&mut created_text, &card_id);
                created_present[i] = true;
            }
            CardEffectKind::UpdateContent => {
                let payload = source
                    .card
                    .as_ref()
                    .ok_or_else(|| crate::Error::RustError("update without payload".to_owned()))?;
                let hash = learning_core::card_content_hash(
                    payload.intent,
                    &payload.prompt,
                    &payload.answer,
                    &payload.accepted_answers,
                );
                let accepted_json = serde_json::to_string(&payload.accepted_answers)?;
                update_card_content(
                    database,
                    source.card_id.as_deref().unwrap_or_default(),
                    &payload.prompt,
                    payload.explanation.as_deref(),
                    None,
                    &accepted_json,
                    &hash,
                    at,
                )?;
            }
            CardEffectKind::SetStatus => {
                update_card_status(
                    database,
                    source.card_id.as_deref().unwrap_or_default(),
                    card_plan_status(plan, i),
                    source.reason.as_deref(),
                    at,
                )?;
            }
            CardEffectKind::AddAcceptedAnswer | CardEffectKind::RemoveAcceptedAnswer => {
                let Some(card_index) =
                    find_card(state, source.card_id.as_deref().unwrap_or_default())
                else {
                    return Err(crate::Error::RustError(
                        "planned card target disappeared from bounded state".to_owned(),
                    ));
                };

                let mut accepted = [""; MAX_ACCEPTED_PLUS_ONE];
                let mut accepted_len = 0usize;
                for offset in 0..card_accepted_len(state, card_index) {
                    let candidate = card_accepted_at(state, card_index, offset);
                    if plan.effect[i] == CardEffectKind::RemoveAcceptedAnswer
                        && candidate.trim().eq_ignore_ascii_case(
                            source.accepted_answer.as_deref().unwrap_or_default().trim(),
                        )
                    {
                        continue;
                    }
                    accepted[accepted_len] = candidate;
                    accepted_len += 1;
                }
                if plan.effect[i] == CardEffectKind::AddAcceptedAnswer {
                    accepted[accepted_len] = source.accepted_answer.as_deref().unwrap_or_default();
                    accepted_len += 1;
                }

                let intent = card_intent(state, card_index).ok_or_else(|| {
                    crate::Error::RustError("stored card intent is invalid".to_owned())
                })?;
                let hash = learning_core::card_content_hash_refs(
                    intent,
                    card_prompt(state, card_index),
                    card_answer(state, card_index),
                    accepted[..accepted_len].iter().copied(),
                );
                let accepted_json = serde_json::to_string(&accepted[..accepted_len])?;
                update_accepted_answers(
                    database,
                    source.card_id.as_deref().unwrap_or_default(),
                    &accepted_json,
                    &hash,
                    at,
                )?;
            }
            CardEffectKind::None => {}
        }

        let audit_card_id = if created_present[i] {
            Some(text_at(&created_text, created_ids[i]))
        } else {
            source.card_id.as_deref()
        };
        let after_json = source
            .card
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        let reason = if !card_plan_note(plan, i).is_empty() {
            card_plan_note(plan, i)
        } else {
            source.reason.as_deref().unwrap_or(default_reason)
        };
        let verdict = match plan.verdict[i] {
            CardVerdict::Applied => "applied",
            CardVerdict::Rejected => "rejected",
            CardVerdict::Warned => "warned",
        };

        insert_card_mutation(
            database,
            &crate::api::new_id("cm"),
            audit_card_id,
            crate::repositories::mutation_operation_db(&source.operation),
            &source.client_operation_id,
            None,
            after_json.as_deref(),
            source_lesson_id,
            "tutor",
            reason,
            pack_id,
            pack_version,
            verdict,
            at,
        )?;

        summary.push(serde_json::json!({
            "client_operation_id": source.client_operation_id,
            "verdict": verdict,
            "note": if card_plan_note(plan, i).is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::Value::String(card_plan_note(plan, i).to_owned())
            },
            "card_id": audit_card_id,
        }));
    }

    Ok(summary)
}

pub(crate) fn card_plan_error(error: CardPlanError) -> crate::Error {
    crate::Error::operation(
        422,
        "card_mutation_rejected",
        format!("{error:?}"),
        serde_json::Value::Null,
    )
}

#[cfg(test)]
// Test fixtures intentionally fail fast and use compact tuple snapshots.
#[allow(clippy::unwrap_used, clippy::type_complexity)]
mod tests {
    use super::*;

    #[test]
    fn mutation_preparation_is_scoped_to_one_language_profile() {
        let path = std::env::temp_dir().join(format!(
            "lingua-card-profile-boundary-{}.sqlite3",
            uuid::Uuid::new_v4(),
        ));
        let db = crate::storage::open(&path).unwrap();
        db.execute_batch(
            "INSERT INTO language_packs
             (id,package_name,version,contract_version,target_language,target_locale,
              display_name,installed_at,updated_at)
             VALUES('es-ES','language-pack-es-es','0.2.0',1,'es','es-ES','Spanish',
                    '2026-10-10T00:00:00Z','2026-10-10T00:00:00Z');
             INSERT INTO learner_profile(id,created_at,updated_at)
             VALUES('learner-a','2026-10-10T00:00:00Z','2026-10-10T00:00:00Z'),
                   ('learner-b','2026-10-10T00:00:00Z','2026-10-10T00:00:00Z');
             INSERT INTO learner_language_profiles
             (id,learner_id,language_pack_id,source_language,target_locale,
              estimated_level,current_lesson_number,status,is_default,preferences_json,
              created_at,updated_at)
             VALUES
             ('profile-a','learner-a','es-ES','ru','es-ES','A1',0,'active',1,'{}',
              '2026-10-10T00:00:00Z','2026-10-10T00:00:00Z'),
             ('profile-b','learner-b','es-ES','ru','es-ES','A1',0,'active',1,'{}',
              '2026-10-10T00:00:00Z','2026-10-10T00:00:00Z');",
        )
        .unwrap();

        let payload = tutor_contract::CardPayload {
            intent: language_pack_api::CardIntent::Production,
            prompt: "Where?".to_owned(),
            answer: "¿Dónde?".to_owned(),
            accepted_answers: vec![],
            prompt_language: Some("ru".to_owned()),
            explanation: None,
            scenario_tags: vec![],
            skill_tags: vec![],
            source_learning_item_id: None,
            source_error_target_id: None,
            creation_reason: "lesson".to_owned(),
            priority_score: None,
        };
        let hash = learning_core::card_content_hash(
            payload.intent,
            &payload.prompt,
            &payload.answer,
            &payload.accepted_answers,
        );
        db.execute(
            "INSERT INTO cards
             (id,learner_language_profile_id,language_pack_id,language_pack_version,
              intent,status,prompt,prompt_language,answer,answer_language,
              accepted_answers_json,answer_evaluation_mode,creation_reason,content_hash,
              content_version,created_by,created_at,updated_at)
             VALUES('foreign','profile-b','es-ES','0.2.0','production','review',
                    ?1,'ru',?2,'es-ES','[]','exact','lesson',?3,1,'tutor',
                    '2026-10-10T00:00:00Z','2026-10-10T00:00:00Z')",
            params![payload.prompt, payload.answer, hash],
        )
        .unwrap();

        let mutations = [
            tutor_contract::CardMutation {
                operation: tutor_contract::MutationOperation::Suspend,
                client_operation_id: "foreign-suspend".to_owned(),
                card_id: Some("foreign".to_owned()),
                card: None,
                accepted_answer: None,
                reason: None,
            },
            tutor_contract::CardMutation {
                operation: tutor_contract::MutationOperation::Create,
                client_operation_id: "own-create".to_owned(),
                card_id: None,
                card: Some(payload),
                accepted_answer: None,
                reason: None,
            },
        ];

        let (_, plan) = prepare_card_mutations(
            &db,
            "profile-a",
            &mutations,
            learning_core::Timestamp(1_760_000_000),
        )
        .unwrap();
        assert_eq!(plan.verdict[0], CardVerdict::Rejected);
        assert_eq!(plan.effect[0], CardEffectKind::None);
        // Matching content in another profile must not shadow an authorized create.
        assert_ne!(plan.verdict[1], CardVerdict::Rejected);
        assert_eq!(plan.effect[1], CardEffectKind::Create);

        // The batched SQLite row must preserve borrowed UTF-8 text, JSON escapes
        // and accepted-answer order when building the bounded mutation state.
        db.execute(
            "INSERT INTO cards
             (id,learner_language_profile_id,language_pack_id,language_pack_version,
              intent,status,prompt,prompt_language,answer,answer_language,
              accepted_answers_json,answer_evaluation_mode,creation_reason,content_hash,
              content_version,created_by,created_at,updated_at)
             VALUES('own','profile-a','es-ES','0.2.0','production','review',
                    ?1,'ru',?2,'es-ES',?3,'exact','lesson','own-hash',1,'tutor',
                    '2026-10-10T00:00:00Z','2026-10-10T00:00:00Z')",
            params![
                "¿Dónde está?",
                "Respuesta",
                r#"["ni\u00f1o","line\nbreak","🚀"]"#
            ],
        )
        .unwrap();
        let own_mutations = [tutor_contract::CardMutation {
            operation: tutor_contract::MutationOperation::AddAcceptedAnswer,
            client_operation_id: "add-own-answer".to_owned(),
            card_id: Some("own".to_owned()),
            card: None,
            accepted_answer: Some("otra".to_owned()),
            reason: None,
        }];
        let (loaded, own_plan) = prepare_card_mutations(
            &db,
            "profile-a",
            &own_mutations,
            learning_core::Timestamp(1_760_000_000),
        )
        .unwrap();
        assert_eq!(own_plan.verdict[0], CardVerdict::Applied);
        let index = find_card(&loaded, "own").unwrap();
        assert_eq!(card_prompt(&loaded, index), "¿Dónde está?");
        assert_eq!(card_accepted_len(&loaded, index), 3);
        assert_eq!(card_accepted_at(&loaded, index, 0), "niño");
        assert_eq!(card_accepted_at(&loaded, index, 1), "line\nbreak");
        assert_eq!(card_accepted_at(&loaded, index, 2), "🚀");
        drop(db);
        let _ = std::fs::remove_file(path);
    }
}
