//! Scenario variant CRUD routes.
//!
//! POST   /projects/:pid/scenarios        → create a new scenario variant
//! PUT    /projects/:pid/scenarios/:sid   → update variant name/description/strategy_notes
//! DELETE /projects/:pid/scenarios/:sid   → delete variant

use axum::{Json, extract::{Path, State}, http::StatusCode};
use serde::{Deserialize, Serialize};
use noise_data::repository::ProjectRepository;
use uuid::Uuid;

use crate::state::AppState;

// ─── Request / response types ─────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateVariantRequest {
    pub name: String,
    pub description: Option<String>,
    pub strategy_notes: Option<String>,
}

/// All fields are optional for PUT (partial update).
#[derive(Debug, Deserialize)]
pub struct UpdateVariantRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub strategy_notes: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ScenarioSummary {
    pub id: String,
    pub name: String,
    pub is_base: bool,
    pub strategy_notes: String,
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

/// `POST /projects/:pid/scenarios` — create a new scenario variant.
///
/// Returns 201 + `ScenarioSummary` on success.
/// Returns 400 if `pid` is not a valid UUID.
/// Returns 404 if the project does not exist.
/// Returns 422 if `name` is empty.
pub async fn create_variant(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
    Json(body): Json<CreateVariantRequest>,
) -> Result<(StatusCode, Json<ScenarioSummary>), (StatusCode, Json<serde_json::Value>)> {
    let pid = parse_uuid(&project_id)?;

    if body.name.trim().is_empty() {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "error": "Variant name cannot be empty" })),
        ));
    }

    let db = state.db.lock().map_err(internal_error)?;
    let repo = ProjectRepository::new(db.connection());

    let mut project = repo.get(pid).map_err(|_| (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({ "error": format!("Project '{project_id}' not found") })),
    ))?;

    let variant = project.add_variant(&body.name);
    let summary = ScenarioSummary {
        id: variant.id.to_string(),
        name: variant.name.clone(),
        is_base: false,
        strategy_notes: variant.strategy_notes.clone(),
    };

    // Apply optional fields before persisting.
    let variant_id = Uuid::parse_str(&summary.id).unwrap();
    if let Some(v) = project.variant_mut(variant_id) {
        if let Some(desc) = &body.description {
            v.description = desc.clone();
        }
        if let Some(notes) = &body.strategy_notes {
            v.strategy_notes = notes.clone();
        }
    }

    repo.insert(&project).map_err(repo_error)?;

    // Re-fetch the variant to get the final strategy_notes in the summary.
    let final_notes = project
        .variant(variant_id)
        .map(|v| v.strategy_notes.clone())
        .unwrap_or_default();

    Ok((StatusCode::CREATED, Json(ScenarioSummary {
        id: variant_id.to_string(),
        name: body.name,
        is_base: false,
        strategy_notes: final_notes,
    })))
}

