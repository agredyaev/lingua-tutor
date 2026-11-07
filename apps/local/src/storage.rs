//! Local SQLite initialization, validation, and consistent backups.

use crate::Env;
use rusqlite::{Connection, MAIN_DB, OptionalExtension};
use std::fs::{self, DirBuilder, OpenOptions};
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const INIT: &str = include_str!("../migrations/0001_init.sql");
#[cfg(any(test, fuzzing))]
const LEGACY_INIT: &str = include_str!("test_fixtures_legacy_0001.sql");
const MIGRATION_2: &str = include_str!("../migrations/0002_planner_history.sql");
const MIGRATION_3: &str = include_str!("../migrations/0003_review_methodology.sql");
const MIGRATION_4: &str = include_str!("../migrations/0004_methodology_calibration.sql");
const MIGRATION_5: &str = include_str!("../migrations/0005_methodology_semantics.sql");
const MIGRATION_6: &str = include_str!("../migrations/0006_language_neutral_storage.sql");
const MIGRATION_7: &str = include_str!("../migrations/0007_semantic_metadata.sql");
const MIGRATION_8: &str = include_str!("../migrations/0008_remove_review_experiments.sql");
const MIGRATION_9: &str = include_str!("../migrations/0009_error_recheck_cycles.sql");
const MIGRATION_10: &str = include_str!("../migrations/0010_error_recheck_repair_action.sql");
const MIGRATION_11: &str = include_str!("../migrations/0011_planner_tutor_tasks_snapshot.sql");
const SCHEMA_VERSION: i64 = 11;
const SCHEMA_NAME: &str = "0011_planner_tutor_tasks_snapshot.sql";
const MIGRATION_NAMES: [&str; SCHEMA_VERSION as usize] = [
    "0001_init.sql",
    "0002_planner_history.sql",
    "0003_review_methodology.sql",
    "0004_methodology_calibration.sql",
    "0005_methodology_semantics.sql",
    "0006_language_neutral_storage.sql",
    "0007_semantic_metadata.sql",
    "0008_remove_review_experiments.sql",
    "0009_error_recheck_cycles.sql",
    "0010_error_recheck_repair_action.sql",
    SCHEMA_NAME,
];

fn private_dir(path: &Path) -> Result<(), String> {
    let mut builder = DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    builder.mode(0o700);
    builder.create(path).map_err(|e| e.to_string())
}

fn open_private_new(path: &Path) -> std::io::Result<fs::File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    options.open(path)
}

fn restrict_private_file(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

fn table_exists(connection: &Connection, name: &str) -> Result<bool, String> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
            [name],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())
}

fn validate_migration_history(connection: &Connection) -> Result<(), String> {
    let mut statement = connection
        .prepare("SELECT id,name FROM schema_migrations ORDER BY id")
        .map_err(|e| format!("unknown database schema: {e}"))?;
    let mut rows = statement
        .query([])
        .map_err(|e| format!("unknown database schema: {e}"))?;
    let mut expected_index = 0_usize;

    while let Some(row) = rows
        .next()
        .map_err(|e| format!("unknown database schema: {e}"))?
    {
        let id: i64 = row
            .get(0)
            .map_err(|e| format!("unknown database schema: {e}"))?;
        let name: String = row
            .get(1)
            .map_err(|e| format!("unknown database schema: {e}"))?;
        let Some(expected_name) = MIGRATION_NAMES.get(expected_index) else {
            return Err(format!(
                "unsupported SQLite migration: id={id}, name={name}"
            ));
        };
        let expected_id = i64::try_from(expected_index + 1)
            .map_err(|_| "migration history exceeds supported range".to_owned())?;
        if id != expected_id || name != *expected_name {
            return Err(format!(
                "unsupported SQLite migration: id={id}, name={name}"
            ));
        }
        expected_index += 1;
    }

    if expected_index != MIGRATION_NAMES.len() {
        return Err(format!(
            "unsupported SQLite migration history: expected {} entries, found {expected_index}",
            MIGRATION_NAMES.len()
        ));
    }
    Ok(())
}

fn initialize(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch("BEGIN IMMEDIATE;")
        .map_err(|e| e.to_string())?;
    let result = connection
        .execute_batch(INIT)
        .and_then(|_| {
            connection.execute_batch(
                "CREATE TABLE schema_migrations(
                    id INTEGER PRIMARY KEY,
                    name TEXT UNIQUE NOT NULL,
                    applied_at TEXT DEFAULT CURRENT_TIMESTAMP NOT NULL
                 );
                 INSERT INTO schema_migrations(id,name) VALUES (1,'0001_init.sql');",
            )
        })
        .and_then(|_| connection.execute_batch(MIGRATION_2))
        .and_then(|_| {
            connection.execute_batch(
                "INSERT INTO schema_migrations(id,name) VALUES (2,'0002_planner_history.sql');",
            )
        })
        .and_then(|_| connection.execute_batch(MIGRATION_3))
        .and_then(|_| {
            connection.execute_batch(
                "INSERT INTO schema_migrations(id,name) VALUES (3,'0003_review_methodology.sql');",
            )
        })
        .and_then(|_| connection.execute_batch(MIGRATION_4))
        .and_then(|_| {
            connection.execute_batch(
                "INSERT INTO schema_migrations(id,name) VALUES (4,'0004_methodology_calibration.sql');",
            )
        })
        .and_then(|_| connection.execute_batch(MIGRATION_5))
        .and_then(|_| {
            connection.execute_batch(
                "INSERT INTO schema_migrations(id,name) VALUES (5,'0005_methodology_semantics.sql');
                 INSERT INTO schema_migrations(id,name) VALUES (6,'0006_language_neutral_storage.sql');
                 INSERT INTO schema_migrations(id,name) VALUES (7,'0007_semantic_metadata.sql');
                 INSERT INTO schema_migrations(id,name) VALUES (8,'0008_remove_review_experiments.sql');",
            )
        })
        .and_then(|_| connection.execute_batch(MIGRATION_9))
        .and_then(|_| {
            connection.execute_batch(
                "INSERT INTO schema_migrations(id,name) VALUES (9,'0009_error_recheck_cycles.sql');",
            )
        })
        .and_then(|_| connection.execute_batch(MIGRATION_10))
        .and_then(|_| {
            connection.execute_batch(
                "INSERT INTO schema_migrations(id,name) VALUES (10,'0010_error_recheck_repair_action.sql');",
            )
        })
        .and_then(|_| connection.execute_batch(MIGRATION_11))
        .and_then(|_| {
            connection.execute_batch(
                "INSERT INTO schema_migrations(id,name) VALUES (11,'0011_planner_tutor_tasks_snapshot.sql');
                 PRAGMA user_version=11;
                 COMMIT;",
            )
        });
    if let Err(error) = result {
        let _ = connection.execute_batch("ROLLBACK");
        return Err(error.to_string());
    }
    Ok(())
}

