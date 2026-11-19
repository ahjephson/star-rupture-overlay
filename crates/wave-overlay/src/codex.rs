//! On-demand catalogue preparation. No HUD, key bindings or background worker are started here.
use anyhow::{Context, Result, ensure};
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
pub fn self_test(directory: &Path) -> Result<()> {
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
    let searches = result.search("calcium", 32)?;
    let first = result.catalogue.items.values().next().context("Catalogue item")?;
    let icon = result.icon(&first.icon)?;
    let report = serde_json::json!({
        "metrics":result.metrics,"elapsedMillis":started.elapsed().as_millis(),
        "items":result.catalogue.items.len(),"recipes":result.catalogue.recipes.len(),
        "buildings":result.catalogue.buildings.len(),"coreTiers":result.catalogue.core_tiers.len(),
        "search":searches,"firstIcon":[icon.width,icon.height],
        "retainedIconBytes":result.retained_icon_bytes(),"readOnly":true,"gameCodeInvoked":false
    });
    std::fs::write(directory.join("catalogue-check.json"), serde_json::to_vec_pretty(&report)?)?;
    Ok(())
}
