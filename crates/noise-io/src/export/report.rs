//! Calculation report export — plain text and Markdown summary.
//!
//! Generates human-readable summaries of noise calculation results including
//! statistics, exceedance counts, and source contributions.

use std::path::Path;
use thiserror::Error;

/// Errors during report generation.
#[derive(Debug, Error)]
pub enum ReportError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("No data: {0}")]
    NoData(String),
}

/// Statistics for a single noise level grid.
#[derive(Debug, Clone)]
pub struct GridStats {
    /// Minimum finite level (dBA).
    pub min_db: f32,
    /// Maximum finite level (dBA).
    pub max_db: f32,
    /// Mean of finite levels (dBA).
    pub mean_db: f32,
    /// Number of finite cells.
    pub count: usize,
    /// Total cells (including NODATA).
    pub total_cells: usize,
}

impl GridStats {
    /// Compute statistics from a grid slice (NODATA = `f32::NEG_INFINITY`).
    pub fn from_grid(levels: &[f32]) -> Result<Self, ReportError> {
        let finite: Vec<f32> = levels.iter().copied().filter(|v| v.is_finite()).collect();
        if finite.is_empty() {
            return Err(ReportError::NoData("All cells are NODATA".into()));
        }
        let min_db = finite.iter().copied().fold(f32::INFINITY, f32::min);
        let max_db = finite.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let mean_db = finite.iter().sum::<f32>() / finite.len() as f32;
        Ok(GridStats {
            min_db,
            max_db,
            mean_db,
            count: finite.len(),
            total_cells: levels.len(),
        })
    }

    /// Percentage of cells with level ≥ `threshold_db`.
    pub fn exceedance_pct(&self, threshold_db: f32, levels: &[f32]) -> f64 {
        if self.count == 0 { return 0.0; }
        let exceed = levels.iter().filter(|&&v| v >= threshold_db).count();
        exceed as f64 / self.count as f64 * 100.0
    }
}

/// A named source contribution for the report.
#[derive(Debug, Clone)]
pub struct SourceReport {
    pub id: u64,
    pub name: String,
    pub lw_dba: f64,
}

/// Full report data for one calculation.
#[derive(Debug, Clone)]
pub struct NoiseReport {
    pub project_name: String,
    pub scenario_name: String,
    pub metric: String,
    pub grid_stats: GridStats,
    pub sources: Vec<SourceReport>,
    /// Exceedance thresholds to report (dBA).
    pub thresholds: Vec<f32>,
}

impl NoiseReport {
    /// Render as a Markdown string.
    pub fn to_markdown(&self, levels: &[f32]) -> String {
        let mut s = String::new();
        s.push_str(&format!("# Noise Report — {}\n\n", self.project_name));
        s.push_str(&format!("**Scenario:** {}  \n", self.scenario_name));
        s.push_str(&format!("**Metric:** {}  \n\n", self.metric));

        s.push_str("## Grid Statistics\n\n");
        s.push_str("| Parameter | Value |\n");
        s.push_str("|---|---|\n");
        s.push_str(&format!("| Min level | {:.1} dBA |\n", self.grid_stats.min_db));
        s.push_str(&format!("| Max level | {:.1} dBA |\n", self.grid_stats.max_db));
        s.push_str(&format!("| Mean level | {:.1} dBA |\n", self.grid_stats.mean_db));
        s.push_str(&format!("| Receiver cells | {} / {} |\n",
            self.grid_stats.count, self.grid_stats.total_cells));

        if !self.thresholds.is_empty() {
            s.push_str("\n## Exceedance Summary\n\n");
            s.push_str("| Threshold | Cells exceeding |\n");
            s.push_str("|---|---|\n");
            for &t in &self.thresholds {
                let pct = self.grid_stats.exceedance_pct(t, levels);
                s.push_str(&format!("| ≥ {:.0} dBA | {:.1}% |\n", t, pct));
            }
        }

        if !self.sources.is_empty() {
            s.push_str("\n## Noise Sources\n\n");
            s.push_str("| ID | Name | Lw (dBA) |\n");
            s.push_str("|---|---|---|\n");
            for src in &self.sources {
                s.push_str(&format!("| {} | {} | {:.1} |\n", src.id, src.name, src.lw_dba));
            }
        }

        s
    }

