//! Cross-crate integration tests. Test code only; never ships
//! (02-workspace.md §1 `tests/`). Suites live in `tests/integration/tests/`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rusqlite::Connection;
use std::fs;
use std::path::PathBuf;

/// Path of the native runtime migration directory.
#[must_use]
pub fn migrations_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../apps/local/migrations")
}

/// Fresh in-memory DB with every migration applied in numeric order and
/// foreign keys ON (04-database.md §1).
#[must_use]
pub fn migrated_connection() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();

    let mut files: Vec<PathBuf> = fs::read_dir(migrations_dir())
        .expect("apps/local/migrations directory exists")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "sql"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no migration files found");

    for file in files {
        if file.file_name().is_some_and(|name| {
            let name = name.to_string_lossy();
            name.starts_with("0006_language_neutral_storage")
                || name.starts_with("0007_semantic_metadata")
        }) {
            continue;
        }
        let sql = fs::read_to_string(&file).unwrap();
        conn.execute_batch(&sql)
            .unwrap_or_else(|e| panic!("{} failed: {e}", file.display()));
    }
    conn
}

/// Seeds learner + language profile (`llp-1`) for row-level tests.
pub fn seed_profile(conn: &Connection) {
    conn.execute_batch(
        "INSERT INTO learner_profile (id, created_at, updated_at)
         VALUES ('learner-1', '2026-07-12T00:00:00Z', '2026-07-12T00:00:00Z');
         INSERT INTO learner_language_profiles (id, learner_id,
             source_language, target_locale, created_at, updated_at)
         VALUES ('llp-1', 'learner-1', 'ru', 'zz-ZZ',
                 '2026-07-12T00:00:00Z', '2026-07-12T00:00:00Z');",
    )
    .unwrap();
}
