//! Shared daily card-budget calculations using the learner's IANA timezone.

use crate::Result;
use crate::repositories::{review, to_rfc3339};
use chrono::{Datelike as _, Duration, NaiveDate, Utc};
use rusqlite::{OptionalExtension as _, params};

/// Start of the learner's current local calendar day, expressed in UTC.
pub(crate) fn local_day_start_utc(
    at: learning_core::Timestamp,
    timezone: &str,
) -> Result<learning_core::Timestamp> {
    match timezone {
        "UTC" | "Etc/UTC" => Ok(learning_core::Timestamp(at.0 - at.0.rem_euclid(86_400))),
        "Europe/Madrid" => madrid_day_start_utc(at),
        _ => Err(crate::Error::operation(
            500,
            "timezone_unsupported",
            format!("stored timezone '{timezone}' is not supported by the local runtime"),
            serde_json::Value::Null,
        )),
    }
}

fn madrid_day_start_utc(at: learning_core::Timestamp) -> Result<learning_core::Timestamp> {
    let utc = chrono::DateTime::<Utc>::from_timestamp(at.0, 0)
        .ok_or_else(|| crate::Error::RustError("timestamp outside chrono range".to_owned()))?;
    let year = utc.year();
    let start = madrid_transition_utc(year, 3)?;
    let end = madrid_transition_utc(year, 10)?;
    let current_offset = if utc >= start && utc < end {
        7_200
    } else {
        3_600
    };
    let local = utc.naive_utc() + Duration::seconds(current_offset);
    let local_date = local.date();

    let march = last_sunday(year, 3)?;
    let october = last_sunday(year, 10)?;
    let midnight_offset = if local_date > march && local_date <= october {
        7_200
    } else {
        3_600
    };
    let local_midnight = local_date
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| crate::Error::RustError("failed to construct local midnight".to_owned()))?;
    Ok(learning_core::Timestamp(
        local_midnight.and_utc().timestamp() - midnight_offset,
    ))
}

fn madrid_transition_utc(year: i32, month: u32) -> Result<chrono::DateTime<Utc>> {
    let date = last_sunday(year, month)?;
    let naive = date
        .and_hms_opt(1, 0, 0)
        .ok_or_else(|| crate::Error::RustError("failed to construct DST transition".to_owned()))?;
    Ok(naive.and_utc())
}

fn last_sunday(year: i32, month: u32) -> Result<NaiveDate> {
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    let next_month_start = NaiveDate::from_ymd_opt(next_year, next_month, 1)
        .ok_or_else(|| crate::Error::RustError("invalid calendar date".to_owned()))?;
    let last_day = next_month_start - Duration::days(1);
    let days_since_sunday = i64::from(last_day.weekday().num_days_from_sunday());
    Ok(last_day - Duration::days(days_since_sunday))
}

fn previous_two_study_days(
    database: &rusqlite::Connection,
    profile_id: &str,
    today_start: learning_core::Timestamp,
    timezone: &str,
) -> Result<Option<(adaptation::DailySignal, adaptation::DailySignal)>> {
    const MAX_SESSION_SCAN: usize = 64;
    let mut day_starts = [0_i64; 2];
    let mut day_len = 0usize;
    {
        let mut statement = database.prepare(
            "SELECT completed_at
             FROM review_sessions
             WHERE learner_language_profile_id=?1
               AND completed_at IS NOT NULL
               AND completed_at < ?2
             ORDER BY completed_at DESC
             LIMIT ?3",
        )?;
        let mut rows = statement.query(params![
            profile_id,
            to_rfc3339(today_start),
            u32::try_from(MAX_SESSION_SCAN).unwrap_or(u32::MAX)
        ])?;
        while let Some(row) = rows.next()? {
            let completed_at: String = row.get(0)?;
            let completed = crate::repositories::from_rfc3339(&completed_at)
                .map_err(|error| crate::Error::RustError(error.to_string()))?;
            let start = local_day_start_utc(completed, timezone)?.0;
            if day_starts[..day_len].contains(&start) {
                continue;
            }
            day_starts[day_len] = start;
            day_len += 1;
            if day_len == 2 {
                break;
            }
        }
    }
    if day_len < 2 {
        return Ok(None);
    }

    let newer = daily_signal(
        database,
        profile_id,
        learning_core::Timestamp(day_starts[0]),
        timezone,
    )?;
    let older = daily_signal(
        database,
        profile_id,
        learning_core::Timestamp(day_starts[1]),
        timezone,
    )?;
    Ok(Some((newer, older)))
}

