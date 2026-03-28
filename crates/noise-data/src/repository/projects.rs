//! CRUD for projects and scenarios.

use super::RepoError;
use crate::scenario::{Project, Scenario, ScenarioVariant};
use rusqlite::{Connection, params};
use uuid::Uuid;

pub struct ProjectRepository<'conn> {
    conn: &'conn Connection,
}

impl<'conn> ProjectRepository<'conn> {
    pub fn new(conn: &'conn Connection) -> Self {
        Self { conn }
    }

    // ── Project CRUD ──────────────────────────────────────────────────────────

    /// Upsert a project: insert if new, update metadata if already exists.
    /// Does NOT cascade-delete child rows (uses INSERT OR IGNORE + UPDATE).
    pub fn insert(&self, project: &Project) -> Result<(), RepoError> {
        let id_str = project.id.to_string();
        // Insert without replacing (so FK children survive).
        self.conn.execute(
            "INSERT OR IGNORE INTO projects (id, name, description, crs_epsg, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                id_str, project.name, project.description,
                project.crs_epsg, project.created_at, project.updated_at,
            ],
        )?;
        // Update mutable metadata if row already existed.
        self.conn.execute(
            "UPDATE projects SET name=?1, description=?2, updated_at=?3 WHERE id=?4",
            params![project.name, project.description, project.updated_at, id_str],
        )?;
        // Upsert base scenario.
        self.upsert_scenario(project.id, &project.base_scenario, true)?;
        // Upsert variants.
        for variant in &project.variants {
            self.upsert_variant(project.id, variant)?;
        }
        Ok(())
    }

    pub fn get(&self, project_id: Uuid) -> Result<Project, RepoError> {
        let id_str = project_id.to_string();
        let (name, description, crs_epsg, created_at, updated_at): (String, String, u32, String, String) =
            self.conn.query_row(
                "SELECT name, description, crs_epsg, created_at, updated_at FROM projects WHERE id=?1",
                params![id_str],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            ).map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => RepoError::NotFound(0),
                other => RepoError::Sqlite(other),
            })?;

        let base_scenario = self.get_base_scenario(&id_str)?;
        let variants = self.list_variants(&id_str)?;

        Ok(Project { id: project_id, name, description, created_at, updated_at, crs_epsg, base_scenario, variants })
    }

    pub fn list_all(&self) -> Result<Vec<(Uuid, String)>, RepoError> {
        let mut stmt = self.conn.prepare("SELECT id, name FROM projects ORDER BY created_at")?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        rows.map(|r| r.map_err(RepoError::Sqlite))
            .map(|r| r.and_then(|(id_str, name)| {
                Uuid::parse_str(&id_str)
                    .map(|id| (id, name))
                    .map_err(|_| RepoError::Validation("Invalid UUID in database".into()))
            }))
            .collect()
    }

    pub fn delete(&self, project_id: Uuid) -> Result<(), RepoError> {
        let changed = self.conn.execute(
            "DELETE FROM projects WHERE id=?1",
            params![project_id.to_string()],
        )?;
        if changed == 0 { return Err(RepoError::NotFound(0)); }
        Ok(())
    }

    /// Delete a specific (non-base) scenario variant by its UUID.
    ///
    /// Cascades to `scene_objects` and `calculation_results` via FK ON DELETE CASCADE.
    /// Returns `Err(NotFound)` if the row does not exist or is a base scenario.
    pub fn delete_variant(&self, variant_id: Uuid) -> Result<(), RepoError> {
        let changed = self.conn.execute(
            "DELETE FROM scenarios WHERE id=?1 AND is_base=0",
            params![variant_id.to_string()],
        )?;
        if changed == 0 { return Err(RepoError::NotFound(0)); }
        Ok(())
    }

    // ── Scenario helpers ──────────────────────────────────────────────────────

    fn upsert_scenario(&self, project_id: Uuid, scenario: &Scenario, is_base: bool) -> Result<(), RepoError> {
        self.conn.execute(
            "INSERT OR IGNORE INTO scenarios (id, project_id, name, description, is_base)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                scenario.id.to_string(), project_id.to_string(),
                scenario.name, scenario.description, is_base as i32,
            ],
        )?;
        self.conn.execute(
            "UPDATE scenarios SET name=?1 WHERE id=?2",
            params![scenario.name, scenario.id.to_string()],
        )?;
        Ok(())
    }

    fn upsert_variant(&self, project_id: Uuid, variant: &ScenarioVariant) -> Result<(), RepoError> {
        let overrides_json = serde_json::to_string(&variant.overrides)?;
        let packed = format!("{}|{}", variant.strategy_notes, overrides_json);
        self.conn.execute(
            "INSERT OR IGNORE INTO scenarios
             (id, project_id, name, description, is_base, parent_id)
             VALUES (?1, ?2, ?3, ?4, 0, ?5)",
            params![
                variant.id.to_string(), project_id.to_string(),
                variant.name, packed, variant.parent_scenario_id.to_string(),
            ],
        )?;
        self.conn.execute(
            "UPDATE scenarios SET name=?1, description=?2 WHERE id=?3",
            params![variant.name, packed, variant.id.to_string()],
        )?;
        Ok(())
    }

    fn get_base_scenario(&self, project_id: &str) -> Result<Scenario, RepoError> {
        self.conn.query_row(
            "SELECT id, name, description FROM scenarios WHERE project_id=?1 AND is_base=1",
            params![project_id],
            |r| {
                let id_str: String = r.get(0)?;
                Ok(Scenario {
                    id: Uuid::parse_str(&id_str).unwrap_or_default(),
                    name: r.get(1)?,
                    description: r.get(2)?,
                })
            },
        ).map_err(|_| RepoError::ScenarioNotFound(project_id.into()))
    }

    fn list_variants(&self, project_id: &str) -> Result<Vec<ScenarioVariant>, RepoError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, description, parent_id FROM scenarios
             WHERE project_id=?1 AND is_base=0 ORDER BY rowid",
        )?;
        let rows = stmt.query_map(params![project_id], |r| {
            let id_str: String = r.get(0)?;
            let parent_str: Option<String> = r.get(3)?;
            Ok((id_str, r.get::<_, String>(1)?, r.get::<_, String>(2)?, parent_str))
        })?;

        let mut variants = Vec::new();
        for row in rows {
            let (id_str, name, desc_packed, parent_str) = row?;
            let parent_id = parent_str
                .and_then(|s| Uuid::parse_str(&s).ok())
                .unwrap_or_default();
            // Unpack strategy_notes|overrides_json from description column.
            let (notes, overrides) = if let Some((n, oj)) = desc_packed.split_once('|') {
                let overrides = serde_json::from_str(oj).unwrap_or_default();
                (n.to_owned(), overrides)
            } else {
                (desc_packed, Vec::new())
            };
            variants.push(ScenarioVariant {
                id: Uuid::parse_str(&id_str).unwrap_or_default(),
                name,
                description: String::new(),
                parent_scenario_id: parent_id,
                strategy_notes: notes,
                overrides,
            });
        }
        Ok(variants)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::scenario::Project;

    fn make_db_and_project() -> (Database, Project) {
        let db = Database::open_in_memory().unwrap();
        let mut p = Project::new("Downtown Ring Road", 32650);
        p.add_variant("Variant A: Barrier");
        p.add_variant("Variant B: Green Wall");
        (db, p)
    }

    #[test]
    fn insert_and_get_roundtrip() {
        let (db, project) = make_db_and_project();
        let repo = ProjectRepository::new(db.connection());
        repo.insert(&project).unwrap();
        let fetched = repo.get(project.id).unwrap();
        assert_eq!(fetched.name, project.name);
        assert_eq!(fetched.crs_epsg, 32650);
        assert_eq!(fetched.variants.len(), 2);
    }

    #[test]
    fn list_all_returns_inserted_projects() {
        let (db, project) = make_db_and_project();
        let repo = ProjectRepository::new(db.connection());
        repo.insert(&project).unwrap();
        let list = repo.list_all().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].1, "Downtown Ring Road");
    }

    #[test]
    fn delete_removes_project() {
        let (db, project) = make_db_and_project();
        let repo = ProjectRepository::new(db.connection());
        repo.insert(&project).unwrap();
        repo.delete(project.id).unwrap();
        let list = repo.list_all().unwrap();
        assert!(list.is_empty());
    }

    #[test]
    fn variant_strategy_notes_persisted() {
        let (db, mut project) = make_db_and_project();
        project.variants[0].strategy_notes = "Install 5m concrete barrier along N side".into();
        let repo = ProjectRepository::new(db.connection());
        repo.insert(&project).unwrap();
        let fetched = repo.get(project.id).unwrap();
        assert!(fetched.variants[0].strategy_notes.contains("5m concrete"));
    }
}
