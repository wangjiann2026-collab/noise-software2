//! SQLite database wrapper with migration support.

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

    /// Open an in-memory database (tests).
    pub fn open_in_memory() -> Result<Self, DbError> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        let db = Self { conn };
        db.run_migrations()?;
        Ok(db)
    }

    fn run_migrations(&self) -> Result<(), DbError> {
        // Run all migrations in order.
        for sql in MIGRATIONS {
            self.conn
                .execute_batch(sql)
                .map_err(|e| DbError::Migration(e.to_string()))?;
        }
        Ok(())
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }
}

/// All SQL migrations in ascending order. Each is idempotent (CREATE IF NOT EXISTS).
const MIGRATIONS: &[&str] = &[
    include_str!("migrations/001_initial.sql"),
    include_str!("migrations/002_indexes.sql"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_db_opens_successfully() {
        assert!(Database::open_in_memory().is_ok());
    }
}
