use super::{
    archive::{Archive, Export},
    model::*,
};
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use starrupture_observation_model::codex::{NativeContext, asset_id};
use std::collections::{BTreeMap, BTreeSet};
fn id(v: &Value) -> Result<String> {
    let s = v
        .as_str()
        .context("Native asset reference")?
        .split('.')
        .next()
        .context("Native package")?;
    ensure!(asset_id(s), "Catalogue asset namespace: {s}");
    Ok(s.into())
}
fn rows(v: &Value) -> Result<&[Value]> {
    if v.is_null() {
        Ok(&[])
    } else {
        let a = v.as_array().context("Native array")?;
        ensure!(a.len() <= 1024, "Native array bound");
        Ok(a)
    }
}
fn number(v: &Value) -> Result<u32> {
    Ok(u32::try_from(v.as_u64().context("Native nonnegative integer")?)?)
}
fn bill(v: &Value, item: &str, count: &str) -> Result<Vec<Amount>> {
    let a = rows(v)?;
    ensure!(a.len() <= 128, "Material bill bound");
    a.iter()
        .map(|r| {
            Ok(Amount {
                item: id(&r[item])?,
                quantity: number(&r[count])?,
            })
        })
        .collect()
}
fn float(v: &Value, default: f32) -> Result<f32> {
    if v.is_null() {
        return Ok(default);
    }
    let n = v.as_f64().context("Native float")? as f32;
    ensure!(n.is_finite(), "Finite native float");
    Ok(n)
}
fn flag(v: &Value, default: bool) -> Result<bool> {
    if v.is_null() {
        Ok(default)
    } else {
        v.as_bool().context("Native boolean")
    }
}
struct Text(BTreeMap<String, Value>);
impl Text {
    fn get(&self, v: &Value) -> Result<String> {
        for k in ["SourceString", "CultureInvariantString"] {
            if let Some(s) = v[k].as_str() {
                return Ok(s.into());
            }
        }
        let t = v["TableId"].as_str().with_context(|| format!("Text table missing in {v}"))?;
        let k = v["Key"].as_str().context("Text key")?;
        self.0
            .get(t)
            .and_then(|t| t[k].as_str())
            .map(str::to_owned)
            .with_context(|| format!("Unresolved text {t}:{k}"))
    }
    fn optional(&self, v: &Value) -> Result<Option<String>> {
        if v.is_null() || v.get("CultureInvariantString").is_some_and(Value::is_null) {
            Ok(None)
        } else {
            Ok(Some(self.get(v)?))
        }
    }
}
fn selected(
    archive: &mut Archive<'_>,
    package: &str,
    schema: &str,
    reference: &str,
) -> Result<Option<Export>> {
    let matches: Vec<_> = archive
        .exports(package, schema)?
        .into_iter()
        .filter(|e| {
            e.path == reference
                || (!reference.starts_with('/') && reference.rsplit('.').next() == Some(&e.name))
        })
        .collect();
    ensure!(matches.len() <= 1, "Ambiguous selected trait");
    Ok(matches.into_iter().next())
}
fn crafting(
    archive: &mut Archive<'_>,
    entity: &Value,
    defaults: &starrupture_observation_model::codex::Defaults,
) -> Result<(Option<Crafting>, bool)> {
    if entity.is_null() {
        return Ok((None, false));
    }
    let mut current = id(entity)?;
    let mut seen = BTreeSet::new();
    let mut found = None;
    let mut core = false;
    for _ in 0..16 {
        ensure!(seen.insert(current.clone()), "Entity ancestry cycle");
        let mut configs = archive.exports(&current, "CrMassBuildingEntityConfigAsset")?;
        configs.extend(archive.exports(&current, "MassEntityConfigAsset")?);
        ensure!(configs.len() == 1, "Ambiguous entity config {current}");
        let p = &configs[0].props["Config"];
        let mut traits = Vec::new();
        for reference in rows(&p["Traits"])? {
            let r = reference.as_str().context("Trait identity")?;
            let pkg = if r.starts_with('/') {
                id(reference)?
            } else {
                current.clone()
            };
            if let Some(e) = selected(archive, &pkg, "CrBuildingCraftingTrait", r)? {
                traits.push(e)
            }
            if selected(archive, &pkg, "CrMassBuildingBaseCoreTrait", r)?.is_some() {
                core = true
            }
        }
        ensure!(traits.len() <= 1, "Duplicate selected crafting traits");
        if found.is_none()
            && let Some(e) = traits.first()
        {
            let cp = &e.props["CraftingParameters"];
            found = Some(Crafting {
                collection: if cp["RecipeCollection"].is_null() {
                    None
                } else {
                    Some(id(&cp["RecipeCollection"])?)
                },
                recipe_order: Vec::new(),
                loop_seconds: float(&cp["CraftingLoopDuration"], defaults.crafting_loop_seconds)?,
                overwrite_seconds: float(
                    &cp["OverwriteRecipeBuildTime"],
                    defaults.overwrite_recipe_seconds,
                )?,
                processor: flag(&cp["bUseCraftingProcessor"], defaults.use_crafting_processor)?,
            })
        }
        if p["Parent"].is_null() {
            return Ok((found, core));
        }
        current = id(&p["Parent"])?;
    }
    anyhow::bail!("Entity ancestry depth")
}
// Cold preparation indexes authored package names once. Only referenced payloads are decoded.
pub(super) fn build(archive: &mut Archive<'_>, context: NativeContext) -> Result<Catalogue> {
    context.validate().map_err(anyhow::Error::msg)?;
    let paths: Vec<_> = archive.paths.keys().cloned().collect();
    let tables: Vec<_> = paths
        .iter()
        .filter(|p| p.starts_with("/Game/Chimera/Localization/StringTables/"))
        .cloned()
        .collect();
    ensure!(tables.len() <= 128, "String table candidate bound");
    let mut text = Text(BTreeMap::new());
    for p in tables {
        let (key, value) = archive.table(&p)?;
        text.0.insert(key, value["entries"].clone());
    }
    let mut corporations = Vec::new();
    let mut upgrades = Vec::new();
    let mut flows = Vec::new();
    for p in &paths {
        let leaf = p.rsplit('/').next().context("Package leaf")?;
        if leaf.starts_with("CD_") {
            for e in archive.exports(p, "CrCorporationData")? {
                corporations.push((p.clone(), e.props))
            }
        }
        if leaf.starts_with("BR_") {
            for e in archive.exports(p, "CrBuildingUpgradeRecipeData")? {
                upgrades.push((p.clone(), e.props))
            }
        }
        if leaf.starts_with("DA_Tutorial_Flow") {
            for e in archive.exports(p, "CrFlowEventDataAsset")? {
                flows.push((p.clone(), e.props))
            }
        }
    }
    ensure!(
        corporations.len() <= 32 && upgrades.len() <= 128 && flows.len() <= 32,
        "Unlock source bound"
    );
    let offers = |target: &str, field: &str| -> Result<Vec<CorporationOffer>> {
        let mut out = Vec::new();
        for (c, p) in &corporations {
            for level in rows(&p["Levels"])? {
                if rows(&level[field])?.iter().any(|v| id(v).is_ok_and(|v| v == target)) {
                    out.push(CorporationOffer {
                        id: c.clone(),
                        name: text.get(&p["DisplayText"])?,
                        level: number(&level["Level"])?,
                    })
                }
            }
        }
        Ok(out)
    };
    let mut routes: BTreeMap<String, Vec<BuildingUnlock>> = BTreeMap::new();
    for (c, p) in &corporations {
        let name = text.get(&p["DisplayText"])?;
        for level in rows(&p["Levels"])? {
            let n = number(&level["Level"])?;
            for b in rows(&level["BuildingRewards"])? {
                routes.entry(id(b)?).or_default().push(BuildingUnlock::Corporation {
                    id: c.clone(),
                    name: name.clone(),
                    level: n,
                    collection: None,
                })
            }
            for collection in rows(&level["BuildingCollectionRewards"])? {
                let cid = id(collection)?;
                let e = archive.one(&cid, "CrBuildingCollectionData")?;
                for b in rows(&e.props["Buildings"])? {
                    routes.entry(id(b)?).or_default().push(BuildingUnlock::Corporation {
                        id: c.clone(),
                        name: name.clone(),
                        level: n,
                        collection: Some(cid.clone()),
                    })
                }
            }
        }
    }
    for (u, p) in upgrades {
        let target = id(&p["BuildingReward"])?;
        let rid = id(&p["Recipe"])?;
        let re = archive.one(&rid, "CrItemRecipeData")?;
        let tasks = rows(&p["Tasks"])?
            .iter()
            .map(|t| {
                Ok(Task {
                    id: t["ID"].as_str().context("Task GUID")?.into(),
                    text: text.get(&t["EntryText"])?,
                    context: t["Context"].as_str().context("Task context")?.into(),
                    value: t["ContextValue"].as_str().context("Task value")?.into(),
                    count: number(&t["ContextNumber"])?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        routes.entry(target).or_default().push(BuildingUnlock::Upgrade {
            id: u.clone(),
            offers: offers(&u, "BuildingUpgradeRecipes")?,
            tasks,
            materials: bill(&re.props["UnlockRequirements"], "Key", "Value")?,
        });
    }
    for (flow, p) in flows {
        let listeners = archive.exports(&flow, "CrContextualEventFlowListener")?;
        let events = archive.exports(&flow, "CrSetBuildingsAvailableFlowEvent")?;
        for r in rows(&p["ContextualEventListeners"])? {
            let r = r.as_str().context("Listener reference")?;
            let found: Vec<_> = listeners.iter().filter(|e| e.path == r).collect();
            ensure!(found.len() == 1, "Referenced tutorial listener missing/ambiguous");
            let l = &found[0].props;
            let matched: Vec<_> =
                events.iter().filter(|e| e.path == l["Event"].as_str().unwrap_or("")).collect();
            ensure!(matched.len() <= 1, "Ambiguous tutorial event");
            if let Some(e) = matched.first() {
                if e.props["bAvailable"] != true {
                    continue;
                }
                for b in rows(&e.props["BuildingDataArray"])? {
                    routes.entry(id(&b["package"])?).or_default().push(BuildingUnlock::Tutorial {
                        flow: flow.clone(),
                        objective: l["ContextValue"].as_str().context("Tutorial objective")?.into(),
                        condition: l["Context"].as_str().context("Tutorial condition")?.into(),
                    });
                }
            }
        }
    }
    let mut buildings = BTreeMap::new();
    let mut links: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut excluded = BTreeMap::new();
    for root in &context.buildings {
        if [8, 9, 10].contains(&root.category) {
            excluded.insert(root.id.clone(), "Non-production building category".into());
            continue;
        }
        ensure!(root.category < 8 || root.category == 11, "Unsupported normal building category");
        let p = archive.one(&root.id, "CrBuildingData")?.props;
        let (mut crafting, core) =
            crafting(archive, &p["EntityType"]["EntityConfig"]["package"], &context.defaults)?;
        if let Some(c) = &mut crafting
            && let Some(cid) = &c.collection
        {
            let e = archive.one(cid, "CrItemRecipeCollection")?;
            c.recipe_order = rows(&e.props["Recipes"])?.iter().map(id).collect::<Result<_>>()?;
            for recipe in &c.recipe_order {
                let v = links.entry(recipe.clone()).or_default();
                ensure!(!v.contains(&root.id), "Duplicate building recipe link");
                v.push(root.id.clone());
            }
        }
        let unlocks = routes.remove(&root.id).unwrap_or_default();
        ensure!(
            !p["ResourceRequirements"]["Requirements"].is_null(),
            "Construction bill omitted: {}",
            root.id
        );
        buildings.insert(
            root.id.clone(),
            Building {
                name: text.get(&p["BuildingName"])?,
                icon: id(&p["Icon"]["ResourceObject"])?,
                category: root.category,
                materials: bill(&p["ResourceRequirements"]["Requirements"], "Item", "Quantity")?,
                add_platform: flag(&p["AddBasePlatformCost"], context.defaults.add_platform)?,
                add_pillar: flag(&p["AddPillarCost"], context.defaults.add_pillar)?,
                unlocks,
                core_progression: core,
                crafting,
            },
        );
    }
    let weapon_purchases = weapon_purchases(archive, &context, &text)?;
    let mut weapons: BTreeMap<String, Vec<Weapon>> = BTreeMap::new();
    for (id, weapon) in &weapon_purchases {
        weapons.entry(weapon.ammo.clone()).or_default().push(Weapon {
            id: id.clone(),
            name: weapon.name.clone(),
        });
    }
    let mut recipes = BTreeMap::new();
    for (r, producers) in links {
        let p = archive.one(&r, "CrItemRecipeData")?.props;
        let kind = p["UnlockType"].as_str().context("Recipe unlock classification")?;
        if kind == "ECrRecipeUnlockType::DevelopmentOnly" {
            excluded.insert(r, "Development-only recipe".into());
            continue;
        }
        let output = Amount {
            item: id(&p["OutputItem"]["Item"])?,
            quantity: number(&p["OutputItem"]["Count"])?,
        };
        let unlock = match kind {
            "ECrRecipeUnlockType::RecipeTable" => Unlock::Research {
                materials: bill(&p["UnlockRequirements"], "Key", "Value")?,
            },
            "ECrRecipeUnlockType::Building" => Unlock::Building,
            "ECrRecipeUnlockType::Ammo" => Unlock::FirstWeaponPurchase {
                weapons: weapons
                    .get(&output.item)
                    .cloned()
                    .context("No registered weapon-to-ammunition link")?,
            },
            "ECrRecipeUnlockType::UnlockedByDefault" => Unlock::Default,
            _ => anyhow::bail!("Unsupported unlock kind {kind}"),
        };
        recipes.insert(
            r,
            Recipe {
                name: text.get(&p["DisplayText"])?,
                output,
                inputs: bill(&p["NeededResources"], "Item", "Count")?,
                producers,
                base_seconds: float(&p["BuildTime"], context.defaults.recipe_seconds)?,
                icon: id(&p["Icon"]["ResourceObject"])?,
                unlock,
            },
        );
    }
    let core = archive.one(&context.core_asset, "CrBaseCoreDataAsset")?;
    let core_tiers = rows(&core.props["BaseCoreLevels"])?
        .iter()
        .enumerate()
        .map(|(i, t)| {
            Ok(CoreTier {
                source_index: u32::try_from(i)?,
                materials: bill(&t["ItemsToUpgrade"], "Item", "Count")?,
                cooling: number(&t["CoolingCapacity"])?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let mut required = BTreeSet::new();
    let mut add = |bill: &[Amount]| {
        required.extend(bill.iter().map(|a| a.item.clone()));
    };
    for r in recipes.values() {
        add(std::slice::from_ref(&r.output));
        add(&r.inputs);
        if let Unlock::Research {
            materials,
        } = &r.unlock
        {
            add(materials)
        }
    }
    for b in buildings.values() {
        add(&b.materials);
        for u in &b.unlocks {
            if let BuildingUnlock::Upgrade {
                materials,
                ..
            } = u
            {
                add(materials)
            }
        }
    }
    for t in &core_tiers {
        add(&t.materials)
    }
    let equip_ids = weapon_purchases
        .values()
        .flat_map(|weapon| {
            weapon
                .equip_materials
                .iter()
                .chain(weapon.mods.values().flat_map(|upgrade| &upgrade.equip_materials))
        })
        .map(|amount| amount.item.as_str())
        .collect::<BTreeSet<_>>();
    let mut equipment_material_names = BTreeMap::new();
    for id in equip_ids {
        let equip = archive.one(id, "@CDO")?;
        let name = text.optional(&equip.props["ItemName"])?;
        // A packaged placeholder is not a name that the station displays.
        equipment_material_names
            .insert(id.to_owned(), name.filter(|s| s != "<MISSING STRING TABLE ENTRY>"));
    }
    let war_bond_currency = if weapon_purchases.is_empty() {
        None
    } else {
        let currency = archive.one(WAR_BONDS, "@CDO")?;
        Some(text.get(&currency.props["ItemName"])?)
    };
    let mut items = BTreeMap::new();
    for i in required {
        let p = archive.one(&i, "@CDO")?.props;
        items.insert(
            i,
            Item {
                name: text.get(&p["ItemName"])?,
                description: text.optional(&p["ItemDescription"])?,
                icon: id(&p["ItemIcon"]["ResourceObject"])?,
            },
        );
    }
    let mut corporation_records = BTreeMap::new();
    for (key, data) in corporations {
        let mut levels = Vec::new();
        for level in rows(&data["Levels"])? {
            // Some rewards use non-item classes with no catalogue item presentation.
            let mut rewards = BTreeMap::<String, u32>::new();
            for row in rows(&level["ItemRewards"])? {
                let package = id(&row["Item"])?;
                if !items.contains_key(&package) {
                    let Some(asset) = archive
                        .exports(&package, "@CDO")
                        .ok()
                        .and_then(|mut rows| (rows.len() == 1).then(|| rows.swap_remove(0).props))
                    else {
                        continue;
                    };
                    let (Ok(name), Ok(icon)) =
                        (text.get(&asset["ItemName"]), id(&asset["ItemIcon"]["ResourceObject"]))
                    else {
                        continue;
                    };
                    items.insert(
                        package.clone(),
                        Item {
                            name,
                            icon,
                            description: text.optional(&asset["ItemDescription"]).ok().flatten(),
                        },
                    );
                }
                let quantity = number(&row["Amount"])?;
                if quantity > 0 {
                    let total = rewards.entry(package).or_default();
                    *total = total.checked_add(quantity).context("Corporation reward quantity")?;
                }
            }
            levels.push(CorporationLevel {
                level: number(&level["Level"])?,
                reputation: number(&level["ReputationRequired"])?,
                items: rewards
                    .into_iter()
                    .map(|(item, quantity)| Amount {
                        item,
                        quantity,
                    })
                    .collect(),
                features: rows(&level["FeatureRewards"])?
                    .iter()
                    .filter_map(|value| {
                        let (_, feature) = value.as_str()?.split_once("::")?;
                        Some(
                            match feature {
                                "BuildingDrone" => "Builder drone",
                                "MedTool" => "Med tool",
                                "BiggerInventory" => "Expanded inventory",
                                "Dash" => "Dash",
                                "Grenade" => "Grenades",
                                "Map" => "Map",
                                _ => return None,
                            }
                            .into(),
                        )
                    })
                    .collect(),
                inventory_slots: level["InventorySlotRewards"]
                    .as_u64()
                    .unwrap_or_default()
                    .try_into()?,
            });
        }
        corporation_records.insert(
            key,
            Corporation {
                name: text.get(&data["DisplayText"])?,
                native_name: data["Name"].as_str().context("Corporation native name")?.into(),
                icon: id(&data["Icon"]["ResourceObject"]).ok(),
                description: text.optional(&data["Description"]).ok().flatten(),
                levels,
            },
        );
    }
    let corporations = corporation_records;
    let catalogue = Catalogue {
        version: 8,
        lock_icon: [
            "/Game/Chimera/UI/CommonAssets/Textures/SR_Locked_icon",
            "/Game/Chimera/UI/Buildings/Crafting/Textures/SR_lockedIcon2",
        ]
        .into_iter()
        .find(|path| archive.paths.contains_key(*path))
        .map(str::to_owned),
        context,
        items,
        recipes,
        buildings,
        core_tiers,
        corporations,
        weapon_purchases,
        war_bond_currency,
        equipment_material_names,
        excluded,
    };
    catalogue.validate()?;
    Ok(catalogue)
}
fn weapon_mod(archive: &mut Archive<'_>, id: &str) -> Result<Value> {
    let mut exports = archive.exports(id, "CrWeaponModDataAsset")?;
    if exports.is_empty() {
        exports = archive.exports(id, "CrSightWeaponModDataAsset")?;
    }
    ensure!(exports.len() == 1, "Weapon modification export: {id}");
    Ok(exports.swap_remove(0).props)
}

fn mod_tag<'a>(value: &'a Value, field: &str) -> Result<&'a str> {
    value[field]["TagName"]
        .as_str()
        .filter(|s| !s.is_empty())
        .with_context(|| format!("Missing {field} tag"))
}

fn weapon_purchases(
    archive: &mut Archive<'_>,
    context: &NativeContext,
    text: &Text,
) -> Result<BTreeMap<String, WeaponPurchase>> {
    let registered = context.weapon_mod_assets.iter().map(String::as_str).collect::<BTreeSet<_>>();
    let mut mods = BTreeMap::new();
    for &package in &registered {
        mods.insert(package.to_owned(), weapon_mod(archive, package)?);
    }
    let mut result = BTreeMap::new();
    for class in &context.weapon_classes {
        let (package, class_name) = class.split_once('.').context("Weapon class identity")?;
        let export = archive.one(package, "@CDO")?;
        ensure!(export.name.strip_prefix("Default__") == Some(class_name), "Weapon CDO identity");
        let p = export.props;
        if p["AmmoItem"].is_null() {
            continue;
        }
        let mut defaults = BTreeSet::new();
        let mut slot_order = Vec::new();
        let mut family = None;
        for row in rows(&p["DefaultMods"])? {
            let package = id(&row["Value"]["package"])?;
            if !mods.contains_key(&package) {
                mods.insert(package.clone(), weapon_mod(archive, &package)?);
            }
            let def = &mods[&package];
            let tag = mod_tag(def, "CompatibleWeapon")?;
            ensure!(
                family.as_deref().is_none_or(|family| family == tag),
                "Conflicting default weapon mod compatibility"
            );
            ensure!(
                row["Key"]["TagName"].as_str() == Some(mod_tag(def, "ModSlot")?),
                "Default weapon mod slot mismatch"
            );
            family = Some(tag.to_owned());
            slot_order.push(mod_tag(def, "ModSlot")?.to_owned());
            defaults.insert(package);
        }
        let hidden = rows(&p["ModSlotsToHideInArmory"])?;
        let mut upgrades = BTreeMap::new();
        for (id_key, data) in &mods {
            let is_default = defaults.contains(id_key);
            let is_registered = registered.contains(id_key.as_str());
            if !is_default
                && (!is_registered || family.as_deref() != Some(mod_tag(data, "CompatibleWeapon")?))
            {
                continue;
            }
            let slot = mod_tag(data, "ModSlot")?;
            upgrades.insert(
                id_key.clone(),
                WeaponMod {
                    icon: id(&data["ModIcon"]["package"]).ok(),
                    name: text.get(&data["ModDisplayNameName"])?,
                    slot: slot.into(),
                    sort_priority: data["DisplaySortPriority"].as_i64().unwrap_or(0),
                    war_bonds: number(&data["UnlockTokenCost"])?,
                    equip_materials: bill(&data["RequiredItemsToApplyMod"], "Key", "Value")?,
                    default_for_weapon: is_default,
                    registered_for_unlock: is_registered,
                    hidden_slot: hidden.iter().any(|record| record["TagName"] == slot),
                    provider: if data["DefaultProvider"].is_null() {
                        None
                    } else {
                        Some(id(&data["DefaultProvider"])?)
                    },
                },
            );
        }
        result.insert(
            package.into(),
            WeaponPurchase {
                icon: id(&p["ItemIcon"]["ResourceObject"]).ok(),
                description: text.optional(&p["ItemDescription"]).ok().flatten(),
                name: text.get(&p["ItemName"])?,
                ammo: id(&p["AmmoItem"])?,
                war_bonds: number(&p["UnlockTokenCost"])?,
                equip_materials: bill(&p["RequiredItemsToApplyWeapon"], "Key", "Value")?,
                mods: upgrades,
                slot_order,
            },
        );
    }
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_zero_is_not_replaced_by_default() {
        assert_eq!(float(&serde_json::json!(0), 10.0).unwrap(), 0.0);
        assert_eq!(float(&Value::Null, 10.0).unwrap(), 10.0);
        assert!(bill(&serde_json::json!([{"Item":"../x","Count":1}]), "Item", "Count").is_err());
    }
}
