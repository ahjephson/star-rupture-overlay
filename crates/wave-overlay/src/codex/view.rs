//! Joins authored catalogue rows to the current game's positive visibility evidence.
use starrupture_assets::codex::{Amount, BuildingUnlock, Catalogue, Unlock};
use starrupture_overlay_reader::codex::Progress;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Tab {
    Production,
    Buildings,
    Interiors,
    Equipment,
    Corporations,
}

impl Tab {
    pub const ALL: [Self; 5] =
        [Self::Production, Self::Buildings, Self::Interiors, Self::Equipment, Self::Corporations];

    pub fn label(self) -> &'static str {
        match self {
            Self::Production => "Production",
            Self::Buildings => "Buildings",
            Self::Interiors => "Interiors",
            Self::Equipment => "Equipment",
            Self::Corporations => "Corporations",
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(super) enum Entry {
    Recipe {
        id: String,
        machine: String,
        locked: bool,
    },
    Item(String),
    Building(String),
    Weapon(String),
    Corporation(String),
}

pub(super) struct Row {
    pub key: Entry,
    pub title: String,
    pub summary: String,
    pub icon: Option<String>,
    pub locked: bool,
}

const TRAINING: &str = "/Game/Chimera/Corporations/CD_StartingCorp";
const CORPORATE_TERMINAL_ORDER: [&str; 5] = [
    "FutureCorporation",
    "SelenianCorporation",
    "GriffithsCorporation",
    "MoonCorporation",
    "CleverCorporation",
];

fn building_tab(id: &str) -> Tab {
    if id.starts_with("/Game/Chimera/Buildings/Interiors/") {
        Tab::Interiors
    } else {
        Tab::Buildings
    }
}
fn owned(progress: &Progress, package: &str) -> bool {
    progress
        .owned_equipment
        .iter()
        .any(|class| class == package || class.split_once('.').is_some_and(|(id, _)| id == package))
}

fn material_name<'a>(c: &'a Catalogue, id: &str) -> &'a str {
    c.items
        .get(id)
        .map(|item| item.name.as_str())
        .or_else(|| c.equipment_material_names.get(id).and_then(|name| name.as_deref()))
        .unwrap_or("Unknown item")
}
fn amounts(c: &Catalogue, values: &[Amount]) -> String {
    values
        .iter()
        .map(|amount| format!("{} × {}", amount.quantity, material_name(c, &amount.item)))
        .collect::<Vec<_>>()
        .join("  ·  ")
}

impl Row {
    fn new(key: Entry, title: &str, summary: String, icon: Option<&str>, locked: bool) -> Self {
        Self {
            key,
            title: title.into(),
            summary,
            icon: icon.map(str::to_owned),
            locked,
        }
    }
}

pub(super) fn item_row(c: &Catalogue, id: &str) -> Option<Row> {
    let item = c.items.get(id)?;
    Some(Row::new(Entry::Item(id.into()), &item.name, String::new(), Some(&item.icon), false))
}

