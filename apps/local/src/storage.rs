//! Local SQLite initialization, validation, and consistent backups.

use crate::Env;
use rusqlite::{Connection, MAIN_DB, OptionalExtension};
use std::fs::{self, DirBuilder, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const INIT: &str = include_str!("../../../migrations/0001_init.sql");
const MIGRATION_2: &str = include_str!("../../../migrations/0002_planner_history.sql");
const SCHEMA_VERSION: i64 = 2;
const SCHEMA_NAME: &str = "0002_planner_history.sql";

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
        (1, true) => migrate_v1_to_v2(&connection)?,
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
    validate(&copy)?;

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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
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
    fn migrates_v1_database_to_v2() {
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
        assert_eq!(version, 2);
        assert_eq!(table_exists, 1);
        assert_eq!(latest, "0002_planner_history.sql");
        drop(migrated);
        fs::remove_dir_all(dir).unwrap();
    }
}