fn migrate_v1_to_v2(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch("BEGIN IMMEDIATE;")
        .map_err(|e| e.to_string())?;
    let result = connection.execute_batch(MIGRATION_2).and_then(|_| {
        connection.execute_batch(
            "INSERT INTO schema_migrations(id,name) VALUES (2,'0002_planner_history.sql');
                 PRAGMA user_version=2;
                 COMMIT;",
        )
    });
    if let Err(error) = result {
        let _ = connection.execute_batch("ROLLBACK");
        return Err(error.to_string());
    }
    Ok(())
}

fn migrate_v2_to_v3(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch("BEGIN IMMEDIATE;")
        .map_err(|e| e.to_string())?;
    let result = connection.execute_batch(MIGRATION_3).and_then(|_| {
        connection.execute_batch(
            "INSERT INTO schema_migrations(id,name) VALUES (3,'0003_review_methodology.sql');
             PRAGMA user_version=3;
             COMMIT;",
        )
    });
    if let Err(error) = result {
        let _ = connection.execute_batch("ROLLBACK");
        return Err(error.to_string());
    }
    Ok(())
}

fn migrate_v3_to_v4(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch("BEGIN IMMEDIATE;")
        .map_err(|e| e.to_string())?;
    let result = connection.execute_batch(MIGRATION_4).and_then(|_| {
        connection.execute_batch(
            "INSERT INTO schema_migrations(id,name) VALUES (4,'0004_methodology_calibration.sql');
             PRAGMA user_version=4;
             COMMIT;",
        )
    });
    if let Err(error) = result {
        let _ = connection.execute_batch("ROLLBACK");
        return Err(error.to_string());
    }
    Ok(())
}

fn migrate_v4_to_v5(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch("BEGIN IMMEDIATE;")
        .map_err(|e| e.to_string())?;
    let result = connection.execute_batch(MIGRATION_5).and_then(|_| {
        connection.execute_batch(
            "INSERT INTO schema_migrations(id,name) VALUES (5,'0005_methodology_semantics.sql');
             PRAGMA user_version=5;
             COMMIT;",
        )
    });
    if let Err(error) = result {
        let _ = connection.execute_batch("ROLLBACK");
        return Err(error.to_string());
    }
    Ok(())
}

fn migrate_v5_to_v6(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch("BEGIN IMMEDIATE;")
        .map_err(|e| e.to_string())?;
    let result = connection.execute_batch(MIGRATION_6).and_then(|_| {
        connection.execute_batch(
            "INSERT INTO schema_migrations(id,name) VALUES (6,'0006_language_neutral_storage.sql');
             PRAGMA user_version=6;
             COMMIT;",
        )
    });
    if let Err(error) = result {
        let _ = connection.execute_batch("ROLLBACK");
        return Err(error.to_string());
    }
    Ok(())
}

fn migrate_v6_to_v7(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch("BEGIN IMMEDIATE;")
        .map_err(|e| e.to_string())?;
    let result = connection.execute_batch(MIGRATION_7).and_then(|_| {
        connection.execute_batch(
            "INSERT INTO schema_migrations(id,name) VALUES (7,'0007_semantic_metadata.sql');
             PRAGMA user_version=7;
             COMMIT;",
        )
    });
    if let Err(error) = result {
        let _ = connection.execute_batch("ROLLBACK");
        return Err(error.to_string());
    }
    Ok(())
}

fn migrate_v7_to_v8(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch("BEGIN IMMEDIATE;")
        .map_err(|e| e.to_string())?;
    let result = connection.execute_batch(MIGRATION_8).and_then(|_| {
        connection.execute_batch(
            "INSERT INTO schema_migrations(id,name) VALUES (8,'0008_remove_review_experiments.sql');
             PRAGMA user_version=8;
             COMMIT;",
        )
    });
    if let Err(error) = result {
        let _ = connection.execute_batch("ROLLBACK");
        return Err(error.to_string());
    }
    Ok(())
}

fn migrate_v8_to_v9(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch("BEGIN IMMEDIATE;")
        .map_err(|e| e.to_string())?;
    let result = connection.execute_batch(MIGRATION_9).and_then(|_| {
        connection.execute_batch(
            "INSERT INTO schema_migrations(id,name) VALUES (9,'0009_error_recheck_cycles.sql');
             PRAGMA user_version=9;
             COMMIT;",
        )
    });
    if let Err(error) = result {
        let _ = connection.execute_batch("ROLLBACK");
        return Err(error.to_string());
    }
    Ok(())
}

fn migrate_v9_to_v10(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch("BEGIN IMMEDIATE;")
        .map_err(|e| e.to_string())?;
    let result = connection.execute_batch(MIGRATION_10).and_then(|_| {
        connection.execute_batch(
            "INSERT INTO schema_migrations(id,name) VALUES (10,'0010_error_recheck_repair_action.sql');
             PRAGMA user_version=10;
             COMMIT;",
        )
    });
    if let Err(error) = result {
        let _ = connection.execute_batch("ROLLBACK");
        return Err(error.to_string());
    }
    Ok(())
}

fn migrate_v10_to_v11(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch("BEGIN IMMEDIATE;")
        .map_err(|e| e.to_string())?;
    let result = connection.execute_batch(MIGRATION_11).and_then(|_| {
        connection.execute_batch(
            "INSERT INTO schema_migrations(id,name) VALUES (11,'0011_planner_tutor_tasks_snapshot.sql');
             PRAGMA user_version=11;
             COMMIT;",
        )
    });
    if let Err(error) = result {
        let _ = connection.execute_batch("ROLLBACK");
        return Err(error.to_string());
    }
    Ok(())
}