pub(super) fn rows(c: &Catalogue, p: &Progress, tab: Tab, query: &str) -> Vec<Row> {
    let mut result = Vec::new();
    match tab {
        Tab::Production => {
            for (id, recipe) in &c.recipes {
                let locked = match &recipe.unlock {
                    Unlock::FirstWeaponPurchase {
                        weapons,
                    } => {
                        if !weapons.iter().any(|weapon| owned(p, &weapon.id))
                            || !p.recipes.contains(id)
                        {
                            continue;
                        }
                        false
                    }
                    Unlock::Research {
                        ..
                    } => {
                        if p.research.contains(id) {
                            true
                        } else if p.recipes.contains(id) {
                            false
                        } else {
                            continue;
                        }
                    }
                    Unlock::Building | Unlock::Default => {
                        if !p.recipes.contains(id) {
                            continue;
                        }
                        false
                    }
                };
                for machine in &recipe.producers {
                    if !p.buildings.contains(machine) {
                        continue;
                    }
                    let Some(building) = c.buildings.get(machine) else {
                        continue;
                    };
                    let inputs = amounts(c, &recipe.inputs);
                    result.push(Row::new(
                        Entry::Recipe {
                            id: id.clone(),
                            machine: machine.clone(),
                            locked,
                        },
                        &recipe.name,
                        format!("{}  ·  ×{}  ·  {}", building.name, recipe.output.quantity, inputs),
                        Some(&recipe.icon),
                        locked,
                    ));
                }
            }
        }
        Tab::Buildings | Tab::Interiors => {
            for (id, building) in &c.buildings {
                if building_tab(id) != tab {
                    continue;
                }
                let locked = !p.buildings.contains(id);
                let summary = match building.category {
                    0 => "Extractor",
                    1 => "Production machine",
                    2 => "Power",
                    3 => "Structure",
                    4 => "Interior",
                    5 => "Base infrastructure",
                    6 => "Logistics",
                    _ => "Building",
                };
                result.push(Row::new(
                    Entry::Building(id.clone()),
                    &building.name,
                    summary.into(),
                    Some(&building.icon),
                    locked,
                ));
            }
        }
        Tab::Equipment => {
            if p.buildings.contains("/Game/Chimera/Buildings/Interiors/Armory/BD_Armory") {
                for (id, weapon) in c.context.weapon_classes.iter().filter_map(|class| {
                    let (id, _) = class.split_once('.')?;
                    c.weapon_purchases.get_key_value(id)
                }) {
                    let locked = !owned(p, id);
                    let summary = if locked {
                        format!("{} War Bonds", weapon.war_bonds)
                    } else {
                        "Owned".into()
                    };
                    result.push(Row::new(
                        Entry::Weapon(id.clone()),
                        &weapon.name,
                        summary,
                        weapon.icon.as_deref(),
                        locked,
                    ));
                }
            }
        }
        Tab::Corporations => {
            for (id, corporation) in &c.corporations {
                if id == TRAINING {
                    continue;
                }
                let Some(level) = p.corporations.get(&corporation.native_name) else {
                    continue;
                };
                result.push(Row::new(
                    Entry::Corporation(id.clone()),
                    &corporation.name,
                    format!("Level {level}"),
                    corporation.icon.as_deref(),
                    false,
                ));
            }
        }
    }
    let terms: Vec<_> = query.split_whitespace().map(str::to_lowercase).collect();
    result.retain(|row| {
        let searchable = format!("{} {}", row.title, row.summary).to_lowercase();
        terms.iter().all(|term| searchable.contains(term))
    });
    if tab == Tab::Corporations {
        result.sort_by_key(|row| {
            let Entry::Corporation(id) = &row.key else {
                return CORPORATE_TERMINAL_ORDER.len();
            };
            CORPORATE_TERMINAL_ORDER
                .iter()
                .position(|name| *name == c.corporations[id].native_name)
                .unwrap_or(CORPORATE_TERMINAL_ORDER.len())
        });
    } else if tab != Tab::Equipment {
        result.sort_by(|a, b| {
            a.locked
                .cmp(&b.locked)
                .then_with(|| a.title.cmp(&b.title))
                .then_with(|| a.summary.cmp(&b.summary))
        });
    }
    result
}

