//! On-demand catalogue preparation and native Codex window.
mod backdrop;
mod banner;
mod panel;
pub(crate) mod pins;
mod view;
mod window;
use anyhow::{Context, Result, ensure};
pub(crate) use panel::{HOTKEY, Panel};
use starrupture_assets::codex::{NativeContext, Prepared, Store};
use starrupture_observation_model::shutdown::StopSignal;
use std::{
    path::Path,
    sync::{Arc, atomic::AtomicBool},
};

fn build(
    directory: &Path,
    store: &Store,
    paks: &Path,
    context: NativeContext,
    mappings: &[u8],
    stop: &StopSignal,
) -> Result<Prepared> {
    let output = directory.join("catalogue");
    std::fs::create_dir_all(&output)?;
    let mappings_path = output.join("native.usmap");
    std::fs::write(&mappings_path, mappings)?;
    let error = output.join("error.txt");
    if error.exists() {
        std::fs::remove_file(&error)?;
    }
    let request = output.join("request.json");
    let value = serde_json::json!({
        "assets":{"paks":paks,"mappings":mappings_path,"output":output,"progress":output.join("progress.txt"),"error":error,"gameHash":context.game_hash},
        "context":context
    });
    std::fs::write(&request, serde_json::to_vec(&value)?)?;
    let result = crate::recipe_tracker::run_catalogue_child(
        &std::env::current_exe()?,
        &request,
        &error,
        stop,
        &AtomicBool::new(true),
    );
    // The shared schema store owns the reusable copy; this is only the confined child input.
    let cleanup = std::fs::remove_file(&mappings_path);
    result?;
    cleanup?;
    let mut prepared =
        store.lookup(paks, &context)?.context("Prepared catalogue cache unavailable")?;
    let metrics = crate::recipe_tracker::read_asset_input(&output.join("preparation.json"), 4096)?;
    prepared.metrics = serde_json::from_slice(&metrics)?;
    Ok(prepared)
}
pub fn self_test(directory: &Path, live: bool) -> Result<()> {
    ensure!(directory.is_absolute(), "Absolute catalogue diagnostic directory required");
    std::fs::create_dir_all(directory)?;
    let directory = directory.canonicalize()?;
    let stop = Arc::new(StopSignal::default());
    let store = Store::new(directory.join("catalogue"));
    let started = std::time::Instant::now();
    let result = std::thread::scope(|scope| {
        scope
            .spawn(|| {
                starrupture_overlay_reader::codex::prepare(
                    &directory,
                    stop.clone(),
                    |paks, context| store.lookup(paks, context),
                    |paks, context, mappings, stop| {
                        build(&directory, &store, paks, context, mappings, stop)
                    },
                )
            })
            .join()
    })
    .map_err(|_| anyhow::anyhow!("Catalogue preparation worker failed"))??;
    if live {
        let live_started = std::time::Instant::now();
        let mut sample = None;
        std::thread::scope(|scope| {
            scope.spawn(|| {
                stop.wait_until(std::time::Instant::now() + std::time::Duration::from_secs(10));
                stop.request();
            });
            let observed = starrupture_overlay_reader::codex::observe(
                &directory,
                stop.clone(),
                None,
                &result.catalogue.context,
                &pins::upgrade_recipes(&result.catalogue),
                |_, value| {
                    if value.is_some() {
                        sample = value;
                        stop.request();
                    }
                },
            );
            stop.request();
            observed
        })?;
        let sample = sample.context("Codex live sample unavailable")?;
        let tabs: Vec<_> = view::Tab::ALL.into_iter().map(|tab| {
            let rows = view::rows(&result.catalogue, &sample, tab, "");
            serde_json::json!({"tab":tab.label(),"rows":rows.len(),"locked":rows.iter().filter(|r|r.locked).count()})
        }).collect();
        let hrs: Vec<_> =
            view::rows(&result.catalogue, &sample, view::Tab::Production, "Heat Resistant Sheet")
                .into_iter()
                .filter(|r| r.title == "Heat Resistant Sheet")
                .map(|r| r.summary)
                .collect();
        let report = serde_json::json!({
            "buildings": sample.buildings, "recipes": sample.recipes, "research": sample.research,
            "upgradeSubmissions": sample.upgrade_submissions,
            "ownedEquipment": sample.owned_equipment, "corporations": sample.corporations,
            "corporationReputation": sample.corporation_reputation,
            "highestUnlockedCoreTier": sample.highest_unlocked_core_tier,
            "cores": sample.cores.iter().map(|core| [core.id, core.tier_index, core.capacity]).collect::<Vec<_>>(),
            "tabs": tabs, "heatResistantSheet": hrs, "elapsedMillis":live_started.elapsed().as_millis()
        });
        std::fs::write(
            directory.join("catalogue-live-check.json"),
            serde_json::to_vec_pretty(&report)?,
        )?;
    }
    // Exercise every equipment detail against the game's current catalogue independently of ownership.
    let equipment_progress = starrupture_overlay_reader::codex::Progress {
        buildings: ["/Game/Chimera/Buildings/Interiors/Armory/BD_Armory".into()].into(),
        owned_equipment: result.catalogue.weapon_purchases.keys().cloned().collect(),
        recipes: Default::default(),
        research: Default::default(),
        upgrade_submissions: Default::default(),
        corporations: Default::default(),
        corporation_reputation: Default::default(),
        cores: Vec::new(),
        highest_unlocked_core_tier: None,
    };
    let equipment = view::rows(&result.catalogue, &equipment_progress, view::Tab::Equipment, "");
    let equipment_tiles: usize = equipment
        .iter()
        .map(|row| {
            view::details(&result.catalogue, &equipment_progress, row, None)
                .iter()
                .map(|row| row.tiles.len())
                .sum::<usize>()
        })
        .sum();
    let searches = result.search("calcium", 32)?;
    let first = result.catalogue.items.values().next().context("Catalogue item")?;
    let icon = result.icon(&first.icon)?;
    let report = serde_json::json!({
        "metrics":result.metrics,"elapsedMillis":started.elapsed().as_millis(),
        "items":result.catalogue.items.len(),"recipes":result.catalogue.recipes.len(),
        "buildings":result.catalogue.buildings.len(),"coreTiers":result.catalogue.core_tiers.len(),
        "search":searches,"firstIcon":[icon.width,icon.height],
        "equipmentViews":equipment.len(),"equipmentTiles":equipment_tiles,
        "retainedIconBytes":result.retained_icon_bytes(),"readOnly":true,"gameCodeInvoked":false
    });
    std::fs::write(directory.join("catalogue-check.json"), serde_json::to_vec_pretty(&report)?)?;
    Ok(())
}
