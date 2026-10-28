//! Scalar completed-lesson correction.

use crate::{Env, Result};
use rusqlite::params;

pub(crate) fn correct_lesson_record(
    env: &Env,
    lesson_id: &str,
    tutor_summary: Option<&str>,
    next_goal: Option<&str>,
    duration_seconds: Option<i64>,
) -> Result<serde_json::Value> {
    let duration = duration_seconds.filter(|duration| (0..=86_400).contains(duration));
    let database = env.connection()?;
    let changed = database.execute(
        crate::repositories::lessons::CORRECT_LESSON_RECORD,
        params![
            lesson_id,
            tutor_summary,
            next_goal,
            duration,
            crate::repositories::to_rfc3339(crate::api::now()),
        ],
    )?;
    if changed == 0 {
        return crate::api::operation_error(
            400,
            "invalid_arguments",
            "unknown lesson_id or lesson not completed",
        );
    }
    crate::json(&serde_json::json!({"status":"corrected"}))
}

#[cfg(test)]
// Test fixtures intentionally fail fast and use compact tuple snapshots.
#[allow(clippy::unwrap_used, clippy::type_complexity)]
mod tests {
    use super::*;
    use crate::Env;

    fn env(label: &str) -> (std::path::PathBuf, Env) {
        let path = std::env::temp_dir().join(format!(
            "lingua-correct-{label}-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let db = crate::storage::open(&path).unwrap();
        db.execute_batch(
            r#"INSERT INTO language_packs(id,package_name,version,contract_version,target_language,target_locale,display_name,installed_at,updated_at)
               VALUES('es-ES','language-pack-es-es','0.2.0',1,'es','es-ES','Spanish','a','a');
               INSERT INTO learner_profile(id,created_at,updated_at) VALUES('learner-1','a','a');
               INSERT INTO learner_language_profiles(id,learner_id,language_pack_id,source_language,target_locale,created_at,updated_at)
               VALUES('llp-1','learner-1','es-ES','ru','es-ES','a','a');
               INSERT INTO lessons(id,learner_language_profile_id,language_pack_id,language_pack_version,lesson_number,status,scenario,planned_goal,source,started_at,completed_at,created_at,updated_at)
               VALUES('lesson-1','llp-1','es-ES','0.2.0',1,'completed','x','g','import','a','a','a','a');"#
        ).unwrap();
        drop(db);
        (path.clone(), Env::new(path, "k".to_owned()))
    }

    fn snap(env: &Env) -> (Option<String>, Option<String>, Option<u32>) {
        env.connection()
            .unwrap()
            .query_row(
                "SELECT tutor_summary,next_goal,duration_seconds FROM lessons WHERE id='lesson-1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap()
    }

    #[test]
    fn correction_updates_only_mutable_lesson_summary_fields() {
        let (path, env) = env("new");
        correct_lesson_record(&env, "lesson-1", Some("sum"), Some("next"), Some(123)).unwrap();
        assert_eq!(
            snap(&env),
            (Some("sum".to_owned()), Some("next".to_owned()), Some(123))
        );
        let _ = std::fs::remove_file(path);
    }
}