pub(super) struct Tile {
    pub title: String,
    pub status: String,
    pub icon: Option<String>,
    pub locked: bool,
    pub materials: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LevelStatus {
    Unlocked,
    Next,
    Locked,
}

pub(super) struct Detail {
    pub section: bool,
    pub produced: bool,
    pub warm_icon: bool,
    pub docked: bool,
    pub tiles: Vec<Tile>,
    pub heading: String,
    pub body: String,
    pub icon: Option<String>,
    pub locked: bool,
    pub pin: Option<String>,
    pub link: Option<(Tab, Entry)>,
    pub corporation_level: Option<u32>,
    pub level_status: Option<LevelStatus>,
}

pub(super) fn details(
    c: &Catalogue,
    p: &Progress,
    row: &Row,
    target_level: Option<u32>,
) -> Vec<Detail> {
    let mut details = Vec::new();
    fn add(details: &mut Vec<Detail>, heading: &str, body: String, icon: Option<&str>) {
        if !body.is_empty() {
            details.push(Detail {
                section: false,
                produced: false,
                warm_icon: false,
                docked: false,
                tiles: Vec::new(),
                heading: heading.into(),
                body,
                icon: icon.map(str::to_owned),
                locked: false,
                pin: None,
                link: None,
                corporation_level: None,
                level_status: None,
            });
        }
    }

    fn section<'a>(details: &'a mut Vec<Detail>, heading: &str) -> &'a mut Detail {
        details.push(Detail {
            section: true,
            produced: false,
            warm_icon: false,
            docked: false,
            tiles: Vec::new(),
            heading: heading.into(),
            body: String::new(),
            icon: None,
            locked: false,
            pin: None,
            link: None,
            corporation_level: None,
            level_status: None,
        });
        details.last_mut().expect("section just added")
    }

    let add_corporation_requirement =
        |details: &mut Vec<Detail>, heading: &str, id: &str, name: &str, level: u32| {
            if id == TRAINING {
                add(details, "Training", format!("Complete training milestone {level}"), None);
                return;
            }
            let corporation = c.corporations.get(id);
            add(
                details,
                heading,
                format!("{name} · Level {level}"),
                corporation.and_then(|corp| corp.icon.as_deref()),
            );
            if corporation.is_some_and(|corp| p.corporations.contains_key(&corp.native_name)) {
                let detail = details.last_mut().expect("requirement just added");
                detail.link = Some((Tab::Corporations, Entry::Corporation(id.into())));
                detail.corporation_level = Some(level);
            }
        };
    let add_materials =
        |details: &mut Vec<Detail>, heading: &str, materials: &[Amount], link_items: bool| {
            for amount in materials {
                add(
                    details,
                    heading,
                    format!("{} × {}", amount.quantity, material_name(c, &amount.item)),
                    c.items.get(&amount.item).map(|item| item.icon.as_str()),
                );
                let detail = details.last_mut().unwrap();
                detail.warm_icon = true;
                if link_items {
                    detail.link = Some((Tab::Production, Entry::Item(amount.item.clone())));
                }
            }
        };
    match &row.key {
        Entry::Recipe {
            id,
            machine,
            locked,
        } => {
            let recipe = &c.recipes[id];
            let building = &c.buildings[machine];
            let cycle = if recipe.base_seconds > 0.0 {
                format!("{} s base cycle", recipe.base_seconds)
            } else {
                "Base cycle unavailable".into()
            };
            if let Some(description) = &c.items[&recipe.output.item].description {
                add(&mut details, "Description", description.clone(), None);
            }
            if !recipe.inputs.is_empty() {
                section(&mut details, "Production inputs");
                add_materials(&mut details, "", &recipe.inputs, true);
            }
            if *locked
                && let Unlock::Research {
                    materials,
                } = &recipe.unlock
            {
                section(&mut details, "Unlock at Recipe Station");
                for amount in materials {
                    let item = &c.items[&amount.item];
                    add(
                        &mut details,
                        "",
                        format!("{} × {}", amount.quantity, item.name),
                        Some(&item.icon),
                    );
                }
            }
            add(
                &mut details,
                "Produced in",
                format!("{}  ·  ×{}  ·  {cycle}", building.name, recipe.output.quantity),
                Some(&building.icon),
            );
            let machine_row = details.last_mut().unwrap();
            machine_row.docked = true;
            machine_row.link = Some((building_tab(machine), Entry::Building(machine.clone())));
        }
        Entry::Item(id) => {
            let item = &c.items[id];
            if let Some(description) = &item.description {
                add(&mut details, "Description", description.clone(), None);
            }
            section(&mut details, "Production recipes");
            let section_index = details.len();
            for production in rows(c, p, Tab::Production, "") {
                if let Entry::Recipe {
                    id: recipe,
                    ..
                } = &production.key
                    && c.recipes[recipe].output.item == *id
                {
                    add(&mut details, "", production.summary, production.icon.as_deref());
                    let detail = details.last_mut().unwrap();
                    detail.locked = production.locked;
                    detail.warm_icon = true;
                    detail.link = Some((Tab::Production, production.key));
                }
            }
            if details.len() == section_index {
                add(&mut details, "", "No production recipes are visible.".into(), None);
            }
        }
        Entry::Building(id) => {
            let building = &c.buildings[id];
            if !building.materials.is_empty() || building.add_platform || building.add_pillar {
                section(&mut details, "Construction");
                add_materials(&mut details, "", &building.materials, false);
            }
            if building.add_platform || building.add_pillar {
                add(
                    &mut details,
                    "Placement",
                    "Additional foundation costs may apply.".into(),
                    None,
                );
            }
            if !building.unlocks.is_empty() {
                section(&mut details, "Unlock requirements");
            }
            for unlock in &building.unlocks {
                match unlock {
                    BuildingUnlock::Corporation {
                        id,
                        name,
                        level,
                        ..
                    } => {
                        add_corporation_requirement(
                            &mut details,
                            "Corporation unlock",
                            id,
                            name,
                            *level,
                        );
                    }
                    BuildingUnlock::Upgrade {
                        offers,
                        tasks,
                        materials,
                        ..
                    } => {
                        for offer in offers {
                            add_corporation_requirement(
                                &mut details,
                                "Upgrade unlock",
                                &offer.id,
                                &offer.name,
                                offer.level,
                            );
                        }
                        add_materials(&mut details, "Upgrade materials", materials, false);
                        for task in tasks {
                            add(&mut details, "Upgrade requirement", task.text.clone(), None);
                        }
                    }
                    BuildingUnlock::Tutorial {
                        ..
                    } => {
                        add(
                            &mut details,
                            "Unlock",
                            "Complete the corresponding tutorial objective.".into(),
                            None,
                        );
                    }
                }
            }
            if building.core_progression {
                section(&mut details, "Base Core levels");
                let highest = super::pins::highest_core_tier(c, p);
                for tier in &c.core_tiers {
                    let index = tier.source_index;
                    let status = match highest {
                        _ if index == 0 => "Base level",
                        Some(highest) if index <= highest => "Unlocked",
                        Some(highest) if index == highest + 1 => "Next unlock",
                        Some(_) => "Locked",
                        None => "Unlock status unavailable",
                    };
                    section(&mut details, &format!("Level {} · {status}", index + 1));
                    add(
                        &mut details,
                        "",
                        format!("{} cooling capacity", tier.cooling),
                        Some(&building.icon),
                    );
                    details.last_mut().unwrap().locked = status == "Locked";
                    if index > 0
                        && highest.is_some_and(|level| index <= level + 1)
                        && !tier.materials.is_empty()
                    {
                        details.last_mut().unwrap().pin = Some(format!("core-tier:{index}"));
                        add_materials(&mut details, "", &tier.materials, true);
                    }
                }
            }
            if !row.locked
                && let Some(crafting) = &building.crafting
            {
                let first = details.len();
                section(&mut details, "Produced items");
                details[first].produced = true;
                let mut produced = rows(c, p, Tab::Production, "")
                    .into_iter()
                    .filter_map(|row| match &row.key {
                        Entry::Recipe {
                            id: recipe,
                            machine,
                            ..
                        } if machine == id => Some((recipe.clone(), row)),
                        _ => None,
                    })
                    .collect::<std::collections::BTreeMap<_, _>>();
                for production in crafting.recipe_order.iter().filter_map(|id| produced.remove(id))
                {
                    add(&mut details, "", production.title, production.icon.as_deref());
                    let detail = details.last_mut().unwrap();
                    detail.locked = production.locked;
                    detail.produced = true;
                    detail.warm_icon = true;
                    detail.link = Some((Tab::Production, production.key));
                }
                if details.len() == first + 1 {
                    details.pop();
                }
            }
        }
        Entry::Weapon(id) => {
            let weapon = &c.weapon_purchases[id];
            if let Some(description) = &weapon.description {
                add(&mut details, "Description", description.clone(), None);
            }
            if row.locked {
                add(
                    &mut details,
                    "First purchase",
                    format!("{} War Bonds at the Equipment Upgrade Station", weapon.war_bonds),
                    None,
                );
            } else {
                let ammo = &c.items[&weapon.ammo];
                section(&mut details, "Ammunition");
                add(&mut details, "", ammo.name.clone(), Some(&ammo.icon));
                if !weapon.equip_materials.is_empty() {
                    section(&mut details, "Weapon equip materials");
                    add_materials(&mut details, "", &weapon.equip_materials, false);
                }
                let mut slots = std::collections::BTreeMap::<_, Vec<Tile>>::new();
                let mut upgrades: Vec<_> = weapon
                    .mods
                    .iter()
                    .filter(|(_, m)| {
                        !m.hidden_slot && (m.registered_for_unlock || m.default_for_weapon)
                    })
                    .collect();
                upgrades.sort_by_key(|(id, upgrade)| {
                    (
                        upgrade.sort_priority,
                        !upgrade.default_for_weapon,
                        c.context.weapon_mod_assets.iter().position(|source| source == *id),
                    )
                });
                for (_, upgrade) in upgrades {
                    let purchased = upgrade.default_for_weapon
                        || upgrade.provider.as_ref().is_some_and(|provider| owned(p, provider));
                    let status = if upgrade.default_for_weapon {
                        "Standard".into()
                    } else if purchased {
                        "Owned".into()
                    } else {
                        format!("{} War Bonds", upgrade.war_bonds)
                    };
                    slots.entry(upgrade.slot.as_str()).or_default().push(Tile {
                        title: upgrade.name.clone(),
                        status,
                        locked: !purchased,
                        icon: upgrade.icon.clone(),
                        materials: amounts(c, &upgrade.equip_materials),
                    });
                }
                let mut slots: Vec<_> = slots.into_iter().collect();
                slots.sort_by_key(|(slot, _)| {
                    weapon.slot_order.iter().position(|value| value == slot).unwrap_or(usize::MAX)
                });
                for (slot, tiles) in slots {
                    let title = match slot.rsplit('.').next().unwrap_or(slot) {
                        "Front" => "Barrel",
                        "Mag" => "Magazine",
                        other => other,
                    };
                    section(&mut details, title);
                    let mut tiles = tiles.into_iter();
                    loop {
                        let row: Vec<_> = tiles.by_ref().take(3).collect();
                        if row.is_empty() {
                            break;
                        }
                        details.push(Detail {
                            section: false,
                            produced: false,
                            warm_icon: false,
                            docked: false,
                            tiles: row,
                            heading: String::new(),
                            body: String::new(),
                            icon: None,
                            locked: false,
                            pin: None,
                            link: None,
                            corporation_level: None,
                            level_status: None,
                        });
                    }
                }
            }
        }
        Entry::Corporation(id) => {
            let corporation = &c.corporations[id];
            let Some(&current) = p.corporations.get(&corporation.native_name) else {
                return details;
            };
            if let Some(description) = &corporation.description {
                add(&mut details, "Description", description.clone(), None);
            }
            for level in corporation.levels.iter().filter(|r| {
                r.level > 1
                    && (r.level <= current.saturating_add(2) || Some(r.level) == target_level)
            }) {
                let status = if level.level <= current {
                    LevelStatus::Unlocked
                } else if level.level == current.saturating_add(1) {
                    LevelStatus::Next
                } else {
                    LevelStatus::Locked
                };
                section(&mut details, &format!("Level {}", level.level)).level_status =
                    Some(status);
                add(
                    &mut details,
                    "",
                    format!(
                        "{} reputation",
                        if level.reputation > 10_000 {
                            format!("{:.1}K", f64::from(level.reputation) / 1000.0)
                        } else {
                            level.reputation.to_string()
                        }
                    ),
                    None,
                );
                for (key, building) in &c.buildings {
                    if building.unlocks.iter().any(|rule| match rule {
                        BuildingUnlock::Corporation {
                            id: who,
                            level: at,
                            ..
                        } => who == id && *at == level.level,
                        BuildingUnlock::Upgrade {
                            offers,
                            ..
                        } => offers.iter().any(|o| o.id == *id && o.level == level.level),
                        BuildingUnlock::Tutorial {
                            ..
                        } => false,
                    }) {
                        let heading = if p.buildings.contains(key) {
                            "Unlocked building"
                        } else {
                            "Building reward"
                        };
                        add(&mut details, heading, building.name.clone(), Some(&building.icon));
                        let detail = details.last_mut().unwrap();
                        detail.locked = !p.buildings.contains(key);
                        detail.link = Some((building_tab(key), Entry::Building(key.clone())));
                    }
                }
                for amount in &level.items {
                    let item = &c.items[&amount.item];
                    add(
                        &mut details,
                        "Item reward",
                        format!("{} × {}", amount.quantity, item.name),
                        Some(&item.icon),
                    );
                }
                for feature in &level.features {
                    add(&mut details, "Feature reward", feature.clone(), None);
                }
                if level.inventory_slots != 0 {
                    add(
                        &mut details,
                        "Inventory reward",
                        format!("+{} slots", level.inventory_slots),
                        None,
                    );
                }
            }
        }
    }
    details
}

