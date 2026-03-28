//! CRUD repository for the `users` table.
//!
//! Uses prepared statements throughout — no string concatenation in queries.

use super::RepoError;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A persisted platform user.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoredUser {
    pub id: String,
    pub username: String,
    pub password_hash: String,
    pub email: String,
    /// Role string: "admin" | "analyst" | "viewer".
    pub role: String,
    pub created_at: String,
    pub last_login_at: Option<String>,
}

impl StoredUser {
    pub fn new(
        username: impl Into<String>,
        password_hash: impl Into<String>,
        email: impl Into<String>,
        role: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            username: username.into(),
            password_hash: password_hash.into(),
            email: email.into(),
            role: role.into(),
            created_at: utc_now(),
            last_login_at: None,
        }
    }
}

fn utc_now() -> String {
    // Use SystemTime for a simple ISO-8601 timestamp without chrono dep.
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Format as YYYY-MM-DDTHH:MM:SSZ
    let s = d % 60;
    let m = (d / 60) % 60;
    let h = (d / 3600) % 24;
    let days = d / 86400;
    // Epoch day → approximate calendar (good enough for audit timestamps).
    let y400 = days / 146097;
    let rem  = days % 146097;
    let y100 = (rem / 36524).min(3);
    let rem  = rem - y100 * 36524;
    let y4   = rem / 1461;
    let rem  = rem % 1461;
    let y1   = (rem / 365).min(3);
    let year = y400 * 400 + y100 * 100 + y4 * 4 + y1 + 1970;
    let yday = rem - y1 * 365;
    let (month, mday) = day_of_year_to_month(yday, is_leap(year));
    format!("{year:04}-{month:02}-{mday:02}T{h:02}:{m:02}:{s:02}Z")
}

fn is_leap(y: u64) -> bool { y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) }

fn day_of_year_to_month(yday: u64, leap: bool) -> (u64, u64) {
    let days = if leap {
        [31,29,31,30,31,30,31,31,30,31,30,31u64]
    } else {
        [31,28,31,30,31,30,31,31,30,31,30,31u64]
    };
    let mut rem = yday;
    for (i, &d) in days.iter().enumerate() {
        if rem < d { return (i as u64 + 1, rem + 1); }
        rem -= d;
    }
    (12, 31)
}

pub struct UserRepository<'conn> {
    conn: &'conn Connection,
}

impl<'conn> UserRepository<'conn> {
    pub fn new(conn: &'conn Connection) -> Self {
        Self { conn }
    }

    /// Insert a new user. Fails if username or email already taken.
    pub fn insert(&self, user: &StoredUser) -> Result<(), RepoError> {
        self.conn.execute(
            "INSERT INTO users (id, username, password_hash, email, role, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                user.id, user.username, user.password_hash,
                user.email, user.role, user.created_at
            ],
        )?;
        Ok(())
    }

    /// Fetch a user by their UUID primary key.
    pub fn get_by_id(&self, id: &str) -> Result<StoredUser, RepoError> {
        self.conn.query_row(
            "SELECT id, username, password_hash, email, role, created_at, last_login_at
             FROM users WHERE id=?1",
            params![id],
            row_to_user,
        ).map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows =>
                RepoError::Validation(format!("User id={id} not found")),
            other => RepoError::Sqlite(other),
        })
    }

    /// Fetch a user by their unique username.
    pub fn get_by_username(&self, username: &str) -> Result<StoredUser, RepoError> {
        self.conn.query_row(
            "SELECT id, username, password_hash, email, role, created_at, last_login_at
             FROM users WHERE username=?1",
            params![username],
            row_to_user,
        ).map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows =>
                RepoError::Validation(format!("User '{username}' not found")),
            other => RepoError::Sqlite(other),
        })
    }

    /// Fetch a user by their unique email.
    pub fn get_by_email(&self, email: &str) -> Result<StoredUser, RepoError> {
        self.conn.query_row(
            "SELECT id, username, password_hash, email, role, created_at, last_login_at
             FROM users WHERE email=?1",
            params![email],
            row_to_user,
        ).map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows =>
                RepoError::Validation(format!("Email '{email}' not found")),
            other => RepoError::Sqlite(other),
        })
    }

    /// List all users (admin view).
    pub fn list(&self) -> Result<Vec<StoredUser>, RepoError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, username, password_hash, email, role, created_at, last_login_at
             FROM users ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], row_to_user)?;
        rows.map(|r| r.map_err(RepoError::Sqlite)).collect()
    }

    /// Update a user's role (admin only).
    pub fn update_role(&self, id: &str, role: &str) -> Result<(), RepoError> {
        let changed = self.conn.execute(
            "UPDATE users SET role=?1 WHERE id=?2",
            params![role, id],
        )?;
        if changed == 0 {
            return Err(RepoError::Validation(format!("User id={id} not found")));
        }
        Ok(())
    }

    /// Record last-login timestamp.
    pub fn update_last_login(&self, id: &str) -> Result<(), RepoError> {
        self.conn.execute(
            "UPDATE users SET last_login_at=?1 WHERE id=?2",
            params![utc_now(), id],
        )?;
        Ok(())
    }

    /// Delete a user by ID.
    pub fn delete(&self, id: &str) -> Result<(), RepoError> {
        let changed = self.conn.execute("DELETE FROM users WHERE id=?1", params![id])?;
        if changed == 0 {
            return Err(RepoError::Validation(format!("User id={id} not found")));
        }
        Ok(())
    }

    /// Return the count of users with a given role.
    pub fn count_by_role(&self, role: &str) -> Result<usize, RepoError> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM users WHERE role=?1",
            params![role],
            |r| r.get(0),
        )?;
        Ok(n as usize)
    }
}

