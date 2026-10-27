//! Batch capability evidence scan shared by lesson and read-only tutor operations.
//! SQLite is authoritative. One request-local scratch is reused for each item.
use crate::Result;
use crate::repositories::from_rfc3339;
use rusqlite::{Connection, params};

const MAX_ITEMS: usize = 200;
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
    for i in 0..data.len {
        if data.context_key_present[i] {
            contexts[i] = Some(text_at(&data.text, data.context_key[i]));
        }
        if data.context_family_present[i] {
            families[i] = Some(text_at(&data.text, data.context_family[i]));
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
        "WITH selected AS MATERIALIZED (
            SELECT id,target_text,status,
                   ROW_NUMBER() OVER (ORDER BY updated_at DESC) AS ord
            FROM learning_items
            WHERE learner_language_profile_id=?1
              AND status IN ('introduced','learning','transfer_pending',
                             'provisionally_mastered','mastered')
            ORDER BY updated_at DESC LIMIT 200
        ),
        evidence AS (
            SELECT e.learning_item_id,e.kind,e.success,e.pack_metadata_json,
                   e.recorded_at,e.id,e.observation_order,
                   ROW_NUMBER() OVER (
                     PARTITION BY e.learning_item_id,e.kind
                     ORDER BY e.recorded_at DESC,e.observation_order DESC,e.rowid DESC
                   ) AS rn
            FROM learning_item_evidence e JOIN selected s ON s.id=e.learning_item_id
            WHERE e.kind NOT IN ('listening','correction')
        ),
        listening AS (
            SELECT l.learning_item_id,'listening' AS kind,l.success,
                   json_object(
                     'assistance_level', l.assistance_level,
                     'response_millis', l.response_millis,
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
                   ) AS pack_metadata_json,l.recorded_at,l.id,0 AS observation_order,
                   ROW_NUMBER() OVER (
                     PARTITION BY l.learning_item_id
                     ORDER BY l.recorded_at DESC,l.id DESC
                   ) AS rn
            FROM listening_attempts l JOIN selected s ON s.id=l.learning_item_id
        ),
        sampled AS (
            SELECT learning_item_id,kind,success,pack_metadata_json,recorded_at,id,observation_order
            FROM evidence WHERE rn<=12
            UNION ALL
            SELECT learning_item_id,kind,success,pack_metadata_json,recorded_at,id,observation_order
            FROM listening WHERE rn<=12
        )
        SELECT s.ord,s.id,s.target_text,s.status,
               e.kind,e.success,e.pack_metadata_json,e.recorded_at
        FROM selected s LEFT JOIN sampled e ON e.learning_item_id=s.id
        ORDER BY s.ord,e.recorded_at DESC,e.observation_order DESC,e.id DESC",
    )?;
    let mut rows = statement.query(params![profile_id])?;
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
    fn listening_variation_ignores_source_reference_without_explicit_metadata() {
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
                kind TEXT NOT NULL,
                success INTEGER NOT NULL,
                pack_metadata_json TEXT,
                recorded_at TEXT NOT NULL,
                observation_order INTEGER NOT NULL DEFAULT 0
             );
             CREATE TABLE listening_attempts(
                id TEXT PRIMARY KEY,
                learner_language_profile_id TEXT NOT NULL,
                learning_item_id TEXT,
                source_reference TEXT,
                source_variant_id TEXT,
                talker_key TEXT,
                context_tag TEXT,
                success INTEGER NOT NULL,
                assistance_level INTEGER,
                response_millis INTEGER,
                probe_phase TEXT,
                recorded_at TEXT NOT NULL
             );
             INSERT INTO learning_items VALUES(
                'li-1','llp-1','hola','learning','2026-10-23T10:00:00Z'
             );
             INSERT INTO listening_attempts VALUES
                ('la-1','llp-1','li-1','audio-a',NULL,NULL,NULL,1,0,1200,'diagnostic','2026-10-23T10:01:00Z'),
                ('la-2','llp-1','li-1','audio-b',NULL,NULL,NULL,1,2,2400,'practice','2026-10-23T10:02:00Z');",
        )
        .unwrap();

        let mut distinct = None;
        let mut unaided = None;
        let mut latency = None;
        visit_item_capabilities(&db, "llp-1", 1_761_213_000, |item| {
            distinct = Some(item.listening.distinct_success_contexts);
            unaided = item.listening.unaided_success_rate;
            latency = item.listening.latency_weighted_millis;
            Ok(())
        })
        .unwrap();
        assert_eq!(distinct, Some(0));
        assert_eq!(unaided, Some(1.0));
        assert!(latency.is_some_and(|millis| (1_200..=2_400).contains(&millis)));

        db.execute(
            "UPDATE listening_attempts
             SET source_variant_id=CASE id WHEN 'la-1' THEN 'variant-a' ELSE 'variant-b' END",
            [],
        )
        .unwrap();
        visit_item_capabilities(&db, "llp-1", 1_761_213_000, |item| {
            distinct = Some(item.listening.distinct_success_contexts);
            Ok(())
        })
        .unwrap();
        assert_eq!(distinct, Some(2));
    }

    #[test]
    fn reused_evidence_slot_does_not_inherit_previous_item_metadata() {
        let db = Connection::open_in_memory().unwrap();
        let mut statement = db
            .prepare("SELECT 0,0,0,0,'recognition',1,?1,'2026-10-18T10:00:00Z'")
            .unwrap();
        let mut scratch = empty_evidence_columns();
        {
            let mut rows = statement.query([Some(
                r#"{"assistance_level":2,"response_millis":400,"context_key":"old","context_family":"family"}"#
            )]).unwrap();
            append_evidence_row(&mut scratch, rows.next().unwrap().unwrap()).unwrap();
        }
        assert_eq!(scratch.len, 1);
        assert!(scratch.assistance_present[0]);
        assert!(scratch.response_present[0]);
        assert!(scratch.context_key_present[0]);
        assert!(scratch.context_family_present[0]);
        clear_evidence_columns(&mut scratch);
        {
            let mut rows = statement.query([Option::<&str>::None]).unwrap();
            append_evidence_row(&mut scratch, rows.next().unwrap().unwrap()).unwrap();
        }
        assert_eq!(scratch.len, 1);
        assert!(!scratch.assistance_present[0]);
        assert!(!scratch.response_present[0]);
        assert!(!scratch.context_key_present[0]);
        assert!(!scratch.context_family_present[0]);
        assert!(scratch.text.is_empty());
    }
}
