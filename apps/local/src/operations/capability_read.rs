//! Batch capability evidence scan shared by lesson and read-only tutor operations.
//! SQLite is authoritative. One request-local scratch is reused for each item.
use crate::Result;
use crate::repositories::from_rfc3339;
use rusqlite::{Connection, params};

const MAX_ITEMS: usize = 200;
const MASTERED_REVALIDATION_SLOTS: usize = 16;
const MAX_EVIDENCE_PER_ITEM: usize = adaptation::MAX_CAPABILITY_OBSERVATIONS * 5;

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

struct EvidenceColumns {
    len: usize,
    capability: [adaptation::CapabilityKind; MAX_EVIDENCE_PER_ITEM],
    success: [bool; MAX_EVIDENCE_PER_ITEM],
    assistance_level: [u8; MAX_EVIDENCE_PER_ITEM],
    assistance_present: [bool; MAX_EVIDENCE_PER_ITEM],
    response_millis: [u32; MAX_EVIDENCE_PER_ITEM],
    response_present: [bool; MAX_EVIDENCE_PER_ITEM],
    context_key: [TextRef; MAX_EVIDENCE_PER_ITEM],
    context_key_present: [bool; MAX_EVIDENCE_PER_ITEM],
    context_family: [TextRef; MAX_EVIDENCE_PER_ITEM],
    context_family_present: [bool; MAX_EVIDENCE_PER_ITEM],
    episode_key: [TextRef; MAX_EVIDENCE_PER_ITEM],
    episode_key_present: [bool; MAX_EVIDENCE_PER_ITEM],
    recorded_at_unix: [i64; MAX_EVIDENCE_PER_ITEM],
    text: String,
}
fn empty_evidence_columns() -> EvidenceColumns {
    EvidenceColumns {
        len: 0,
        capability: [adaptation::CapabilityKind::Recognition; MAX_EVIDENCE_PER_ITEM],
        success: [false; MAX_EVIDENCE_PER_ITEM],
        assistance_level: [0; MAX_EVIDENCE_PER_ITEM],
        assistance_present: [false; MAX_EVIDENCE_PER_ITEM],
        response_millis: [0; MAX_EVIDENCE_PER_ITEM],
        response_present: [false; MAX_EVIDENCE_PER_ITEM],
        context_key: [TextRef::default(); MAX_EVIDENCE_PER_ITEM],
        context_key_present: [false; MAX_EVIDENCE_PER_ITEM],
        context_family: [TextRef::default(); MAX_EVIDENCE_PER_ITEM],
        context_family_present: [false; MAX_EVIDENCE_PER_ITEM],
        episode_key: [TextRef::default(); MAX_EVIDENCE_PER_ITEM],
        episode_key_present: [false; MAX_EVIDENCE_PER_ITEM],
        recorded_at_unix: [0; MAX_EVIDENCE_PER_ITEM],
        text: String::new(),
    }
}

fn clear_evidence_columns(data: &mut EvidenceColumns) {
    data.len = 0;
    data.text.clear();
}

