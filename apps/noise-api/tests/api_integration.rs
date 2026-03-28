//! HTTP-level integration tests for the noise-api.
//!
//! Uses [`tower::ServiceExt::oneshot`] to drive the Axum router without
//! binding a real TCP port.  Each test creates a fresh in-memory database and
//! issues JWT tokens signed with the default development secret.

use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
};
use noise_api::{build_router, AppState};
use noise_auth::jwt::TokenService;
use noise_data::entities::{SceneObject, sources::PointSource};
use nalgebra::Point3;
use tower::ServiceExt;
use uuid::Uuid;

// ─── Helpers ──────────────────────────────────────────────────────────────────

const JWT_SECRET: &[u8] = b"change-me-in-production";

fn analyst_token() -> String {
    let svc = TokenService::new(JWT_SECRET);
    svc.issue(Uuid::new_v4(), "analyst", "analyst").unwrap()
}

fn admin_token() -> String {
    let svc = TokenService::new(JWT_SECRET);
    svc.issue(Uuid::new_v4(), "admin", "admin").unwrap()
}

fn app() -> axum::Router {
    let state = AppState::in_memory().unwrap();
    build_router(state)
}

/// Build a GET request with a Bearer token.
fn get(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method(Method::GET)
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

/// Build a POST request with a JSON body and optional Bearer token.
fn post_json(uri: &str, body: serde_json::Value, token: Option<&str>) -> Request<Body> {
    let mut b = Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(t) = token {
        b = b.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    b.body(Body::from(body.to_string())).unwrap()
}

/// Build a DELETE request with a Bearer token.
fn delete(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method(Method::DELETE)
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

/// Build a PUT request with a JSON body and Bearer token.
fn put_json(uri: &str, body: serde_json::Value, token: &str) -> Request<Body> {
    Request::builder()
        .method(Method::PUT)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::from(body.to_string()))
        .unwrap()
}

/// Deserialise the response body as JSON.
async fn json_body(resp: axum::response::Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

// ─── /health ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn health_returns_ok() {
    let resp = app()
        .oneshot(Request::get("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    assert_eq!(body["status"], "ok");
}

// ─── /info ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn info_returns_version() {
    let resp = app()
        .oneshot(Request::get("/info").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    assert!(body["version"].is_string());
}

// ─── /auth/login ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn login_unknown_user_returns_401() {
    let body = serde_json::json!({ "username": "ghost", "password": "nopass" });
    let resp = app()
        .oneshot(post_json("/auth/login", body, None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ─── /projects ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_projects_requires_auth() {
    let resp = app()
        .oneshot(Request::get("/projects").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_and_list_project() {
    let token = analyst_token();
    let router = app();

    // Create
    let body = serde_json::json!({ "name": "Integration Test Project", "epsg": 32650 });
    let resp = router
        .clone()
        .oneshot(post_json("/projects", body, Some(&token)))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let created = json_body(resp).await;
    let pid = created["id"].as_str().unwrap().to_string();

    // List
    let resp = router
        .oneshot(get("/projects", &token))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let page = json_body(resp).await;
    let items = page["items"].as_array().unwrap();
    let ids: Vec<&str> = items.iter()
        .filter_map(|p| p["id"].as_str())
        .collect();
    assert!(ids.contains(&pid.as_str()));
    assert!(page["total"].as_u64().unwrap() >= 1);
}

#[tokio::test]
async fn get_project_not_found_returns_404() {
    let token = analyst_token();
    let missing = Uuid::new_v4();
    let resp = app()
        .oneshot(get(&format!("/projects/{missing}"), &token))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// ─── /projects/:pid/scenarios ─────────────────────────────────────────────────

#[tokio::test]
async fn list_scenarios_returns_base_scenario() {
    let token = analyst_token();
    let router = app();

    let body = serde_json::json!({ "name": "Scenario Test", "epsg": 32633 });
    let resp = router
        .clone()
        .oneshot(post_json("/projects", body, Some(&token)))
        .await
        .unwrap();
    let created = json_body(resp).await;
    let pid = created["id"].as_str().unwrap();

    let resp = router
        .oneshot(get(&format!("/projects/{pid}/scenarios"), &token))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let scenarios = json_body(resp).await;
    assert!(!scenarios.as_array().unwrap().is_empty());
}

// ─── Scene objects ────────────────────────────────────────────────────────────

async fn create_project_and_scenario(token: &str) -> (axum::Router, String, String) {
    let router = app();
    let body = serde_json::json!({ "name": "Obj Test Project", "epsg": 32650 });
    let resp = router
        .clone()
        .oneshot(post_json("/projects", body, Some(token)))
        .await
        .unwrap();
    let created = json_body(resp).await;
    let pid = created["id"].as_str().unwrap().to_string();

    // Get base scenario id
    let resp = router
        .clone()
        .oneshot(get(&format!("/projects/{pid}/scenarios"), token))
        .await
        .unwrap();
    let scenarios = json_body(resp).await;
    let sid = scenarios[0]["id"].as_str().unwrap().to_string();

    (router, pid, sid)
}

#[tokio::test]
async fn objects_list_empty_initially() {
    let token = analyst_token();
    let (router, pid, sid) = create_project_and_scenario(&token).await;

    let resp = router
        .oneshot(get(&format!("/projects/{pid}/scenarios/{sid}/objects"), &token))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let list = json_body(resp).await;
    assert!(list.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn create_and_get_object() {
    let token = analyst_token();
    let (router, pid, sid) = create_project_and_scenario(&token).await;

    let ps = SceneObject::PointSource(PointSource::omnidirectional(
        1, "Fan Unit", Point3::new(100.0, 200.0, 0.5), [85.0; 8],
    ));
    let ps_json: serde_json::Value = serde_json::to_value(&ps).unwrap();

    // Create
    let resp = router
        .clone()
        .oneshot(post_json(&format!("/projects/{pid}/scenarios/{sid}/objects"), ps_json, Some(&token)))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let created = json_body(resp).await;
    let row_id = created["row_id"].as_i64().unwrap();
    assert_eq!(created["object_type"], "point_source");

    // Get
    let resp = router
        .oneshot(get(&format!("/projects/{pid}/scenarios/{sid}/objects/{row_id}"), &token))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let obj = json_body(resp).await;
    assert_eq!(obj["name"], "Fan Unit");
}

#[tokio::test]
async fn update_object_changes_name() {
    let token = analyst_token();
    let (router, pid, sid) = create_project_and_scenario(&token).await;

    let ps = SceneObject::PointSource(PointSource::omnidirectional(
        2, "Old Name", Point3::new(10.0, 10.0, 0.5), [80.0; 8],
    ));
    let resp = router
        .clone()
        .oneshot(post_json(
            &format!("/projects/{pid}/scenarios/{sid}/objects"),
            serde_json::to_value(&ps).unwrap(),
            Some(&token),
        ))
        .await
        .unwrap();
    let created = json_body(resp).await;
    let row_id = created["row_id"].as_i64().unwrap();

    let updated = SceneObject::PointSource(PointSource::omnidirectional(
        2, "New Name", Point3::new(10.0, 10.0, 0.5), [80.0; 8],
    ));
    let resp = router
        .oneshot(put_json(
            &format!("/projects/{pid}/scenarios/{sid}/objects/{row_id}"),
            serde_json::to_value(&updated).unwrap(),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    assert_eq!(body["name"], "New Name");
}

#[tokio::test]
async fn delete_object_returns_204() {
    let token = analyst_token();
    let (router, pid, sid) = create_project_and_scenario(&token).await;

    let ps = SceneObject::PointSource(PointSource::omnidirectional(
        3, "Temp Source", Point3::new(5.0, 5.0, 0.5), [78.0; 8],
    ));
    let resp = router
        .clone()
        .oneshot(post_json(
            &format!("/projects/{pid}/scenarios/{sid}/objects"),
            serde_json::to_value(&ps).unwrap(),
            Some(&token),
        ))
        .await
        .unwrap();
    let created = json_body(resp).await;
    let row_id = created["row_id"].as_i64().unwrap();

    let resp = router
        .oneshot(delete(
            &format!("/projects/{pid}/scenarios/{sid}/objects/{row_id}"),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}

// ─── /scenarios/:id/calculate ─────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn calculate_returns_completed_job() {
    let token = analyst_token();
    let (router, _pid, sid) = create_project_and_scenario(&token).await;

    let body = serde_json::json!({
        "metric": "Lden",
        "grid_type": "horizontal",
        "resolution_m": 20.0,
        "extent": [0.0, 0.0, 100.0, 100.0]
    });
    let resp = router
        .oneshot(post_json(&format!("/scenarios/{sid}/calculate"), body, Some(&token)))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let job = json_body(resp).await;
    assert_eq!(job["status"], "completed");
    assert_eq!(job["progress_pct"], 100);
    assert!(job["result"].is_object());
    assert_eq!(job["result"]["nx"], 5);
    assert_eq!(job["result"]["ny"], 5);
}

#[tokio::test(flavor = "multi_thread")]
async fn get_job_after_calculate() {
    let token = analyst_token();
    let (router, _pid, sid) = create_project_and_scenario(&token).await;

    let body = serde_json::json!({
        "resolution_m": 25.0,
        "extent": [0.0, 0.0, 50.0, 50.0]
    });
    let resp = router
        .clone()
        .oneshot(post_json(&format!("/scenarios/{sid}/calculate"), body, Some(&token)))
        .await
        .unwrap();
    let job = json_body(resp).await;
    let job_id = job["job_id"].as_u64().unwrap();

    let resp = router
        .oneshot(get(&format!("/jobs/{job_id}"), &token))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let fetched = json_body(resp).await;
    assert_eq!(fetched["status"], "completed");
}

#[tokio::test]
async fn get_job_zero_returns_404() {
    let token = analyst_token();
    let resp = app()
        .oneshot(get("/jobs/0", &token))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn get_job_missing_returns_404() {
    let token = analyst_token();
    let resp = app()
        .oneshot(get("/jobs/99999", &token))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn calculate_requires_auth() {
    let resp = app()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/scenarios/some-id/calculate")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ─── Admin routes ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_users_requires_admin() {
    // analyst token should be rejected
    let token = analyst_token();
    let resp = app()
        .oneshot(get("/users", &token))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

// ─── Export endpoints ─────────────────────────────────────────────────────────

/// Run a full calculate cycle and return the calc_result_id from the job.
#[tokio::test(flavor = "multi_thread")]
async fn export_geojson_after_calculate() {
    let token = analyst_token();
    let (router, _pid, sid) = create_project_and_scenario(&token).await;

    // Submit calculation.
    let body = serde_json::json!({
        "resolution_m": 10.0,
        "extent": [0.0, 0.0, 50.0, 50.0]
    });
    let resp = router
        .clone()
        .oneshot(post_json(&format!("/scenarios/{sid}/calculate"), body, Some(&token)))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let job = json_body(resp).await;
    let job_id = job["job_id"].as_u64().unwrap();

    // Get the job to retrieve calc_result_id (embedded in result).
    let resp = router
        .clone()
        .oneshot(get(&format!("/jobs/{job_id}"), &token))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Export GeoJSON using calc_id = 1 (first calc in fresh DB).
    let resp = router
        .oneshot(get("/calculations/1/export/geojson", &token))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp.headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(ct.contains("geo+json"), "expected geo+json content-type, got {ct}");

    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let fc: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(fc["type"], "FeatureCollection");
}

#[tokio::test(flavor = "multi_thread")]
async fn export_asc_returns_plain_text() {
    let token = analyst_token();
    let (router, _pid, sid) = create_project_and_scenario(&token).await;

    let body = serde_json::json!({ "resolution_m": 10.0, "extent": [0.0, 0.0, 30.0, 30.0] });
    router
        .clone()
        .oneshot(post_json(&format!("/scenarios/{sid}/calculate"), body, Some(&token)))
        .await
        .unwrap();

    let resp = router
        .oneshot(get("/calculations/1/export/asc", &token))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let text = std::str::from_utf8(&bytes).unwrap();
    assert!(text.contains("ncols"), "expected ASC header");
    assert!(text.contains("cellsize"), "expected cellsize field");
}

#[tokio::test(flavor = "multi_thread")]
async fn export_csv_has_header_row() {
    let token = analyst_token();
    let (router, _pid, sid) = create_project_and_scenario(&token).await;

    let body = serde_json::json!({ "resolution_m": 10.0, "extent": [0.0, 0.0, 20.0, 20.0] });
    router
        .clone()
        .oneshot(post_json(&format!("/scenarios/{sid}/calculate"), body, Some(&token)))
        .await
        .unwrap();

    let resp = router
        .oneshot(get("/calculations/1/export/csv", &token))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let text = std::str::from_utf8(&bytes).unwrap();
    assert!(text.starts_with("x,y,level_dba\n"), "CSV must start with header");
}

#[tokio::test]
async fn export_missing_calc_returns_404() {
    let token = analyst_token();
    let resp = app()
        .oneshot(get("/calculations/9999/export/geojson", &token))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn export_requires_auth() {
    let resp = app()
        .oneshot(Request::get("/calculations/1/export/geojson").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ─── Admin routes ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_users_with_admin_token() {
    let token = admin_token();
    let resp = app()
        .oneshot(get("/users", &token))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
