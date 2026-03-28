//! Project and scenario REST API routes.
//!
//! GET  /projects             → list all projects (in-memory demo)
//! POST /projects             → create a new project
//! GET  /projects/:id         → get project info
//! GET  /projects/:id/scenarios → list scenarios for a project

use axum::{Json, extract::Path, http::StatusCode};
use serde::{Deserialize, Serialize};
use noise_data::scenario::{Project, ScenarioVariant};
use uuid::Uuid;

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

/// GET /projects — return a synthetic project list.
pub async fn list_projects() -> Json<Vec<ProjectSummary>> {
    // In a full implementation this would query the DB.
    Json(vec![
        ProjectSummary {
            id: "demo-project-1".into(),
            name: "Demo City Noise Study".into(),
            crs_epsg: 32650,
            scenario_count: 2,
        },
    ])
}

/// POST /projects — create a new project, return summary.
pub async fn create_project(
    Json(body): Json<CreateProjectRequest>,
) -> Result<Json<ProjectSummary>, (StatusCode, Json<serde_json::Value>)> {
    if body.name.trim().is_empty() {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "error": "Project name cannot be empty" })),
        ));
    }
    let crs = body.crs_epsg.unwrap_or(32650);
    let project = Project::new(&body.name, crs);
    Ok(Json(ProjectSummary {
        id: project.id.to_string(),
        name: project.name.clone(),
        crs_epsg: crs,
        scenario_count: 1,
    }))
}

/// GET /projects/:id — return project info.
pub async fn get_project(
    Path(project_id): Path<String>,
) -> Result<Json<ProjectSummary>, (StatusCode, Json<serde_json::Value>)> {
    // Demo: accept any UUID-like ID and return synthetic data.
    if project_id.is_empty() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Project not found" })),
        ));
    }
    Ok(Json(ProjectSummary {
        id: project_id,
        name: "Demo Project".into(),
        crs_epsg: 32650,
        scenario_count: 1,
    }))
}

/// GET /projects/:id/scenarios — list scenarios for a project.
pub async fn list_scenarios(
    Path(project_id): Path<String>,
) -> Json<Vec<ScenarioSummary>> {
    // Demo: return two synthetic scenarios.
    Json(vec![
        ScenarioSummary {
            id: format!("{project_id}_base"),
            name: "Base Case".into(),
            is_base: true,
            strategy_notes: "Existing conditions".into(),
        },
        ScenarioSummary {
            id: format!("{project_id}_barrier"),
            name: "Barrier Option A".into(),
            is_base: false,
            strategy_notes: "3 m barrier on north side".into(),
        },
    ])
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn list_projects_returns_demos() {
        let resp = list_projects().await;
        assert!(!resp.0.is_empty());
        assert_eq!(resp.0[0].crs_epsg, 32650);
    }

    #[tokio::test]
    async fn create_project_valid() {
        let req = CreateProjectRequest {
            name: "Test Project".into(),
            crs_epsg: Some(32651),
            description: None,
        };
        let resp = create_project(Json(req)).await.unwrap();
        assert_eq!(resp.0.name, "Test Project");
        assert_eq!(resp.0.crs_epsg, 32651);
        assert!(!resp.0.id.is_empty());
    }

    #[tokio::test]
    async fn create_project_empty_name_returns_422() {
        let req = CreateProjectRequest {
            name: "".into(),
            crs_epsg: None,
            description: None,
        };
        let result = create_project(Json(req)).await;
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().0, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn get_project_returns_summary() {
        let resp = get_project(Path("proj-123".into())).await.unwrap();
        assert_eq!(resp.0.id, "proj-123");
    }

    #[tokio::test]
    async fn list_scenarios_for_project() {
        let resp = list_scenarios(Path("proj-1".into())).await;
        assert_eq!(resp.0.len(), 2);
        assert!(resp.0[0].is_base);
        assert!(!resp.0[1].is_base);
    }
}
