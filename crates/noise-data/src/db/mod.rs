//! SQLite database interface.

use rusqlite::Connection;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DbError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Migration failed: {0}")]
    Migration(String),
}

/// Application database wrapper.
pub struct Database {
    conn: Connection,
}

impl Database {
    /// Open (or create) a project database at `path`.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DbError> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        let db = Self { conn };
        db.run_migrations()?;
        Ok(db)
    }

    /// Open an in-memory database (for tests).
    pub fn open_in_memory() -> Result<Self, DbError> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        let db = Self { conn };
        db.run_migrations()?;
        Ok(db)
    }

    fn run_migrations(&self) -> Result<(), DbError> {
        self.conn.execute_batch(include_str!("migrations/001_initial.sql"))
            .map_err(|e| DbError::Migration(e.to_string()))
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_db_opens_successfully() {
        let db = Database::open_in_memory();
        assert!(db.is_ok(), "Failed: {:?}", db.err());
    }
}
