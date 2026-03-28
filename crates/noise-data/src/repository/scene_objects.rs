//! CRUD for scene objects (all entity types unified in one table).

use super::RepoError;
use crate::entities::{ObjectType, SceneObject};
use rusqlite::{Connection, params};

pub struct SceneObjectRepository<'conn> {
    conn: &'conn Connection,
}

impl<'conn> SceneObjectRepository<'conn> {
    pub fn new(conn: &'conn Connection) -> Self {
        Self { conn }
    }

    // ── Insert ────────────────────────────────────────────────────────────────

    /// Insert a new scene object. Returns the auto-assigned row ID.
    pub fn insert(&self, scenario_id: &str, obj: &SceneObject) -> Result<i64, RepoError> {
        let obj_type = obj.object_type().as_str();
        let name = obj.name();
        let data = serde_json::to_string(obj)?;
        let wkt = centroid_wkt(obj);

        self.conn.execute(
            "INSERT INTO scene_objects (scenario_id, object_type, name, geometry_wkt, data_json)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![scenario_id, obj_type, name, wkt, data],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    // ── Select ────────────────────────────────────────────────────────────────

    /// Fetch a single object by row ID.
    pub fn get(&self, row_id: i64) -> Result<SceneObject, RepoError> {
        let data: String = self.conn.query_row(
            "SELECT data_json FROM scene_objects WHERE id = ?1",
            params![row_id],
            |row| row.get(0),
        ).map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => RepoError::NotFound(row_id as u64),
            other => RepoError::Sqlite(other),
        })?;
        Ok(serde_json::from_str(&data)?)
    }

    /// List all objects for a scenario, optionally filtered by type.
    pub fn list(
        &self,
        scenario_id: &str,
        object_type: Option<ObjectType>,
    ) -> Result<Vec<(i64, SceneObject)>, RepoError> {
        let rows = self.list_raw(scenario_id, object_type)?;
        rows.into_iter()
            .map(|(id, json)| Ok((id, serde_json::from_str(&json)?)))
            .collect()
    }

    fn list_raw(
        &self,
        scenario_id: &str,
        object_type: Option<ObjectType>,
    ) -> Result<Vec<(i64, String)>, RepoError> {
        if let Some(ot) = object_type {
            let mut stmt = self.conn.prepare(
                "SELECT id, data_json FROM scene_objects
                 WHERE scenario_id = ?1 AND object_type = ?2
                 ORDER BY id",
            )?;
            let rows: Vec<(i64, String)> = stmt
                .query_map(params![scenario_id, ot.as_str()], |r| {
                    Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
                })?
                .collect::<Result<_, _>>()?;
            Ok(rows)
        } else {
            let mut stmt = self.conn.prepare(
                "SELECT id, data_json FROM scene_objects
                 WHERE scenario_id = ?1
                 ORDER BY id",
            )?;
            let rows: Vec<(i64, String)> = stmt
                .query_map(params![scenario_id], |r| {
                    Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
                })?
                .collect::<Result<_, _>>()?;
            Ok(rows)
        }
    }

    /// Count objects in a scenario by type.
    pub fn count(&self, scenario_id: &str, object_type: Option<ObjectType>) -> Result<u64, RepoError> {
        let count: i64 = match object_type {
            Some(ot) => self.conn.query_row(
                "SELECT COUNT(*) FROM scene_objects WHERE scenario_id = ?1 AND object_type = ?2",
                params![scenario_id, ot.as_str()],
                |r| r.get(0),
            )?,
            None => self.conn.query_row(
                "SELECT COUNT(*) FROM scene_objects WHERE scenario_id = ?1",
                params![scenario_id],
                |r| r.get(0),
            )?,
        };
        Ok(count as u64)
    }

    // ── Update ────────────────────────────────────────────────────────────────

    pub fn update(&self, row_id: i64, obj: &SceneObject) -> Result<(), RepoError> {
        let data = serde_json::to_string(obj)?;
        let wkt = centroid_wkt(obj);
        let changed = self.conn.execute(
            "UPDATE scene_objects SET name=?1, geometry_wkt=?2, data_json=?3 WHERE id=?4",
            params![obj.name(), wkt, data, row_id],
        )?;
        if changed == 0 {
            return Err(RepoError::NotFound(row_id as u64));
        }
        Ok(())
    }

    // ── Delete ────────────────────────────────────────────────────────────────

    pub fn delete(&self, row_id: i64) -> Result<(), RepoError> {
        let changed = self.conn.execute(
            "DELETE FROM scene_objects WHERE id = ?1",
            params![row_id],
        )?;
        if changed == 0 {
            return Err(RepoError::NotFound(row_id as u64));
        }
        Ok(())
    }

    /// Delete all objects of a given type from a scenario.
    pub fn delete_by_type(&self, scenario_id: &str, object_type: ObjectType) -> Result<u64, RepoError> {
        let changed = self.conn.execute(
            "DELETE FROM scene_objects WHERE scenario_id=?1 AND object_type=?2",
            params![scenario_id, object_type.as_str()],
        )?;
        Ok(changed as u64)
    }

    // ── Spatial search (approximate, using geometry_wkt point) ───────────────

