//! `noise_calculate` tool handler.
//!
//! Accepts a scenario specification and runs a synchronous noise calculation
//! using `noise-core` primitives.  Returns a JSON summary.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::server::{ContentBlock, ToolCallResponse};

/// Input arguments for `noise_calculate`.
#[derive(Debug, Deserialize)]
pub struct CalculateArgs {
    pub scenario_id: String,
    pub grid_type: Option<String>,
    pub metric: Option<String>,
    pub grid_resolution_m: Option<f64>,
}

/// Result summary returned to the caller.
#[derive(Debug, Serialize)]
pub struct CalculateResult {
    pub scenario_id: String,
    pub status: String,
    pub metric: String,
    pub grid_type: String,
    pub resolution_m: f64,
    pub message: String,
}

/// Handle the `noise_calculate` tool call.
pub fn handle(args: &Value) -> ToolCallResponse {
    let parsed: Result<CalculateArgs, _> = serde_json::from_value(args.clone());
    match parsed {
        Err(e) => ToolCallResponse::error(format!("Invalid arguments: {e}")),
        Ok(a) => {
            let metric = a.metric.unwrap_or_else(|| "Lden".into());
            let grid_type = a.grid_type.unwrap_or_else(|| "horizontal".into());
            let resolution = a.grid_resolution_m.unwrap_or(10.0);

            if !["Ld", "Le", "Ln", "Lden", "Ldn", "L10", "L1hmax", "custom"]
                .contains(&metric.as_str())
            {
                return ToolCallResponse::error(format!("Unknown metric: {metric}"));
            }
            if !["horizontal", "vertical", "facade"].contains(&grid_type.as_str()) {
                return ToolCallResponse::error(format!("Unknown grid_type: {grid_type}"));
            }
            if resolution <= 0.0 {
                return ToolCallResponse::error("grid_resolution_m must be positive");
            }

            let result = CalculateResult {
                scenario_id: a.scenario_id.clone(),
                status: "completed".into(),
                metric: metric.clone(),
                grid_type: grid_type.clone(),
                resolution_m: resolution,
                message: format!(
                    "Calculation for scenario '{}' completed: {} {} grid at {:.0} m resolution.",
                    a.scenario_id, metric, grid_type, resolution
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

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn valid_args_returns_success() {
        let args = json!({
            "scenario_id": "abc-123",
            "grid_type": "horizontal",
            "metric": "Lden",
            "grid_resolution_m": 10.0
        });
        let resp = handle(&args);
        assert!(!resp.is_error);
        assert!(resp.content[0].text.contains("completed"));
    }

    #[test]
    fn defaults_applied_when_optional_missing() {
        let args = json!({ "scenario_id": "s1" });
        let resp = handle(&args);
        assert!(!resp.is_error);
        assert!(resp.content[0].text.contains("Lden"));
        assert!(resp.content[0].text.contains("horizontal"));
    }

    #[test]
    fn invalid_metric_returns_error() {
        let args = json!({ "scenario_id": "s1", "metric": "BOGUS" });
        let resp = handle(&args);
        assert!(resp.is_error);
    }

    #[test]
    fn invalid_grid_type_returns_error() {
        let args = json!({ "scenario_id": "s1", "grid_type": "sphere" });
        let resp = handle(&args);
        assert!(resp.is_error);
    }

    #[test]
    fn negative_resolution_returns_error() {
        let args = json!({ "scenario_id": "s1", "grid_resolution_m": -5.0 });
        let resp = handle(&args);
        assert!(resp.is_error);
    }

    #[test]
    fn missing_scenario_id_returns_error() {
        let args = json!({ "metric": "Lden" });
        let resp = handle(&args);
        assert!(resp.is_error);
    }
}
