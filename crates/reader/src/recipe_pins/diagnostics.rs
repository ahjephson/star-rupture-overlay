//! Bounded diagnostics for loaded-save identity. Does not save or invoke game code.
use crate::{Cache, RuntimeCache, platform};
use anyhow::{Context, Result};
use std::{path::Path, sync::Arc};
pub fn self_test(directory: &Path) -> Result<()> {
    use starrupture_observation_model::shutdown::StopSignal;
    std::fs::create_dir_all(directory)?;
    std::fs::create_dir_all(directory.join("cache"))?;
    let stop = Arc::new(StopSignal::default());
    let budget = platform::ReadBudget::new(stop);
    budget.renew(60000);
    let process = platform::find_game(budget.clone())?;
    let cache = std::path::PathBuf::from(std::env::var_os("LOCALAPPDATA").context("App data")?)
        .join("StarRuptureWaveOverlay/cache/session.json");
    smol::block_on(async {
        let record = match crate::cache::read_json::<Cache>(&cache, 100000)
            .ok()
            .filter(|c| crate::cache_matches(c, &process.identity))
        {
            Some(c) => c,
            None => crate::discover(&process).await?,
        };
        std::fs::write(directory.join("cache/session.json"), serde_json::to_vec(&record)?)?;
        let base = RuntimeCache::new(record)?;
        let live = crate::live_context::resident::Resident::open(
            &process,
            &base,
            &directory.join("cache"),
        )
        .await?;
        let ctx = live.context();
        budget.renew(60000);
        let binding = super::Binding::discover(&ctx, &base, live.world_address()).await?;
        budget.renew(5000);
        let first = binding
            .current(&ctx, live.world_address(), std::path::Path::new(&process.identity.executable))
            .await?;
        let second = binding
            .current(&ctx, live.world_address(), std::path::Path::new(&process.identity.executable))
            .await?;
        anyhow::ensure!(
            first.key == second.key && first.label == second.label,
            "Loaded world changed between identity checks"
        );
        let report = serde_json::json!({"status":"passed","worldLabel":first.label,"worldKey":first.key,"gameMemoryWrites":false,"saveFilesRead":false,"saveFilesWritten":false});
        std::fs::write(
            directory.join("identity-validation.json"),
            serde_json::to_vec_pretty(&report)?,
        )?;
        println!("{}", report);
        Ok(())
    })
}
