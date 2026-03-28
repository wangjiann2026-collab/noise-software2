//! Scene object CRUD REST API routes.
//!
//! Scene objects (point sources, buildings, barriers, receivers …) belong to a
//! specific scenario within a project and are persisted via
//! [`SceneObjectRepository`].
//!
//! # Routes
//! | Method | Path                                          | Description         |
//! |--------|-----------------------------------------------|---------------------|
//! | GET    | /projects/:pid/scenarios/:sid/objects         | List objects        |
//! | POST   | /projects/:pid/scenarios/:sid/objects         | Create object       |
//! | GET    | /projects/:pid/scenarios/:sid/objects/:oid    | Get one object      |
//! | PUT    | /projects/:pid/scenarios/:sid/objects/:oid    | Replace object      |
//! | DELETE | /projects/:pid/scenarios/:sid/objects/:oid    | Delete object       |
//!
//! # Object JSON format
//! Objects are serialised as a tagged enum — the `"type"` field selects the
//! variant.  Example point source:
//! ```json
//! {
//!   "type": "point_source",
//!   "id": 1,
//!   "name": "Cooling fan",
//!   "position": { "x": 100.0, "y": 200.0, "z": 0.5 },
//!   "lw_db":    [85,85,85,85,85,85,85,85],
//!   "lwa_db":   92.0,
//!   "directivity_index_db": null
//! }
//! ```

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use noise_data::{
    entities::{ObjectType, SceneObject},
    repository::SceneObjectRepository,
};

use crate::state::AppState;

/// Query params for listing objects.
#[derive(Debug, Deserialize)]
pub struct ListObjectsQuery {
    /// Filter by object type (e.g. `point_source`, `building`).
    #[serde(rename = "type")]
    pub object_type: Option<String>,
}

/// Compact per-object summary returned in list responses.
#[derive(Debug, Serialize)]
pub struct ObjectSummary {
    /// Database row ID (used for GET/PUT/DELETE).
    pub row_id:      i64,
    pub object_type: String,
    pub name:        String,
    pub object_id:   u64,
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

/// `GET /projects/:pid/scenarios/:sid/objects[?type=…]`
pub async fn list_objects(
    State(state): State<AppState>,
    Path((_pid, sid)): Path<(String, String)>,
    Query(q): Query<ListObjectsQuery>,
) -> Result<Json<Vec<ObjectSummary>>, (StatusCode, Json<serde_json::Value>)> {
    let filter = q.object_type.as_deref()
        .and_then(ObjectType::from_str);
    // Warn (but don't error) when an unrecognised type string is given.
    if q.object_type.is_some() && filter.is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!(
                    "Unknown object type '{}'. Valid types: point_source, road_source, \
                     railway_source, line_source, receiver, building, barrier, bridge, …",
                    q.object_type.unwrap()
                )
            })),
        ));
    }

    let db = state.db.lock().map_err(internal)?;
    let repo = SceneObjectRepository::new(db.connection());
    let objects = repo.list(&sid, filter).map_err(repo_err)?;

    let summaries = objects.into_iter().map(|(row_id, obj)| ObjectSummary {
        row_id,
        object_type: obj.object_type().as_str().into(),
        name:        obj.name().into(),
        object_id:   obj.id(),
    }).collect();

    Ok(Json(summaries))
}

/// `POST /projects/:pid/scenarios/:sid/objects`
///
/// Body: a complete [`SceneObject`] JSON (tagged with `"type"`).
pub async fn create_object(
    State(state): State<AppState>,
    Path((_pid, sid)): Path<(String, String)>,
    Json(body): Json<SceneObject>,
) -> Result<(StatusCode, Json<ObjectSummary>), (StatusCode, Json<serde_json::Value>)> {
    let db = state.db.lock().map_err(internal)?;
    let repo = SceneObjectRepository::new(db.connection());
    let row_id = repo.insert(&sid, &body).map_err(repo_err)?;

    Ok((
        StatusCode::CREATED,
        Json(ObjectSummary {
            row_id,
            object_type: body.object_type().as_str().into(),
            name:        body.name().into(),
            object_id:   body.id(),
        }),
    ))
}

