//! Read-only recipe research and station targeting.
use crate::{Cache, RuntimeCache, platform};
use anyhow::{Context, Result, ensure};
use serde_json::json;
use starrupture_observation_model::shutdown::StopSignal;
use std::{path::Path, sync::Arc};
mod mappings;
mod runtime;
pub trait AssetSetup: Send {
    fn enabled(&self) -> bool;
    fn cached(&mut self, executable: &Path) -> Result<Option<Tracker>>;
    fn preparing(&mut self);
    fn ready(&mut self);
    fn complete(
        &mut self,
        executable: &Path,
        mappings: &[u8],
        stop: &StopSignal,
    ) -> Result<Tracker>;
    fn failed(&mut self, message: &str);
}
pub(crate) use mappings::generate as asset_mappings;
pub(crate) use runtime::Binding;
fn foreground_process() -> u32 {
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    let mut pid = 0;
    unsafe {
        GetWindowThreadProcessId(GetForegroundWindow(), &mut pid);
    }
    pid
}
fn validate_shortcut_context(
    process: u32,
    required_cursor: Option<[i32; 2]>,
    foreground: u32,
    cursor: Option<[i32; 2]>,
) -> Result<()> {
    ensure!(foreground == process, "Keep StarRupture focused until the shortcut completes.");
    ensure!(
        required_cursor.is_none() || required_cursor == cursor,
        "Keep the mouse over the recipe until it is pinned, then try again."
    );
    Ok(())
}
pub(crate) fn foreground_matches(process: u32) -> Result<()> {
    validate_shortcut_context(process, None, foreground_process(), None)
}
pub(crate) fn cursor_matches(cursor: Option<[i32; 2]>, process: u32) -> Result<()> {
    use windows_sys::Win32::{Foundation::POINT, UI::WindowsAndMessaging::GetCursorPos};
    let expected = cursor.context("Recipe capture has no cursor position")?;
    let mut point = POINT::default();
    ensure!(unsafe { GetCursorPos(&mut point) } != 0, "Recipe cursor position unavailable");
    validate_shortcut_context(
        process,
        Some(expected),
        foreground_process(),
        Some([point.x, point.y]),
    )
}

#[cfg(test)]
mod shortcut_tests {
    use super::*;
    #[test]
    fn clearing_tolerates_recentered_or_unavailable_cursor_but_capture_requires_its_target() {
        for actual in [Some([20, 30]), Some([960, 540]), None] {
            assert!(validate_shortcut_context(42, None, 42, actual).is_ok());
            assert_eq!(
                validate_shortcut_context(42, Some([20, 30]), 42, actual).is_ok(),
                actual == Some([20, 30])
            );
            assert!(validate_shortcut_context(42, None, 99, actual).is_err());
            assert!(validate_shortcut_context(42, Some([20, 30]), 99, actual).is_err());
        }
    }
}
#[derive(Clone)]
pub struct Tracker {
    pub catalogue: Arc<starrupture_observation_model::recipe::Catalogue>,
    pub control: Arc<starrupture_observation_model::recipe::Control>,
}
pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(debug_assertions)]
mod probe;
#[cfg(debug_assertions)]
pub use probe::probe;

/// Exercise connection preparation on the production worker, without a HUD
/// or a shortcut request. All observations and cancellation use the usual path.
pub fn self_test_warmup(directory: &Path) -> Result<()> {
    use starrupture_observation_model::overlay_stream::Payload;
    let exe = std::env::current_exe()?;
    let catalogue: starrupture_observation_model::recipe::Catalogue =
        serde_json::from_slice(&std::fs::read(
            exe.parent().context("Executable folder")?.join("recipe-assets/catalogue.json"),
        )?)?;
    catalogue.validate().map_err(anyhow::Error::msg)?;
    let control = Arc::new(starrupture_observation_model::recipe::Control::default());
    let stop = Arc::new(StopSignal::default());
    let started = std::time::Instant::now();
    let mut worlds = 0;
    crate::overlay_stream::run_with_tracker(
        &directory.join("cache"),
        stop.clone(),
        |message| {
            if matches!(message.payload, Payload::World(_)) {
                worlds += 1;
            }
            if worlds >= 24 || started.elapsed() >= std::time::Duration::from_secs(20) {
                stop.request();
            }
            Ok(())
        },
        |_| {},
        Some(Tracker {
            catalogue: Arc::new(catalogue),
            control: control.clone(),
        }),
    )?;
    ensure!(worlds >= 24, "Warmup diagnostic did not receive enough live observations");
    ensure!(
        control.view().snapshot.is_none() && !control.request().active,
        "Warmup changed recipe selection"
    );
    std::fs::write(
        directory.join("recipe-warmup-validation.json"),
        serde_json::to_vec_pretty(
            &json!({"readOnly":true,"gameCodeInvoked":false,"worldObservations":worlds,"selectionChanged":false,"readerStopped":true}),
        )?,
    )?;
    Ok(())
}

