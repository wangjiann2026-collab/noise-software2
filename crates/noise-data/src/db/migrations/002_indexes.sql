-- ============================================================
-- Migration 002: Additional indexes for performance
-- ============================================================

-- Index for calculation result lookups by scenario + metric.
CREATE INDEX IF NOT EXISTS idx_calc_results_scenario_metric
    ON calculation_results(scenario_id, metric);

-- Index for scenario parent lookup (variant chains).
CREATE INDEX IF NOT EXISTS idx_scenarios_parent
    ON scenarios(parent_id)
    WHERE parent_id IS NOT NULL;