#[cfg(test)]
mod tests {
    use super::*;
    use starrupture_assets::codex::{
        Building, Item, NativeContext, Recipe, WeaponMod, WeaponPurchase,
    };
    use starrupture_observation_model::codex::Defaults;
    use std::collections::{BTreeMap, BTreeSet};

    fn fixture() -> (Catalogue, Progress) {
        let recipe = |machine: &str, unlock| Recipe {
            name: "Heat Resistant Sheet".into(),
            output: Amount {
                item: "sheet".into(),
                quantity: 4,
            },
            inputs: vec![Amount {
                item: "sheet".into(),
                quantity: 1,
            }],
            producers: vec![machine.into()],
            base_seconds: 4.0,
            icon: String::new(),
            unlock,
        };
        let building = |name: &str| Building {
            name: name.into(),
            icon: String::new(),
            category: 0,
            materials: vec![],
            add_platform: false,
            add_pillar: false,
            unlocks: vec![],
            core_progression: false,
            crafting: None,
        };
        let c = Catalogue {
            version: 8,
            lock_icon: None,
            context: NativeContext {
                version: 1,
                game_hash: String::new(),
                mode_class: String::new(),
                state_class: String::new(),
                keeper_class: String::new(),
                buildings: vec![],
                weapon_classes: vec![],
                weapon_mod_assets: vec![],
                core_asset: String::new(),
                defaults: Defaults {
                    add_platform: false,
                    add_pillar: false,
                    recipe_seconds: 4.0,
                    crafting_loop_seconds: 0.0,
                    overwrite_recipe_seconds: 0.0,
                    use_crafting_processor: false,
                },
            },
            items: BTreeMap::from([(
                "sheet".into(),
                Item {
                    name: "Sheet".into(),
                    description: Some("A sample item".into()),
                    icon: String::new(),
                },
            )]),
            buildings: BTreeMap::from([
                ("v1".into(), building("Furnace")),
                ("v2".into(), building("Furnace v2")),
            ]),
            recipes: BTreeMap::from([
                (
                    "base".into(),
                    recipe(
                        "v1",
                        Unlock::Research {
                            materials: vec![],
                        },
                    ),
                ),
                ("faster".into(), recipe("v2", Unlock::Building)),
                ("alternative".into(), recipe("v2", Unlock::Building)),
                ("hidden".into(), recipe("v2", Unlock::Default)),
            ]),
            core_tiers: vec![],
            corporations: BTreeMap::new(),
            weapon_purchases: BTreeMap::new(),
            war_bond_currency: None,
            equipment_material_names: BTreeMap::new(),
            excluded: BTreeMap::new(),
        };
        let progress = Progress {
            buildings: BTreeSet::from(["v1".into()]),
            recipes: BTreeSet::from(["faster".into(), "alternative".into()]),
            research: BTreeSet::from(["base".into()]),
            owned_equipment: BTreeSet::new(),
            corporations: BTreeMap::new(),
            cores: Vec::new(),
            highest_unlocked_core_tier: None,
        };
        (c, progress)
    }

