//! Local process primitives: request-local SQLite environment and operation results.
#![allow(missing_docs)]

use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{message}")]
    Operation {
        status: u16,
        code: &'static str,
        message: String,
        details: serde_json::Value,
    },
    #[error("{0}")]
    RustError(String),
}

impl Error {
    pub fn operation(
        status: u16,
        code: &'static str,
        message: impl Into<String>,
        details: serde_json::Value,
    ) -> Self {
        Self::Operation {
            status,
            code,
            message: message.into(),
            details,
        }
    }
}

impl From<rusqlite::Error> for Error {
    fn from(error: rusqlite::Error) -> Self {
        Self::RustError(error.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Self::RustError(error.to_string())
    }
}

#[derive(Clone)]
pub struct Env {
    db_path: PathBuf,
    signing_key: String,
}

impl Env {
    pub fn new(db_path: PathBuf, signing_key: String) -> Self {
        Self {
            db_path,
            signing_key,
        }
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    pub fn connection(&self) -> Result<Connection> {
        let connection = Connection::open(&self.db_path)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.execute_batch("PRAGMA foreign_keys=ON;")?;
        Ok(connection)
    }

    pub fn signing_key(&self) -> &str {
        &self.signing_key
    }
}

pub fn json<T: serde::Serialize + ?Sized>(value: &T) -> Result<serde_json::Value> {
    Ok(serde_json::to_value(value)?)
}
