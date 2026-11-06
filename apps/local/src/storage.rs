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
const SCHEMA_VERSION: i64 = 7;
const SCHEMA_NAME: &str = "0007_semantic_metadata.sql";

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
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
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
        }
        (2, true) => {
            migrate_v2_to_v3(&connection)?;
            migrate_v3_to_v4(&connection)?;
            migrate_v4_to_v5(&connection)?;
            migrate_v5_to_v6(&connection)?;
            migrate_v6_to_v7(&connection)?;
        }
        (3, true) => {
            migrate_v3_to_v4(&connection)?;
            migrate_v4_to_v5(&connection)?;
            migrate_v5_to_v6(&connection)?;
            migrate_v6_to_v7(&connection)?;
        }
        (4, true) => {
            migrate_v4_to_v5(&connection)?;
            migrate_v5_to_v6(&connection)?;
            migrate_v6_to_v7(&connection)?;
        }
        (5, true) => {
            migrate_v5_to_v6(&connection)?;
            migrate_v6_to_v7(&connection)?;
        }
        (6, true) => migrate_v6_to_v7(&connection)?,
        (SCHEMA_VERSION, true) => {
            let name: String = connection
                .query_row(
                    "SELECT name FROM schema_migrations ORDER BY id DESC LIMIT 1",
                    [],
                    |row| row.get(0),
                )
                .map_err(|e| format!("unknown database schema: {e}"))?;
            if name != SCHEMA_NAME {
                return Err(format!("unsupported SQLite migration: {name}"));
            }
        }
        _ => return Err(format!("unsupported SQLite schema version {version}")),
    }

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
        assert_eq!(version, 7);
        assert_eq!(table_exists, 1);
        assert_eq!(latest, "0007_semantic_metadata.sql");
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
        assert_eq!(version, 7);
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
        assert_eq!(version, 7);
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

        assert_eq!(version, 7);
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

        assert_eq!(version, 7);
        assert_eq!(error_code.as_deref(), Some("verb.copula"));
        assert_eq!(
            metadata.as_deref(),
            Some("{\"context_family\":\"service.request_bill\"}")
        );
        assert_eq!(dead_card_column, 0);
        assert_eq!(dead_item_column, 0);
        assert_eq!(target_text, "hola");
        assert_eq!(answer, "a");
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
    let version = 1 + i64::from(seed % 5);
    let db = Connection::open(&path).expect("db");
    db.execute_batch(LEGACY_INIT).expect("v1 schema");
    db.execute_batch("CREATE TABLE schema_migrations(id INTEGER PRIMARY KEY,name TEXT UNIQUE NOT NULL,applied_at TEXT DEFAULT CURRENT_TIMESTAMP NOT NULL); INSERT INTO schema_migrations(id,name) VALUES(1,'0001_init.sql');PRAGMA user_version=1;").expect("history");
    for (v, sql, name) in [
        (2, MIGRATION_2, "0002_planner_history.sql"),
        (3, MIGRATION_3, "0003_review_methodology.sql"),
        (4, MIGRATION_4, "0004_methodology_calibration.sql"),
        (5, MIGRATION_5, "0005_methodology_semantics.sql"),
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
        .execute("UPDATE schema_migrations SET name='unknown' WHERE id=7", [])
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

    if seed & 7 == 0 {
        crate::assets::write_to(&dir.join("assets")).expect("embedded assets");
    }
    fs::remove_dir_all(dir).expect("cleanup");
}