    /// Render as plain text.
    pub fn to_text(&self, levels: &[f32]) -> String {
        let mut s = String::new();
        s.push_str(&format!("NOISE REPORT — {}\n", self.project_name));
        s.push_str(&"=".repeat(60));
        s.push('\n');
        s.push_str(&format!("Scenario : {}\n", self.scenario_name));
        s.push_str(&format!("Metric   : {}\n", self.metric));
        s.push('\n');
        s.push_str("GRID STATISTICS\n");
        s.push_str(&"-".repeat(40));
        s.push('\n');
        s.push_str(&format!("  Min : {:.1} dBA\n", self.grid_stats.min_db));
        s.push_str(&format!("  Max : {:.1} dBA\n", self.grid_stats.max_db));
        s.push_str(&format!("  Mean: {:.1} dBA\n", self.grid_stats.mean_db));
        s.push_str(&format!("  Cells: {} / {}\n", self.grid_stats.count, self.grid_stats.total_cells));
        s.push('\n');

        if !self.thresholds.is_empty() {
            s.push_str("EXCEEDANCES\n");
            s.push_str(&"-".repeat(40));
            s.push('\n');
            for &t in &self.thresholds {
                let pct = self.grid_stats.exceedance_pct(t, levels);
                s.push_str(&format!("  >= {:.0} dBA : {:.1}%\n", t, pct));
            }
            s.push('\n');
        }

        if !self.sources.is_empty() {
            s.push_str("SOURCES\n");
            s.push_str(&"-".repeat(40));
            s.push('\n');
            for src in &self.sources {
                s.push_str(&format!("  [{:3}] {:30} {:5.1} dBA\n",
                    src.id, src.name, src.lw_dba));
            }
        }
        s
    }

    /// Write Markdown report to file.
    pub fn write_markdown(&self, levels: &[f32], path: impl AsRef<Path>) -> Result<(), ReportError> {
        std::fs::write(path, self.to_markdown(levels)).map_err(ReportError::Io)
    }

    /// Write plain-text report to file.
    pub fn write_text(&self, levels: &[f32], path: impl AsRef<Path>) -> Result<(), ReportError> {
        std::fs::write(path, self.to_text(levels)).map_err(ReportError::Io)
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_levels() -> Vec<f32> {
        vec![55.0, 60.0, 65.0, 70.0, f32::NEG_INFINITY, 58.0]
    }

    fn sample_report(stats: GridStats) -> NoiseReport {
        NoiseReport {
            project_name: "Test Project".into(),
            scenario_name: "Base Case".into(),
            metric: "Lden".into(),
            grid_stats: stats,
            sources: vec![
                SourceReport { id: 1, name: "Main Road".into(), lw_dba: 75.0 },
                SourceReport { id: 2, name: "Factory".into(),   lw_dba: 80.5 },
            ],
            thresholds: vec![55.0, 65.0, 70.0],
        }
    }

    #[test]
    fn stats_min_max_mean() {
        let levels = sample_levels();
        let stats = GridStats::from_grid(&levels).unwrap();
        assert!((stats.min_db - 55.0).abs() < 0.01);
        assert!((stats.max_db - 70.0).abs() < 0.01);
        assert_eq!(stats.count, 5);
        assert_eq!(stats.total_cells, 6);
    }

    #[test]
    fn stats_all_nodata_returns_error() {
        let levels = vec![f32::NEG_INFINITY; 4];
        assert!(GridStats::from_grid(&levels).is_err());
    }

    #[test]
    fn exceedance_pct_correct() {
        let levels = sample_levels();
        let stats = GridStats::from_grid(&levels).unwrap();
        // ≥ 65: 65.0, 70.0 → 2/5 = 40%
        let pct = stats.exceedance_pct(65.0, &levels);
        assert!((pct - 40.0).abs() < 0.01, "got {pct}");
    }

    #[test]
    fn markdown_contains_key_sections() {
        let levels = sample_levels();
        let stats = GridStats::from_grid(&levels).unwrap();
        let report = sample_report(stats);
        let md = report.to_markdown(&levels);
        assert!(md.contains("# Noise Report"));
        assert!(md.contains("## Grid Statistics"));
        assert!(md.contains("## Exceedance Summary"));
        assert!(md.contains("## Noise Sources"));
        assert!(md.contains("Main Road"));
    }

    #[test]
    fn text_contains_key_sections() {
        let levels = sample_levels();
        let stats = GridStats::from_grid(&levels).unwrap();
        let report = sample_report(stats);
        let txt = report.to_text(&levels);
        assert!(txt.contains("NOISE REPORT"));
        assert!(txt.contains("GRID STATISTICS"));
        assert!(txt.contains("EXCEEDANCES"));
        assert!(txt.contains("Factory"));
    }

    #[test]
    fn mean_computed_correctly() {
        let levels = vec![60.0f32, 70.0, 80.0];
        let stats = GridStats::from_grid(&levels).unwrap();
        assert!((stats.mean_db - 70.0).abs() < 0.01);
    }

    #[test]
    fn write_text_file() {
        let levels = sample_levels();
        let stats = GridStats::from_grid(&levels).unwrap();
        let report = sample_report(stats);
        let path = std::env::temp_dir().join("noise_report_test.txt");
        report.write_text(&levels, &path).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("NOISE REPORT"));
        let _ = std::fs::remove_file(&path);
    }
}
