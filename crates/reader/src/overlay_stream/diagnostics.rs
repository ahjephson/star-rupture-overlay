//! Finite live worker isolation check. Uses isolated controls and a deliberate recipe delay.
use super::*;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
struct Preparation {
    catalogue: Arc<starrupture_observation_model::recipe::Catalogue>,
    stop: Arc<StopSignal>,
    started: Arc<AtomicU64>,
    cached: Arc<AtomicU64>,
    ready: Arc<AtomicBool>,
}
impl crate::recipe::AssetSetup for Preparation {
    fn enabled(&self) -> bool {
        true
    }
    fn cached(&mut self, _: &Path, hash: &str) -> Result<Option<crate::recipe::Tracker>> {
        ensure!(self.catalogue.game_hash == hash, "Worker diagnostic catalogue build mismatch");
        self.started.store(clock_ms(), Ordering::SeqCst);
        self.stop.wait_until(Instant::now() + Duration::from_secs(12));
        ensure!(!self.stop.is_requested(), "Worker diagnostic cancelled");
        self.cached.store(clock_ms(), Ordering::SeqCst);
        Ok(Some(crate::recipe::Tracker {
            catalogue: self.catalogue.clone(),
            control: Arc::new(Default::default()),
        }))
    }
    fn preparing(&mut self) {}
    fn ready(&mut self) {
        self.ready.store(true, Ordering::SeqCst);
    }
    fn complete(
        &mut self,
        _: &Path,
        _: &str,
        _: &[u8],
        _: &StopSignal,
    ) -> Result<crate::recipe::Tracker> {
        anyhow::bail!("Worker diagnostic must not extract assets")
    }
    fn failed(&mut self, message: &str) {
        eprintln!("Worker diagnostic preparation failed: {message}");
    }
}
pub fn self_test(directory: &Path, catalogue_file: &Path) -> Result<()> {
    ensure!(directory.is_absolute(), "Absolute private worker diagnostic directory required");
    std::fs::create_dir_all(directory)?;
    let catalogue: starrupture_observation_model::recipe::Catalogue =
        crate::cache::read_json(catalogue_file, 2_000_000)?;
    catalogue.validate().map_err(anyhow::Error::msg)?;
    let stop = Arc::new(StopSignal::default());
    let timeout = stop.clone();
    let timer = std::thread::spawn(move || {
        timeout.wait_until(Instant::now() + Duration::from_secs(90));
        timeout.request();
    });
    let started = Arc::new(AtomicU64::new(0));
    let cached = Arc::new(AtomicU64::new(0));
    let ready = Arc::new(AtomicBool::new(false));
    let setup = Preparation {
        catalogue: Arc::new(catalogue),
        stop: stop.clone(),
        started: started.clone(),
        cached: cached.clone(),
        ready: ready.clone(),
    };
    let navigation = Arc::new(starrupture_observation_model::compass::Control::default());
    navigation.set(true);
    let begin = clock_ms();
    let mut validator =
        starrupture_observation_model::overlay_stream::Validator::new(std::process::id(), begin);
    let mut ready_at: Option<Instant> = None;
    let (mut worlds, mut compasses, mut during_wave, mut during_compass, mut sessions) =
        (0, 0, 0, 0, 0);
    let (mut first_world, mut first_compass) = (None, None);
    let mut compass_times = Vec::new();
    let result = run_with_navigation(
        directory,
        stop.clone(),
        |message| {
            validator.accept(&message, clock_ms()).map_err(anyhow::Error::msg)?;
            if ready.load(Ordering::SeqCst) {
                ready_at.get_or_insert_with(Instant::now);
            }
            let blocked = started.load(Ordering::SeqCst) != 0 && !ready.load(Ordering::SeqCst);
            match message.payload {
                Payload::Ready => sessions += 1,
                Payload::World(_) => {
                    worlds += 1;
                    first_world.get_or_insert(clock_ms() - begin);
                    if blocked {
                        during_wave += 1;
                    }
                }
                Payload::Compass(Some(sample)) => {
                    compasses += 1;
                    first_compass.get_or_insert(clock_ms() - begin);
                    compass_times.push(sample.finished_ms);
                    if blocked {
                        during_compass += 1;
                    }
                }
                _ => {}
            }
            if ready_at.is_some_and(|t| t.elapsed() >= Duration::from_secs(5))
                && worlds >= 24
                && compasses >= 180
            {
                stop.request();
            }
            Ok(())
        },
        |_| {},
        None,
        Some(Box::new(setup)),
        Some(navigation),
    );
    stop.request();
    let _ = timer.join();
    result?;
    ensure!(
        sessions == 1 && ready.load(Ordering::SeqCst),
        "Worker diagnostic did not finish initialization in one session"
    );
    ensure!(during_wave >= 8 && during_compass >= 10, "Recipe preparation blocked other features");
    let gaps: Vec<_> = compass_times.windows(2).map(|w| w[1].saturating_sub(w[0])).collect();
    let report = serde_json::json!({"status":"passed","featureRegistryScans":crate::recipe::registry_scans(),"sharedSessions":sessions,"worldSamples":worlds,"compassSamples":compasses,
        "waveSamplesDuringRecipePreparation":during_wave,"compassSamplesDuringRecipePreparation":during_compass,
        "firstWorldMs":first_world,"firstCompassMs":first_compass,"recipeCacheDelayMs":cached.load(Ordering::SeqCst).saturating_sub(started.load(Ordering::SeqCst)),
        "meanCompassIntervalMs":gaps.iter().sum::<u64>() as f64/gaps.len().max(1) as f64,"maxCompassIntervalMs":gaps.iter().max(),
        "gameMemoryWrites":false,"assetExtraction":false,"workersJoined":true});
    std::fs::write(
        directory.join("feature-workers-validation.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{report}");
    Ok(())
}