    #[test]
    fn corporation_level_headings_follow_live_progress() {
        use starrupture_assets::codex::{Corporation, CorporationLevel};

        let (mut catalogue, mut progress) = fixture();
        let id = "example";
        let native_name = "Example";
        catalogue.corporations.insert(
            id.into(),
            Corporation {
                name: "Training Corporation".into(),
                native_name: native_name.into(),
                icon: None,
                description: None,
                levels: (1..=6)
                    .map(|level| CorporationLevel {
                        level,
                        reputation: level * 1000,
                        items: Vec::new(),
                        inventory_slots: 0,
                        features: Vec::new(),
                    })
                    .collect(),
            },
        );
        progress.corporations.insert(native_name.into(), 2);
        let row = rows(&catalogue, &progress, Tab::Corporations, "").remove(0);
        let headings = |progress: &Progress| {
            details(&catalogue, progress, &row, Some(6))
                .into_iter()
                .filter(|detail| detail.section)
                .map(|detail| (detail.heading, detail.level_status))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            headings(&progress),
            [
                ("Level 2".into(), Some(LevelStatus::Unlocked)),
                ("Level 3".into(), Some(LevelStatus::Next)),
                ("Level 4".into(), Some(LevelStatus::Locked)),
                ("Level 6".into(), Some(LevelStatus::Locked)),
            ]
        );

        progress.corporations.insert(native_name.into(), 3);
        assert_eq!(
            headings(&progress)[1..3],
            [
                ("Level 3".into(), Some(LevelStatus::Unlocked)),
                ("Level 4".into(), Some(LevelStatus::Next)),
            ]
        );

        progress.corporations.insert(native_name.into(), 6);
        assert_eq!(headings(&progress).len(), 5);
        assert!(
            headings(&progress).iter().all(|(_, status)| *status == Some(LevelStatus::Unlocked))
        );

        progress.corporations.clear();
        assert!(headings(&progress).is_empty());
    }