/// Open a fresh or existing local SQLite database. Unknown schemas fail closed.
pub fn open(path: &Path) -> Result<Connection, String> {
    private_dir(path.parent().ok_or("database path has no parent")?)?;
    let connection = Connection::open(path).map_err(|e| e.to_string())?;
    restrict_private_file(path)?;
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    connection
        .execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL;")
        .map_err(|e| e.to_string())?;

    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;
    let has_schema = table_exists(&connection, "learner_profile")?;

    match (version, has_schema) {
        (0, false) => initialize(&connection)?,
        (1, true) => {
            migrate_v1_to_v2(&connection)?;
            migrate_v2_to_v3(&connection)?;
            migrate_v3_to_v4(&connection)?;
            migrate_v4_to_v5(&connection)?;
            migrate_v5_to_v6(&connection)?;
            migrate_v6_to_v7(&connection)?;
            migrate_v7_to_v8(&connection)?;
            migrate_v8_to_v9(&connection)?;
            migrate_v9_to_v10(&connection)?;
            migrate_v10_to_v11(&connection)?;
        }
        (2, true) => {
            migrate_v2_to_v3(&connection)?;
            migrate_v3_to_v4(&connection)?;
            migrate_v4_to_v5(&connection)?;
            migrate_v5_to_v6(&connection)?;
            migrate_v6_to_v7(&connection)?;
            migrate_v7_to_v8(&connection)?;
            migrate_v8_to_v9(&connection)?;
            migrate_v9_to_v10(&connection)?;
            migrate_v10_to_v11(&connection)?;
        }
        (3, true) => {
            migrate_v3_to_v4(&connection)?;
            migrate_v4_to_v5(&connection)?;
            migrate_v5_to_v6(&connection)?;
            migrate_v6_to_v7(&connection)?;
            migrate_v7_to_v8(&connection)?;
            migrate_v8_to_v9(&connection)?;
            migrate_v9_to_v10(&connection)?;
            migrate_v10_to_v11(&connection)?;
        }
        (4, true) => {
            migrate_v4_to_v5(&connection)?;
            migrate_v5_to_v6(&connection)?;
            migrate_v6_to_v7(&connection)?;
            migrate_v7_to_v8(&connection)?;
            migrate_v8_to_v9(&connection)?;
            migrate_v9_to_v10(&connection)?;
            migrate_v10_to_v11(&connection)?;
        }
        (5, true) => {
            migrate_v5_to_v6(&connection)?;
            migrate_v6_to_v7(&connection)?;
            migrate_v7_to_v8(&connection)?;
            migrate_v8_to_v9(&connection)?;
            migrate_v9_to_v10(&connection)?;
            migrate_v10_to_v11(&connection)?;
        }
        (6, true) => {
            migrate_v6_to_v7(&connection)?;
            migrate_v7_to_v8(&connection)?;
            migrate_v8_to_v9(&connection)?;
            migrate_v9_to_v10(&connection)?;
            migrate_v10_to_v11(&connection)?;
        }
        (7, true) => {
            migrate_v7_to_v8(&connection)?;
            migrate_v8_to_v9(&connection)?;
            migrate_v9_to_v10(&connection)?;
            migrate_v10_to_v11(&connection)?;
        }
        (8, true) => {
            migrate_v8_to_v9(&connection)?;
            migrate_v9_to_v10(&connection)?;
            migrate_v10_to_v11(&connection)?;
        }
        (9, true) => {
            migrate_v9_to_v10(&connection)?;
            migrate_v10_to_v11(&connection)?;
        }
        (10, true) => migrate_v10_to_v11(&connection)?,
        (SCHEMA_VERSION, true) => {}
        _ => return Err(format!("unsupported SQLite schema version {version}")),
    }

    validate_migration_history(&connection)?;
    validate(&connection)?;
    Ok(connection)
}

/// Check that the SQLite file and its foreign keys are sound.
pub fn validate(connection: &Connection) -> Result<(), String> {
    let integrity: String = connection
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;
    if integrity != "ok" {
        return Err(format!("integrity_check: {integrity}"));
    }
    let broken: Option<String> = connection
        .query_row("PRAGMA foreign_key_check", [], |row| row.get(0))
        .optional()
        .map_err(|e| e.to_string())?;
    if broken.is_some() {
        return Err("foreign_key_check failed".into());
    }
    Ok(())
}

