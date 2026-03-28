//! Multi-scenario variant management.
//!
//! Each project has a base scenario plus optional variants for
//! comparing different noise mitigation strategies.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A complete noise assessment project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub created_at: String,
    pub updated_at: String,
    /// EPSG coordinate system code (e.g., 4326 for WGS84).
    pub crs_epsg: u32,
    pub base_scenario: Scenario,
    pub variants: Vec<ScenarioVariant>,
}

impl Project {
    pub fn new(name: impl Into<String>, crs_epsg: u32) -> Self {
        let now = chrono_now();
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            description: String::new(),
            created_at: now.clone(),
            updated_at: now,
            crs_epsg,
            base_scenario: Scenario::new("Base Scenario"),
            variants: Vec::new(),
        }
    }

    /// Add a new variant derived from the base scenario.
    pub fn add_variant(&mut self, name: impl Into<String>) -> &ScenarioVariant {
        let variant = ScenarioVariant::new(name, self.base_scenario.id);
        self.variants.push(variant);
        self.variants.last().unwrap()
    }

    /// Find a variant by ID.
    pub fn variant(&self, id: Uuid) -> Option<&ScenarioVariant> {
        self.variants.iter().find(|v| v.id == id)
    }
}

/// A noise scenario containing all scene objects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scenario {
    pub id: Uuid,
    pub name: String,
    pub description: String,
}

impl Scenario {
    pub fn new(name: impl Into<String>) -> Self {
        Self { id: Uuid::new_v4(), name: name.into(), description: String::new() }
    }
}

/// A scenario variant that stores only the differences (delta) from the base.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioVariant {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    /// ID of the parent scenario this variant is derived from.
    pub parent_scenario_id: Uuid,
    /// Notes describing the mitigation strategy.
    pub strategy_notes: String,
    /// Override objects specific to this variant (delta storage).
    pub overrides: Vec<ObjectOverride>,
}

impl ScenarioVariant {
    pub fn new(name: impl Into<String>, parent_id: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            description: String::new(),
            parent_scenario_id: parent_id,
            strategy_notes: String::new(),
            overrides: Vec::new(),
        }
    }
}

/// A single object override in a variant (add, remove, or modify).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ObjectOverride {
    Add { object_json: String },
    Remove { object_id: u64 },
    Modify { object_id: u64, patch_json: String },
}

fn chrono_now() -> String {
    // Simple RFC3339 timestamp without chrono dependency in scaffold.
    "2026-01-01T00:00:00Z".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_starts_with_no_variants() {
        let p = Project::new("Test Project", 32650);
        assert!(p.variants.is_empty());
    }

    #[test]
    fn add_variant_increments_count() {
        let mut p = Project::new("Test", 32650);
        p.add_variant("Variant A");
        p.add_variant("Variant B");
        assert_eq!(p.variants.len(), 2);
    }

    #[test]
    fn variant_parent_matches_base() {
        let mut p = Project::new("Test", 32650);
        let base_id = p.base_scenario.id;
        p.add_variant("V1");
        assert_eq!(p.variants[0].parent_scenario_id, base_id);
    }
}