    #[test]
    fn base_core_shows_one_building_and_gates_shared_level_costs() {
        use starrupture_assets::codex::CoreTier;
        use starrupture_overlay_reader::codex::Core;

        let (mut catalogue, mut progress) = fixture();
        let building = catalogue.buildings.get_mut("v1").unwrap();
        building.name = "Base Core".into();
        building.core_progression = true;
        catalogue.core_tiers = [1000, 2500, 4000, 6000]
            .into_iter()
            .enumerate()
            .map(|(index, cooling)| CoreTier {
                source_index: index as u32,
                materials: vec![Amount {
                    item: "sheet".into(),
                    quantity: index as u32 * 5,
                }],
                cooling,
            })
            .collect();
        progress.cores = [12, 24, 36]
            .map(|id| Core {
                id,
                tier_index: u32::from(id == 36),
                capacity: if id == 36 {
                    2500
                } else {
                    1000
                },
                submitted: None,
            })
            .to_vec();
        progress.highest_unlocked_core_tier = Some(1);

        let rows = rows(&catalogue, &progress, Tab::Buildings, "base core");
        assert_eq!(rows.len(), 1);
        let details = details(&catalogue, &progress, &rows[0], None);
        assert_eq!(
            details
                .iter()
                .filter(|row| row.section)
                .map(|row| row.heading.as_str())
                .collect::<Vec<_>>(),
            [
                "Base Core levels",
                "Level 1 · Base level",
                "Level 2 · Unlocked",
                "Level 3 · Next unlock",
                "Level 4 · Locked"
            ]
        );
        assert!(!details.iter().any(|row| row.heading.starts_with("Placed core")));
        assert_eq!(details.iter().filter(|row| row.body == "10 × Sheet").count(), 1);
        assert!(details.iter().filter(|row| matches!(row.body.as_str(), "5 × Sheet" | "10 × Sheet"))
            .all(|row| matches!(&row.link, Some((Tab::Production, Entry::Item(id))) if id == "sheet")));
        assert!(details.iter().any(|row| row.pin.as_deref() == Some("core-tier:1")));
        assert!(!details.iter().any(|row| row.body == "15 × Sheet"));
        assert_eq!(
            super::super::pins::card(&catalogue, &progress, "core:12:1".into()).unwrap().locked,
            Some(false)
        );
        assert_eq!(
            super::super::pins::card(&catalogue, &progress, "core:36:2".into()).unwrap().locked,
            Some(true)
        );
    }