fn row_to_user(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredUser> {
    Ok(StoredUser {
        id:            row.get(0)?,
        username:      row.get(1)?,
        password_hash: row.get(2)?,
        email:         row.get(3)?,
        role:          row.get(4)?,
        created_at:    row.get(5)?,
        last_login_at: row.get(6)?,
    })
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    fn setup() -> Database {
        Database::open_in_memory().unwrap()
    }

    fn alice() -> StoredUser {
        StoredUser::new("alice", "$argon2id$...", "alice@example.com", "analyst")
    }

    #[test]
    fn insert_and_get_by_id() {
        let db = setup();
        let repo = UserRepository::new(db.connection());
        let u = alice();
        repo.insert(&u).unwrap();
        let fetched = repo.get_by_id(&u.id).unwrap();
        assert_eq!(fetched.username, "alice");
        assert_eq!(fetched.role, "analyst");
    }

    #[test]
    fn get_by_username() {
        let db = setup();
        let repo = UserRepository::new(db.connection());
        let u = alice();
        repo.insert(&u).unwrap();
        let fetched = repo.get_by_username("alice").unwrap();
        assert_eq!(fetched.id, u.id);
    }

    #[test]
    fn get_by_email() {
        let db = setup();
        let repo = UserRepository::new(db.connection());
        repo.insert(&alice()).unwrap();
        let fetched = repo.get_by_email("alice@example.com").unwrap();
        assert_eq!(fetched.username, "alice");
    }

    #[test]
    fn duplicate_username_returns_error() {
        let db = setup();
        let repo = UserRepository::new(db.connection());
        repo.insert(&alice()).unwrap();
        let result = repo.insert(&alice()); // same username + email
        assert!(result.is_err());
    }

    #[test]
    fn list_returns_all_users() {
        let db = setup();
        let repo = UserRepository::new(db.connection());
        repo.insert(&alice()).unwrap();
        repo.insert(&StoredUser::new("bob", "h", "bob@b.com", "viewer")).unwrap();
        assert_eq!(repo.list().unwrap().len(), 2);
    }

    #[test]
    fn update_role() {
        let db = setup();
        let repo = UserRepository::new(db.connection());
        let u = alice();
        repo.insert(&u).unwrap();
        repo.update_role(&u.id, "admin").unwrap();
        let fetched = repo.get_by_id(&u.id).unwrap();
        assert_eq!(fetched.role, "admin");
    }

    #[test]
    fn delete_user() {
        let db = setup();
        let repo = UserRepository::new(db.connection());
        let u = alice();
        repo.insert(&u).unwrap();
        repo.delete(&u.id).unwrap();
        assert!(repo.get_by_id(&u.id).is_err());
    }

    #[test]
    fn count_by_role() {
        let db = setup();
        let repo = UserRepository::new(db.connection());
        repo.insert(&alice()).unwrap();
        repo.insert(&StoredUser::new("bob",  "h", "bob@b.com",   "viewer")).unwrap();
        repo.insert(&StoredUser::new("carl", "h", "carl@c.com",  "viewer")).unwrap();
        assert_eq!(repo.count_by_role("viewer").unwrap(),  2);
        assert_eq!(repo.count_by_role("analyst").unwrap(), 1);
        assert_eq!(repo.count_by_role("admin").unwrap(),   0);
    }
}