    /// Find all objects within a bounding box (XY only, point geometry only).
    pub fn find_in_bbox(
        &self,
        scenario_id: &str,
        xmin: f64, ymin: f64,
        xmax: f64, ymax: f64,
    ) -> Result<Vec<(i64, SceneObject)>, RepoError> {
        // geometry_wkt stored as "POINT(x y)" for point objects.
        let rows: Vec<(i64, String, Option<String>)> = {
            let mut stmt = self.conn.prepare(
                "SELECT id, data_json, geometry_wkt FROM scene_objects
                 WHERE scenario_id=?1 AND geometry_wkt IS NOT NULL
                 ORDER BY id",
            )?;
            let collected: Vec<(i64, String, Option<String>)> = stmt
                .query_map(params![scenario_id], |r| {
                    Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?))
                })?
                .collect::<Result<_, _>>()?;
            collected
        };

        let mut result = Vec::new();
        for (id, json, wkt_opt) in rows {
            if let Some(wkt) = wkt_opt {
                if let Some((x, y)) = parse_point_wkt(&wkt) {
                    if x >= xmin && x <= xmax && y >= ymin && y <= ymax {
                        result.push((id, serde_json::from_str(&json)?));
                    }
                }
            }
        }
        Ok(result)
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn centroid_wkt(obj: &SceneObject) -> Option<String> {
    obj.centroid().map(|p| format!("POINT({} {})", p.x, p.y))
}

fn parse_point_wkt(wkt: &str) -> Option<(f64, f64)> {
    // Format: "POINT(x y)"
    let inner = wkt.strip_prefix("POINT(")?.strip_suffix(')')?;
    let mut parts = inner.split_whitespace();
    let x = parts.next()?.parse().ok()?;
    let y = parts.next()?.parse().ok()?;
    Some((x, y))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::entities::{ReceiverPoint, SceneObject};
    use crate::scenario::Project;
    use nalgebra::Point3;

    fn setup() -> (Database, String) {
        let db = Database::open_in_memory().unwrap();
        let mut proj = Project::new("Test", 32650);
        let scenario_id = proj.base_scenario.id.to_string();
        // Insert project & scenario into DB.
        let pr = ProjectRepository::new(db.connection());
        pr.insert(&proj).unwrap();
        (db, scenario_id)
    }

    use crate::repository::ProjectRepository;

    fn receiver(id: u64, x: f64, y: f64) -> SceneObject {
        SceneObject::Receiver(ReceiverPoint::new(id, format!("R{id}"), x, y, 0.0, 4.0))
    }

    #[test]
    fn insert_and_get_receiver() {
        let (db, sid) = setup();
        let repo = SceneObjectRepository::new(db.connection());
        let obj = receiver(1, 100.0, 200.0);
        let row_id = repo.insert(&sid, &obj).unwrap();
        let fetched = repo.get(row_id).unwrap();
        assert_eq!(fetched.name(), "R1");
    }

    #[test]
    fn list_filters_by_type() {
        let (db, sid) = setup();
        let repo = SceneObjectRepository::new(db.connection());
        repo.insert(&sid, &receiver(1, 0.0, 0.0)).unwrap();
        repo.insert(&sid, &receiver(2, 10.0, 0.0)).unwrap();
        let all = repo.list(&sid, None).unwrap();
        let receivers = repo.list(&sid, Some(ObjectType::Receiver)).unwrap();
        let buildings = repo.list(&sid, Some(ObjectType::Building)).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(receivers.len(), 2);
        assert_eq!(buildings.len(), 0);
    }

    #[test]
    fn update_changes_name() {
        let (db, sid) = setup();
        let repo = SceneObjectRepository::new(db.connection());
        let row_id = repo.insert(&sid, &receiver(1, 0.0, 0.0)).unwrap();
        let updated = SceneObject::Receiver(ReceiverPoint::new(1, "Renamed", 0.0, 0.0, 0.0, 4.0));
        repo.update(row_id, &updated).unwrap();
        let fetched = repo.get(row_id).unwrap();
        assert_eq!(fetched.name(), "Renamed");
    }

    #[test]
    fn delete_removes_object() {
        let (db, sid) = setup();
        let repo = SceneObjectRepository::new(db.connection());
        let row_id = repo.insert(&sid, &receiver(1, 0.0, 0.0)).unwrap();
        repo.delete(row_id).unwrap();
        assert!(matches!(repo.get(row_id), Err(RepoError::NotFound(_))));
    }

    #[test]
    fn count_returns_correct_value() {
        let (db, sid) = setup();
        let repo = SceneObjectRepository::new(db.connection());
        for i in 0..5 {
            repo.insert(&sid, &receiver(i, i as f64 * 10.0, 0.0)).unwrap();
        }
        assert_eq!(repo.count(&sid, None).unwrap(), 5);
        assert_eq!(repo.count(&sid, Some(ObjectType::Receiver)).unwrap(), 5);
        assert_eq!(repo.count(&sid, Some(ObjectType::Building)).unwrap(), 0);
    }

    #[test]
    fn find_in_bbox_spatial_filter() {
        let (db, sid) = setup();
        let repo = SceneObjectRepository::new(db.connection());
        repo.insert(&sid, &receiver(1, 10.0,  10.0)).unwrap();
        repo.insert(&sid, &receiver(2, 50.0,  50.0)).unwrap();
        repo.insert(&sid, &receiver(3, 200.0, 200.0)).unwrap();
        let found = repo.find_in_bbox(&sid, 0.0, 0.0, 100.0, 100.0).unwrap();
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn delete_nonexistent_returns_not_found() {
        let (db, sid) = setup();
        let _ = sid;
        let repo = SceneObjectRepository::new(db.connection());
        assert!(matches!(repo.delete(9999), Err(RepoError::NotFound(_))));
    }
}