    #[test]
    fn exact_recipe_variants_respect_building_version_and_research() {
        let (c, mut progress) = fixture();
        let search = |p: &Progress| rows(&c, p, Tab::Production, "sheet furnace");
        let initial = search(&progress);
        assert_eq!(initial.len(), 1);
        assert!(initial[0].locked);
        progress.buildings.insert("v2".into());
        let revealed = search(&progress);
        assert_eq!(revealed.len(), 3);
        assert_eq!(revealed.iter().filter(|row| row.locked).count(), 1);
        assert!(
            revealed
                .iter()
                .any(|row| matches!(&row.key, Entry::Recipe { id, .. } if id == "faster"))
        );
        assert!(
            revealed
                .iter()
                .any(|row| matches!(&row.key, Entry::Recipe { id, .. } if id == "alternative"))
        );
    }

    #[test]
    fn production_scrolls_description_and_inputs_above_the_linked_machine() {
        let (catalogue, progress) = fixture();
        let row = rows(&catalogue, &progress, Tab::Production, "").remove(0);
        let view = details(&catalogue, &progress, &row, None);
        assert_eq!(view.first().unwrap().heading, "Description");
        assert_eq!(view[1].heading, "Production inputs");
        assert!(matches!(&view[2].link, Some((Tab::Production, Entry::Item(id))) if id == "sheet"));
        let machine = view.last().unwrap();
        assert!(machine.docked);
        assert!(matches!(&machine.link, Some((Tab::Buildings, Entry::Building(id))) if id == "v1"));
    }

    #[test]
    fn linked_item_shows_each_visible_recipe_and_works_without_any() {
        let (catalogue, mut progress) = fixture();
        let item = item_row(&catalogue, "sheet").unwrap();
        progress.buildings.insert("v2".into());
        let links = details(&catalogue, &progress, &item, None);
        assert_eq!(links.iter().filter(|detail| detail.link.is_some()).count(), 3);
        assert_eq!(links.iter().filter(|detail| detail.locked).count(), 1);
        progress.recipes.clear();
        progress.research.clear();
        let no_recipes = details(&catalogue, &progress, &item, None);
        assert_eq!(no_recipes[0].body, "A sample item");
        assert_eq!(no_recipes[2].body, "No production recipes are visible.");
    }

