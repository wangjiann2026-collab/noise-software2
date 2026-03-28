//! MCP HTTP server built on Axum — stub for Phase 6.
//!
//! Implements the MCP protocol:
//!   POST /mcp/v1/tools/list   → list available tools
//!   POST /mcp/v1/tools/call   → invoke a tool

use axum::{Json, Router, routing::post};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::schema::{McpTool, all_tools};

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
    // Full dispatch implemented in Phase 6.
    let msg = format!("Tool '{}' invoked — implementation pending (Phase 6)", req.name);
    Json(ToolCallResponse {
        content: vec![ContentBlock::text(msg)],
        is_error: false,
    })
}
