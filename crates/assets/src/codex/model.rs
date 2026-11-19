use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use starrupture_observation_model::codex::{NativeContext, asset_id};
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Amount {
    pub item: String,
    pub quantity: u32,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Item {
    pub name: String,
    pub description: Option<String>,
    pub icon: String,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum Unlock {
    Research {
        materials: Vec<Amount>,
    },
    Building,
    FirstWeaponPurchase {
        weapons: Vec<Weapon>,
    },
    Default,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Weapon {
    pub id: String,
    pub name: String,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    pub name: String,
    pub output: Amount,
    pub inputs: Vec<Amount>,
    pub producers: Vec<String>,
    /// Authored/default recipe time only; zero does not assert instantaneous production.
    pub base_seconds: f32,
    pub icon: String,
    pub unlock: Unlock,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum BuildingUnlock {
    Corporation {
        id: String,
        name: String,
        level: u32,
        collection: Option<String>,
    },
    Upgrade {
        id: String,
        offers: Vec<CorporationOffer>,
        tasks: Vec<Task>,
        materials: Vec<Amount>,
    },
    Tutorial {
        flow: String,
        objective: String,
        condition: String,
    },
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorporationOffer {
    pub id: String,
    pub name: String,
    pub level: u32,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Task {
    pub id: String,
    pub text: String,
    pub context: String,
    pub value: String,
    pub count: u32,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Crafting {
    pub collection: Option<String>,
    pub recipe_list_known: bool,
    pub loop_seconds: f32,
    pub overwrite_seconds: f32,
    pub processor: bool,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Building {
    pub name: String,
    pub icon: String,
    pub category: u8,
    pub materials: Vec<Amount>,
    pub add_platform: bool,
    pub add_pillar: bool,
    pub unlocks: Vec<BuildingUnlock>,
    /// Sequential upgrade tiers, not evidence of initial placement availability.
    pub core_progression: bool,
    pub crafting: Option<Crafting>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreTier {
    pub source_index: u32,
    pub materials: Vec<Amount>,
    pub cooling: u32,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalogue {
    pub version: u32,
    pub context: NativeContext,
    pub items: BTreeMap<String, Item>,
    pub recipes: BTreeMap<String, Recipe>,
    pub buildings: BTreeMap<String, Building>,
    pub core_tiers: Vec<CoreTier>,
    pub excluded: BTreeMap<String, String>,
}
impl Catalogue {
    pub fn validate(&self) -> Result<()> {
        self.context.validate().map_err(anyhow::Error::msg)?;
        ensure!(
            self.version == 1
                && !self.items.is_empty()
                && self.items.len() <= 4096
                && !self.recipes.is_empty()
                && self.recipes.len() <= 4096
                && !self.buildings.is_empty()
                && self.buildings.len() <= 1024
                && self.excluded.len() <= 8192
                && (1..=64).contains(&self.core_tiers.len()),
            "Catalogue size/version bounds"
        );
        let bill = |rows: &[Amount]| -> Result<()> {
            ensure!(rows.len() <= 128, "Material list bound");
            let mut seen = std::collections::BTreeSet::new();
            ensure!(
                rows.iter().all(|v| v.quantity > 0
                    && self.items.contains_key(&v.item)
                    && seen.insert(&v.item)),
                "Invalid/duplicate material reference"
            );
            Ok(())
        };
        for (id, item) in &self.items {
            ensure!(
                asset_id(id)
                    && valid_name(&item.name)
                    && asset_id(&item.icon)
                    && item.description.as_ref().is_none_or(|s| s.len() <= 8192),
                "Invalid item metadata"
            );
        }
        for (id, b) in &self.buildings {
            ensure!(
                asset_id(id)
                    && valid_name(&b.name)
                    && asset_id(&b.icon)
                    && b.category != 8
                    && self
                        .context
                        .buildings
                        .iter()
                        .any(|v| v.id == *id && v.category == b.category),
                "Invalid/unrooted building"
            );
            bill(&b.materials)?;
            ensure!(
                (!b.unlocks.is_empty() || b.core_progression) && b.unlocks.len() <= 64,
                "Missing/excessive building unlock routes"
            );
            for u in &b.unlocks {
                match u {
                    BuildingUnlock::Upgrade {
                        materials,
                        tasks,
                        offers,
                        ..
                    } => {
                        bill(materials)?;
                        ensure!(tasks.len() <= 128 && offers.len() <= 64, "Upgrade route bound");
                    }
                    BuildingUnlock::Corporation {
                        id,
                        name,
                        ..
                    } => ensure!(asset_id(id) && valid_name(name), "Invalid corporation route"),
                    BuildingUnlock::Tutorial {
                        flow,
                        objective,
                        condition,
                    } => ensure!(
                        asset_id(flow)
                            && !objective.is_empty()
                            && objective.len() <= 256
                            && condition.len() <= 256,
                        "Invalid tutorial route"
                    ),
                }
            }
            if let Some(c) = &b.crafting {
                ensure!(
                    c.loop_seconds.is_finite()
                        && c.loop_seconds >= 0.0
                        && c.overwrite_seconds.is_finite()
                        && c.overwrite_seconds >= -1.0,
                    "Invalid crafting defaults"
                );
            }
        }
        for (id, r) in &self.recipes {
            ensure!(
                asset_id(id)
                    && valid_name(&r.name)
                    && asset_id(&r.icon)
                    && self.items.contains_key(&r.output.item)
                    && r.output.quantity > 0
                    && r.base_seconds.is_finite()
                    && (0.0..=86400.0).contains(&r.base_seconds)
                    && !r.producers.is_empty()
                    && r.producers.len() <= 64
                    && r.producers.iter().all(|b| self.buildings.contains_key(b)),
                "Invalid recipe/reference"
            );
            bill(&r.inputs)?;
            match &r.unlock {
                Unlock::Research {
                    materials,
                } => {
                    bill(materials)?;
                }
                Unlock::FirstWeaponPurchase {
                    weapons,
                } => ensure!(
                    !weapons.is_empty()
                        && weapons.len() <= 128
                        && weapons.iter().all(|w| asset_id(&w.id)
                            && valid_name(&w.name)
                            && self
                                .context
                                .weapon_classes
                                .iter()
                                .any(|c| c.split('.').next() == Some(w.id.as_str()))),
                    "Unregistered ammunition weapon"
                ),
                _ => {}
            }
        }
        for (i, tier) in self.core_tiers.iter().enumerate() {
            ensure!(
                tier.source_index as usize == i && tier.cooling > 0,
                "Invalid Core tier ordering/capacity"
            );
            bill(&tier.materials)?;
        }
        Ok(())
    }
}
fn valid_name(s: &str) -> bool {
    !s.is_empty() && s.len() <= 2048 && !s.contains('\0')
}
