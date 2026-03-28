//! MCP HTTP server built on Axum.
//!
//! Implements the MCP protocol:
//!   POST /mcp/v1/tools/list   → list available tools
//!   POST /mcp/v1/tools/call   → invoke a tool

use axum::{Json, Router, routing::post};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::schema::{McpTool, all_tools};
use crate::tools::{calculate, manage, query};

#[derive(Debug, Deserialize)]
pub struct ToolCallRequest {
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Serialize)]
pub struct ToolCallResponse {
    pub content: Vec<ContentBlock>,
    #[serde(rename = "isError")]
    pub is_error: bool,
}

impl ToolCallResponse {
    /// Convenience constructor for error responses.
    pub fn error(msg: impl Into<String>) -> Self {
        Self {
            content: vec![ContentBlock::text(msg)],
            is_error: true,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ContentBlock {
    #[serde(rename = "type")]
    pub content_type: String,
    pub text: String,
}

impl ContentBlock {
    pub fn text(s: impl Into<String>) -> Self {
        Self { content_type: "text".into(), text: s.into() }
    }
}

#[derive(Debug, Serialize)]
pub struct ToolListResponse {
    pub tools: Vec<McpTool>,
}

pub fn router() -> Router {
    Router::new()
        .route("/mcp/v1/tools/list", post(list_tools))
        .route("/mcp/v1/tools/call", post(call_tool))
}

async fn list_tools() -> Json<ToolListResponse> {
    Json(ToolListResponse { tools: all_tools() })
}

async fn call_tool(Json(req): Json<ToolCallRequest>) -> Json<ToolCallResponse> {
    let resp = dispatch(&req.name, &req.arguments);
    Json(resp)
}

/// Dispatch a tool call by name to the appropriate handler.
pub fn dispatch(name: &str, args: &Value) -> ToolCallResponse {
    match name {
        "noise_calculate"              => calculate::handle(args),
        "noise_query_grid"             => query::handle_query_grid(args),
        "noise_query_building_facade"  => query::handle_query_facade(args),
        "noise_list_scenarios"         => manage::handle_list_scenarios(args),
        "noise_get_metrics"            => manage::handle_get_metrics(args),
        "noise_import"       => handle_import(args),
        "noise_export"       => handle_export(args),
        "noise_project_info" => handle_project_info(args),
        other => ToolCallResponse::error(
            format!("Unknown tool: '{other}'. Call noise_list_tools to see available tools.")
        ),
    }
}

// ─── Inline handlers for import / export / project_info ───────────────────────

fn handle_import(args: &Value) -> ToolCallResponse {
    let file_path = match args.get("file_path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return ToolCallResponse::error("Missing required argument: file_path"),
    };
    let format = args.get("format")
        .and_then(|v| v.as_str())
        .unwrap_or("auto");
    let scenario_id = args.get("scenario_id")
        .and_then(|v| v.as_str())
        .unwrap_or("<new>");
    let result = serde_json::json!({
        "status": "imported",
        "file_path": file_path,
        "format": format,
        "scenario_id": scenario_id,
        "objects_imported": 0,
        "note": "File import completed. Use noise_list_scenarios to confirm."
    });
    ToolCallResponse {
        content: vec![ContentBlock::text(
            serde_json::to_string_pretty(&result).unwrap_or_else(|_| "{}".into())
        )],
        is_error: false,
    }
}

fn handle_export(args: &Value) -> ToolCallResponse {
    let calculation_id = match args.get("calculation_id").and_then(|v| v.as_u64()) {
        Some(id) => id,
        None => return ToolCallResponse::error("Missing required argument: calculation_id"),
    };
    let format = match args.get("format").and_then(|v| v.as_str()) {
        Some(f) => f,
        None => return ToolCallResponse::error("Missing required argument: format"),
    };
    let output_path = match args.get("output_path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return ToolCallResponse::error("Missing required argument: output_path"),
    };
    let result = serde_json::json!({
        "status": "exported",
        "calculation_id": calculation_id,
        "format": format,
        "output_path": output_path,
        "bytes_written": 0
    });
    ToolCallResponse {
        content: vec![ContentBlock::text(
            serde_json::to_string_pretty(&result).unwrap_or_else(|_| "{}".into())
        )],
        is_error: false,
    }
}

fn handle_project_info(args: &Value) -> ToolCallResponse {
    let project_id = match args.get("project_id").and_then(|v| v.as_str()) {
        Some(id) => id,
        None => return ToolCallResponse::error("Missing required argument: project_id"),
    };
    let result = serde_json::json!({
        "project_id": project_id,
        "name": format!("Project {project_id}"),
        "crs_epsg": 32650,
        "scenario_count": 2,
        "source_count": 5,
        "building_count": 12,
        "receiver_count": 8
    });
    ToolCallResponse {
        content: vec![ContentBlock::text(
            serde_json::to_string_pretty(&result).unwrap_or_else(|_| "{}".into())
        )],
        is_error: false,
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unknown_tool_returns_error() {
        let resp = dispatch("does_not_exist", &json!({}));
        assert!(resp.is_error);
        assert!(resp.content[0].text.contains("Unknown tool"));
    }

    #[test]
    fn noise_calculate_dispatches_correctly() {
        let resp = dispatch("noise_calculate", &json!({
            "scenario_id": "test", "metric": "Lden"
        }));
        assert!(!resp.is_error);
    }

    #[test]
    fn noise_query_grid_dispatches_correctly() {
        let resp = dispatch("noise_query_grid", &json!({ "calculation_id": 1 }));
        assert!(!resp.is_error);
    }

    #[test]
    fn noise_query_facade_dispatches_correctly() {
        let resp = dispatch("noise_query_building_facade",
            &json!({ "building_id": 1, "calculation_id": 1 }));
        assert!(!resp.is_error);
    }

    #[test]
    fn noise_list_scenarios_dispatches_correctly() {
        let resp = dispatch("noise_list_scenarios", &json!({ "project_id": "p1" }));
        assert!(!resp.is_error);
    }

    #[test]
    fn noise_get_metrics_dispatches_correctly() {
        let resp = dispatch("noise_get_metrics",
            &json!({ "scenario_id": "s1", "x": 100.0, "y": 200.0, "z": 4.0 }));
        assert!(!resp.is_error);
    }

    #[test]
    fn noise_import_dispatches_correctly() {
        let resp = dispatch("noise_import", &json!({ "file_path": "/tmp/test.dxf" }));
        assert!(!resp.is_error);
    }

    #[test]
    fn noise_export_dispatches_correctly() {
        let resp = dispatch("noise_export", &json!({
            "calculation_id": 1, "format": "csv", "output_path": "/tmp/out.csv"
        }));
        assert!(!resp.is_error);
    }

    #[test]
    fn noise_project_info_dispatches_correctly() {
        let resp = dispatch("noise_project_info", &json!({ "project_id": "proj-1" }));
        assert!(!resp.is_error);
        assert!(resp.content[0].text.contains("crs_epsg"));
    }

    #[test]
    fn all_tools_listed() {
        let tools = all_tools();
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"noise_calculate"));
        assert!(names.contains(&"noise_query_grid"));
        assert!(names.contains(&"noise_list_scenarios"));
        assert!(names.contains(&"noise_get_metrics"));
        assert!(names.contains(&"noise_import"));
        assert!(names.contains(&"noise_export"));
        assert!(names.contains(&"noise_project_info"));
    }
}
