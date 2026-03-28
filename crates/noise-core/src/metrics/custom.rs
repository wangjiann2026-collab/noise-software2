//! User-defined custom noise evaluation metric formulas.
//!
//! Users can define formulas such as:
//!   "10 * log10((12 * 10^(Ld/10) + 4 * 10^(Le/10) + 8 * 10^(Ln/10)) / 24)"
//!
//! Variables available: Ld, Le, Ln, Leq

use evalexpr::{context_map, eval_float_with_context, DefaultNumericTypes, HashMapContext};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CustomMetricError {
    #[error("Formula evaluation error: {0}")]
    EvalError(String),
    #[error("Formula is empty")]
    EmptyFormula,
}

/// A user-defined noise metric formula.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomMetric {
    pub name: String,
    pub unit: String,
    /// Formula string using variables: Ld, Le, Ln, Leq
    pub formula: String,
}

impl CustomMetric {
    pub fn new(name: impl Into<String>, unit: impl Into<String>, formula: impl Into<String>) -> Self {
        Self { name: name.into(), unit: unit.into(), formula: formula.into() }
    }

    /// Evaluate the formula with given period levels.
    pub fn evaluate(&self, ld: f64, le: f64, ln: f64, leq: f64) -> Result<f64, CustomMetricError> {
        if self.formula.trim().is_empty() {
            return Err(CustomMetricError::EmptyFormula);
        }
        let context: HashMapContext<DefaultNumericTypes> = context_map! {
            "Ld"  => float ld,
            "Le"  => float le,
            "Ln"  => float ln,
            "Leq" => float leq,
        }
        .map_err(|e| CustomMetricError::EvalError(e.to_string()))?;

        eval_float_with_context(&self.formula, &context)
            .map_err(|e| CustomMetricError::EvalError(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_formula_returns_ld() {
        let m = CustomMetric::new("TestLd", "dBA", "Ld");
        let result = m.evaluate(65.0, 60.0, 55.0, 62.0).unwrap();
        assert!((result - 65.0).abs() < 1e-9);
    }

    #[test]
    fn simple_arithmetic_formula() {
        let m = CustomMetric::new("Custom", "dBA", "Ld + 5");
        let result = m.evaluate(60.0, 0.0, 0.0, 0.0).unwrap();
        assert!((result - 65.0).abs() < 1e-9);
    }

    #[test]
    fn empty_formula_returns_error() {
        let m = CustomMetric::new("Bad", "dBA", "  ");
        assert!(matches!(m.evaluate(60.0, 55.0, 50.0, 58.0), Err(CustomMetricError::EmptyFormula)));
    }
}