/// `GET /projects/:pid/scenarios/:sid/objects/:oid`
pub async fn get_object(
    State(state): State<AppState>,
    Path((_pid, _sid, row_id)): Path<(String, String, i64)>,
) -> Result<Json<SceneObject>, (StatusCode, Json<serde_json::Value>)> {
    let db = state.db.lock().map_err(internal)?;
    let repo = SceneObjectRepository::new(db.connection());
    let obj = repo.get(row_id).map_err(|_| (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({ "error": format!("Object row {row_id} not found") })),
    ))?;
    Ok(Json(obj))
}

/// `PUT /projects/:pid/scenarios/:sid/objects/:oid`
///
/// Replaces the stored object with the supplied body.
pub async fn update_object(
    State(state): State<AppState>,
    Path((_pid, _sid, row_id)): Path<(String, String, i64)>,
    Json(body): Json<SceneObject>,
) -> Result<Json<ObjectSummary>, (StatusCode, Json<serde_json::Value>)> {
    let db = state.db.lock().map_err(internal)?;
    let repo = SceneObjectRepository::new(db.connection());
    repo.update(row_id, &body).map_err(|_| (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({ "error": format!("Object row {row_id} not found") })),
    ))?;
    Ok(Json(ObjectSummary {
        row_id,
        object_type: body.object_type().as_str().into(),
        name:        body.name().into(),
        object_id:   body.id(),
    }))
}

/// `DELETE /projects/:pid/scenarios/:sid/objects/:oid`
pub async fn delete_object(
    State(state): State<AppState>,
    Path((_pid, _sid, row_id)): Path<(String, String, i64)>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let db = state.db.lock().map_err(internal)?;
    let repo = SceneObjectRepository::new(db.connection());
    repo.delete(row_id).map_err(|_| (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({ "error": format!("Object row {row_id} not found") })),
    ))?;
    Ok(StatusCode::NO_CONTENT)
}

// ─── Error helpers ────────────────────────────────────────────────────────────

fn repo_err(e: noise_data::repository::RepoError) -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::INTERNAL_SERVER_ERROR,
     Json(serde_json::json!({ "error": e.to_string() })))
}

