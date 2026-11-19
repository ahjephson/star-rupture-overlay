use starrupture_assets::codex::{Amount, BuildingUnlock, Catalogue, Unlock};
use starrupture_overlay_reader::codex::Progress;

pub(crate) struct Card {
    pub key: String,
    pub name: String,
    pub icon: String,
    pub locked: Option<bool>,
    pub materials: Vec<Material>,
}

pub(crate) struct Material {
    pub amount: Amount,
    pub submitted: Option<u32>,
}

pub(super) fn key(c: &Catalogue, entry: &super::view::Entry) -> Option<String> {
    use super::view::Entry;
    match entry {
        Entry::Recipe {
            id,
            ..
        } => Some(id.clone()),
        Entry::Building(id) => c
            .buildings
            .get(id)?
            .unlocks
            .iter()
            .any(|unlock| matches!(unlock, BuildingUnlock::Upgrade { .. }))
            .then(|| format!("upgrade:{id}")),
        _ => None,
    }
}

pub(super) fn highest_core_tier(c: &Catalogue, p: &Progress) -> Option<u32> {
    p.highest_unlocked_core_tier.filter(|highest| (*highest as usize) < c.core_tiers.len())
}

pub(crate) fn card(c: &Catalogue, p: &Progress, key: String) -> Option<Card> {
    if let Some(id) = key.strip_prefix("upgrade:") {
        let building = c.buildings.get(id)?;
        let materials = building.unlocks.iter().find_map(|unlock| match unlock {
            BuildingUnlock::Upgrade {
                materials,
                ..
            } => Some(materials.clone()),
            _ => None,
        })?;
        return Some(Card {
            locked: Some(!p.buildings.contains(id)),
            name: format!("{} upgrade", building.name),
            icon: building.icon.clone(),
            key,
            materials: unobserved(materials),
        });
    }
    if let Some(tier) = key.strip_prefix("core-tier:") {
        let tier = tier.parse::<u32>().ok()?;
        let highest = highest_core_tier(c, p);
        let target = c.core_tiers.iter().find(|entry| entry.source_index == tier)?;
        let building = c.buildings.values().find(|entry| entry.core_progression)?;
        return Some(Card {
            name: format!("Base Core · Level {}", tier + 1),
            icon: building.icon.clone(),
            key,
            locked: highest.map(|level| tier > level),
            materials: if highest.is_some_and(|level| tier <= level + 1) {
                unobserved(target.materials.clone())
            } else {
                Vec::new()
            },
        });
    }
    if let Some(core) = key.strip_prefix("core:") {
        let (id, tier) = core.split_once(':')?;
        let id = id.parse::<u32>().ok()?;
        let tier = tier.parse::<u32>().ok()?;
        let index = p.cores.iter().position(|core| core.id == id);
        let target = c.core_tiers.iter().find(|entry| entry.source_index == tier)?;
        let building = c.buildings.values().find(|entry| entry.core_progression)?;
        let core = index.map(|index| &p.cores[index]);
        let highest = highest_core_tier(c, p);
        let locked = highest.map(|highest| tier > highest);
        let material_visible = highest.is_some_and(|highest| tier <= highest + 1);
        let name = index.map_or_else(
            || format!("Core {id} · Level {}", tier + 1),
            |i| format!("Placed core {} · Level {}", i + 1, tier + 1),
        );
        return Some(Card {
            name,
            icon: building.icon.clone(),
            key,
            locked,
            materials: target
                .materials
                .iter()
                .filter(|_| material_visible)
                .cloned()
                .map(|amount| {
                    let submitted = core
                        .and_then(|core| core.submitted.as_ref())
                        .filter(|(selected, _)| *selected == tier)
                        .map(|(_, counts)| counts.get(&amount.item).copied().unwrap_or(0));
                    Material {
                        amount,
                        submitted,
                    }
                })
                .collect(),
        });
    }
    let recipe = c.recipes.get(&key)?;
    let locked = if p.research.contains(&key) {
        Some(true)
    } else if p.recipes.contains(&key) {
        Some(false)
    } else {
        None
    };
    let materials = match (&recipe.unlock, locked) {
        (
            Unlock::Research {
                materials,
            },
            Some(true) | None,
        ) => materials.clone(),
        _ => recipe.inputs.clone(),
    };
    Some(Card {
        name: recipe.name.clone(),
        icon: recipe.icon.clone(),
        key,
        locked,
        materials: unobserved(materials),
    })
}

fn unobserved(amounts: Vec<Amount>) -> Vec<Material> {
    amounts
        .into_iter()
        .map(|amount| Material {
            amount,
            submitted: None,
        })
        .collect()
}