    #[test]
    fn produced_item_rows_keep_authored_order_even_when_unlocked_out_of_order() {
        let (mut catalogue, mut progress) = fixture();
        let later = catalogue.recipes.get_mut("faster").unwrap();
        later.producers = vec!["v1".into()];
        later.name = "A later recipe".into();
        catalogue.buildings.get_mut("v1").unwrap().crafting =
            Some(starrupture_assets::codex::Crafting {
                collection: None,
                recipe_order: vec!["base".into(), "faster".into()],
                loop_seconds: 4.0,
                overwrite_seconds: 0.0,
                processor: true,
            });
        let building = rows(&catalogue, &progress, Tab::Buildings, "Furnace").remove(0);
        let entries = |progress: &Progress| {
            details(&catalogue, progress, &building, None)
                .into_iter()
                .filter(|row| row.produced && !row.section)
                .map(|row| {
                    assert!(row.heading.is_empty());
                    let Some((
                        Tab::Production,
                        Entry::Recipe {
                            id,
                            ..
                        },
                    )) = row.link
                    else {
                        panic!("Produced item missing its recipe link");
                    };
                    (id, row.locked)
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(entries(&progress), [("base".into(), true), ("faster".into(), false)]);
        progress.research.remove("base");
        progress.recipes.insert("base".into());
        assert_eq!(entries(&progress), [("base".into(), false), ("faster".into(), false)]);
    }

    #[test]
    fn corporations_follow_terminal_order_and_training_has_no_corporation_link() {
        use starrupture_assets::codex::Corporation;

        let (mut catalogue, mut progress) = fixture();
        for (id, native_name) in [
            ("future", "FutureCorporation"),
            ("clever", "CleverCorporation"),
            (TRAINING, "StartingCorporation"),
        ] {
            catalogue.corporations.insert(
                id.into(),
                Corporation {
                    name: id.into(),
                    native_name: native_name.into(),
                    icon: None,
                    description: None,
                    levels: Vec::new(),
                },
            );
            progress.corporations.insert(native_name.into(), 1);
        }
        let listed = rows(&catalogue, &progress, Tab::Corporations, "");
        assert_eq!(
            listed.iter().map(|row| row.title.as_str()).collect::<Vec<_>>(),
            ["future", "clever"]
        );
        let building = catalogue.buildings.get_mut("v1").unwrap();
        building.unlocks = [TRAINING, "future"]
            .into_iter()
            .map(|id| BuildingUnlock::Corporation {
                id: id.into(),
                name: id.into(),
                level: 2,
                collection: None,
            })
            .collect();
        let building = rows(&catalogue, &progress, Tab::Buildings, "Furnace").remove(0);
        let view = details(&catalogue, &progress, &building, None);
        let training = view.iter().find(|row| row.heading == "Training").unwrap();
        assert_eq!(training.body, "Complete training milestone 2");
        assert!(training.link.is_none());
        let corporation = view.iter().find(|row| row.heading == "Corporation unlock").unwrap();
        assert!(
            matches!(&corporation.link, Some((Tab::Corporations, Entry::Corporation(id))) if id == "future")
        );
        assert_eq!(corporation.corporation_level, Some(2));
    }

    #[test]
    fn extracted_raw_material_cannot_be_pinned() {
        let (mut catalogue, progress) = fixture();
        catalogue.recipes.get_mut("base").unwrap().inputs.clear();
        let entry = |id: &str| Entry::Recipe {
            id: id.into(),
            machine: "v1".into(),
            locked: false,
        };
        assert!(super::super::pins::key(&catalogue, &entry("base")).is_none());
        assert!(super::super::pins::card(&catalogue, &progress, "base".into()).is_none());
        assert!(super::super::pins::key(&catalogue, &entry("faster")).is_some());
    }

    #[test]
    fn equipment_materials_outside_the_item_map_do_not_crash_selection() {
        let (mut c, mut p) = fixture();
        c.equipment_material_names.insert("gear".into(), Some("Gear".into()));
        let materials = vec![Amount {
            item: "gear".into(),
            quantity: 2,
        }];
        c.weapon_purchases.insert(
            "weapon".into(),
            WeaponPurchase {
                icon: None,
                description: None,
                name: "Test weapon".into(),
                ammo: "sheet".into(),
                war_bonds: 0,
                equip_materials: materials.clone(),
                slot_order: vec!["Weapon.Mag".into()],
                mods: BTreeMap::from([(
                    "mod".into(),
                    WeaponMod {
                        icon: Some("icon".into()),
                        name: "Magazine".into(),
                        slot: "Weapon.Mag".into(),
                        sort_priority: 0,
                        war_bonds: 20,
                        equip_materials: materials,
                        default_for_weapon: false,
                        registered_for_unlock: true,
                        hidden_slot: false,
                        provider: None,
                    },
                )]),
            },
        );
        p.buildings.insert("/Game/Chimera/Buildings/Interiors/Armory/BD_Armory".into());
        c.context.weapon_classes.push("weapon.Weapon_C".into());
        p.owned_equipment.insert("weapon".into());
        let row = rows(&c, &p, Tab::Equipment, "").remove(0);
        let view = details(&c, &p, &row, None);
        assert!(view.iter().any(|row| row.body == "2 × Gear"));
        assert!(view.iter().any(|row| row.section && row.heading == "Magazine"));
        let tile = view.iter().flat_map(|row| &row.tiles).next().unwrap();
        assert_eq!(tile.icon.as_deref(), Some("icon"));
        assert_eq!(tile.materials, "2 × Gear");
        assert!(tile.locked);
    }
}
