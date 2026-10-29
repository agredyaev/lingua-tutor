use super::*;

pub(super) fn deepen_lesson_result(
    env: &Env,
    profile: &str,
    result: &mut tutor_contract::LessonResult,
    seed: u8,
) {
    use learning_core::{EvidenceKind, LearningItemStatus};
    let source = result.new_learning_items[0].clone();
    match seed % 14 {
        0 => {
            result.new_learning_items[0].target_text = "quiero agua".into();
            result
                .new_learning_items
                .push(tutor_contract::NewLearningItem {
                    client_item_key: Some("also-water".into()),
                    ..source
                });
        }
        1 => {
            result.evidence[0].learning_item_id = Some("missing".into());
        }
        2 => {
            result.lesson.lesson_id = Some("missing".into());
        }
        3 => {
            result.mastery_changes[0].new_status = LearningItemStatus::ProvisionallyMastered;
        }
        4 => {
            result.mastery_changes[0].new_status = LearningItemStatus::Mastered;
        }
        5 => {
            result.mastery_changes[0].new_status = LearningItemStatus::Introduced;
        }
        6 => {
            result.mastery_changes[0].new_status = LearningItemStatus::Learning;
        }
        7 => {
            result.evidence[0].kind = EvidenceKind::Transfer;
            result.evidence[0].context_family = Some("transport.ask_stop".into());
            result.interventions[0].target_capability = EvidenceKind::Transfer;
        }
        8 => {
            result.evidence[0].context_family = Some("unknown.family".into());
        }
        9 => {
            let target: Option<String> = env
                .connection()
                .expect("db")
                .query_row(
                    "SELECT id FROM error_targets WHERE learner_language_profile_id=?1 LIMIT 1",
                    [profile],
                    |r| r.get(0),
                )
                .optional()
                .expect("target query");
            result.error_occurrences[0].error_target_id = target;
        }
        10 => {
            result.error_occurrences[0].error_target_id = Some("missing".into());
        }
        11 => {
            let i = &mut result.interventions[0];
            i.learning_item_id = None;
            i.client_item_key = source.client_item_key;
            i.target_capability = EvidenceKind::Listening;
            i.evidence_index = None;
            i.listening_attempt_index = Some(0);
        }
        12 => {
            result.interventions.clear();
            result.mastery_changes.clear();
            result.listening_attempts[0].learning_item_id = Some("li-fuzz".into());
            result.listening_attempts[0].client_item_key = None;
        }
        _ => {
            result.language_pack_id = "unknown-pack".into();
        }
    }
}
