//! Local SQLite initialization, validation, and consistent backups.

use crate::Env;
use rusqlite::{Connection, MAIN_DB, OptionalExtension};
use std::fs::{self, DirBuilder, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const INIT: &str = include_str!("../migrations/0001_init.sql");
const MIGRATION_2: &str = include_str!("../migrations/0002_planner_history.sql");
const MIGRATION_3: &str = include_str!("../migrations/0003_review_methodology.sql");
const MIGRATION_4: &str = include_str!("../migrations/0004_methodology_calibration.sql");
const MIGRATION_5: &str = include_str!("../migrations/0005_methodology_semantics.sql");
const SCHEMA_VERSION: i64 = 5;
const SCHEMA_NAME: &str = "0005_methodology_semantics.sql";

fn private_dir(path: &Path) -> Result<(), String> {
    let mut builder = DirBuilder::new();
    builder
        .recursive(true)
        .mode(0o700)
        .create(path)
        .map_err(|e| e.to_string())
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

/// Open a fresh or existing local SQLite database. Unknown schemas fail closed.
pub fn open(path: &Path) -> Result<Connection, String> {
    private_dir(path.parent().ok_or("database path has no parent")?)?;
    let connection = Connection::open(path).map_err(|e| e.to_string())?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    connection
        .execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL;")
        .map_err(|e| e.to_string())?;

    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;
    let has_schema = table_exists(&connection, "language_packs")?;

    match (version, has_schema) {
        (0, false) => initialize(&connection)?,
        (1, true) => {
            migrate_v1_to_v2(&connection)?;
            migrate_v2_to_v3(&connection)?;
            migrate_v3_to_v4(&connection)?;
            migrate_v4_to_v5(&connection)?;
        }
        (2, true) => {
            migrate_v2_to_v3(&connection)?;
            migrate_v3_to_v4(&connection)?;
            migrate_v4_to_v5(&connection)?;
        }
        (3, true) => {
            migrate_v3_to_v4(&connection)?;
            migrate_v4_to_v5(&connection)?;
        }
        (4, true) => migrate_v4_to_v5(&connection)?,
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
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
        {
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
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&target)
        .map_err(|e| e.to_string())?;
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
        fs::set_permissions(&existing, fs::Permissions::from_mode(0o755)).unwrap();
        open(&existing.join("child.sqlite3")).unwrap();
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
        connection.execute_batch(INIT).unwrap();
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
        assert_eq!(version, 5);
        assert_eq!(table_exists, 1);
        assert_eq!(latest, "0005_methodology_semantics.sql");
        drop(migrated);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn migrates_v2_database_to_current() {
        let dir = std::env::temp_dir().join(format!("lingua-migrate-v2-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("lingua.sqlite3");
        let connection = Connection::open(&path).unwrap();
        connection.execute_batch(INIT).unwrap();
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
        assert_eq!(version, 5);
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
        connection.execute_batch(INIT).unwrap();
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
        assert_eq!(version, 5);
        assert_eq!(task_column, 1);
        assert_eq!(listening_column, 1);
        drop(migrated);
        fs::remove_dir_all(dir).unwrap();
    }
}

// FUZZ SUPPORT BELOW: excluded from production line coverage.

/// Historical schemas, atomic migration failure, backup/restore and signing-key lifecycle.
#[cfg(feature = "fuzzing")]
#[allow(clippy::expect_used, missing_docs)]
pub(crate) fn fuzz_storage(data: &[u8]) {
    let seed = data.first().copied().unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("lingua-storage-fuzz-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).expect("directory");
    let path = dir.join("db.sqlite3");
    let version = 1 + i64::from(seed % 5);
    let db = Connection::open(&path).expect("db");
    db.execute_batch(INIT).expect("v1 schema");
    db.execute_batch("CREATE TABLE schema_migrations(id INTEGER PRIMARY KEY,name TEXT UNIQUE NOT NULL,applied_at TEXT DEFAULT CURRENT_TIMESTAMP NOT NULL); INSERT INTO schema_migrations(id,name) VALUES(1,'0001_init.sql');PRAGMA user_version=1;").expect("history");
    for (v, sql, name) in [
        (2, MIGRATION_2, "0002_planner_history.sql"),
        (3, MIGRATION_3, "0003_review_methodology.sql"),
        (4, MIGRATION_4, "0004_methodology_calibration.sql"),
        (5, MIGRATION_5, SCHEMA_NAME),
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
        .execute("UPDATE schema_migrations SET name='unknown' WHERE id=5", [])
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