/// Finite production-reader validation. Does not open a HUD or change selection.
pub fn self_test(directory: &Path) -> Result<()> {
    std::fs::create_dir_all(directory)?;
    std::fs::create_dir_all(directory.join("cache"))?;
    let stop = Arc::new(StopSignal::default());
    let budget = platform::ReadBudget::new(stop.clone());
    let process = platform::find_game(budget.clone())?;
    ensure!(process.identity.executable_sha256 == crate::BUILD_HASH, "Unsupported game executable");
    let exe = std::env::current_exe()?;
    let pack = std::fs::read(
        exe.parent().context("Executable folder")?.join("recipe-assets/catalogue.json"),
    )?;
    let catalogue: starrupture_observation_model::recipe::Catalogue =
        serde_json::from_slice(&pack)?;
    catalogue.validate().map_err(anyhow::Error::msg)?;
    let cache = std::path::PathBuf::from(std::env::var_os("LOCALAPPDATA").context("App data")?)
        .join("StarRuptureWaveOverlay/cache/session.json");
    let cached = crate::cache::read_json::<Cache>(&cache, 100000)
        .ok()
        .filter(|r| crate::cache_matches(r, &process.identity));
    let value = smol::block_on(async {
        let record = match cached {
            Some(r) => r,
            None => crate::discover(&process).await?,
        };
        let base = RuntimeCache::new(record)?;
        budget.renew(60000);
        let mut live = crate::live_context::resident::Resident::open(
            &process,
            &base,
            &directory.join("cache"),
        )
        .await?;
        let ctx = live.context();
        let binding_started = std::time::Instant::now();
        let binding = Binding::discover(&ctx).await?;
        let binding_millis = binding_started.elapsed().as_millis();
        let registry_before = runtime::registry_scans();
        budget.renew(5000);
        let station_started = std::time::Instant::now();
        live.menu(&process, &base).await?;
        let in_station = binding.in_station(&ctx, &base).await?;
        live.menu(&process, &base).await?;
        let station_again = binding.in_station(&ctx, &base).await?;
        live.menu(&process, &base).await?;
        let station_check_millis = station_started.elapsed().as_millis();
        let capture_started = std::time::Instant::now();
        let capture = binding.capture(&ctx, &base, &catalogue).await;
        let capture_millis = capture_started.elapsed().as_millis();
        ensure!(capture_millis < 5000, "Recipe capture scan exceeded its read budget");
        let capture_probe = match capture {
            Ok(id) => json!({"recipe":id,"status":"hovered_row_observed"}),
            Err(e) => json!({"status":"unavailable","reason":format!("{e:#}")}),
        };
        budget.renew(5000);
        let warm_started = std::time::Instant::now();
        let warm_capture = binding.capture(&ctx, &base, &catalogue).await;
        let warm_capture_millis = warm_started.elapsed().as_millis();
        let warm_probe = match warm_capture {
            Ok(id) => json!({"recipe":id,"status":"hovered_row_observed"}),
            Err(e) => json!({"status":"unavailable","reason":format!("{e:#}")}),
        };
        let mut samples = Vec::new();
        for _ in 0..3 {
            budget.renew(5000);
            let sample_started = std::time::Instant::now();
            live.menu(&process, &base).await?;
            let sample = binding
                .sample(
                    &ctx,
                    &base,
                    live.world_address(),
                    "/Game/Chimera/Crafting/CR_Biofilament",
                    &catalogue,
                    1,
                )
                .await?;
            live.menu(&process, &base).await?;
            samples.push(json!({"recipe":sample.recipe_id,"observedAtMs":sample.observed_at_ms,"readMillis":sample_started.elapsed().as_millis(),"ingredients":sample.ingredients.iter().map(|i|json!({"item":i.item_id,"submitted":i.submitted,"required":i.required,"remaining":i.remaining,"bagCount":i.bag_count})).collect::<Vec<_>>()}));
        }
        budget.renew(5000);
        live.menu(&process, &base).await?;
        let inventory = binding.bag_inventory(&ctx, &base).await?;
        live.menu(&process, &base).await?;
        let hot_path_registry_scans = runtime::registry_scans() - registry_before;
        ensure!(hot_path_registry_scans == 0, "Recipe hot path performed global discovery");
        #[cfg(debug_assertions)]
        let widget_diagnostic = {
            budget.renew(60000);
            binding.diagnostic(&ctx, &base).await.context("Station widget diagnostic")?
        };
        #[cfg(not(debug_assertions))]
        let widget_diagnostic = serde_json::Value::Null;
        Ok::<_, anyhow::Error>(
            json!({"readOnly":true,"gameCodeInvoked":false,"process":process.identity,"samples":samples,"bagInventory":inventory,"bindingDiscoveryMillis":binding_millis,"inRecipeStation":in_station,"repeatedInRecipeStation":station_again,"twoStationChecksMillis":station_check_millis,"captureScanMillis":capture_millis,"captureProbe":capture_probe,"warmCaptureMillis":warm_capture_millis,"warmCaptureProbe":warm_probe,"hotPathRegistryScans":hot_path_registry_scans,"hoverCaptureVerified":false,"widgetDiagnostic":widget_diagnostic}),
        )
    })?;
    stop.request();
    std::fs::write(
        directory.join("recipe-reader-validation.json"),
        serde_json::to_vec_pretty(&value)?,
    )?;
    Ok(())
}
