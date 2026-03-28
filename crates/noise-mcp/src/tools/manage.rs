//! `noise_list_scenarios` and `noise_get_metrics` tool handlers.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::server::{ContentBlock, ToolCallResponse};

// ─── noise_list_scenarios ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ListScenariosArgs {
    pub project_id: String,
}

#[derive(Debug, Serialize)]
pub struct ScenarioSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct ListScenariosResult {
    pub project_id: String,
    pub scenario_count: usize,
    pub scenarios: Vec<ScenarioSummary>,
}

pub fn handle_list_scenarios(args: &Value) -> ToolCallResponse {
    let parsed: Result<ListScenariosArgs, _> = serde_json::from_value(args.clone());
    match parsed {
        Err(e) => ToolCallResponse::error(format!("Invalid arguments: {e}")),
        Ok(a) => {
            // Demonstration: return synthetic scenario list.
            let scenarios = vec![
                ScenarioSummary {
                    id: format!("{}_base", a.project_id),
                    name: "Base Case".into(),
                    description: "Existing conditions without mitigation".into(),
                    status: "calculated".into(),
                },
                ScenarioSummary {
                    id: format!("{}_barrier", a.project_id),
                    name: "Barrier Option A".into(),
                    description: "3 m noise barrier on north side of road".into(),
                    status: "pending".into(),
                },
            ];
            let result = ListScenariosResult {
                project_id: a.project_id.clone(),
                scenario_count: scenarios.len(),
                scenarios,
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

// ─── noise_get_metrics ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct GetMetricsArgs {
    pub scenario_id: String,
    pub x: f64,
    pub y: f64,
    pub z: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct MetricsResult {
    pub scenario_id: String,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    /// Day-time level (07:00–19:00), dBA.
    pub ld_dba: f64,
    /// Evening-time level (19:00–23:00), dBA.
    pub le_dba: f64,
    /// Night-time level (23:00–07:00), dBA.
    pub ln_dba: f64,
    /// Day-evening-night level (EU standard), dBA.
    pub lden_dba: f64,
    /// Day-night level (US standard), dBA.
    pub ldn_dba: f64,
}

impl MetricsResult {
    /// Compute Lden from Ld/Le/Ln using EU formula.
    /// Lden = 10 * log10( (12*10^(Ld/10) + 4*10^((Le+5)/10) + 8*10^((Ln+10)/10)) / 24 )
    pub fn lden(ld: f64, le: f64, ln: f64) -> f64 {
        let d = 12.0 * 10f64.powf(ld / 10.0);
        let e = 4.0  * 10f64.powf((le + 5.0) / 10.0);
        let n = 8.0  * 10f64.powf((ln + 10.0) / 10.0);
        10.0 * (d + e + n).log10() - 10.0 * 24f64.log10()
    }

    /// Compute Ldn from Ld/Ln using US formula.
    /// Ldn = 10 * log10( (15*10^(Ld/10) + 9*10^((Ln+10)/10)) / 24 )
    pub fn ldn(ld: f64, ln: f64) -> f64 {
        let d = 15.0 * 10f64.powf(ld / 10.0);
        let n = 9.0  * 10f64.powf((ln + 10.0) / 10.0);
        10.0 * (d + n).log10() - 10.0 * 24f64.log10()
    }
}

pub fn handle_get_metrics(args: &Value) -> ToolCallResponse {
    let parsed: Result<GetMetricsArgs, _> = serde_json::from_value(args.clone());
    match parsed {
        Err(e) => ToolCallResponse::error(format!("Invalid arguments: {e}")),
        Ok(a) => {
            let z = a.z.unwrap_or(4.0);

            // Demonstration values — in production these would be interpolated
            // from a pre-calculated grid.
            let ld = 62.0;
            let le = 58.0;
            let ln = 52.0;
            let lden = MetricsResult::lden(ld, le, ln);
            let ldn  = MetricsResult::ldn(ld, ln);

            let result = MetricsResult {
                scenario_id: a.scenario_id,
                x: a.x,
                y: a.y,
                z,
                ld_dba: ld,
                le_dba: le,
                ln_dba: ln,
                lden_dba: (lden * 10.0).round() / 10.0,
                ldn_dba:  (ldn  * 10.0).round() / 10.0,
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
    fn list_scenarios_valid() {
        let args = json!({ "project_id": "proj-1" });
        let resp = handle_list_scenarios(&args);
        assert!(!resp.is_error);
        let txt = &resp.content[0].text;
        assert!(txt.contains("Base Case"));
        assert!(txt.contains("proj-1"));
    }

    #[test]
    fn list_scenarios_missing_id_error() {
        let resp = handle_list_scenarios(&json!({}));
        assert!(resp.is_error);
    }

    #[test]
    fn get_metrics_valid() {
        let args = json!({ "scenario_id": "s1", "x": 100.0, "y": 200.0 });
        let resp = handle_get_metrics(&args);
        assert!(!resp.is_error);
        let txt = &resp.content[0].text;
        assert!(txt.contains("lden_dba"));
        assert!(txt.contains("ldn_dba"));
    }

    #[test]
    fn get_metrics_default_z() {
        let args = json!({ "scenario_id": "s1", "x": 0.0, "y": 0.0 });
        let resp = handle_get_metrics(&args);
        assert!(!resp.is_error);
        assert!(resp.content[0].text.contains("4.0"));
    }

    #[test]
    fn lden_formula_correct() {
        // Verify EU Lden formula with equal Ld=Le=Ln=60 dBA:
        // Lden = 10*log10((12*10^6 + 4*10^6.5 + 8*10^7) / 24)
        let lden = MetricsResult::lden(60.0, 60.0, 60.0);
        // Should be about 66.4 dBA (night +10 dB penalty dominates)
        assert!(lden > 64.0 && lden < 68.0, "lden = {lden:.2}");
    }

    #[test]
    fn ldn_formula_correct() {
        // With Ld=60, Ln=60: Ldn = 10*log10((15*10^6 + 9*10^7)/24)
        let ldn = MetricsResult::ldn(60.0, 60.0);
        assert!(ldn > 64.0 && ldn < 70.0, "ldn = {ldn:.2}");
    }
}
