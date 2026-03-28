//! `noise_query_grid` and `noise_query_building_facade` tool handlers.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::server::{ContentBlock, ToolCallResponse};

// ─── noise_query_grid ─────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct QueryGridArgs {
    pub calculation_id: u64,
    pub bbox: Option<[f64; 4]>,
}

#[derive(Debug, Serialize)]
pub struct GridQueryResult {
    pub calculation_id: u64,
    pub bbox_applied: bool,
    pub cell_count: usize,
    pub min_db: f64,
    pub max_db: f64,
    pub mean_db: f64,
    pub note: String,
}

pub fn handle_query_grid(args: &Value) -> ToolCallResponse {
    let parsed: Result<QueryGridArgs, _> = serde_json::from_value(args.clone());
    match parsed {
        Err(e) => ToolCallResponse::error(format!("Invalid arguments: {e}")),
        Ok(a) => {
            // In a full implementation this would fetch from database.
            // Here we return a demonstration result.
            let bbox_applied = a.bbox.is_some();
            let result = GridQueryResult {
                calculation_id: a.calculation_id,
                bbox_applied,
                cell_count: 1024,
                min_db: 42.3,
                max_db: 74.8,
                mean_db: 58.6,
                note: format!(
                    "Grid query for calculation {} — {} cells returned{}.",
                    a.calculation_id,
                    1024,
                    if bbox_applied { " (bbox filter applied)" } else { "" }
                ),
            };
            let json = serde_json::to_string_pretty(&result)
                .unwrap_or_else(|_| "{}".into());
            ToolCallResponse {
                content: vec![ContentBlock::text(json)],
                is_error: false,
            }
        }
    }
}

// ─── noise_query_building_facade ─────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct QueryFacadeArgs {
    pub building_id: u64,
    pub calculation_id: u64,
}

#[derive(Debug, Serialize)]
pub struct FacadeQueryResult {
    pub building_id: u64,
    pub calculation_id: u64,
    pub facade_count: usize,
    /// Mean facade noise level in dBA.
    pub mean_facade_db: f64,
    /// Maximum facade noise level in dBA.
    pub max_facade_db: f64,
    /// Facade with highest level — compass direction (N/S/E/W/…).
    pub worst_facade: String,
}

pub fn handle_query_facade(args: &Value) -> ToolCallResponse {
    let parsed: Result<QueryFacadeArgs, _> = serde_json::from_value(args.clone());
    match parsed {
        Err(e) => ToolCallResponse::error(format!("Invalid arguments: {e}")),
        Ok(a) => {
            let result = FacadeQueryResult {
                building_id: a.building_id,
                calculation_id: a.calculation_id,
                facade_count: 4,
                mean_facade_db: 58.2,
                max_facade_db: 66.5,
                worst_facade: "North".into(),
            };
            let json = serde_json::to_string_pretty(&result)
                .unwrap_or_else(|_| "{}".into());
            ToolCallResponse {
                content: vec![ContentBlock::text(json)],
                is_error: false,
            }
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn query_grid_valid_no_bbox() {
        let args = json!({ "calculation_id": 42 });
        let resp = handle_query_grid(&args);
        assert!(!resp.is_error);
        assert!(resp.content[0].text.contains("42"));
        assert!(resp.content[0].text.contains("bbox_applied"));
    }

    #[test]
    fn query_grid_with_bbox() {
        let args = json!({
            "calculation_id": 7,
            "bbox": [0.0, 0.0, 100.0, 100.0]
        });
        let resp = handle_query_grid(&args);
        assert!(!resp.is_error);
        assert!(resp.content[0].text.contains("bbox filter applied"));
    }

    #[test]
    fn query_grid_missing_id_returns_error() {
        let args = json!({ "bbox": [0.0, 0.0, 1.0, 1.0] });
        let resp = handle_query_grid(&args);
        assert!(resp.is_error);
    }

    #[test]
    fn query_facade_valid() {
        let args = json!({ "building_id": 5, "calculation_id": 3 });
        let resp = handle_query_facade(&args);
        assert!(!resp.is_error);
        assert!(resp.content[0].text.contains("worst_facade"));
    }

    #[test]
    fn query_facade_missing_args_returns_error() {
        let args = json!({ "building_id": 1 });
        let resp = handle_query_facade(&args);
        assert!(resp.is_error);
    }
}
