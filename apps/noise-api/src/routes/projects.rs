//! Project and scenario REST API routes — backed by SQLite via [`AppState`].
//!
//! GET  /projects              → list all projects
//! POST /projects              → create a new project
//! GET  /projects/:id          → get project info
//! GET  /projects/:id/scenarios → list scenarios for a project

use axum::{Json, extract::{Path, State}, http::StatusCode};
use serde::{Deserialize, Serialize};
use noise_data::{
    repository::ProjectRepository,
    scenario::{Project, ScenarioVariant},
};
use uuid::Uuid;

use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
    pub crs_epsg: Option<u32>,
    pub description: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ProjectSummary {
    pub id: String,
    pub name: String,
    pub crs_epsg: u32,
    pub scenario_count: usize,
}

#[derive(Debug, Serialize)]
pub struct ScenarioSummary {
    pub id: String,
    pub name: String,
    pub is_base: bool,
    pub strategy_notes: String,
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

/// `GET /projects` — list all projects stored in the database.
pub async fn list_projects(
    State(state): State<AppState>,
) -> Result<Json<Vec<ProjectSummary>>, (StatusCode, Json<serde_json::Value>)> {
    let db = state.db.lock().map_err(internal_error)?;
    let repo = ProjectRepository::new(db.connection());
    let projects = repo.list_all().map_err(repo_error)?;

    let summaries = projects.into_iter().map(|(id, name)| {
        // Fetch full project to get variant count.
        let scenario_count = repo.get(id)
            .map(|p| 1 + p.variants.len())
            .unwrap_or(1);
        ProjectSummary {
            id: id.to_string(),
            name,
            crs_epsg: 0,   // populated in full get below
            scenario_count,
        }
    }).collect::<Vec<_>>();

    Ok(Json(summaries))
}

/// `POST /projects` — create a new project and persist it.
pub async fn create_project(
    State(state): State<AppState>,
    Json(body): Json<CreateProjectRequest>,
) -> Result<Json<ProjectSummary>, (StatusCode, Json<serde_json::Value>)> {
    if body.name.trim().is_empty() {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "error": "Project name cannot be empty" })),
        ));
    }
    let crs = body.crs_epsg.unwrap_or(32650);
    let mut project = Project::new(&body.name, crs);
    if let Some(desc) = &body.description {
        project.description = desc.clone();
    }

    let db = state.db.lock().map_err(internal_error)?;
    let repo = ProjectRepository::new(db.connection());
    repo.insert(&project).map_err(repo_error)?;

    Ok(Json(ProjectSummary {
        id: project.id.to_string(),
        name: project.name,
        crs_epsg: crs,
        scenario_count: 1,
    }))
}

/// `GET /projects/:id` — return full project info.
pub async fn get_project(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
) -> Result<Json<ProjectSummary>, (StatusCode, Json<serde_json::Value>)> {
    let uid = parse_uuid(&project_id)?;
    let db = state.db.lock().map_err(internal_error)?;
    let repo = ProjectRepository::new(db.connection());
    let project = repo.get(uid).map_err(|_| (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({ "error": format!("Project '{project_id}' not found") })),
    ))?;

    Ok(Json(ProjectSummary {
        id: project.id.to_string(),
        name: project.name,
        crs_epsg: project.crs_epsg,
        scenario_count: 1 + project.variants.len(),
    }))
}

/// `GET /projects/:id/scenarios` — list all scenarios for a project.
pub async fn list_scenarios(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
) -> Result<Json<Vec<ScenarioSummary>>, (StatusCode, Json<serde_json::Value>)> {
    let uid = parse_uuid(&project_id)?;
    let db = state.db.lock().map_err(internal_error)?;
    let repo = ProjectRepository::new(db.connection());
    let project = repo.get(uid).map_err(|_| (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({ "error": format!("Project '{project_id}' not found") })),
    ))?;

    let mut summaries = vec![ScenarioSummary {
        id: project.base_scenario.id.to_string(),
        name: project.base_scenario.name.clone(),
        is_base: true,
        strategy_notes: "Base case — existing conditions".into(),
    }];

    for v in &project.variants {
        summaries.push(ScenarioSummary {
            id: v.id.to_string(),
            name: v.name.clone(),
            is_base: false,
            strategy_notes: v.strategy_notes.clone(),
        });
    }

    Ok(Json(summaries))
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

    fn test_state() -> AppState {
        AppState::in_memory().expect("in-memory DB failed")
    }

    #[tokio::test]
    async fn list_projects_empty_initially() {
        let state = test_state();
        let resp = list_projects(State(state)).await.unwrap();
        assert!(resp.0.is_empty());
    }

    #[tokio::test]
    async fn create_and_list_project() {
        let state = test_state();
        let req = CreateProjectRequest {
            name: "Test Project".into(),
            crs_epsg: Some(32651),
            description: Some("A test project".into()),
        };
        let created = create_project(State(state.clone()), Json(req)).await.unwrap();
        assert_eq!(created.0.name, "Test Project");
        assert_eq!(created.0.crs_epsg, 32651);
        assert!(!created.0.id.is_empty());

        let list = list_projects(State(state)).await.unwrap();
        assert_eq!(list.0.len(), 1);
    }

    #[tokio::test]
    async fn create_project_empty_name_returns_422() {
        let state = test_state();
        let req = CreateProjectRequest {
            name: "".into(),
            crs_epsg: None,
            description: None,
        };
        let result = create_project(State(state), Json(req)).await;
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().0, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn get_project_roundtrip() {
        let state = test_state();
        let req = CreateProjectRequest {
            name: "City Study".into(),
            crs_epsg: Some(32650),
            description: None,
        };
        let created = create_project(State(state.clone()), Json(req)).await.unwrap();
        let id = created.0.id.clone();

        let fetched = get_project(State(state), Path(id)).await.unwrap();
        assert_eq!(fetched.0.name, "City Study");
        assert_eq!(fetched.0.crs_epsg, 32650);
    }

    #[tokio::test]
    async fn get_project_invalid_uuid_returns_400() {
        let state = test_state();
        let result = get_project(State(state), Path("not-a-uuid".into())).await;
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().0, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn get_project_missing_returns_404() {
        let state = test_state();
        let missing_id = Uuid::new_v4().to_string();
        let result = get_project(State(state), Path(missing_id)).await;
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().0, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn list_scenarios_returns_base_plus_variants() {
        let state = test_state();
        // Create a project with variants.
        let req = CreateProjectRequest {
            name: "Ring Road".into(),
            crs_epsg: Some(32650),
            description: None,
        };
        let created = create_project(State(state.clone()), Json(req)).await.unwrap();
        let pid = created.0.id.clone();

        // Insert project with variants directly via DB.
        {
            let db = state.db.lock().unwrap();
            let repo = ProjectRepository::new(db.connection());
            let fetched_uuid = Uuid::parse_str(&pid).unwrap();
            let mut project = repo.get(fetched_uuid).unwrap();
            project.add_variant("Barrier Option A");
            project.add_variant("Green Belt Option");
            repo.insert(&project).unwrap();
        }

        let scenarios = list_scenarios(State(state), Path(pid)).await.unwrap();
        // 1 base + 2 variants = 3 total.
        assert_eq!(scenarios.0.len(), 3);
        assert!(scenarios.0[0].is_base);
        assert!(!scenarios.0[1].is_base);
        assert!(!scenarios.0[2].is_base);
    }
}