fn signing_key(dir: &Path) -> Result<String, String> {
    let path = dir.join("secret.key");
    if !path.exists() {
        let value = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        match open_private_new(&path) {
            Ok(mut file) => file
                .write_all(value.as_bytes())
                .map_err(|e| e.to_string())?,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    let mut value = String::new();
    OpenOptions::new()
        .read(true)
        .open(path)
        .map_err(|e| e.to_string())?
        .read_to_string(&mut value)
        .map_err(|e| e.to_string())?;
    if value.len() < 32 {
        return Err("local signing key is too short".into());
    }
    Ok(value)
}

/// Initialize request environment with the local database and signing key.
pub fn environment(path: &Path) -> Result<Env, String> {
    open(path)?;
    let key = signing_key(path.parent().ok_or("database path has no parent")?)?;
    Ok(Env::new(path.to_path_buf(), key))
}

/// Write and verify an online SQLite backup outside the repository.
pub fn backup(path: &Path) -> Result<PathBuf, String> {
    if !path.is_file() {
        return Err(format!("database does not exist: {}", path.display()));
    }
    let source = open(path)?;
    let dir = path
        .parent()
        .ok_or("database path has no parent")?
        .join("backups");
    private_dir(&dir)?;
    let name = format!(
        "lingua-{}-{}.sqlite3",
        chrono::Utc::now().format("%Y%m%dT%H%M%S"),
        uuid::Uuid::new_v4().simple()
    );
    let target = dir.join(name);
    open_private_new(&target).map_err(|e| e.to_string())?;
    if let Err(error) = source.backup(MAIN_DB, &target, None) {
        let _ = fs::remove_file(&target);
        return Err(error.to_string());
    }
    let copy = Connection::open(&target).map_err(|e| e.to_string())?;
    copy.execute_batch("PRAGMA foreign_keys=ON")
        .map_err(|e| e.to_string())?;
    if let Err(error) = validate(&copy) {
        drop(copy);
        let _ = fs::remove_file(&target);
        return Err(error);
    }
    drop(copy);

    let cutoff = SystemTime::now() - Duration::from_secs(90 * 24 * 60 * 60);
    for entry in fs::read_dir(&dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("lingua-")
            && name.ends_with(".sqlite3")
            && entry
                .metadata()
                .map_err(|e| e.to_string())?
                .modified()
                .map_err(|e| e.to_string())?
                < cutoff
        {
            fs::remove_file(entry.path()).map_err(|e| e.to_string())?;
        }
    }
    Ok(target)
}

/// Create a consistent snapshot on startup if no verified backup was made recently.
/// File metadata is sufficient here because backups are only published after validation.
pub fn backup_if_due(path: &Path) -> Result<Option<PathBuf>, String> {
    let dir = path
        .parent()
        .ok_or("database path has no parent")?
        .join("backups");
    let now = SystemTime::now();
    if dir.is_dir() {
        for entry in fs::read_dir(&dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("lingua-")
                && name.ends_with(".sqlite3")
                && entry.file_type().map_err(|e| e.to_string())?.is_file()
                && entry
                    .metadata()
                    .map_err(|e| e.to_string())?
                    .modified()
                    .ok()
                    .and_then(|at| now.duration_since(at).ok())
                    .is_some_and(|age| age < Duration::from_secs(24 * 60 * 60))
            {
                return Ok(None);
            }
        }
    }
    backup(path).map(Some)
}

#[cfg(test)]
// Test fixtures intentionally fail fast and use compact tuple snapshots.
#[allow(clippy::unwrap_used, clippy::type_complexity)]
mod tests {
    use super::*;

    #[test]
    fn local_database_backup_and_unknown_schema() {
        let dir = std::env::temp_dir().join(format!("lingua-storage-{}", uuid::Uuid::new_v4()));
        let path = dir.join("lingua.sqlite3");
        let connection = open(&path).unwrap();
        connection
            .execute_batch("CREATE TABLE smoke(value TEXT); INSERT INTO smoke VALUES ('kept');")
            .unwrap();
        drop(connection);
        let copy = backup(&path).unwrap();
        let saved: String = Connection::open(copy)
            .unwrap()
            .query_row("SELECT value FROM smoke", [], |row| row.get(0))
            .unwrap();
        assert_eq!(saved, "kept");

        let unknown = dir.join("unknown.sqlite3");
        Connection::open(&unknown)
            .unwrap()
            .execute_batch("CREATE TABLE language_packs(id TEXT); PRAGMA user_version=99;")
            .unwrap();
        assert!(open(&unknown).is_err());
        assert!(unknown.exists());

        let existing = dir.join("existing");
        fs::create_dir(&existing).unwrap();
        #[cfg(unix)]
        {
            fs::set_permissions(&existing, fs::Permissions::from_mode(0o755)).unwrap();
        }
        open(&existing.join("child.sqlite3")).unwrap();
        #[cfg(unix)]
        assert_eq!(
            fs::metadata(&existing).unwrap().permissions().mode() & 0o777,
            0o755
        );
        assert!(backup(&dir.join("missing.sqlite3")).is_err());
        assert!(!dir.join("missing.sqlite3").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_tampered_intermediate_migration_name() {
        let dir =
            std::env::temp_dir().join(format!("lingua-migration-history-{}", uuid::Uuid::new_v4()));
        let path = dir.join("lingua.sqlite3");
        drop(open(&path).unwrap());

        let connection = Connection::open(&path).unwrap();
        connection
            .execute(
                "UPDATE schema_migrations SET name='unknown' WHERE id=10",
                [],
            )
            .unwrap();
        drop(connection);

        assert!(
            open(&path).is_err(),
            "current schema must reject a corrupted intermediate migration name"
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn verified_backup_restores_application_state_after_database_loss() {
        let dir = std::env::temp_dir().join(format!("lingua-restore-{}", uuid::Uuid::new_v4()));
        let path = dir.join("lingua.sqlite3");
        let connection = open(&path).unwrap();
        connection
            .execute(
                "INSERT INTO learner_profile(id,created_at,updated_at)
                 VALUES('learner-restore','2026-10-23T12:00:00Z','2026-10-23T12:00:00Z')",
                [],
            )
            .unwrap();
        drop(connection);

        let saved = backup(&path).unwrap();
        fs::remove_file(&path).unwrap();
        fs::copy(&saved, &path).unwrap();

        let restored = open(&path).unwrap();
        validate(&restored).unwrap();
        let learner_id: String = restored
            .query_row(
                "SELECT id FROM learner_profile WHERE id='learner-restore'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(learner_id, "learner-restore");

        drop(restored);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn startup_backup_is_created_once_per_day() {
        let dir = std::env::temp_dir().join(format!("lingua-auto-{}", uuid::Uuid::new_v4()));
        let path = dir.join("lingua.sqlite3");
        let connection = open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE snapshot_probe(value TEXT);
                                   INSERT INTO snapshot_probe VALUES ('durable');",
            )
            .unwrap();
        drop(connection);
        let saved = backup_if_due(&path).unwrap().unwrap();
        assert!(saved.is_file());
        assert!(backup_if_due(&path).unwrap().is_none());
        let value: String = Connection::open(saved)
            .unwrap()
            .query_row("SELECT value FROM snapshot_probe", [], |row| row.get(0))
            .unwrap();
        assert_eq!(value, "durable");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn migrates_v1_database_to_current() {
        let dir = std::env::temp_dir().join(format!("lingua-migrate-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("lingua.sqlite3");
        let connection = Connection::open(&path).unwrap();
        connection.execute_batch(LEGACY_INIT).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE schema_migrations(
                    id INTEGER PRIMARY KEY,
                    name TEXT UNIQUE NOT NULL,
                    applied_at TEXT DEFAULT CURRENT_TIMESTAMP NOT NULL
                 );
                 INSERT INTO schema_migrations(id,name) VALUES (1,'0001_init.sql');
                 PRAGMA user_version=1;",
            )
            .unwrap();
        drop(connection);

        let migrated = open(&path).unwrap();
        let version: i64 = migrated
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        let table_exists: i64 = migrated
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='planner_decisions')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let latest: String = migrated
            .query_row(
                "SELECT name FROM schema_migrations ORDER BY id DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
        assert_eq!(table_exists, 1);
        assert_eq!(latest, SCHEMA_NAME);
        drop(migrated);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn migrates_v2_database_to_current() {
        let dir = std::env::temp_dir().join(format!("lingua-migrate-v2-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("lingua.sqlite3");
        let connection = Connection::open(&path).unwrap();
        connection.execute_batch(LEGACY_INIT).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE schema_migrations(
                    id INTEGER PRIMARY KEY,
                    name TEXT UNIQUE NOT NULL,
                    applied_at TEXT DEFAULT CURRENT_TIMESTAMP NOT NULL
                 );
                 INSERT INTO schema_migrations(id,name) VALUES (1,'0001_init.sql');",
            )
            .unwrap();
        connection.execute_batch(MIGRATION_2).unwrap();
        connection
            .execute_batch(
                "INSERT INTO schema_migrations(id,name) VALUES (2,'0002_planner_history.sql');
                 PRAGMA user_version=2;",
            )
            .unwrap();
        drop(connection);

        let migrated = open(&path).unwrap();
        let version: i64 = migrated
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        let budget_table: i64 = migrated
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master
                 WHERE type='table' AND name='review_daily_budget')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
        assert_eq!(budget_table, 1);
        drop(migrated);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn migrates_v3_database_to_current() {
        let dir = std::env::temp_dir().join(format!("lingua-migrate-v3-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("lingua.sqlite3");
        let connection = Connection::open(&path).unwrap();
        connection.execute_batch(LEGACY_INIT).unwrap();
        connection.execute_batch("CREATE TABLE schema_migrations(id INTEGER PRIMARY KEY,name TEXT UNIQUE NOT NULL,applied_at TEXT DEFAULT CURRENT_TIMESTAMP NOT NULL); INSERT INTO schema_migrations(id,name) VALUES (1,'0001_init.sql');").unwrap();
        connection.execute_batch(MIGRATION_2).unwrap();
        connection.execute_batch(MIGRATION_3).unwrap();
        connection.execute_batch("INSERT INTO schema_migrations(id,name) VALUES (2,'0002_planner_history.sql'); INSERT INTO schema_migrations(id,name) VALUES (3,'0003_review_methodology.sql'); PRAGMA user_version=3;").unwrap();
        drop(connection);
        let migrated = open(&path).unwrap();
        let version: i64 = migrated
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        let task_column: i64 = migrated.query_row("SELECT COUNT(*) FROM pragma_table_info('lesson_metrics') WHERE name='task_success_count'", [], |row| row.get(0)).unwrap();
        let listening_column: i64 = migrated.query_row("SELECT COUNT(*) FROM pragma_table_info('listening_attempts') WHERE name='source_variant_id'", [], |row| row.get(0)).unwrap();
        assert_eq!(version, SCHEMA_VERSION);
        assert_eq!(task_column, 1);
        assert_eq!(listening_column, 1);
        drop(migrated);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn migrates_v5_pack_storage_to_language_neutral_schema_without_data_loss() {
        let dir = std::env::temp_dir().join(format!("lingua-migrate-v5-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("lingua.sqlite3");
        let connection = Connection::open(&path).unwrap();
        connection.execute_batch(LEGACY_INIT).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE schema_migrations(
                id INTEGER PRIMARY KEY,
                name TEXT UNIQUE NOT NULL,
                applied_at TEXT DEFAULT CURRENT_TIMESTAMP NOT NULL
             );
             INSERT INTO schema_migrations(id,name) VALUES (1,'0001_init.sql');",
            )
            .unwrap();
        connection.execute_batch(MIGRATION_2).unwrap();
        connection.execute_batch(MIGRATION_3).unwrap();
        connection.execute_batch(MIGRATION_4).unwrap();
        connection.execute_batch(MIGRATION_5).unwrap();
        connection
            .execute_batch(
                "INSERT INTO schema_migrations(id,name) VALUES (2,'0002_planner_history.sql');
             INSERT INTO schema_migrations(id,name) VALUES (3,'0003_review_methodology.sql');
             INSERT INTO schema_migrations(id,name) VALUES (4,'0004_methodology_calibration.sql');
             INSERT INTO schema_migrations(id,name) VALUES (5,'0005_methodology_semantics.sql');
             INSERT INTO language_packs
                (id,package_name,version,contract_version,target_language,target_locale,
                 display_name,installed_at,updated_at)
             VALUES('es-ES','legacy','0.2.0',1,'es','es-ES','Spanish','a','a');
             INSERT INTO learner_profile(id,created_at,updated_at)
             VALUES('learner-1','a','a');
             INSERT INTO learner_language_profiles
                (id,learner_id,language_pack_id,source_language,target_locale,created_at,updated_at)
             VALUES('llp-1','learner-1','es-ES','ru','es-ES','a','a');
             INSERT INTO cards
                (id,learner_language_profile_id,language_pack_id,language_pack_version,
                 intent,status,prompt,prompt_language,answer,answer_language,
                 accepted_answers_json,answer_evaluation_mode,creation_reason,
                 content_hash,created_by,created_at,updated_at)
             VALUES('card-1','llp-1','es-ES','0.2.0','production','new','p','ru','respuesta',
                    'es-ES','[]','exact','legacy','h','system','a','a');
             PRAGMA user_version=5;",
            )
            .unwrap();
        drop(connection);

        let migrated = open(&path).unwrap();
        let version: i64 = migrated
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        let pack_table: i64 = migrated
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='language_packs')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let profile_pack_column: i64 = migrated
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('learner_language_profiles') WHERE name='language_pack_id'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let card_pack_columns: i64 = migrated
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('cards')
                 WHERE name IN ('language_pack_id','language_pack_version')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let (source, target): (String, String) = migrated
            .query_row(
                "SELECT source_language,target_locale FROM learner_language_profiles WHERE id='llp-1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        let answer: String = migrated
            .query_row("SELECT answer FROM cards WHERE id='card-1'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
        assert_eq!(pack_table, 0);
        assert_eq!(profile_pack_column, 0);
        assert_eq!(card_pack_columns, 0);
        assert_eq!((source.as_str(), target.as_str()), ("ru", "es-ES"));
        assert_eq!(answer, "respuesta");
        drop(migrated);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn migrates_v6_semantic_metadata_without_data_loss() {
        let dir = std::env::temp_dir().join(format!("lingua-migrate-v6-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("lingua.sqlite3");
        let connection = Connection::open(&path).unwrap();
        connection.execute_batch(LEGACY_INIT).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE schema_migrations(
                id INTEGER PRIMARY KEY,
                name TEXT UNIQUE NOT NULL,
                applied_at TEXT DEFAULT CURRENT_TIMESTAMP NOT NULL
             );
             INSERT INTO schema_migrations(id,name) VALUES (1,'0001_init.sql');",
            )
            .unwrap();
        connection.execute_batch(MIGRATION_2).unwrap();
        connection.execute_batch(MIGRATION_3).unwrap();
        connection.execute_batch(MIGRATION_4).unwrap();
        connection.execute_batch(MIGRATION_5).unwrap();
        connection.execute_batch(MIGRATION_6).unwrap();
        connection
            .execute_batch(
                "INSERT INTO schema_migrations(id,name) VALUES (2,'0002_planner_history.sql');
             INSERT INTO schema_migrations(id,name) VALUES (3,'0003_review_methodology.sql');
             INSERT INTO schema_migrations(id,name) VALUES (4,'0004_methodology_calibration.sql');
             INSERT INTO schema_migrations(id,name) VALUES (5,'0005_methodology_semantics.sql');
             INSERT INTO schema_migrations(id,name) VALUES (6,'0006_language_neutral_storage.sql');
             INSERT INTO learner_profile(id,created_at,updated_at)
             VALUES('learner-1','a','a');
             INSERT INTO learner_language_profiles
                (id,learner_id,source_language,target_locale,created_at,updated_at)
             VALUES('llp-1','learner-1','ru','es-ES','a','a');
             INSERT INTO learning_items
                (id,learner_language_profile_id,item_type,target_text,normalized_target_text,
                 status,language_metadata_json,created_at,updated_at)
             VALUES('li-1','llp-1','phrase','hola','hola','learning','{\"legacy\":true}','a','a');
             INSERT INTO lessons
                (id,learner_language_profile_id,lesson_number,status,scenario,planned_goal,
                 source,created_at,updated_at)
             VALUES('lesson-1','llp-1',1,'completed','s','g','pwa','a','a');
             INSERT INTO learning_item_evidence
                (id,learning_item_id,lesson_id,kind,success,detail,pack_metadata_json,recorded_at)
             VALUES('ev-1','li-1','lesson-1','transfer',1,'d',
                    '{\"context_family\":\"service.request_bill\"}','a');
             INSERT INTO error_targets
                (id,learner_language_profile_id,core_category,pack_error_code,
                 original_production,corrected_production,created_at,updated_at)
             VALUES('et-1','llp-1','grammar','verb.copula','wrong','right','a','a');
             INSERT INTO cards
                (id,learner_language_profile_id,intent,status,prompt,prompt_language,answer,
                 answer_language,accepted_answers_json,answer_evaluation_mode,creation_reason,
                 content_hash,created_by,created_at,updated_at)
             VALUES('card-1','llp-1','production','new','p','ru','a','es-ES','[]',
                    'exact','legacy','h','system','a','a');
             INSERT INTO card_schedule_state
                (card_id,algorithm,algorithm_version,parameter_set_id,difficulty,stability,
                 last_review_at,due_at,scheduled_interval_seconds,lapse_count,review_count,
                 state_version,state_hash,updated_at)
             VALUES('card-1','fsrs','fsrs-rs-6.6.1',NULL,NULL,NULL,NULL,
                    '2026-05-28T20:26:40Z',NULL,0,0,1,'76f85c283c7af879','a');
             PRAGMA user_version=6;",
            )
            .unwrap();
        drop(connection);

        let migrated = open(&path).unwrap();
        let version: i64 = migrated
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        let error_code: Option<String> = migrated
            .query_row(
                "SELECT error_code FROM error_targets WHERE id='et-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let metadata: Option<String> = migrated
            .query_row(
                "SELECT metadata_json FROM learning_item_evidence WHERE id='ev-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let dead_card_column: i64 = migrated
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('cards') WHERE name='answer_evaluation_mode'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let dead_item_column: i64 = migrated
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('learning_items') WHERE name='language_metadata_json'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let scheduler_table: i64 = migrated
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='scheduler_parameter_sets')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let parameter_column: i64 = migrated
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('card_schedule_state') WHERE name='parameter_set_id'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let target_text: String = migrated
            .query_row(
                "SELECT target_text FROM learning_items WHERE id='li-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let answer: String = migrated
            .query_row("SELECT answer FROM cards WHERE id='card-1'", [], |row| {
                row.get(0)
            })
            .unwrap();
        let stored_schedule_hash: String = migrated
            .query_row(
                "SELECT state_hash FROM card_schedule_state WHERE card_id='card-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let migrated_schedule = review_engine::ScheduledState {
            algorithm: review_engine::ALGORITHM.to_owned(),
            algorithm_version: review_engine::ALGORITHM_VERSION.to_owned(),
            difficulty: None,
            stability: None,
            last_review_at: None,
            due_at: Some(review_engine::Timestamp(1_780_000_000)),
            scheduled_interval_seconds: None,
            lapse_count: 0,
            review_count: 0,
            state_version: 1,
        };

        assert_eq!(version, SCHEMA_VERSION);
        assert_eq!(error_code.as_deref(), Some("verb.copula"));
        assert_eq!(
            metadata.as_deref(),
            Some("{\"context_family\":\"service.request_bill\"}")
        );
        assert_eq!(dead_card_column, 0);
        assert_eq!(dead_item_column, 0);
        assert_eq!(scheduler_table, 0);
        assert_eq!(parameter_column, 0);
        assert_eq!(
            stored_schedule_hash,
            review_engine::state_hash(&migrated_schedule),
            "v8 cleanup must preserve the v7 optimistic-concurrency hash"
        );
        assert_eq!(target_text, "hola");
        assert_eq!(answer, "a");
        drop(migrated);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn migrates_v10_planner_rows_without_fabricating_tutor_task_snapshots() {
        let dir = std::env::temp_dir().join(format!("lingua-migrate-v10-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("lingua.sqlite3");
        let connection = Connection::open(&path).unwrap();
        connection.execute_batch(LEGACY_INIT).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE schema_migrations(
                    id INTEGER PRIMARY KEY,
                    name TEXT UNIQUE NOT NULL,
                    applied_at TEXT DEFAULT CURRENT_TIMESTAMP NOT NULL
                 );
                 INSERT INTO schema_migrations(id,name) VALUES (1,'0001_init.sql');",
            )
            .unwrap();
        for (version, sql, name) in [
            (2, MIGRATION_2, "0002_planner_history.sql"),
            (3, MIGRATION_3, "0003_review_methodology.sql"),
            (4, MIGRATION_4, "0004_methodology_calibration.sql"),
            (5, MIGRATION_5, "0005_methodology_semantics.sql"),
            (6, MIGRATION_6, "0006_language_neutral_storage.sql"),
            (7, MIGRATION_7, "0007_semantic_metadata.sql"),
            (8, MIGRATION_8, "0008_remove_review_experiments.sql"),
            (9, MIGRATION_9, "0009_error_recheck_cycles.sql"),
            (10, MIGRATION_10, "0010_error_recheck_repair_action.sql"),
        ] {
            connection.execute_batch(sql).unwrap();
            connection
                .execute(
                    "INSERT INTO schema_migrations(id,name) VALUES(?1,?2)",
                    rusqlite::params![version, name],
                )
                .unwrap();
        }
        connection
            .execute_batch(
                "INSERT INTO learner_profile(id,created_at,updated_at)
                 VALUES('learner-1','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z');
                 INSERT INTO learner_language_profiles
                    (id,learner_id,source_language,target_locale,created_at,updated_at)
                 VALUES('llp-1','learner-1','ru','es-ES',
                    '2026-01-01T00:00:00Z','2026-01-01T00:00:00Z');
                 INSERT INTO planner_decisions
                    (id,learner_language_profile_id,lesson_id,planner_version,
                     input_json,plan_json,task_candidates_json,capability_state_json,
                     focus_threads_json,outcome_json,created_at,updated_at)
                 VALUES('pd-legacy','llp-1',NULL,'planner-v3',
                        '{}','{}','[]','[]','[]',NULL,
                        '2026-01-01T00:00:00Z','2026-01-01T00:00:00Z');
                 PRAGMA user_version=10;",
            )
            .unwrap();
        drop(connection);

        let migrated = open(&path).unwrap();
        let tasks: Option<String> = migrated
            .query_row(
                "SELECT tutor_tasks_json FROM planner_decisions WHERE id='pd-legacy'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            tasks, None,
            "migration must not fabricate an authoritative empty tutor-task snapshot for legacy decisions"
        );
        drop(migrated);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn migrates_v8_recheck_history_without_preserving_premature_pending_cycles() {
        let dir = std::env::temp_dir().join(format!("lingua-migrate-v8-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("lingua.sqlite3");
        let connection = Connection::open(&path).unwrap();
        connection.execute_batch(LEGACY_INIT).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE schema_migrations(
                    id INTEGER PRIMARY KEY,
                    name TEXT UNIQUE NOT NULL,
                    applied_at TEXT DEFAULT CURRENT_TIMESTAMP NOT NULL
                 );
                 INSERT INTO schema_migrations(id,name) VALUES (1,'0001_init.sql');",
            )
            .unwrap();
        for (version, sql, name) in [
            (2, MIGRATION_2, "0002_planner_history.sql"),
            (3, MIGRATION_3, "0003_review_methodology.sql"),
            (4, MIGRATION_4, "0004_methodology_calibration.sql"),
            (5, MIGRATION_5, "0005_methodology_semantics.sql"),
            (6, MIGRATION_6, "0006_language_neutral_storage.sql"),
            (7, MIGRATION_7, "0007_semantic_metadata.sql"),
            (8, MIGRATION_8, "0008_remove_review_experiments.sql"),
        ] {
            connection.execute_batch(sql).unwrap();
            connection
                .execute(
                    "INSERT INTO schema_migrations(id,name) VALUES(?1,?2)",
                    rusqlite::params![version, name],
                )
                .unwrap();
        }
        connection
            .execute_batch(
                "INSERT INTO learner_profile(id,created_at,updated_at)
                 VALUES('learner-1','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z');
                 INSERT INTO learner_language_profiles
                    (id,learner_id,source_language,target_locale,created_at,updated_at)
                 VALUES('llp-1','learner-1','ru','es-ES',
                    '2026-01-01T00:00:00Z','2026-01-01T00:00:00Z');
                 INSERT INTO error_targets
                    (id,learner_language_profile_id,core_category,original_production,
                     corrected_production,occurrence_count,method_change_required,
                     status,created_at,updated_at)
                 VALUES
                    ('et-1','llp-1','grammar','wrong','right',3,0,'recheck',
                     '2026-01-01T00:00:00Z','2026-01-01T00:00:00Z'),
                    ('et-incidental','llp-1','grammar','one-off','correct',1,0,'active',
                     '2026-01-01T00:00:00Z','2026-01-01T00:00:00Z');
                 INSERT INTO error_rechecks
                    (id,error_target_id,offset_days,due_at,completed_at,passed)
                 VALUES
                    ('rc-1','et-1',1,'2026-01-02T00:00:00Z','2026-01-02T00:00:00Z',1),
                    ('rc-3','et-1',3,'2026-01-04T00:00:00Z',NULL,NULL),
                    ('rc-incidental','et-incidental',1,'2026-01-02T00:00:00Z',NULL,NULL);
                 PRAGMA user_version=8;",
            )
            .unwrap();
        drop(connection);

        let migrated = open(&path).unwrap();
        let version: i64 = migrated
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        let rows: Vec<(String, u32, u32, Option<u8>)> = migrated
            .prepare(
                "SELECT id,cycle,offset_days,passed
                 FROM error_rechecks ORDER BY offset_days",
            )
            .unwrap()
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();

        let repair_action_column: u32 = migrated
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('error_rechecks') WHERE name='repair_action'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
        assert_eq!(repair_action_column, 1);
        assert_eq!(
            rows,
            vec![
                ("rc-1".to_owned(), 0, 1, Some(1)),
                ("rc-3".to_owned(), 0, 3, None),
            ]
        );
        migrated
            .execute(
                "INSERT INTO error_rechecks
                 (id,error_target_id,cycle,offset_days,due_at)
                 VALUES('rc-next','et-1',1,1,'2026-01-05T00:00:00Z')",
                [],
            )
            .unwrap();
        drop(migrated);
        fs::remove_dir_all(dir).unwrap();
    }
}

// FUZZ SUPPORT BELOW: excluded from production line coverage.

/// Historical schemas, atomic migration failure, backup/restore and signing-key lifecycle.
#[cfg(fuzzing)]
#[allow(clippy::expect_used, missing_docs)]
pub(crate) fn fuzz_storage(data: &[u8]) {
    let seed = data.first().copied().unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("lingua-storage-fuzz-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).expect("directory");
    let path = dir.join("db.sqlite3");
    let version = 1 + i64::from(seed % 10);
    let db = Connection::open(&path).expect("db");
    db.execute_batch(LEGACY_INIT).expect("v1 schema");
    db.execute_batch("CREATE TABLE schema_migrations(id INTEGER PRIMARY KEY,name TEXT UNIQUE NOT NULL,applied_at TEXT DEFAULT CURRENT_TIMESTAMP NOT NULL); INSERT INTO schema_migrations(id,name) VALUES(1,'0001_init.sql');PRAGMA user_version=1;").expect("history");
    for (v, sql, name) in [
        (2, MIGRATION_2, "0002_planner_history.sql"),
        (3, MIGRATION_3, "0003_review_methodology.sql"),
        (4, MIGRATION_4, "0004_methodology_calibration.sql"),
        (5, MIGRATION_5, "0005_methodology_semantics.sql"),
        (6, MIGRATION_6, "0006_language_neutral_storage.sql"),
        (7, MIGRATION_7, "0007_semantic_metadata.sql"),
        (8, MIGRATION_8, "0008_remove_review_experiments.sql"),
        (9, MIGRATION_9, "0009_error_recheck_cycles.sql"),
        (10, MIGRATION_10, "0010_error_recheck_repair_action.sql"),
    ] {
        if v <= version {
            db.execute_batch(sql).expect("seed schema");
            db.execute(
                "INSERT INTO schema_migrations(id,name) VALUES(?1,?2)",
                rusqlite::params![v, name],
            )
            .expect("migration history");
            db.pragma_update(None, "user_version", v)
                .expect("schema version");
        }
    }
    drop(db);
    let migrated = open(&path).expect("migrate historical schema");
    validate(&migrated).expect("migration integrity");
    drop(migrated);
    let env = environment(&path).expect("environment");
    assert_eq!(env.db_path(), path);
    drop(env);
    let key = signing_key(&dir).expect("key");
    assert_eq!(key, signing_key(&dir).expect("stable key"));
    let saved = backup_if_due(&path).expect("backup").expect("first backup");
    assert!(backup_if_due(&path).expect("backup due").is_none());
    let old = SystemTime::now() - Duration::from_secs(91 * 86400);
    fs::File::open(&saved)
        .expect("backup file")
        .set_times(fs::FileTimes::new().set_modified(old))
        .expect("age backup");
    let second = backup(&path).expect("backup retention");
    assert!(!saved.exists());
    fs::remove_file(&path).expect("remove original");
    fs::copy(&second, &path).expect("restore");
    drop(open(&path).expect("restored backup"));
    assert!(backup(&dir.join("missing.sqlite3")).is_err());
    fs::write(dir.join("secret.key"), b"short").expect("short key");
    assert!(environment(&path).is_err());
    let bad = dir.join("unknown.sqlite3");
    Connection::open(&bad)
        .expect("unknown db")
        .execute_batch("CREATE TABLE language_packs(id TEXT); PRAGMA user_version=99;")
        .expect("unknown schema");
    assert!(open(&bad).is_err());
    let current = Connection::open(&path).expect("current db");
    current
        .execute(
            "UPDATE schema_migrations SET name='unknown' WHERE id=10",
            [],
        )
        .expect("unknown migration name");
    drop(current);
    assert!(open(&path).is_err());
    // Force a migration failure after BEGIN; the published version must not move.
    let broken = dir.join("rollback.sqlite3");
    let db = Connection::open(&broken).expect("rollback db");
    db.execute_batch(INIT).expect("init");
    db.execute_batch("CREATE TABLE schema_migrations(id INTEGER PRIMARY KEY,name TEXT UNIQUE NOT NULL);INSERT INTO schema_migrations VALUES(1,'0001_init.sql');PRAGMA user_version=1; CREATE TABLE planner_decisions(id TEXT);").expect("conflicting migration");
    drop(db);
    assert!(open(&broken).is_err());
    let db = Connection::open(&broken).expect("rollback db");
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .expect("version"),
        1
    );
    drop(db);

    // Exercise rollback paths at every migration boundary.
    let init_broken = dir.join("init-rollback.sqlite3");
    Connection::open(&init_broken)
        .expect("init rollback db")
        .execute_batch("CREATE TABLE learner_profile(id TEXT); PRAGMA user_version=0;")
        .expect("conflicting initial schema");
    assert!(open(&init_broken).is_err());

    let v2_broken = dir.join("v2-rollback.sqlite3");
    let db = Connection::open(&v2_broken).expect("v2 rollback db");
    db.execute_batch(INIT).expect("v1");
    db.execute_batch("CREATE TABLE schema_migrations(id INTEGER PRIMARY KEY,name TEXT UNIQUE NOT NULL,applied_at TEXT DEFAULT CURRENT_TIMESTAMP NOT NULL);INSERT INTO schema_migrations VALUES(1,'0001_init.sql',CURRENT_TIMESTAMP);")
        .expect("history");
    db.execute_batch(MIGRATION_2).expect("v2");
    db.execute_batch("INSERT INTO schema_migrations(id,name) VALUES(2,'0002_planner_history.sql');PRAGMA user_version=2;CREATE TABLE review_daily_budget(id TEXT);")
        .expect("v2 conflict");
    drop(db);
    assert!(open(&v2_broken).is_err());

    let v3_broken = dir.join("v3-rollback.sqlite3");
    let db = Connection::open(&v3_broken).expect("v3 rollback db");
    db.execute_batch(INIT).expect("v1");
    db.execute_batch("CREATE TABLE schema_migrations(id INTEGER PRIMARY KEY,name TEXT UNIQUE NOT NULL,applied_at TEXT DEFAULT CURRENT_TIMESTAMP NOT NULL);INSERT INTO schema_migrations VALUES(1,'0001_init.sql',CURRENT_TIMESTAMP);")
        .expect("history");
    db.execute_batch(MIGRATION_2).expect("v2");
    db.execute_batch(MIGRATION_3).expect("v3");
    db.execute_batch("INSERT INTO schema_migrations(id,name) VALUES(2,'0002_planner_history.sql');INSERT INTO schema_migrations(id,name) VALUES(3,'0003_review_methodology.sql');PRAGMA user_version=3;ALTER TABLE lesson_metrics ADD COLUMN task_attempt_count INTEGER NOT NULL DEFAULT 0;")
        .expect("v3 conflict");
    drop(db);
    assert!(open(&v3_broken).is_err());

    let v4_broken = dir.join("v4-rollback.sqlite3");
    let db = Connection::open(&v4_broken).expect("v4 rollback db");
    db.execute_batch(INIT).expect("v1");
    db.execute_batch("CREATE TABLE schema_migrations(id INTEGER PRIMARY KEY,name TEXT UNIQUE NOT NULL,applied_at TEXT DEFAULT CURRENT_TIMESTAMP NOT NULL);INSERT INTO schema_migrations VALUES(1,'0001_init.sql',CURRENT_TIMESTAMP);")
        .expect("history");
    db.execute_batch(MIGRATION_2).expect("v2");
    db.execute_batch(MIGRATION_3).expect("v3");
    db.execute_batch(MIGRATION_4).expect("v4");
    db.execute_batch("INSERT INTO schema_migrations(id,name) VALUES(2,'0002_planner_history.sql');INSERT INTO schema_migrations(id,name) VALUES(3,'0003_review_methodology.sql');INSERT INTO schema_migrations(id,name) VALUES(4,'0004_methodology_calibration.sql');PRAGMA user_version=4;ALTER TABLE learning_item_evidence ADD COLUMN observation_order INTEGER NOT NULL DEFAULT 0;")
        .expect("v4 conflict");
    drop(db);
    assert!(open(&v4_broken).is_err());

    // Foreign-key validation must fail closed even when SQLite allowed the bad insert.
    let invalid_fk = Connection::open_in_memory().expect("fk validation db");
    invalid_fk
        .execute_batch(
            "PRAGMA foreign_keys=OFF;
             CREATE TABLE parent(id INTEGER PRIMARY KEY);
             CREATE TABLE child(parent_id INTEGER REFERENCES parent(id));
             INSERT INTO child(parent_id) VALUES(99);",
        )
        .expect("invalid fk fixture");
    assert!(validate(&invalid_fk).is_err());

    let missing_history = dir.join("missing-history.sqlite3");
    drop(open(&missing_history).expect("initialize missing-history fixture"));
    Connection::open(&missing_history)
        .expect("missing-history db")
        .execute("DELETE FROM schema_migrations WHERE id=5", [])
        .expect("delete migration history row");
    assert!(open(&missing_history).is_err());

    let extra_history = dir.join("extra-history.sqlite3");
    drop(open(&extra_history).expect("initialize extra-history fixture"));
    Connection::open(&extra_history)
        .expect("extra-history db")
        .execute(
            "INSERT INTO schema_migrations(id,name) VALUES(12,'0012_unknown.sql')",
            [],
        )
        .expect("append unsupported migration history row");
    assert!(open(&extra_history).is_err());

    if seed & 7 == 0 {
        let assets = dir.join("assets");
        match crate::assets::write_to(&assets) {
            Ok(()) => assert!(assets.join("index.html").is_file()),
            Err(error) => assert_eq!(error, "embedded web assets are missing index.html"),
        }
    }
    fs::remove_dir_all(dir).expect("cleanup");
}