fn append_evidence_row(data: &mut EvidenceColumns, row: &rusqlite::Row<'_>) -> Result<()> {
    let kind = match row.get_ref(4)? {
        rusqlite::types::ValueRef::Null => return Ok(()),
        rusqlite::types::ValueRef::Text(bytes) => {
            std::str::from_utf8(bytes).map_err(|err| crate::Error::RustError(err.to_string()))?
        }
        _ => return Err(crate::Error::RustError("non-text evidence kind".to_owned())),
    };
    let capability = match kind {
        "recognition" => adaptation::CapabilityKind::Recognition,
        "prompted_production" => adaptation::CapabilityKind::PromptedProduction,
        "spontaneous_production" => adaptation::CapabilityKind::SpontaneousProduction,
        "transfer" => adaptation::CapabilityKind::Transfer,
        "listening" => adaptation::CapabilityKind::Listening,
        _ => return Ok(()),
    };
    if data.len == MAX_EVIDENCE_PER_ITEM {
        return Err(crate::Error::RustError(
            "evidence capacity exceeded".to_owned(),
        ));
    }
    let i = data.len;
    // Only active slots are read; initialize optional fields on reuse.
    data.context_key_present[i] = false;
    data.context_family_present[i] = false;
    data.episode_key_present[i] = false;
    data.assistance_present[i] = false;
    data.response_present[i] = false;
    data.capability[i] = capability;
    data.success[i] = row.get::<_, u8>(5)? != 0;
    let metadata = match row.get_ref(6)? {
        rusqlite::types::ValueRef::Text(bytes) => {
            serde_json::from_slice::<serde_json::Value>(bytes).ok()
        }
        _ => None,
    };
    if let Some(level) = metadata
        .as_ref()
        .and_then(|v| v.get("assistance_level"))
        .and_then(serde_json::Value::as_u64)
        .and_then(|n| u8::try_from(n).ok())
    {
        data.assistance_present[i] = true;
        data.assistance_level[i] = level;
    }
    if let Some(millis) = metadata
        .as_ref()
        .and_then(|v| v.get("response_millis"))
        .and_then(serde_json::Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
    {
        data.response_present[i] = true;
        data.response_millis[i] = millis;
    }
    if let Some(v) = metadata
        .as_ref()
        .and_then(|v| v.get("context_key"))
        .and_then(serde_json::Value::as_str)
    {
        data.context_key_present[i] = true;
        data.context_key[i] = push_text(&mut data.text, v);
    }
    if let Some(v) = metadata
        .as_ref()
        .and_then(|v| v.get("context_family"))
        .and_then(serde_json::Value::as_str)
    {
        data.context_family_present[i] = true;
        data.context_family[i] = push_text(&mut data.text, v);
    }
    if let rusqlite::types::ValueRef::Text(bytes) = row.get_ref(8)? {
        let episode =
            std::str::from_utf8(bytes).map_err(|err| crate::Error::RustError(err.to_string()))?;
        if !episode.trim().is_empty() {
            data.episode_key_present[i] = true;
            data.episode_key[i] = push_text(&mut data.text, episode);
        }
    }
    let recorded_at = row
        .get_ref(7)?
        .as_str()
        .map_err(|e| crate::Error::RustError(e.to_string()))?;
    data.recorded_at_unix[i] = from_rfc3339(recorded_at)
        .map_err(|error| crate::Error::RustError(error.to_string()))?
        .0;
    data.len += 1;
    Ok(())
}

fn emit_one(
    item_text: &str,
    item_fields: &[TextRef; 3],
    data: &EvidenceColumns,
    now: i64,
    emit: &mut impl FnMut(&adaptation::ItemCapabilityStateFixed<'_>) -> Result<()>,
) -> Result<()> {
    let mut contexts = [None; MAX_EVIDENCE_PER_ITEM];
    let mut families = [None; MAX_EVIDENCE_PER_ITEM];
    let mut episodes = [None; MAX_EVIDENCE_PER_ITEM];
    for i in 0..data.len {
        if data.context_key_present[i] {
            contexts[i] = Some(text_at(&data.text, data.context_key[i]));
        }
        if data.context_family_present[i] {
            families[i] = Some(text_at(&data.text, data.context_family[i]));
        }
        if data.episode_key_present[i] {
            episodes[i] = Some(text_at(&data.text, data.episode_key[i]));
        }
    }
    let state = adaptation::project_item_capabilities_columns(
        text_at(item_text, item_fields[0]),
        text_at(item_text, item_fields[1]),
        text_at(item_text, item_fields[2]),
        data.len,
        &data.capability,
        &data.success,
        &data.assistance_level,
        &data.assistance_present,
        &data.response_millis,
        &data.response_present,
        &contexts,
        &families,
        &episodes,
        &data.recorded_at_unix,
        now,
    );
    emit(&state)
}

/// Read active items with latest 12 evidence rows per capability via one
/// SQLite SELECT. Emits even items with no observations. Views expire
/// when the callback returns; the caller must retain indices or output.
pub(crate) fn visit_item_capabilities(
    database: &Connection,
    profile_id: &str,
    now: i64,
    mut emit: impl FnMut(&adaptation::ItemCapabilityStateFixed<'_>) -> Result<()>,
) -> Result<usize> {
    let mut statement = database.prepare(
        "WITH operational_rows AS (
            SELECT e.learning_item_id,e.kind,e.success,e.recorded_at,
                   COALESCE(
                     CASE
                       WHEN e.success=0 THEN 0
                       WHEN NOT json_valid(e.metadata_json) THEN 0
                       WHEN e.kind IN ('recognition','listening','spontaneous_production')
                         THEN CAST(json_extract(e.metadata_json,'$.assistance_level') AS INTEGER)=0
                       WHEN e.kind='prompted_production'
                         THEN CAST(json_extract(e.metadata_json,'$.assistance_level') AS INTEGER)>0
                       WHEN e.kind='transfer'
                         THEN CAST(json_extract(e.metadata_json,'$.assistance_level') AS INTEGER)=0
                          AND TRIM(COALESCE(json_extract(e.metadata_json,'$.context_family'),''))!=''
                       ELSE 0
                     END,
                     0
                   ) AS qualified_success,
                   ROW_NUMBER() OVER (
                     PARTITION BY e.learning_item_id,e.kind
                     ORDER BY e.recorded_at DESC,e.observation_order DESC,e.rowid DESC
                   ) AS rn
            FROM learning_item_evidence e
            JOIN learning_items i ON i.id=e.learning_item_id
            WHERE i.learner_language_profile_id=?1
              AND i.status IN ('introduced','learning','transfer_pending',
                               'provisionally_mastered','mastered')
              AND e.kind != 'correction'
              AND CASE
                    WHEN json_valid(e.metadata_json)
                    THEN COALESCE(json_extract(e.metadata_json,'$.probe_phase'),'legacy') != 'practice'
                    ELSE 1
                  END
        ),
        operational_summary AS (
            SELECT learning_item_id,
                   MIN(CASE
                         WHEN kind IN ('listening','spontaneous_production','transfer')
                         THEN recorded_at
                       END) AS revalidation_activity_at,
                   SUM(CASE
                         WHEN kind IN ('listening','spontaneous_production','transfer')
                         THEN 1 ELSE 0
                       END) AS revalidation_capability_count,
                   MAX(CASE
                         WHEN kind IN ('listening','spontaneous_production','transfer')
                          AND qualified_success=0
                         THEN 1 ELSE 0
                       END) AS unresolved_failure
            FROM operational_rows
            WHERE rn=1
            GROUP BY learning_item_id
        ),
        item_activity AS (
            SELECT i.id,i.target_text,i.status,i.updated_at,
                   COALESCE(o.revalidation_activity_at,i.updated_at)
                     AS operational_activity_at,
                   CASE
                     WHEN COALESCE(o.revalidation_capability_count,0)<3 THEN 1
                     ELSE 0
                   END AS operational_missing_focus,
                   COALESCE(o.unresolved_failure,0) AS operational_unresolved_failure
            FROM learning_items i
            LEFT JOIN operational_summary o ON o.learning_item_id=i.id
            WHERE i.learner_language_profile_id=?1
              AND i.status IN ('introduced','learning','transfer_pending',
                               'provisionally_mastered','mastered')
        ),
        candidates AS (
            SELECT id,target_text,status,updated_at,operational_activity_at,
                   ROW_NUMBER() OVER (
                     PARTITION BY CASE WHEN status='mastered' THEN 1 ELSE 0 END
                     ORDER BY
                       CASE WHEN operational_unresolved_failure=1 THEN 0 ELSE 1 END ASC,
                       CASE
                         WHEN status='mastered' AND operational_missing_focus=1 THEN 0
                         WHEN status='mastered' THEN 1
                         ELSE 0
                       END ASC,
                       CASE WHEN status='mastered' THEN operational_activity_at END ASC,
                       CASE WHEN status!='mastered' THEN updated_at END DESC,
                       id ASC
                   ) AS class_ord
            FROM item_activity
        ),
        prioritized AS (
            SELECT id,target_text,status,class_ord,
                   CASE
                     WHEN status!='mastered' AND class_ord<=?2 THEN 0
                     WHEN status='mastered' AND class_ord<=?3 THEN 1
                     WHEN status!='mastered' THEN 2
                     ELSE 3
                   END AS band
            FROM candidates
        ),
        selected AS MATERIALIZED (
            SELECT id,target_text,status,
                   ROW_NUMBER() OVER (ORDER BY band,class_ord,id) AS ord
            FROM prioritized
            ORDER BY band,class_ord,id
            LIMIT ?4
        ),
        evidence_rows AS (
            SELECT e.learning_item_id,e.kind,e.success,e.metadata_json,
                   e.recorded_at,e.id,e.observation_order,e.lesson_id,
                   ROW_NUMBER() OVER (
                     PARTITION BY e.learning_item_id,e.kind,
                       CASE WHEN e.lesson_id IS NULL THEN e.id ELSE e.lesson_id END
                     ORDER BY e.recorded_at DESC,e.observation_order DESC,e.rowid DESC
                   ) AS episode_rn
            FROM learning_item_evidence e JOIN selected s ON s.id=e.learning_item_id
            WHERE e.kind != 'correction'
              AND CASE
                    WHEN json_valid(e.metadata_json)
                    THEN COALESCE(json_extract(e.metadata_json,'$.probe_phase'),'legacy') != 'practice'
                    ELSE 1
                  END
        ),
        evidence AS (
            SELECT learning_item_id,kind,success,metadata_json,recorded_at,id,
                   observation_order,lesson_id,
                   ROW_NUMBER() OVER (
                     PARTITION BY learning_item_id,kind
                     ORDER BY recorded_at DESC,observation_order DESC,id DESC
                   ) AS rn
            FROM evidence_rows
            WHERE episode_rn=1
        ),
        legacy_rows AS (
            SELECT l.learning_item_id,'listening' AS kind,l.success,
                   json_object(
                     'context_key',
                     CASE
                       WHEN l.source_variant_id IS NOT NULL
                         OR l.talker_key IS NOT NULL
                         OR l.context_tag IS NOT NULL
                       THEN 'variant=' || COALESCE(l.source_variant_id,'')
                         || '|talker=' || COALESCE(l.talker_key,'')
                         || '|context=' || COALESCE(l.context_tag,'')
                       ELSE NULL
                     END
                   ) AS metadata_json,l.recorded_at,l.id,0 AS observation_order,l.lesson_id,
                   ROW_NUMBER() OVER (
                     PARTITION BY l.learning_item_id,
                       CASE WHEN l.lesson_id IS NULL THEN l.id ELSE l.lesson_id END
                     ORDER BY l.recorded_at DESC,l.id DESC
                   ) AS episode_rn
            FROM listening_attempts l JOIN selected s ON s.id=l.learning_item_id
            WHERE NOT EXISTS (
                SELECT 1 FROM learning_item_evidence e
                WHERE e.learning_item_id=l.learning_item_id
                  AND e.lesson_id=l.lesson_id
                  AND e.kind='listening'
                  AND e.recorded_at=l.recorded_at
            )
        ),
        legacy_listening AS (
            SELECT learning_item_id,kind,success,metadata_json,recorded_at,id,
                   observation_order,lesson_id,
                   ROW_NUMBER() OVER (
                     PARTITION BY learning_item_id
                     ORDER BY recorded_at DESC,id DESC
                   ) AS rn
            FROM legacy_rows
            WHERE episode_rn=1
        ),
        sampled AS (
            SELECT learning_item_id,kind,success,metadata_json,recorded_at,id,observation_order,lesson_id
            FROM evidence WHERE rn<=12
            UNION ALL
            SELECT learning_item_id,kind,success,metadata_json,recorded_at,id,observation_order,lesson_id
            FROM legacy_listening WHERE rn<=12
        )
        SELECT s.ord,s.id,s.target_text,s.status,
               e.kind,e.success,e.metadata_json,e.recorded_at,e.lesson_id
        FROM selected s LEFT JOIN sampled e ON e.learning_item_id=s.id
        ORDER BY s.ord,e.recorded_at DESC,e.observation_order DESC,e.id DESC",
    )?;
    let active_slots = MAX_ITEMS.saturating_sub(MASTERED_REVALIDATION_SLOTS);
    let mut rows = statement.query(params![
        profile_id,
        i64::try_from(active_slots).unwrap_or(i64::MAX),
        i64::try_from(MASTERED_REVALIDATION_SLOTS).unwrap_or(i64::MAX),
        i64::try_from(MAX_ITEMS).unwrap_or(i64::MAX),
    ])?;
    let mut item_text = String::new();
    let mut item_fields = [TextRef::default(); 3];
    let mut has_item = false;
    let mut ordinal = 0_i64;
    let mut scratch = empty_evidence_columns();
    let mut count = 0;
    while let Some(row) = rows.next()? {
        let next_ordinal: i64 = row.get(0)?;
        if !has_item || next_ordinal != ordinal {
            if has_item {
                emit_one(&item_text, &item_fields, &scratch, now, &mut emit)?;
                count += 1;
                clear_evidence_columns(&mut scratch);
                item_text.clear();
            }
            ordinal = next_ordinal;
            for (field, column) in item_fields.iter_mut().zip(1..=3) {
                *field = push_text(
                    &mut item_text,
                    row.get_ref(column)?
                        .as_str()
                        .map_err(|e| crate::Error::RustError(e.to_string()))?,
                );
            }
            has_item = true;
        }
        append_evidence_row(&mut scratch, row)?;
    }
    if has_item {
        emit_one(&item_text, &item_fields, &scratch, now, &mut emit)?;
        count += 1;
    }
    debug_assert!(count <= MAX_ITEMS);
    Ok(count)
}

#[cfg(test)]
// Test fixtures intentionally fail fast and use compact tuple snapshots.
#[allow(clippy::unwrap_used, clippy::type_complexity)]
mod tests {
    use super::*;

    #[test]
    fn measured_listening_evidence_coexists_with_legacy_attempts_without_double_counting() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE learning_items(
                id TEXT PRIMARY KEY,
                learner_language_profile_id TEXT NOT NULL,
                target_text TEXT NOT NULL,
                status TEXT NOT NULL,
                updated_at TEXT NOT NULL
             );
             CREATE TABLE learning_item_evidence(
                id TEXT PRIMARY KEY,
                learning_item_id TEXT NOT NULL,
                lesson_id TEXT,
                kind TEXT NOT NULL,
                success INTEGER NOT NULL,
                metadata_json TEXT,
                recorded_at TEXT NOT NULL,
                observation_order INTEGER NOT NULL DEFAULT 0
             );
             CREATE TABLE listening_attempts(
                id TEXT PRIMARY KEY,
                learner_language_profile_id TEXT NOT NULL,
                learning_item_id TEXT,
                lesson_id TEXT,
                source_reference TEXT,
                source_variant_id TEXT,
                talker_key TEXT,
                context_tag TEXT,
                transcript TEXT,
                success INTEGER NOT NULL,
                recorded_at TEXT NOT NULL
             );
             INSERT INTO learning_items VALUES(
                'li-1','llp-1','hola','learning','2026-10-23T10:00:00Z'
             );
             INSERT INTO learning_item_evidence VALUES(
                'ev-1','li-1','les-1','listening',1,
                json_object('assistance_level',0,'response_millis',1200,
                            'context_key','variant=a|talker=|context=one'),
                '2026-10-23T10:01:00Z',0
             );
             INSERT INTO listening_attempts VALUES
                ('la-1','llp-1','li-1','les-1','audio-a','a',NULL,'one',NULL,1,'2026-10-23T10:01:00Z'),
                ('la-legacy','llp-1','li-1','les-old','audio-b','b',NULL,'two',NULL,1,'2026-10-22T10:01:00Z');",
        )
        .unwrap();

        let mut attempts = None;
        let mut distinct = None;
        let mut unaided = None;
        let mut latency = None;
        visit_item_capabilities(&db, "llp-1", 1_761_213_000, |item| {
            attempts = Some(item.listening.attempts);
            distinct = Some(item.listening.distinct_success_contexts);
            unaided = item.listening.unaided_success_rate;
            latency = item.listening.latency_weighted_millis;
            Ok(())
        })
        .unwrap();

        assert_eq!(attempts, Some(2));
        assert_eq!(distinct, Some(1));
        assert_eq!(unaided, Some(1.0));
        assert_eq!(latency, Some(1_200));
    }

    #[test]
    fn massed_recent_rows_do_not_evict_independent_episodes() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE learning_items(
                id TEXT PRIMARY KEY,
                learner_language_profile_id TEXT NOT NULL,
                target_text TEXT NOT NULL,
                status TEXT NOT NULL,
                updated_at TEXT NOT NULL
             );
             CREATE TABLE learning_item_evidence(
                id TEXT PRIMARY KEY,
                learning_item_id TEXT NOT NULL,
                lesson_id TEXT,
                kind TEXT NOT NULL,
                success INTEGER NOT NULL,
                metadata_json TEXT,
                recorded_at TEXT NOT NULL,
                observation_order INTEGER NOT NULL DEFAULT 0
             );
             CREATE TABLE listening_attempts(
                id TEXT PRIMARY KEY,
                learner_language_profile_id TEXT NOT NULL,
                learning_item_id TEXT,
                lesson_id TEXT,
                source_reference TEXT,
                source_variant_id TEXT,
                talker_key TEXT,
                context_tag TEXT,
                transcript TEXT,
                success INTEGER NOT NULL,
                recorded_at TEXT NOT NULL
             );
             INSERT INTO learning_items VALUES(
                'li-1','llp-1','hola','learning','2026-10-23T10:00:00Z'
             );
             INSERT INTO learning_item_evidence VALUES
                ('old-1','li-1','les-old-1','spontaneous_production',0,
                 json_object('assistance_level',0),'2026-10-20T10:00:00Z',0),
                ('old-2','li-1','les-old-2','spontaneous_production',0,
                 json_object('assistance_level',0),'2026-10-21T10:00:00Z',0);",
        )
        .unwrap();
        for index in 0..12 {
            db.execute(
                "INSERT INTO learning_item_evidence
                 (id,learning_item_id,lesson_id,kind,success,metadata_json,recorded_at,observation_order)
                 VALUES(?1,'li-1','les-new','spontaneous_production',1,
                        json_object('assistance_level',0),'2026-10-23T10:00:00Z',?2)",
                params![format!("new-{index}"), index],
            )
            .unwrap();
        }

        let mut episodes = None;
        let mut rate = None;
        visit_item_capabilities(&db, "llp-1", 1_761_213_000, |item| {
            episodes = Some(item.spontaneous_production.independent_evidence_episodes);
            rate = item.spontaneous_production.weighted_success_rate;
            Ok(())
        })
        .unwrap();

        assert_eq!(episodes, Some(3));
        assert!(rate.is_some_and(|value| value < 0.50));
    }

    #[test]
    fn practice_success_does_not_erase_fresh_diagnostic_failure() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE learning_items(
                id TEXT PRIMARY KEY,
                learner_language_profile_id TEXT NOT NULL,
                target_text TEXT NOT NULL,
                status TEXT NOT NULL,
                updated_at TEXT NOT NULL
             );
             CREATE TABLE learning_item_evidence(
                id TEXT PRIMARY KEY,
                learning_item_id TEXT NOT NULL,
                lesson_id TEXT,
                kind TEXT NOT NULL,
                success INTEGER NOT NULL,
                metadata_json TEXT,
                recorded_at TEXT NOT NULL,
                observation_order INTEGER NOT NULL DEFAULT 0
             );
             CREATE TABLE listening_attempts(
                id TEXT PRIMARY KEY,
                learner_language_profile_id TEXT NOT NULL,
                learning_item_id TEXT,
                lesson_id TEXT,
                source_reference TEXT,
                source_variant_id TEXT,
                talker_key TEXT,
                context_tag TEXT,
                transcript TEXT,
                success INTEGER NOT NULL,
                recorded_at TEXT NOT NULL
             );
             INSERT INTO learning_items VALUES(
                'li-1','llp-1','hola','learning','2026-10-23T10:10:00Z'
             );
             INSERT INTO learning_item_evidence VALUES
                ('old-1','li-1','les-old-1','spontaneous_production',1,
                 json_object('assistance_level',0,'probe_phase','diagnostic'),
                 '2026-10-20T10:00:00Z',0),
                ('old-2','li-1','les-old-2','spontaneous_production',1,
                 json_object('assistance_level',0,'probe_phase','delayed_retrieval'),
                 '2026-10-21T10:00:00Z',0),
                ('current-fail','li-1','les-current','spontaneous_production',0,
                 json_object('assistance_level',0,'probe_phase','diagnostic'),
                 '2026-10-23T10:00:00Z',0),
                ('current-practice','li-1','les-current','spontaneous_production',1,
                 json_object('assistance_level',0,'probe_phase','practice'),
                 '2026-10-23T10:05:00Z',1);",
        )
        .unwrap();

        let mut latest = None;
        let mut streak = None;
        visit_item_capabilities(&db, "llp-1", 1_761_213_000, |item| {
            latest = item.spontaneous_production.last_observed_success;
            streak = Some(item.spontaneous_production.qualified_success_streak);
            Ok(())
        })
        .unwrap();

        assert_eq!(latest, Some(false));
        assert_eq!(streak, Some(0));
    }

    #[test]
    fn failed_active_item_is_not_starved_by_newer_active_items() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE learning_items(
                id TEXT PRIMARY KEY,
                learner_language_profile_id TEXT NOT NULL,
                target_text TEXT NOT NULL,
                status TEXT NOT NULL,
                updated_at TEXT NOT NULL
             );
             CREATE TABLE learning_item_evidence(
                id TEXT PRIMARY KEY,
                learning_item_id TEXT NOT NULL,
                lesson_id TEXT,
                kind TEXT NOT NULL,
                success INTEGER NOT NULL,
                metadata_json TEXT,
                recorded_at TEXT NOT NULL,
                observation_order INTEGER NOT NULL DEFAULT 0
             );
             CREATE INDEX ix_evidence_item ON learning_item_evidence(learning_item_id, kind);
             CREATE TABLE listening_attempts(
                id TEXT PRIMARY KEY,
                learner_language_profile_id TEXT NOT NULL,
                learning_item_id TEXT,
                lesson_id TEXT,
                source_reference TEXT,
                source_variant_id TEXT,
                talker_key TEXT,
                context_tag TEXT,
                transcript TEXT,
                success INTEGER NOT NULL,
                recorded_at TEXT NOT NULL
             );
             INSERT INTO learning_items VALUES
                ('learning-failed','llp-1','failed active','learning','2025-01-01T00:00:00Z');
             INSERT INTO learning_item_evidence VALUES(
                'ev-failed','learning-failed','les-current','spontaneous_production',0,
                json_object('assistance_level',0,'probe_phase','diagnostic'),
                '2026-11-06T00:00:00Z',0
             );",
        )
        .unwrap();
        for index in 0..184 {
            db.execute(
                "INSERT INTO learning_items
                 VALUES(?1,'llp-1',?2,'learning','2026-11-06T00:00:00Z')",
                params![format!("learning-new-{index:03}"), format!("item {index}")],
            )
            .unwrap();
        }
        for index in 0..16 {
            db.execute(
                "INSERT INTO learning_items
                 VALUES(?1,'llp-1',?2,'mastered',?3)",
                params![
                    format!("mastered-{index:02}"),
                    format!("mastered {index}"),
                    format!("2026-01-{:02}T00:00:00Z", index + 1),
                ],
            )
            .unwrap();
        }

        let mut saw_failed = false;
        let count = visit_item_capabilities(&db, "llp-1", 1_762_435_200, |item| {
            saw_failed |= item.learning_item_id == "learning-failed";
            Ok(())
        })
        .unwrap();

        assert_eq!(count, MAX_ITEMS);
        assert!(
            saw_failed,
            "fresh failure on an active item must outrank newer unfailed active items"
        );
    }

    #[test]
    fn missing_focus_capability_is_stale_even_when_item_row_is_recent() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE learning_items(
                id TEXT PRIMARY KEY,
                learner_language_profile_id TEXT NOT NULL,
                target_text TEXT NOT NULL,
                status TEXT NOT NULL,
                updated_at TEXT NOT NULL
             );
             CREATE TABLE learning_item_evidence(
                id TEXT PRIMARY KEY,
                learning_item_id TEXT NOT NULL,
                lesson_id TEXT,
                kind TEXT NOT NULL,
                success INTEGER NOT NULL,
                metadata_json TEXT,
                recorded_at TEXT NOT NULL,
                observation_order INTEGER NOT NULL DEFAULT 0
             );
             CREATE TABLE listening_attempts(
                id TEXT PRIMARY KEY,
                learner_language_profile_id TEXT NOT NULL,
                learning_item_id TEXT,
                lesson_id TEXT,
                source_reference TEXT,
                source_variant_id TEXT,
                talker_key TEXT,
                context_tag TEXT,
                transcript TEXT,
                success INTEGER NOT NULL,
                recorded_at TEXT NOT NULL
             );
             INSERT INTO learning_items VALUES
                ('missing-transfer','llp-1','missing transfer','mastered','2026-11-06T12:00:00Z');
             INSERT INTO learning_item_evidence VALUES
                ('rec','missing-transfer','les-r','recognition',1,
                 json_object('assistance_level',0,'probe_phase','diagnostic'),
                 '2026-11-06T11:00:00Z',0),
                ('listen','missing-transfer','les-l','listening',1,
                 json_object('assistance_level',0,'probe_phase','diagnostic'),
                 '2026-11-06T11:05:00Z',0),
                ('prod','missing-transfer','les-p','spontaneous_production',1,
                 json_object('assistance_level',0,'probe_phase','diagnostic'),
                 '2026-11-06T11:10:00Z',0);",
        )
        .unwrap();
        for index in 0..184 {
            db.execute(
                "INSERT INTO learning_items
                 VALUES(?1,'llp-1',?2,'learning','2026-11-06T12:00:00Z')",
                params![format!("learning-{index:03}"), format!("item {index}")],
            )
            .unwrap();
        }
        for index in 0..16 {
            let id = format!("complete-{index:02}");
            db.execute(
                "INSERT INTO learning_items VALUES(?1,'llp-1',?2,'mastered',?3)",
                params![
                    id,
                    format!("complete {index}"),
                    format!("2026-01-{:02}T00:00:00Z", index + 1),
                ],
            )
            .unwrap();
            for (kind_index, kind) in ["listening", "spontaneous_production", "transfer"]
                .iter()
                .enumerate()
            {
                let metadata = if *kind == "transfer" {
                    r#"{"assistance_level":0,"probe_phase":"transfer","context_family":"family"}"#
                } else {
                    r#"{"assistance_level":0,"probe_phase":"diagnostic"}"#
                };
                db.execute(
                    "INSERT INTO learning_item_evidence
                     VALUES(?1,?2,?3,?4,1,?5,?6,0)",
                    params![
                        format!("ev-{index}-{kind_index}"),
                        format!("complete-{index:02}"),
                        format!("les-{index}-{kind_index}"),
                        *kind,
                        metadata,
                        format!("2026-01-{:02}T00:00:00Z", index + 1),
                    ],
                )
                .unwrap();
            }
        }

        let mut saw_missing = false;
        visit_item_capabilities(&db, "llp-1", 1_762_435_200, |item| {
            saw_missing |= item.learning_item_id == "missing-transfer";
            Ok(())
        })
        .unwrap();

        assert!(
            saw_missing,
            "a mastered item missing transfer evidence must be treated as stale regardless of row updated_at"
        );
    }

    #[test]
    fn stale_mastered_item_gets_revalidation_slot_at_capacity() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE learning_items(
                id TEXT PRIMARY KEY,
                learner_language_profile_id TEXT NOT NULL,
                target_text TEXT NOT NULL,
                status TEXT NOT NULL,
                updated_at TEXT NOT NULL
             );
             CREATE TABLE learning_item_evidence(
                id TEXT PRIMARY KEY,
                learning_item_id TEXT NOT NULL,
                lesson_id TEXT,
                kind TEXT NOT NULL,
                success INTEGER NOT NULL,
                metadata_json TEXT,
                recorded_at TEXT NOT NULL,
                observation_order INTEGER NOT NULL DEFAULT 0
             );
             CREATE TABLE listening_attempts(
                id TEXT PRIMARY KEY,
                learner_language_profile_id TEXT NOT NULL,
                learning_item_id TEXT,
                lesson_id TEXT,
                source_reference TEXT,
                source_variant_id TEXT,
                talker_key TEXT,
                context_tag TEXT,
                transcript TEXT,
                success INTEGER NOT NULL,
                recorded_at TEXT NOT NULL
             );
             INSERT INTO learning_items VALUES
                ('mastered-old','llp-1','old mastered','mastered','2025-01-01T00:00:00Z');",
        )
        .unwrap();
        for index in 0..184 {
            db.execute(
                "INSERT INTO learning_items
                 VALUES(?1,'llp-1',?2,'learning','2026-11-06T00:00:00Z')",
                params![format!("learning-{index:03}"), format!("item {index}")],
            )
            .unwrap();
        }
        for index in 0..16 {
            db.execute(
                "INSERT INTO learning_items
                 VALUES(?1,'llp-1',?2,'mastered',?3)",
                params![
                    format!("mastered-new-{index:02}"),
                    format!("new mastered {index}"),
                    format!("2026-10-{:02}T00:00:00Z", index + 1),
                ],
            )
            .unwrap();
        }

        let mut saw_old = false;
        let mut saw_freshest = false;
        let count = visit_item_capabilities(&db, "llp-1", 1_762_435_200, |item| {
            saw_old |= item.learning_item_id == "mastered-old";
            saw_freshest |= item.learning_item_id == "mastered-new-15";
            Ok(())
        })
        .unwrap();

        assert_eq!(count, MAX_ITEMS);
        assert!(
            saw_old,
            "stalest mastered item must receive a reserved revalidation slot"
        );
        assert!(
            !saw_freshest,
            "fresh mastered item should not displace the stalest mastered history"
        );
    }

    #[test]
    fn later_success_in_other_capability_does_not_mask_mastered_failure() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE learning_items(
                id TEXT PRIMARY KEY,
                learner_language_profile_id TEXT NOT NULL,
                target_text TEXT NOT NULL,
                status TEXT NOT NULL,
                updated_at TEXT NOT NULL
             );
             CREATE TABLE learning_item_evidence(
                id TEXT PRIMARY KEY,
                learning_item_id TEXT NOT NULL,
                lesson_id TEXT,
                kind TEXT NOT NULL,
                success INTEGER NOT NULL,
                metadata_json TEXT,
                recorded_at TEXT NOT NULL,
                observation_order INTEGER NOT NULL DEFAULT 0
             );
             CREATE INDEX ix_evidence_item ON learning_item_evidence(learning_item_id, kind);
             CREATE TABLE listening_attempts(
                id TEXT PRIMARY KEY,
                learner_language_profile_id TEXT NOT NULL,
                learning_item_id TEXT,
                lesson_id TEXT,
                source_reference TEXT,
                source_variant_id TEXT,
                talker_key TEXT,
                context_tag TEXT,
                transcript TEXT,
                success INTEGER NOT NULL,
                recorded_at TEXT NOT NULL
             );
             INSERT INTO learning_items VALUES
                ('mastered-mixed','llp-1','mixed mastered','mastered','2025-01-01T00:00:00Z');
             INSERT INTO learning_item_evidence VALUES
                ('ev-production-fail','mastered-mixed','les-current','spontaneous_production',0,
                 json_object('assistance_level',0,'probe_phase','diagnostic'),
                 '2026-11-06T10:00:00Z',0),
                ('ev-recognition-success','mastered-mixed','les-current','recognition',1,
                 json_object('assistance_level',0,'probe_phase','diagnostic'),
                 '2026-11-06T10:05:00Z',1);",
        )
        .unwrap();
        for index in 0..184 {
            db.execute(
                "INSERT INTO learning_items
                 VALUES(?1,'llp-1',?2,'learning','2026-11-06T00:00:00Z')",
                params![format!("learning-{index:03}"), format!("item {index}")],
            )
            .unwrap();
        }
        for index in 0..16 {
            db.execute(
                "INSERT INTO learning_items
                 VALUES(?1,'llp-1',?2,'mastered',?3)",
                params![
                    format!("mastered-old-{index:02}"),
                    format!("old mastered {index}"),
                    format!("2026-01-{:02}T00:00:00Z", index + 1),
                ],
            )
            .unwrap();
        }

        let mut saw_mixed = false;
        visit_item_capabilities(&db, "llp-1", 1_762_435_200, |item| {
            saw_mixed |= item.learning_item_id == "mastered-mixed";
            Ok(())
        })
        .unwrap();

        assert!(
            saw_mixed,
            "a later success in another capability must not hide an unresolved mastered failure"
        );
    }

    #[test]
    fn failed_mastered_revalidation_stays_in_active_selection() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE learning_items(
                id TEXT PRIMARY KEY,
                learner_language_profile_id TEXT NOT NULL,
                target_text TEXT NOT NULL,
                status TEXT NOT NULL,
                updated_at TEXT NOT NULL
             );
             CREATE TABLE learning_item_evidence(
                id TEXT PRIMARY KEY,
                learning_item_id TEXT NOT NULL,
                lesson_id TEXT,
                kind TEXT NOT NULL,
                success INTEGER NOT NULL,
                metadata_json TEXT,
                recorded_at TEXT NOT NULL,
                observation_order INTEGER NOT NULL DEFAULT 0
             );
             CREATE INDEX ix_evidence_item ON learning_item_evidence(learning_item_id, kind);
             CREATE TABLE listening_attempts(
                id TEXT PRIMARY KEY,
                learner_language_profile_id TEXT NOT NULL,
                learning_item_id TEXT,
                lesson_id TEXT,
                source_reference TEXT,
                source_variant_id TEXT,
                talker_key TEXT,
                context_tag TEXT,
                transcript TEXT,
                success INTEGER NOT NULL,
                recorded_at TEXT NOT NULL
             );
             INSERT INTO learning_items VALUES
                ('mastered-failed','llp-1','failed mastered','mastered','2025-01-01T00:00:00Z');
             INSERT INTO learning_item_evidence VALUES(
                'ev-failed','mastered-failed','les-current','spontaneous_production',0,
                json_object('assistance_level',0,'probe_phase','diagnostic'),
                '2026-11-06T00:00:00Z',0
             );",
        )
        .unwrap();
        for index in 0..184 {
            db.execute(
                "INSERT INTO learning_items
                 VALUES(?1,'llp-1',?2,'learning','2026-11-06T00:00:00Z')",
                params![format!("learning-{index:03}"), format!("item {index}")],
            )
            .unwrap();
        }
        for index in 0..16 {
            db.execute(
                "INSERT INTO learning_items
                 VALUES(?1,'llp-1',?2,'mastered',?3)",
                params![
                    format!("mastered-old-{index:02}"),
                    format!("old mastered {index}"),
                    format!("2026-01-{:02}T00:00:00Z", index + 1),
                ],
            )
            .unwrap();
        }

        let mut saw_failed = false;
        let count = visit_item_capabilities(&db, "llp-1", 1_762_435_200, |item| {
            saw_failed |= item.learning_item_id == "mastered-failed";
            Ok(())
        })
        .unwrap();

        assert_eq!(count, MAX_ITEMS);
        assert!(
            saw_failed,
            "a mastered item with a fresh independent failure must remain selected for repair"
        );
    }

    #[test]
    fn assisted_success_does_not_resolve_failed_mastered_capability() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE learning_items(
                id TEXT PRIMARY KEY,
                learner_language_profile_id TEXT NOT NULL,
                target_text TEXT NOT NULL,
                status TEXT NOT NULL,
                updated_at TEXT NOT NULL
             );
             CREATE TABLE learning_item_evidence(
                id TEXT PRIMARY KEY,
                learning_item_id TEXT NOT NULL,
                lesson_id TEXT,
                kind TEXT NOT NULL,
                success INTEGER NOT NULL,
                metadata_json TEXT,
                recorded_at TEXT NOT NULL,
                observation_order INTEGER NOT NULL DEFAULT 0
             );
             CREATE INDEX ix_evidence_item ON learning_item_evidence(learning_item_id, kind);
             CREATE TABLE listening_attempts(
                id TEXT PRIMARY KEY,
                learner_language_profile_id TEXT NOT NULL,
                learning_item_id TEXT,
                lesson_id TEXT,
                source_reference TEXT,
                source_variant_id TEXT,
                talker_key TEXT,
                context_tag TEXT,
                transcript TEXT,
                success INTEGER NOT NULL,
                recorded_at TEXT NOT NULL
             );
             INSERT INTO learning_items VALUES
                ('mastered-assisted','llp-1','assisted mastered','mastered','2025-01-01T00:00:00Z');
             INSERT INTO learning_item_evidence VALUES
                ('ev-failed','mastered-assisted','les-current','listening',0,
                 json_object('assistance_level',0,'probe_phase','diagnostic'),
                 '2026-11-06T00:00:00Z',0),
                ('ev-assisted','mastered-assisted','les-current','listening',1,
                 json_object('assistance_level',2,'probe_phase','diagnostic'),
                 '2026-11-06T00:05:00Z',1);",
        )
        .unwrap();
        for index in 0..184 {
            db.execute(
                "INSERT INTO learning_items
                 VALUES(?1,'llp-1',?2,'learning','2026-11-06T00:00:00Z')",
                params![format!("learning-{index:03}"), format!("item {index}")],
            )
            .unwrap();
        }
        for index in 0..16 {
            db.execute(
                "INSERT INTO learning_items
                 VALUES(?1,'llp-1',?2,'mastered',?3)",
                params![
                    format!("mastered-old-{index:02}"),
                    format!("old mastered {index}"),
                    format!("2026-01-{:02}T00:00:00Z", index + 1),
                ],
            )
            .unwrap();
        }

        let mut saw_assisted = false;
        let count = visit_item_capabilities(&db, "llp-1", 1_762_435_200, |item| {
            saw_assisted |= item.learning_item_id == "mastered-assisted";
            Ok(())
        })
        .unwrap();

        assert_eq!(count, MAX_ITEMS);
        assert!(
            saw_assisted,
            "assisted success must not resolve an independent mastered capability failure"
        );
    }

    #[test]
    fn later_easy_success_does_not_hide_failed_mastered_capability() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE learning_items(
                id TEXT PRIMARY KEY,
                learner_language_profile_id TEXT NOT NULL,
                target_text TEXT NOT NULL,
                status TEXT NOT NULL,
                updated_at TEXT NOT NULL
             );
             CREATE TABLE learning_item_evidence(
                id TEXT PRIMARY KEY,
                learning_item_id TEXT NOT NULL,
                lesson_id TEXT,
                kind TEXT NOT NULL,
                success INTEGER NOT NULL,
                metadata_json TEXT,
                recorded_at TEXT NOT NULL,
                observation_order INTEGER NOT NULL DEFAULT 0
             );
             CREATE INDEX ix_evidence_item ON learning_item_evidence(learning_item_id, kind);
             CREATE TABLE listening_attempts(
                id TEXT PRIMARY KEY,
                learner_language_profile_id TEXT NOT NULL,
                learning_item_id TEXT,
                lesson_id TEXT,
                source_reference TEXT,
                source_variant_id TEXT,
                talker_key TEXT,
                context_tag TEXT,
                transcript TEXT,
                success INTEGER NOT NULL,
                recorded_at TEXT NOT NULL
             );
             INSERT INTO learning_items VALUES
                ('mastered-mixed','llp-1','mixed mastered','mastered','2025-01-01T00:00:00Z');
             INSERT INTO learning_item_evidence VALUES
                ('ev-failed','mastered-mixed','les-current','spontaneous_production',0,
                 json_object('assistance_level',0,'probe_phase','diagnostic'),
                 '2026-11-06T00:00:00Z',0),
                ('ev-easy','mastered-mixed','les-current','recognition',1,
                 json_object('assistance_level',0,'probe_phase','diagnostic'),
                 '2026-11-06T00:05:00Z',1);",
        )
        .unwrap();
        for index in 0..184 {
            db.execute(
                "INSERT INTO learning_items
                 VALUES(?1,'llp-1',?2,'learning','2026-11-06T00:00:00Z')",
                params![format!("learning-{index:03}"), format!("item {index}")],
            )
            .unwrap();
        }
        for index in 0..16 {
            db.execute(
                "INSERT INTO learning_items
                 VALUES(?1,'llp-1',?2,'mastered',?3)",
                params![
                    format!("mastered-old-{index:02}"),
                    format!("old mastered {index}"),
                    format!("2026-01-{:02}T00:00:00Z", index + 1),
                ],
            )
            .unwrap();
        }

        let mut saw_mixed = false;
        let count = visit_item_capabilities(&db, "llp-1", 1_762_435_200, |item| {
            saw_mixed |= item.learning_item_id == "mastered-mixed";
            Ok(())
        })
        .unwrap();

        assert_eq!(count, MAX_ITEMS);
        assert!(
            saw_mixed,
            "a later easy-capability success must not hide an unresolved mastered capability failure"
        );
    }

    #[test]
    fn unknown_assistance_keeps_mastered_focus_revalidation_due() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE learning_items(
                id TEXT PRIMARY KEY,
                learner_language_profile_id TEXT NOT NULL,
                target_text TEXT NOT NULL,
                status TEXT NOT NULL,
                updated_at TEXT NOT NULL
             );
             CREATE TABLE learning_item_evidence(
                id TEXT PRIMARY KEY,
                learning_item_id TEXT NOT NULL,
                lesson_id TEXT,
                kind TEXT NOT NULL,
                success INTEGER NOT NULL,
                metadata_json TEXT,
                recorded_at TEXT NOT NULL,
                observation_order INTEGER NOT NULL DEFAULT 0
             );
             CREATE INDEX ix_evidence_item ON learning_item_evidence(learning_item_id, kind);
             CREATE TABLE listening_attempts(
                id TEXT PRIMARY KEY,
                learner_language_profile_id TEXT NOT NULL,
                learning_item_id TEXT,
                lesson_id TEXT,
                source_reference TEXT,
                source_variant_id TEXT,
                talker_key TEXT,
                context_tag TEXT,
                transcript TEXT,
                success INTEGER NOT NULL,
                recorded_at TEXT NOT NULL
             );
             INSERT INTO learning_items VALUES(
                'mastered-unknown-assistance','llp-1','unknown assistance','mastered',
                '2025-01-01T00:00:00Z'
             );
             INSERT INTO learning_item_evidence VALUES
                ('fresh-listening','mastered-unknown-assistance','les-current','listening',1,
                 json_object('probe_phase','diagnostic'),
                 '2026-11-06T00:00:00Z',0),
                ('fresh-spontaneous','mastered-unknown-assistance','les-current','spontaneous_production',1,
                 json_object('probe_phase','delayed_retrieval'),
                 '2026-11-06T00:01:00Z',1),
                ('fresh-transfer','mastered-unknown-assistance','les-current','transfer',1,
                 json_object('probe_phase','transfer','context_family','service.ask_help'),
                 '2026-11-06T00:02:00Z',2);",
        )
        .unwrap();

        for index in 0..184 {
            db.execute(
                "INSERT INTO learning_items
                 VALUES(?1,'llp-1',?2,'learning','2026-11-06T00:00:00Z')",
                params![format!("learning-{index:03}"), format!("item {index}")],
            )
            .unwrap();
        }
        for index in 0..16 {
            db.execute(
                "INSERT INTO learning_items
                 VALUES(?1,'llp-1',?2,'mastered','2026-06-01T00:00:00Z')",
                params![
                    format!("mastered-old-{index:02}"),
                    format!("old mastered {index}")
                ],
            )
            .unwrap();
        }

        let mut saw_target = false;
        let count = visit_item_capabilities(&db, "llp-1", 1_762_435_200, |item| {
            saw_target |= item.learning_item_id == "mastered-unknown-assistance";
            Ok(())
        })
        .unwrap();

        assert_eq!(count, MAX_ITEMS);
        assert!(
            saw_target,
            "fresh rows with unknown assistance are unqualified and must not rotate mastered focus revalidation away"
        );
    }

    #[test]
    fn recognition_failure_does_not_occupy_focus_revalidation_slot() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE learning_items(
                id TEXT PRIMARY KEY,
                learner_language_profile_id TEXT NOT NULL,
                target_text TEXT NOT NULL,
                status TEXT NOT NULL,
                updated_at TEXT NOT NULL
             );
             CREATE TABLE learning_item_evidence(
                id TEXT PRIMARY KEY,
                learning_item_id TEXT NOT NULL,
                lesson_id TEXT,
                kind TEXT NOT NULL,
                success INTEGER NOT NULL,
                metadata_json TEXT,
                recorded_at TEXT NOT NULL,
                observation_order INTEGER NOT NULL DEFAULT 0
             );
             CREATE INDEX ix_evidence_item ON learning_item_evidence(learning_item_id, kind);
             CREATE TABLE listening_attempts(
                id TEXT PRIMARY KEY,
                learner_language_profile_id TEXT NOT NULL,
                learning_item_id TEXT,
                lesson_id TEXT,
                source_reference TEXT,
                source_variant_id TEXT,
                talker_key TEXT,
                context_tag TEXT,
                transcript TEXT,
                success INTEGER NOT NULL,
                recorded_at TEXT NOT NULL
             );
             INSERT INTO learning_items VALUES(
                'mastered-recognition-fail','llp-1','recognition fail','mastered',
                '2025-01-01T00:00:00Z'
             );
             INSERT INTO learning_item_evidence VALUES
                ('fresh-listening','mastered-recognition-fail','les-current','listening',1,
                 json_object('assistance_level',0,'probe_phase','diagnostic'),
                 '2026-11-06T00:00:00Z',0),
                ('fresh-spontaneous','mastered-recognition-fail','les-current','spontaneous_production',1,
                 json_object('assistance_level',0,'probe_phase','delayed_retrieval'),
                 '2026-11-06T00:01:00Z',1),
                ('fresh-transfer','mastered-recognition-fail','les-current','transfer',1,
                 json_object('assistance_level',0,'probe_phase','transfer','context_family','service.ask_help'),
                 '2026-11-06T00:02:00Z',2),
                ('fresh-recognition-fail','mastered-recognition-fail','les-current','recognition',0,
                 json_object('assistance_level',0,'probe_phase','diagnostic'),
                 '2026-11-06T00:03:00Z',3);",
        )
        .unwrap();

        for index in 0..184 {
            db.execute(
                "INSERT INTO learning_items
                 VALUES(?1,'llp-1',?2,'learning','2026-11-06T00:00:00Z')",
                params![format!("learning-{index:03}"), format!("item {index}")],
            )
            .unwrap();
        }
        for index in 0..16 {
            db.execute(
                "INSERT INTO learning_items
                 VALUES(?1,'llp-1',?2,'mastered','2026-06-01T00:00:00Z')",
                params![
                    format!("mastered-old-{index:02}"),
                    format!("old mastered {index}")
                ],
            )
            .unwrap();
        }

        let mut saw_target = false;
        let count = visit_item_capabilities(&db, "llp-1", 1_762_435_200, |item| {
            saw_target |= item.learning_item_id == "mastered-recognition-fail";
            Ok(())
        })
        .unwrap();

        assert_eq!(count, MAX_ITEMS);
        assert!(
            !saw_target,
            "recognition-only failure must not consume the scarce focus-capability revalidation reserve"
        );
    }

    #[test]
    fn fresh_recognition_does_not_hide_stale_productive_revalidation() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE learning_items(
                id TEXT PRIMARY KEY,
                learner_language_profile_id TEXT NOT NULL,
                target_text TEXT NOT NULL,
                status TEXT NOT NULL,
                updated_at TEXT NOT NULL
             );
             CREATE TABLE learning_item_evidence(
                id TEXT PRIMARY KEY,
                learning_item_id TEXT NOT NULL,
                lesson_id TEXT,
                kind TEXT NOT NULL,
                success INTEGER NOT NULL,
                metadata_json TEXT,
                recorded_at TEXT NOT NULL,
                observation_order INTEGER NOT NULL DEFAULT 0
             );
             CREATE INDEX ix_evidence_item ON learning_item_evidence(learning_item_id, kind);
             CREATE TABLE listening_attempts(
                id TEXT PRIMARY KEY,
                learner_language_profile_id TEXT NOT NULL,
                learning_item_id TEXT,
                lesson_id TEXT,
                source_reference TEXT,
                source_variant_id TEXT,
                talker_key TEXT,
                context_tag TEXT,
                transcript TEXT,
                success INTEGER NOT NULL,
                recorded_at TEXT NOT NULL
             );
             INSERT INTO learning_items VALUES(
                'mastered-stale-production','llp-1','stale production','mastered',
                '2025-01-01T00:00:00Z'
             );
             INSERT INTO learning_item_evidence VALUES
                ('old-production','mastered-stale-production','les-old',
                 'spontaneous_production',1,
                 json_object('assistance_level',0,'probe_phase','delayed_retrieval'),
                 '2025-01-01T00:00:00Z',0),
                ('fresh-recognition','mastered-stale-production','les-fresh',
                 'recognition',1,
                 json_object('assistance_level',0,'probe_phase','diagnostic'),
                 '2026-11-06T00:00:00Z',0);",
        )
        .unwrap();

        for index in 0..184 {
            db.execute(
                "INSERT INTO learning_items
                 VALUES(?1,'llp-1',?2,'learning','2026-11-06T00:00:00Z')",
                params![format!("learning-{index:03}"), format!("item {index}")],
            )
            .unwrap();
        }
        for index in 0..16 {
            db.execute(
                "INSERT INTO learning_items
                 VALUES(?1,'llp-1',?2,'mastered','2026-06-01T00:00:00Z')",
                params![
                    format!("mastered-mid-{index:02}"),
                    format!("mid mastered {index}")
                ],
            )
            .unwrap();
        }

        let mut saw_target = false;
        let count = visit_item_capabilities(&db, "llp-1", 1_762_435_200, |item| {
            saw_target |= item.learning_item_id == "mastered-stale-production";
            Ok(())
        })
        .unwrap();

        assert_eq!(count, MAX_ITEMS);
        assert!(
            saw_target,
            "fresh evidence in an easier capability must not postpone stale productive revalidation"
        );
    }

    #[test]
    fn recent_evidence_rotates_mastered_revalidation_slots() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE learning_items(
                id TEXT PRIMARY KEY,
                learner_language_profile_id TEXT NOT NULL,
                target_text TEXT NOT NULL,
                status TEXT NOT NULL,
                updated_at TEXT NOT NULL
             );
             CREATE TABLE learning_item_evidence(
                id TEXT PRIMARY KEY,
                learning_item_id TEXT NOT NULL,
                lesson_id TEXT,
                kind TEXT NOT NULL,
                success INTEGER NOT NULL,
                metadata_json TEXT,
                recorded_at TEXT NOT NULL,
                observation_order INTEGER NOT NULL DEFAULT 0
             );
             CREATE INDEX ix_evidence_item ON learning_item_evidence(learning_item_id, kind);
             CREATE TABLE listening_attempts(
                id TEXT PRIMARY KEY,
                learner_language_profile_id TEXT NOT NULL,
                learning_item_id TEXT,
                lesson_id TEXT,
                source_reference TEXT,
                source_variant_id TEXT,
                talker_key TEXT,
                context_tag TEXT,
                transcript TEXT,
                success INTEGER NOT NULL,
                recorded_at TEXT NOT NULL
             );
             INSERT INTO learning_items VALUES
                ('mastered-old','llp-1','old mastered','mastered','2025-01-01T00:00:00Z');
             INSERT INTO learning_item_evidence VALUES
                ('ev-listening','mastered-old','les-current','listening',1,
                 json_object('assistance_level',0,'probe_phase','diagnostic'),
                 '2026-11-06T00:00:00Z',0),
                ('ev-spontaneous','mastered-old','les-current','spontaneous_production',1,
                 json_object('assistance_level',0,'probe_phase','delayed_retrieval'),
                 '2026-11-06T00:01:00Z',1),
                ('ev-transfer','mastered-old','les-current','transfer',1,
                 json_object('assistance_level',0,'probe_phase','transfer','context_family','service.ask_help'),
                 '2026-11-06T00:02:00Z',2);",
        )
        .unwrap();
        for index in 0..184 {
            db.execute(
                "INSERT INTO learning_items
                 VALUES(?1,'llp-1',?2,'learning','2026-11-06T00:00:00Z')",
                params![format!("learning-{index:03}"), format!("item {index}")],
            )
            .unwrap();
        }
        for index in 0..16 {
            db.execute(
                "INSERT INTO learning_items
                 VALUES(?1,'llp-1',?2,'mastered',?3)",
                params![
                    format!("mastered-new-{index:02}"),
                    format!("new mastered {index}"),
                    format!("2026-10-{:02}T00:00:00Z", index + 1),
                ],
            )
            .unwrap();
        }

        let mut saw_revalidated = false;
        let mut saw_freshest_unchecked = false;
        let count = visit_item_capabilities(&db, "llp-1", 1_762_435_200, |item| {
            saw_revalidated |= item.learning_item_id == "mastered-old";
            saw_freshest_unchecked |= item.learning_item_id == "mastered-new-15";
            Ok(())
        })
        .unwrap();

        assert_eq!(count, MAX_ITEMS);
        assert!(
            !saw_revalidated,
            "recently revalidated mastered item must rotate out of the oldest reserved slots"
        );
        assert!(
            saw_freshest_unchecked,
            "the next not-yet-revalidated mastered item must receive the freed slot"
        );
    }

    #[test]
    fn reused_evidence_slot_does_not_inherit_previous_item_metadata() {
        let db = Connection::open_in_memory().unwrap();
        let mut statement = db
            .prepare("SELECT 0,0,0,0,'recognition',1,?1,'2026-10-18T10:00:00Z',?2")
            .unwrap();
        let mut scratch = empty_evidence_columns();
        {
            let mut rows = statement
                .query([
                    Some(
                        r#"{"assistance_level":2,"response_millis":400,"context_key":"old","context_family":"family"}"#,
                    ),
                    Some("les-1"),
                ])
                .unwrap();
            append_evidence_row(&mut scratch, rows.next().unwrap().unwrap()).unwrap();
        }
        assert_eq!(scratch.len, 1);
        assert!(scratch.assistance_present[0]);
        assert!(scratch.response_present[0]);
        assert!(scratch.context_key_present[0]);
        assert!(scratch.context_family_present[0]);
        assert!(scratch.episode_key_present[0]);
        clear_evidence_columns(&mut scratch);
        {
            let mut rows = statement
                .query([Option::<&str>::None, Option::<&str>::None])
                .unwrap();
            append_evidence_row(&mut scratch, rows.next().unwrap().unwrap()).unwrap();
        }
        assert_eq!(scratch.len, 1);
        assert!(!scratch.assistance_present[0]);
        assert!(!scratch.response_present[0]);
        assert!(!scratch.context_key_present[0]);
        assert!(!scratch.context_family_present[0]);
        assert!(!scratch.episode_key_present[0]);
        assert!(scratch.text.is_empty());
    }
}
