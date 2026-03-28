//! CRUD for calculation results.

use super::RepoError;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalculationResult {
    pub id: i64,
    pub scenario_id: String,
    pub grid_type: String,
    pub metric: String,
    pub calculated_at: String,
    /// Grid results encoded as JSON (Vec<f32> in row-major order).
    pub data: serde_json::Value,
}

pub struct CalculationRepository<'conn> {
    conn: &'conn Connection,
}

impl<'conn> CalculationRepository<'conn> {
    pub fn new(conn: &'conn Connection) -> Self {
        Self { conn }
    }

    pub fn insert(
        &self,
        scenario_id: &str,
        grid_type: &str,
        metric: &str,
        data: &serde_json::Value,
    ) -> Result<i64, RepoError> {
        let now = "2026-01-01T00:00:00Z"; // placeholder; real impl uses chrono
        let data_json = serde_json::to_string(data)?;
        self.conn.execute(
            "INSERT INTO calculation_results
             (scenario_id, grid_type, metric, calculated_at, result_json)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![scenario_id, grid_type, metric, now, data_json],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get(&self, calc_id: i64) -> Result<CalculationResult, RepoError> {
        self.conn.query_row(
            "SELECT id, scenario_id, grid_type, metric, calculated_at, result_json
             FROM calculation_results WHERE id=?1",
            params![calc_id],
            |r| {
                Ok(CalculationResult {
                    id: r.get(0)?,
                    scenario_id: r.get(1)?,
                    grid_type: r.get(2)?,
                    metric: r.get(3)?,
                    calculated_at: r.get(4)?,
                    data: serde_json::from_str(&r.get::<_, String>(5)?)
                        .unwrap_or(serde_json::Value::Null),
                })
            },
        ).map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => RepoError::NotFound(calc_id as u64),
            other => RepoError::Sqlite(other),
        })
    }

    pub fn list_for_scenario(&self, scenario_id: &str) -> Result<Vec<CalculationResult>, RepoError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, scenario_id, grid_type, metric, calculated_at, result_json
             FROM calculation_results WHERE scenario_id=?1 ORDER BY calculated_at DESC",
        )?;
        let rows = stmt.query_map(params![scenario_id], |r| {
            Ok(CalculationResult {
                id: r.get(0)?,
                scenario_id: r.get(1)?,
                grid_type: r.get(2)?,
                metric: r.get(3)?,
                calculated_at: r.get(4)?,
                data: serde_json::from_str(&r.get::<_, String>(5)?)
                    .unwrap_or(serde_json::Value::Null),
            })
        })?;
        rows.map(|r| r.map_err(RepoError::Sqlite)).collect()
    }

    pub fn delete(&self, calc_id: i64) -> Result<(), RepoError> {
        let changed = self.conn.execute(
            "DELETE FROM calculation_results WHERE id=?1",
            params![calc_id],
        )?;
        if changed == 0 { return Err(RepoError::NotFound(calc_id as u64)); }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::repository::ProjectRepository;
    use crate::scenario::Project;
    use serde_json::json;

    fn setup() -> (Database, String) {
        let db = Database::open_in_memory().unwrap();
        let p = Project::new("P", 32650);
        let sid = p.base_scenario.id.to_string();
        ProjectRepository::new(db.connection()).insert(&p).unwrap();
        (db, sid)
    }

    #[test]
    fn insert_and_get_result() {
        let (db, sid) = setup();
        let repo = CalculationRepository::new(db.connection());
        let data = json!({ "grid": [55.0, 60.0, 65.0] });
        let id = repo.insert(&sid, "horizontal", "Lden", &data).unwrap();
        let fetched = repo.get(id).unwrap();
        assert_eq!(fetched.metric, "Lden");
        assert_eq!(fetched.grid_type, "horizontal");
    }

    #[test]
    fn list_for_scenario_returns_all() {
        let (db, sid) = setup();
        let repo = CalculationRepository::new(db.connection());
        repo.insert(&sid, "horizontal", "Ld", &json!({})).unwrap();
        repo.insert(&sid, "horizontal", "Ln", &json!({})).unwrap();
        assert_eq!(repo.list_for_scenario(&sid).unwrap().len(), 2);
    }
}