fn daily_signal(
    database: &rusqlite::Connection,
    profile_id: &str,
    start: learning_core::Timestamp,
    timezone: &str,
) -> Result<adaptation::DailySignal> {
    let next_probe = learning_core::Timestamp(start.0.saturating_add(36 * 3_600));
    let end = local_day_start_utc(next_probe, timezone)?;
    let start_text = to_rfc3339(start);
    let end_text = to_rfc3339(end);

    let review_seconds: u64 = database.query_row(
        "SELECT COALESCE(SUM(review_seconds),0)
         FROM review_sessions
         WHERE learner_language_profile_id=?1
           AND completed_at>=?2 AND completed_at<?3",
        params![profile_id, start_text, end_text],
        |row| row.get(0),
    )?;
    let (total, passed): (u32, u32) = database.query_row(
        "SELECT COUNT(*),
                COALESCE(SUM(CASE WHEN e.grade>1 THEN 1 ELSE 0 END),0)
         FROM review_events e
         JOIN cards c ON c.id=e.card_id
         WHERE c.learner_language_profile_id=?1
           AND e.answered_at>=?2 AND e.answered_at<?3
           AND e.compensates_event_id IS NULL
           AND NOT EXISTS (
               SELECT 1 FROM review_events x WHERE x.compensates_event_id=e.id
           )",
        params![profile_id, start_text, end_text],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let recall_rate = if total == 0 {
        1.0
    } else {
        f64::from(passed) / f64::from(total)
    };
    Ok(adaptation::DailySignal {
        review_minutes: u16::try_from(review_seconds / 60).unwrap_or(u16::MAX),
        recall_rate,
    })
}

/// Remaining daily activation/admission budget.
///
/// The supplied SQL must accept profile_id and local-day-start as its two
/// positional parameters.
pub(crate) fn remaining_budget(
    database: &rusqlite::Connection,
    profile_id: &str,
    at: learning_core::Timestamp,
    consumed_today_query: &str,
) -> Result<u8> {
    let (max_new_cards, max_review_minutes, timezone): (u8, u16, String) = database.query_row(
        "SELECT l.max_new_cards,l.max_review_minutes,l.timezone
             FROM learner_language_profiles p
             JOIN learner_profile l ON l.id=p.learner_id
             WHERE p.id=?1",
        params![profile_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;

    let overdue: u32 = database.query_row(
        review::COUNT_OVERDUE,
        params![profile_id, to_rfc3339(at)],
        |row| row.get(0),
    )?;
    let day_start_at = local_day_start_utc(at, &timezone)?;
    let day_start = to_rfc3339(day_start_at);
    let consumed_today: u32 = database.query_row(
        consumed_today_query,
        params![profile_id, day_start],
        |row| row.get(0),
    )?;

    let yesterday_budget: u8 = database
        .query_row(
            "SELECT effective_budget
             FROM review_daily_budget
             WHERE learner_language_profile_id=?1 AND local_day_start < ?2
             ORDER BY local_day_start DESC LIMIT 1",
            params![profile_id, day_start],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or(max_new_cards);
    let last_two_days = previous_two_study_days(database, profile_id, day_start_at, &timezone)?;
    let budget = adaptation::new_card_budget(
        max_new_cards,
        yesterday_budget,
        overdue,
        max_review_minutes,
        last_two_days,
    );

    database.execute(
        "INSERT INTO review_daily_budget
            (learner_language_profile_id,local_day_start,effective_budget,updated_at)
         VALUES(?1,?2,?3,?4)
         ON CONFLICT(learner_language_profile_id,local_day_start) DO UPDATE SET
            effective_budget=excluded.effective_budget,
            updated_at=excluded.updated_at",
        params![profile_id, day_start, budget, to_rfc3339(at)],
    )?;

    #[allow(clippy::cast_possible_truncation)]
    Ok(budget.saturating_sub(consumed_today.min(u32::from(u8::MAX)) as u8))
}

#[cfg(test)]
// Test fixtures intentionally fail fast and use compact tuple snapshots.
#[allow(clippy::unwrap_used, clippy::type_complexity)]
mod tests {
    use super::*;
    use chrono::NaiveDateTime;

    fn utc_timestamp(text: &str) -> learning_core::Timestamp {
        let parsed = NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S").unwrap();
        learning_core::Timestamp(parsed.and_utc().timestamp())
    }

    #[test]
    fn madrid_winter_and_summer_midnights_use_correct_offsets() {
        assert_eq!(
            local_day_start_utc(utc_timestamp("2026-01-15 12:00:00"), "Europe/Madrid").unwrap(),
            utc_timestamp("2026-01-14 23:00:00")
        );
        assert_eq!(
            local_day_start_utc(utc_timestamp("2026-07-15 12:00:00"), "Europe/Madrid").unwrap(),
            utc_timestamp("2026-07-14 22:00:00")
        );
    }

    #[test]
    fn madrid_dst_transition_days_resolve_calendar_midnight() {
        let spring =
            local_day_start_utc(utc_timestamp("2026-03-29 12:00:00"), "Europe/Madrid").unwrap();
        assert_eq!(spring, utc_timestamp("2026-03-28 23:00:00"));

        let autumn =
            local_day_start_utc(utc_timestamp("2026-10-25 12:00:00"), "Europe/Madrid").unwrap();
        assert_eq!(autumn, utc_timestamp("2026-10-24 22:00:00"));
    }

    #[test]
    fn runtime_budget_uses_two_previous_study_days() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE learner_profile(
                id TEXT PRIMARY KEY, timezone TEXT NOT NULL, max_new_cards INTEGER NOT NULL, max_review_minutes INTEGER NOT NULL
             );
             CREATE TABLE learner_language_profiles(
                id TEXT PRIMARY KEY, learner_id TEXT NOT NULL
             );
             CREATE TABLE cards(
                id TEXT PRIMARY KEY, learner_language_profile_id TEXT NOT NULL, status TEXT NOT NULL
             );
             CREATE TABLE card_schedule_state(
                card_id TEXT PRIMARY KEY, due_at TEXT
             );
             CREATE TABLE review_sessions(
                id TEXT PRIMARY KEY, learner_language_profile_id TEXT NOT NULL,
                completed_at TEXT, review_seconds INTEGER
             );
             CREATE TABLE review_events(
                id TEXT PRIMARY KEY, card_id TEXT NOT NULL, review_session_id TEXT NOT NULL,
                grade INTEGER NOT NULL, compensates_event_id TEXT, answered_at TEXT NOT NULL, created_at TEXT NOT NULL
             );
             CREATE TABLE review_daily_budget(
                learner_language_profile_id TEXT NOT NULL,
                local_day_start TEXT NOT NULL,
                effective_budget INTEGER NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY(learner_language_profile_id, local_day_start)
             );
             INSERT INTO learner_profile VALUES('learner-1','UTC',8,12);
             INSERT INTO learner_language_profiles VALUES('llp-1','learner-1');
             INSERT INTO cards VALUES('c-1','llp-1','review');
             INSERT INTO review_sessions VALUES
                ('s-1','llp-1','2026-10-20T18:00:00Z',480),
                ('s-2','llp-1','2026-10-21T18:00:00Z',480);
             INSERT INTO review_events VALUES
                ('e-1','c-1','s-1',1,NULL,'2026-10-20T18:00:00Z','2026-10-23T18:00:00Z'),
                ('e-2','c-1','s-2',1,NULL,'2026-10-21T18:00:00Z','2026-10-23T18:00:00Z');",
        )
        .unwrap();

        let budget = remaining_budget(
            &db,
            "llp-1",
            utc_timestamp("2026-10-22 12:00:00"),
            "SELECT COUNT(*) FROM review_events WHERE ?1='llp-1' AND ?2 IS NOT NULL AND 0",
        )
        .unwrap();
        assert_eq!(budget, 4);
    }

    #[test]
    fn runtime_budget_uses_profile_review_minutes_limit() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE learner_profile(
                id TEXT PRIMARY KEY, timezone TEXT NOT NULL, max_new_cards INTEGER NOT NULL,
                max_review_minutes INTEGER NOT NULL
             );
             CREATE TABLE learner_language_profiles(
                id TEXT PRIMARY KEY, learner_id TEXT NOT NULL
             );
             CREATE TABLE cards(
                id TEXT PRIMARY KEY, learner_language_profile_id TEXT NOT NULL, status TEXT NOT NULL
             );
             CREATE TABLE card_schedule_state(
                card_id TEXT PRIMARY KEY, due_at TEXT
             );
             CREATE TABLE review_sessions(
                id TEXT PRIMARY KEY, learner_language_profile_id TEXT NOT NULL,
                completed_at TEXT, review_seconds INTEGER
             );
             CREATE TABLE review_events(
                id TEXT PRIMARY KEY, card_id TEXT NOT NULL, review_session_id TEXT NOT NULL,
                grade INTEGER NOT NULL, compensates_event_id TEXT,
                answered_at TEXT NOT NULL, created_at TEXT NOT NULL
             );
             CREATE TABLE review_daily_budget(
                learner_language_profile_id TEXT NOT NULL,
                local_day_start TEXT NOT NULL,
                effective_budget INTEGER NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY(learner_language_profile_id, local_day_start)
             );
             INSERT INTO learner_profile VALUES('learner-1','UTC',8,7);
             INSERT INTO learner_language_profiles VALUES('llp-1','learner-1');
             INSERT INTO cards VALUES('c-1','llp-1','review');
             INSERT INTO review_sessions VALUES
                ('s-1','llp-1','2026-10-20T18:00:00Z',480),
                ('s-2','llp-1','2026-10-21T18:00:00Z',480);
             INSERT INTO review_events VALUES
                ('e-1','c-1','s-1',3,NULL,'2026-10-20T18:00:00Z','2026-10-23T18:00:00Z'),
                ('e-2','c-1','s-2',3,NULL,'2026-10-21T18:00:00Z','2026-10-23T18:00:00Z');",
        )
        .unwrap();

        let budget = remaining_budget(
            &db,
            "llp-1",
            utc_timestamp("2026-10-22 12:00:00"),
            "SELECT COUNT(*) FROM review_events WHERE ?1='llp-1' AND ?2 IS NOT NULL AND 0",
        )
        .unwrap();
        assert_eq!(budget, 4);
    }

    #[test]
    fn runtime_budget_restores_only_one_after_previous_pause() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE learner_profile(
                id TEXT PRIMARY KEY, timezone TEXT NOT NULL, max_new_cards INTEGER NOT NULL,
                max_review_minutes INTEGER NOT NULL
             );
             CREATE TABLE learner_language_profiles(
                id TEXT PRIMARY KEY, learner_id TEXT NOT NULL
             );
             CREATE TABLE cards(
                id TEXT PRIMARY KEY, learner_language_profile_id TEXT NOT NULL, status TEXT NOT NULL
             );
             CREATE TABLE card_schedule_state(
                card_id TEXT PRIMARY KEY, due_at TEXT
             );
             CREATE TABLE review_sessions(
                id TEXT PRIMARY KEY, learner_language_profile_id TEXT NOT NULL,
                completed_at TEXT, review_seconds INTEGER
             );
             CREATE TABLE review_events(
                id TEXT PRIMARY KEY, card_id TEXT NOT NULL, review_session_id TEXT NOT NULL,
                grade INTEGER NOT NULL, compensates_event_id TEXT,
                answered_at TEXT NOT NULL, created_at TEXT NOT NULL
             );
             CREATE TABLE review_daily_budget(
                learner_language_profile_id TEXT NOT NULL,
                local_day_start TEXT NOT NULL,
                effective_budget INTEGER NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY(learner_language_profile_id, local_day_start)
             );
             INSERT INTO learner_profile VALUES('learner-1','UTC',8,12);
             INSERT INTO learner_language_profiles VALUES('llp-1','learner-1');
             INSERT INTO review_daily_budget VALUES(
                'llp-1','2026-10-21T00:00:00+00:00',0,'2026-10-21T00:00:00Z'
             );",
        )
        .unwrap();

        let budget = remaining_budget(
            &db,
            "llp-1",
            utc_timestamp("2026-10-22 12:00:00"),
            "SELECT 0 WHERE ?1='llp-1' AND ?2 IS NOT NULL",
        )
        .unwrap();
        assert_eq!(budget, 1);
    }

    #[test]
    fn invalid_timezone_fails_closed() {
        assert!(local_day_start_utc(learning_core::Timestamp(1_760_000_000), "Not/AZone").is_err());
    }
}
