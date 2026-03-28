//! Multi-scenario variant management.
//!
//! Variants store only the *delta* against the base scenario (add/remove/modify).
//! The `VariantResolver` merges base + delta to produce the final object list.

use crate::entities::SceneObject;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ─── Core types ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub created_at: String,
    pub updated_at: String,
    pub crs_epsg: u32,
    pub base_scenario: Scenario,
    pub variants: Vec<ScenarioVariant>,
}

impl Project {
    pub fn new(name: impl Into<String>, crs_epsg: u32) -> Self {
        let now = timestamp_now();
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

    pub fn add_variant(&mut self, name: impl Into<String>) -> &ScenarioVariant {
        let v = ScenarioVariant::new(name, self.base_scenario.id);
        self.variants.push(v);
        self.variants.last().unwrap()
    }

    pub fn variant(&self, id: Uuid) -> Option<&ScenarioVariant> {
        self.variants.iter().find(|v| v.id == id)
    }

    pub fn variant_mut(&mut self, id: Uuid) -> Option<&mut ScenarioVariant> {
        self.variants.iter_mut().find(|v| v.id == id)
    }
}

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

/// A scenario variant: stores only the diff against the parent scenario.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioVariant {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub parent_scenario_id: Uuid,
    pub strategy_notes: String,
    /// Delta operations applied on top of the base scenario.
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

    /// Add a new object to this variant.
    pub fn add_object(&mut self, obj: &SceneObject) {
        let json = serde_json::to_string(obj).unwrap_or_default();
        self.overrides.push(ObjectOverride::Add { object_json: json });
    }

    /// Remove an object from this variant (by base-scenario object id).
    pub fn remove_object(&mut self, object_id: u64) {
        self.overrides.push(ObjectOverride::Remove { object_id });
    }

    /// Modify an existing object in this variant.
    pub fn modify_object(&mut self, object_id: u64, updated: &SceneObject) {
        let patch = serde_json::to_string(updated).unwrap_or_default();
        self.overrides.push(ObjectOverride::Modify { object_id, patch_json: patch });
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ObjectOverride {
    Add    { object_json: String },
    Remove { object_id: u64 },
    Modify { object_id: u64, patch_json: String },
}

// ─── Variant resolver ─────────────────────────────────────────────────────────

/// Merges a base object list with a variant's delta to produce the effective scene.
pub struct VariantResolver;

impl VariantResolver {
    /// Apply variant overrides to a cloned base object list.
    ///
    /// - `Add`    → appends the new object
    /// - `Remove` → filters out objects with matching id
    /// - `Modify` → replaces the matching object
    pub fn resolve(
        base_objects: Vec<SceneObject>,
        variant: &ScenarioVariant,
    ) -> Vec<SceneObject> {
        let mut objects = base_objects;

        for op in &variant.overrides {
            match op {
                ObjectOverride::Add { object_json } => {
                    if let Ok(obj) = serde_json::from_str::<SceneObject>(object_json) {
                        objects.push(obj);
                    }
                }
                ObjectOverride::Remove { object_id } => {
                    objects.retain(|o| o.id() != *object_id);
                }
                ObjectOverride::Modify { object_id, patch_json } => {
                    if let Ok(replacement) = serde_json::from_str::<SceneObject>(patch_json) {
                        if let Some(pos) = objects.iter().position(|o| o.id() == *object_id) {
                            objects[pos] = replacement;
                        }
                    }
                }
            }
        }
        objects
    }
}

fn timestamp_now() -> String {
    "2026-01-01T00:00:00Z".into()
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::{ReceiverPoint, SceneObject};
    use nalgebra::Point3;

    fn recv(id: u64) -> SceneObject {
        SceneObject::Receiver(ReceiverPoint::new(id, format!("R{id}"), 0.0, 0.0, 0.0, 4.0))
    }

    #[test]
    fn variant_add_appends_object() {
        let base = vec![recv(1), recv(2)];
        let mut project = Project::new("P", 32650);
        project.add_variant("V");
        project.variants[0].add_object(&recv(3));
        let resolved = VariantResolver::resolve(base, &project.variants[0]);
        assert_eq!(resolved.len(), 3);
        assert!(resolved.iter().any(|o| o.id() == 3));
    }

    #[test]
    fn variant_remove_deletes_object() {
        let base = vec![recv(1), recv(2), recv(3)];
        let mut project = Project::new("P", 32650);
        project.add_variant("V");
        project.variants[0].remove_object(2);
        let resolved = VariantResolver::resolve(base, &project.variants[0]);
        assert_eq!(resolved.len(), 2);
        assert!(!resolved.iter().any(|o| o.id() == 2));
    }

    #[test]
    fn variant_modify_replaces_object() {
        let base = vec![recv(1), recv(2)];
        let updated = SceneObject::Receiver(
            ReceiverPoint::new(1, "Modified", 99.0, 99.0, 0.0, 4.0)
        );
        let mut project = Project::new("P", 32650);
        project.add_variant("V");
        project.variants[0].modify_object(1, &updated);
        let resolved = VariantResolver::resolve(base, &project.variants[0]);
        assert_eq!(resolved.len(), 2);
        let r1 = resolved.iter().find(|o| o.id() == 1).unwrap();
        assert_eq!(r1.name(), "Modified");
    }

    #[test]
    fn empty_variant_leaves_base_unchanged() {
        let base = vec![recv(1), recv(2)];
        let mut project = Project::new("P", 32650);
        project.add_variant("Empty");
        let resolved = VariantResolver::resolve(base.clone(), &project.variants[0]);
        assert_eq!(resolved.len(), 2);
    }

    #[test]
    fn compound_variant_add_then_remove() {
        let base = vec![recv(1), recv(2)];
        let mut project = Project::new("P", 32650);
        project.add_variant("V");
        project.variants[0].add_object(&recv(3));
        project.variants[0].remove_object(1); // remove original
        let resolved = VariantResolver::resolve(base, &project.variants[0]);
        assert_eq!(resolved.len(), 2); // 2 original + 1 added - 1 removed
        assert!(!resolved.iter().any(|o| o.id() == 1));
        assert!(resolved.iter().any(|o| o.id() == 3));
    }

    #[test]
    fn project_starts_with_no_variants() {
        let p = Project::new("Test", 32650);
        assert!(p.variants.is_empty());
    }

    #[test]
    fn variant_parent_matches_base() {
        let mut p = Project::new("Test", 32650);
        let base_id = p.base_scenario.id;
        p.add_variant("V1");
        assert_eq!(p.variants[0].parent_scenario_id, base_id);
    }
}