fn internal<E: std::fmt::Display>(e: E) -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::INTERNAL_SERVER_ERROR,
     Json(serde_json::json!({ "error": e.to_string() })))
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use nalgebra::Point3;
    use noise_data::{
        entities::sources::PointSource,
        repository::ProjectRepository,
        scenario::Project,
    };

    fn test_state_with_scenario() -> (AppState, String, String) {
        let state = AppState::in_memory().unwrap();
        let project = Project::new("Test", 32650);
        let pid = project.id.to_string();
        let sid = project.base_scenario.id.to_string();
        {
            let db = state.db.lock().unwrap();
            ProjectRepository::new(db.connection()).insert(&project).unwrap();
        }
        (state, pid, sid)
    }

    fn point_source_obj(id: u64, x: f64, y: f64) -> SceneObject {
        SceneObject::PointSource(PointSource::omnidirectional(
            id,
            format!("Src{id}"),
            Point3::new(x, y, 0.5),
            [85.0; 8],
        ))
    }

    #[tokio::test]
    async fn list_objects_empty_initially() {
        let (state, pid, sid) = test_state_with_scenario();
        let resp = list_objects(
            State(state),
            Path((pid, sid)),
            Query(ListObjectsQuery { object_type: None }),
        ).await.unwrap();
        assert!(resp.0.is_empty());
    }

    #[tokio::test]
    async fn create_and_list_object() {
        let (state, pid, sid) = test_state_with_scenario();
        let obj = point_source_obj(1, 100.0, 200.0);
        let created = create_object(
            State(state.clone()),
            Path((pid.clone(), sid.clone())),
            Json(obj),
        ).await.unwrap();
        assert_eq!(created.1.0.object_type, "point_source");

        let list = list_objects(
            State(state),
            Path((pid, sid)),
            Query(ListObjectsQuery { object_type: None }),
        ).await.unwrap();
        assert_eq!(list.0.len(), 1);
    }

    #[tokio::test]
    async fn get_object_roundtrip() {
        let (state, pid, sid) = test_state_with_scenario();
        let obj = point_source_obj(2, 50.0, 50.0);
        let created = create_object(
            State(state.clone()),
            Path((pid.clone(), sid.clone())),
            Json(obj),
        ).await.unwrap();
        let row_id = created.1.0.row_id;

        let fetched = get_object(
            State(state),
            Path((pid, sid, row_id)),
        ).await.unwrap();
        assert_eq!(fetched.0.name(), "Src2");
    }

    #[tokio::test]
    async fn delete_object_removes_it() {
        let (state, pid, sid) = test_state_with_scenario();
        let obj = point_source_obj(3, 0.0, 0.0);
        let created = create_object(
            State(state.clone()),
            Path((pid.clone(), sid.clone())),
            Json(obj),
        ).await.unwrap();
        let row_id = created.1.0.row_id;

        delete_object(State(state.clone()), Path((pid.clone(), sid.clone(), row_id)))
            .await.unwrap();

        let list = list_objects(
            State(state),
            Path((pid, sid)),
            Query(ListObjectsQuery { object_type: None }),
        ).await.unwrap();
        assert!(list.0.is_empty());
    }

    #[tokio::test]
    async fn update_object_changes_name() {
        let (state, pid, sid) = test_state_with_scenario();
        let obj = point_source_obj(4, 10.0, 10.0);
        let created = create_object(
            State(state.clone()),
            Path((pid.clone(), sid.clone())),
            Json(obj),
        ).await.unwrap();
        let row_id = created.1.0.row_id;

        let updated_obj = SceneObject::PointSource(PointSource::omnidirectional(
            4, "Renamed Fan", Point3::new(10.0, 10.0, 0.5), [90.0; 8],
        ));
        let resp = update_object(
            State(state.clone()),
            Path((pid.clone(), sid.clone(), row_id)),
            Json(updated_obj),
        ).await.unwrap();
        assert_eq!(resp.0.name, "Renamed Fan");
    }

    #[tokio::test]
    async fn filter_by_type_returns_only_matching() {
        let (state, pid, sid) = test_state_with_scenario();
        // Insert a point source and a building.
        let ps = point_source_obj(10, 0.0, 0.0);
        let building = SceneObject::Building(noise_data::entities::Building::new(
            20,
            "Office",
            vec![
                Point3::new(0.0, 0.0, 0.0),
                Point3::new(10.0, 0.0, 0.0),
                Point3::new(10.0, 10.0, 0.0),
                Point3::new(0.0, 10.0, 0.0),
            ],
            15.0,
        ));
        create_object(State(state.clone()), Path((pid.clone(), sid.clone())), Json(ps)).await.unwrap();
        create_object(State(state.clone()), Path((pid.clone(), sid.clone())), Json(building)).await.unwrap();

        let sources = list_objects(
            State(state.clone()),
            Path((pid.clone(), sid.clone())),
            Query(ListObjectsQuery { object_type: Some("point_source".into()) }),
        ).await.unwrap();
        assert_eq!(sources.0.len(), 1);

        let buildings = list_objects(
            State(state),
            Path((pid, sid)),
            Query(ListObjectsQuery { object_type: Some("building".into()) }),
        ).await.unwrap();
        assert_eq!(buildings.0.len(), 1);
    }

    #[tokio::test]
    async fn invalid_type_filter_returns_400() {
        let (state, pid, sid) = test_state_with_scenario();
        let resp = list_objects(
            State(state),
            Path((pid, sid)),
            Query(ListObjectsQuery { object_type: Some("not_a_real_type".into()) }),
        ).await;
        assert!(resp.is_err());
        assert_eq!(resp.err().unwrap().0, StatusCode::BAD_REQUEST);
    }
}