/// `PUT /projects/:pid/scenarios/:sid` — update variant metadata.
///
/// Only fields present in the request body are changed; `null` / absent fields
/// are left untouched.  Returns 200 + `ScenarioSummary` on success.
/// Returns 400 if either UUID is invalid.
/// Returns 404 if the project or scenario does not exist.
/// Returns 409 if the caller tries to rename the base scenario.
pub async fn update_variant(
    State(state): State<AppState>,
    Path((project_id, scenario_id)): Path<(String, String)>,
    Json(body): Json<UpdateVariantRequest>,
) -> Result<Json<ScenarioSummary>, (StatusCode, Json<serde_json::Value>)> {
    let pid = parse_uuid(&project_id)?;
    let sid = parse_uuid(&scenario_id)?;

    let db = state.db.lock().map_err(internal_error)?;
    let repo = ProjectRepository::new(db.connection());

    let mut project = repo.get(pid).map_err(|_| (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({ "error": format!("Project '{project_id}' not found") })),
    ))?;

    // Reject attempts to modify the base scenario.
    if project.base_scenario.id == sid {
        return Err((
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "error": "Cannot rename or modify the base scenario through this endpoint"
            })),
        ));
    }

    let variant = project.variant_mut(sid).ok_or_else(|| (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({ "error": format!("Scenario '{scenario_id}' not found") })),
    ))?;

    if let Some(name) = &body.name {
        if name.trim().is_empty() {
            return Err((
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(serde_json::json!({ "error": "Variant name cannot be empty" })),
            ));
        }
        variant.name = name.clone();
    }
    if let Some(desc) = &body.description {
        variant.description = desc.clone();
    }
    if let Some(notes) = &body.strategy_notes {
        variant.strategy_notes = notes.clone();
    }

    let summary = ScenarioSummary {
        id: variant.id.to_string(),
        name: variant.name.clone(),
        is_base: false,
        strategy_notes: variant.strategy_notes.clone(),
    };

    repo.insert(&project).map_err(repo_error)?;

    Ok(Json(summary))
}

/// `DELETE /projects/:pid/scenarios/:sid` — remove a variant.
///
/// Returns 204 on success.
/// Returns 400 if either UUID is invalid.
/// Returns 404 if the project or scenario does not exist.
/// Returns 409 if the caller tries to delete the base scenario.
pub async fn delete_variant(
    State(state): State<AppState>,
    Path((project_id, scenario_id)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let pid = parse_uuid(&project_id)?;
    let sid = parse_uuid(&scenario_id)?;

    let db = state.db.lock().map_err(internal_error)?;
    let repo = ProjectRepository::new(db.connection());

    // Verify the project exists.
    let project = repo.get(pid).map_err(|_| (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({ "error": format!("Project '{project_id}' not found") })),
    ))?;

    // Reject attempts to delete the base scenario.
    if project.base_scenario.id == sid {
        return Err((
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "error": "Cannot delete the base scenario"
            })),
        ));
    }

    // Verify the variant belongs to this project.
    if project.variant(sid).is_none() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": format!("Scenario '{scenario_id}' not found") })),
        ));
    }

    // Delete the scenario row (cascades to scene_objects and calculations).
    repo.delete_variant(sid).map_err(repo_error)?;

    Ok(StatusCode::NO_CONTENT)
}

// ─── Error helpers ────────────────────────────────────────────────────────────

fn parse_uuid(s: &str) -> Result<Uuid, (StatusCode, Json<serde_json::Value>)> {
    Uuid::parse_str(s).map_err(|_| (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "error": format!("'{s}' is not a valid UUID") })),
    ))
}

fn repo_error(e: noise_data::repository::RepoError) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": e.to_string() })),
    )
}

