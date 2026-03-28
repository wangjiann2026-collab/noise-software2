//! MCP tool schema definitions (JSON Schema for AI agent discovery).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// An MCP tool definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
}

/// Build the complete list of tools exposed by this MCP server.
pub fn all_tools() -> Vec<McpTool> {
    vec![
        McpTool {
            name: "noise_calculate".into(),
            description: "Run acoustic noise calculation for a scenario. Returns job ID for async result retrieval.".into(),
            input_schema: json!({
                "type": "object",
                "required": ["scenario_id", "grid_type", "metric"],
                "properties": {
                    "scenario_id": { "type": "string", "description": "UUID of the scenario to calculate" },
                    "grid_type": {
                        "type": "string",
                        "enum": ["horizontal", "vertical", "facade"],
                        "description": "Type of calculation grid"
                    },
                    "metric": {
                        "type": "string",
                        "enum": ["Ld", "Le", "Ln", "Lden", "Ldn", "L10", "L1hmax", "custom"],
                        "description": "Noise evaluation metric"
                    },
                    "custom_formula": {
                        "type": "string",
                        "description": "Custom formula (required when metric='custom'). Variables: Ld, Le, Ln, Leq"
                    },
                    "grid_resolution_m": {
                        "type": "number",
                        "description": "Grid resolution in metres (default: 10)"
                    }
                }
            }),
        },
        McpTool {
            name: "noise_query_grid".into(),
            description: "Query horizontal noise grid results for a completed calculation.".into(),
            input_schema: json!({
                "type": "object",
                "required": ["calculation_id"],
                "properties": {
                    "calculation_id": { "type": "integer" },
                    "bbox": {
                        "type": "array",
                        "items": { "type": "number" },
                        "minItems": 4,
                        "maxItems": 4,
                        "description": "[xmin, ymin, xmax, ymax] filter"
                    }
                }
            }),
        },
        McpTool {
            name: "noise_query_building_facade".into(),
            description: "Query facade noise levels for a specific building.".into(),
            input_schema: json!({
                "type": "object",
                "required": ["building_id", "calculation_id"],
                "properties": {
                    "building_id": { "type": "integer" },
                    "calculation_id": { "type": "integer" }
                }
            }),
        },
        McpTool {
            name: "noise_list_scenarios".into(),
            description: "List all scenarios and variants in a project.".into(),
            input_schema: json!({
                "type": "object",
                "required": ["project_id"],
                "properties": {
                    "project_id": { "type": "string" }
                }
            }),
        },
        McpTool {
            name: "noise_get_metrics".into(),
            description: "Get standard noise metrics (Ld, Ln, Lden, etc.) at a specific receiver point.".into(),
            input_schema: json!({
                "type": "object",
                "required": ["scenario_id", "x", "y", "z"],
                "properties": {
                    "scenario_id": { "type": "string" },
                    "x": { "type": "number", "description": "X coordinate (project CRS)" },
                    "y": { "type": "number", "description": "Y coordinate (project CRS)" },
                    "z": { "type": "number", "description": "Z coordinate / height (m)", "default": 4.0 }
                }
            }),
        },
    ]
}