fn internal_error<E: std::fmt::Display>(e: E) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": e.to_string() })),
    )
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use noise_data::{repository::ProjectRepository, scenario::Project};

    fn test_state() -> AppState {
        AppState::in_memory().expect("in-memory DB failed")
    }

    /// Helper: insert a project and return its UUID string.
    fn insert_project(state: &AppState, name: &str) -> String {
        let db = state.db.lock().unwrap();
        let repo = ProjectRepository::new(db.connection());
        let project = Project::new(name, 32650);
        let id = project.id.to_string();
        repo.insert(&project).unwrap();
        id
    }

    /// Helper: insert a project that already has one variant, returning
    /// `(project_id, variant_id)`.
    fn insert_project_with_variant(state: &AppState) -> (String, String) {
        let db = state.db.lock().unwrap();
        let repo = ProjectRepository::new(db.connection());
        let mut project = Project::new("Scenario Test Project", 32650);
        let v = project.add_variant("Initial Variant");
        let vid = v.id.to_string();
        let pid = project.id.to_string();
        repo.insert(&project).unwrap();
        (pid, vid)
    }

    // ── create_variant ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn create_variant_returns_201_and_summary() {
        let state = test_state();
        let pid = insert_project(&state, "Ring Road");

        let body = CreateVariantRequest {
            name: "Barrier Option A".into(),
            description: Some("Concrete barrier on north side".into()),
            strategy_notes: Some("Build 5 m barrier".into()),
        };

        let result = create_variant(
            State(state),
            Path(pid),
            Json(body),
        ).await.unwrap();

        assert_eq!(result.0, StatusCode::CREATED);
        let summary = &result.1.0;
        assert_eq!(summary.name, "Barrier Option A");
        assert_eq!(summary.strategy_notes, "Build 5 m barrier");
        assert!(!summary.is_base);
        assert!(!summary.id.is_empty());
    }

    #[tokio::test]
    async fn create_variant_empty_name_returns_422() {
        let state = test_state();
        let pid = insert_project(&state, "Project X");

        let body = CreateVariantRequest { name: "  ".into(), description: None, strategy_notes: None };
        let err = create_variant(State(state), Path(pid), Json(body)).await.unwrap_err();
        assert_eq!(err.0, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn create_variant_invalid_project_uuid_returns_400() {
        let state = test_state();
        let body = CreateVariantRequest { name: "V1".into(), description: None, strategy_notes: None };
        let err = create_variant(State(state), Path("not-a-uuid".into()), Json(body)).await.unwrap_err();
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn create_variant_missing_project_returns_404() {
        let state = test_state();
        let body = CreateVariantRequest { name: "V1".into(), description: None, strategy_notes: None };
        let missing = Uuid::new_v4().to_string();
        let err = create_variant(State(state), Path(missing), Json(body)).await.unwrap_err();
        assert_eq!(err.0, StatusCode::NOT_FOUND);
    }

    // ── update_variant ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn update_variant_changes_name_and_notes() {
        let state = test_state();
        let (pid, vid) = insert_project_with_variant(&state);

        let body = UpdateVariantRequest {
            name: Some("Renamed Variant".into()),
            description: None,
            strategy_notes: Some("Updated notes".into()),
        };

        let result = update_variant(
            State(state),
            Path((pid, vid)),
            Json(body),
        ).await.unwrap();

        assert_eq!(result.0.name, "Renamed Variant");
        assert_eq!(result.0.strategy_notes, "Updated notes");
        assert!(!result.0.is_base);
    }

    #[tokio::test]
    async fn update_variant_partial_update_preserves_unchanged_fields() {
        let state = test_state();
        // Create a variant with known notes.
        let db = state.db.lock().unwrap();
        let repo = ProjectRepository::new(db.connection());
        let mut project = Project::new("City Plan", 32650);
        project.add_variant("Draft");
        project.variants[0].strategy_notes = "Keep existing notes".into();
        let pid = project.id.to_string();
        let vid = project.variants[0].id.to_string();
        repo.insert(&project).unwrap();
        drop(db);

        // Only update name; strategy_notes should be untouched.
        let body = UpdateVariantRequest {
            name: Some("Draft v2".into()),
            description: None,
            strategy_notes: None,
        };
        let result = update_variant(
            State(state),
            Path((pid, vid)),
            Json(body),
        ).await.unwrap();

        assert_eq!(result.0.name, "Draft v2");
        assert_eq!(result.0.strategy_notes, "Keep existing notes");
    }

    #[tokio::test]
    async fn update_variant_base_scenario_returns_409() {
        let state = test_state();
        let db = state.db.lock().unwrap();
        let repo = ProjectRepository::new(db.connection());
        let project = Project::new("Highway Study", 32650);
        let pid = project.id.to_string();
        let base_id = project.base_scenario.id.to_string();
        repo.insert(&project).unwrap();
        drop(db);

        let body = UpdateVariantRequest {
            name: Some("Renamed Base".into()),
            description: None,
            strategy_notes: None,
        };
        let err = update_variant(
            State(state),
            Path((pid, base_id)),
            Json(body),
        ).await.unwrap_err();

        assert_eq!(err.0, StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn update_variant_invalid_project_uuid_returns_400() {
        let state = test_state();
        let body = UpdateVariantRequest { name: None, description: None, strategy_notes: None };
        let err = update_variant(
            State(state),
            Path(("bad-uuid".into(), Uuid::new_v4().to_string())),
            Json(body),
        ).await.unwrap_err();
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn update_variant_invalid_scenario_uuid_returns_400() {
        let state = test_state();
        let pid = insert_project(&state, "X");
        let body = UpdateVariantRequest { name: None, description: None, strategy_notes: None };
        let err = update_variant(
            State(state),
            Path((pid, "not-a-uuid".into())),
            Json(body),
        ).await.unwrap_err();
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn update_variant_missing_project_returns_404() {
        let state = test_state();
        let body = UpdateVariantRequest { name: None, description: None, strategy_notes: None };
        let err = update_variant(
            State(state),
            Path((Uuid::new_v4().to_string(), Uuid::new_v4().to_string())),
            Json(body),
        ).await.unwrap_err();
        assert_eq!(err.0, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn update_variant_missing_scenario_returns_404() {
        let state = test_state();
        let pid = insert_project(&state, "Project Y");
        let body = UpdateVariantRequest { name: None, description: None, strategy_notes: None };
        let err = update_variant(
            State(state),
            Path((pid, Uuid::new_v4().to_string())),
            Json(body),
        ).await.unwrap_err();
        assert_eq!(err.0, StatusCode::NOT_FOUND);
    }

    // ── delete_variant ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn delete_variant_returns_204() {
        let state = test_state();
        let (pid, vid) = insert_project_with_variant(&state);

        let result = delete_variant(
            State(state.clone()),
            Path((pid.clone(), vid)),
        ).await.unwrap();

        assert_eq!(result, StatusCode::NO_CONTENT);

        // Confirm it is gone from the project.
        let db = state.db.lock().unwrap();
        let repo = ProjectRepository::new(db.connection());
        let fetched = repo.get(Uuid::parse_str(&pid).unwrap()).unwrap();
        assert!(fetched.variants.is_empty());
    }

    #[tokio::test]
    async fn delete_variant_base_scenario_returns_409() {
        let state = test_state();
        let db = state.db.lock().unwrap();
        let repo = ProjectRepository::new(db.connection());
        let project = Project::new("Motorway Study", 32650);
        let pid = project.id.to_string();
        let base_id = project.base_scenario.id.to_string();
        repo.insert(&project).unwrap();
        drop(db);

        let err = delete_variant(
            State(state),
            Path((pid, base_id)),
        ).await.unwrap_err();

        assert_eq!(err.0, StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn delete_variant_invalid_project_uuid_returns_400() {
        let state = test_state();
        let err = delete_variant(
            State(state),
            Path(("not-uuid".into(), Uuid::new_v4().to_string())),
        ).await.unwrap_err();
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn delete_variant_invalid_scenario_uuid_returns_400() {
        let state = test_state();
        let pid = insert_project(&state, "Project Z");
        let err = delete_variant(
            State(state),
            Path((pid, "bad-sid".into())),
        ).await.unwrap_err();
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn delete_variant_missing_project_returns_404() {
        let state = test_state();
        let err = delete_variant(
            State(state),
            Path((Uuid::new_v4().to_string(), Uuid::new_v4().to_string())),
        ).await.unwrap_err();
        assert_eq!(err.0, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn delete_variant_missing_scenario_returns_404() {
        let state = test_state();
        let pid = insert_project(&state, "Project W");
        let err = delete_variant(
            State(state),
            Path((pid, Uuid::new_v4().to_string())),
        ).await.unwrap_err();
        assert_eq!(err.0, StatusCode::NOT_FOUND);
    }
}
