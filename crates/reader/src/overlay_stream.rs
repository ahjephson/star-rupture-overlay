//! Embedded read-only engine on the overlay worker thread.
use crate::{
    BUILD_HASH, Cache, RuntimeCache, cache_matches, discover, live_context, platform, sample,
    sample_context,
};
use anyhow::{Context, Result, ensure};
use starrupture_observation_model::overlay_stream::{
    GameIdentity, MenuSample, Message, Payload, Statistics, WorldSample,
};
use starrupture_observation_model::shutdown::StopSignal;
use starrupture_observation_model::{READER_CONTRACT_VERSION, observation::RawObservation};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}
struct Output<'a> {
    sink: &'a mut dyn FnMut(Message) -> Result<()>,
    sequence: u64,
    generation: u64,
    process: Option<GameIdentity>,
}
impl Output<'_> {
    fn send(&mut self, payload: Payload) -> Result<()> {
        self.sequence = self.sequence.checked_add(1).context("Stream sequence exhausted")?;
        let message = Message {
            version: 1,
            reader_pid: std::process::id(),
            sequence: self.sequence,
            generation: self.generation,
            sampled_at_unix_ms: now(),
            process: self.process,
            read_only: true,
            game_code_invoked: false,
            payload,
        };
        ensure!(message.within_bound(), "Stream message bound");
        (self.sink)(message)?;
        Ok(())
    }
}
struct Session {
    process: platform::Process,
    base: RuntimeCache,
    live: live_context::resident::Resident,
}
/// Process presence is independent of whether a world can currently be sampled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GameStatus {
    Waiting,
    Running {
        pid: u32,
        creation_ticks: u64,
        executable: String,
        supported: bool,
    },
    Unknown,
}
async fn attach(
    directory: &Path,
    retained: &mut Option<platform::Process>,
    budget: Arc<platform::ReadBudget>,
    status: &mut impl FnMut(GameStatus),
) -> Result<Session> {
    if retained.is_none() {
        match platform::find_game(budget) {
            Ok(process) => *retained = Some(process),
            Err(error) => {
                status(if error.is::<platform::GameNotRunning>() {
                    GameStatus::Waiting
                } else {
                    GameStatus::Unknown
                });
                return Err(error);
            }
        }
    }
    let process = retained.as_ref().unwrap().clone();
    status(GameStatus::Running {
        pid: process.identity.pid,
        creation_ticks: process.identity.creation_ticks,
        executable: process.identity.executable.clone(),
        supported: process.identity.executable_sha256 == BUILD_HASH,
    });
    ensure!(process.identity.executable_sha256 == BUILD_HASH, "Unsupported game executable");
    let path = directory.join("session.json");
    let cached = crate::cache::read_json::<Cache>(&path, 100000)
        .ok()
        .filter(|c| cache_matches(c, &process.identity));
    let base = if let Some(c) = cached {
        let c = RuntimeCache::new(c)?;
        if sample(&process, &c).await.is_ok() {
            c
        } else {
            RuntimeCache::new(discover(&process).await?)?
        }
    } else {
        RuntimeCache::new(discover(&process).await?)?
    };
    sample(&process, &base).await?;
    let temporary = directory.join(format!("resident-session-{}.tmp", std::process::id()));
    std::fs::write(&temporary, serde_json::to_vec(&base.record)?)?;
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    std::fs::rename(temporary, &path)?;
    let live = live_context::resident::Resident::open(&process, &base, directory).await?;
    Ok(Session {
        process,
        base,
        live,
    })
}
pub fn run(
    directory: &Path,
    stop: Arc<StopSignal>,
    sink: impl FnMut(Message) -> Result<()>,
) -> Result<()> {
    run_with_status(directory, stop, sink, |_| {})
}
pub fn run_with_status(
    directory: &Path,
    stop: Arc<StopSignal>,
    sink: impl FnMut(Message) -> Result<()>,
    status: impl FnMut(GameStatus),
) -> Result<()> {
    run_with_tracker(directory, stop, sink, status, None)
}
pub fn run_with_tracker(
    directory: &Path,
    stop: Arc<StopSignal>,
    sink: impl FnMut(Message) -> Result<()>,
    status: impl FnMut(GameStatus),
    tracker: Option<crate::recipe::Tracker>,
) -> Result<()> {
    run_with_assets(directory, stop, sink, status, tracker, None)
}
pub fn run_with_assets(
    directory: &Path,
    stop: Arc<StopSignal>,
    mut sink: impl FnMut(Message) -> Result<()>,
    mut status: impl FnMut(GameStatus),
    mut tracker: Option<crate::recipe::Tracker>,
    mut setup: Option<Box<dyn crate::recipe::AssetSetup>>,
) -> Result<()> {
    ensure!(!platform::is_elevated()?, "Do not run overlay elevated");
    ensure!(directory.is_absolute(), "Absolute private cache directory required");
    let cadence = crate::cadence::Cadence::new(50, 250).map_err(anyhow::Error::msg)?;
    let budget = platform::ReadBudget::new(stop.clone());
    std::fs::create_dir_all(directory)?;
    jmap_dumper::diag::set_verbosity(jmap_dumper::diag::Verbosity::Quiet);
    let mut output = Output {
        sink: &mut sink,
        sequence: 0,
        generation: 0,
        process: None,
    };
    let mut retained: Option<platform::Process> = None;
    let mut sessions = 0u64;
    let mut menu_reads = 0u64;
    let mut world_reads = 0u64;
    let mut menu_max_micros = 0u128;
    smol::block_on(async {
        let mut session: Option<Session> = None;
        let mut retry = Instant::now();
        let mut failures = 0u32;
        let mut pause_due = Instant::now();
        let mut world_due = Instant::now();
        let mut last_menu = None;
        let mut menu_sent = Instant::now();
        let mut presence_due = Instant::now();
        let mut asset_setup_attempted = false;
        let mut recipe_binding = None;
        let mut recipe_setup_attempts = 0u8;
        let mut recipe_setup_retry = Instant::now();

        let mut recipe_generation = 0;
        let mut recipe_due = Instant::now();
        while !stop.is_requested() {
            budget.renew(5000);
            if Instant::now() >= presence_due
                && let Some(process) = &retained
            {
                presence_due = Instant::now() + Duration::from_secs(1);
                match process.has_exited() {
                    Ok(true) => {
                        // Only a successful OS query confirming exit clears process presence.
                        status(GameStatus::Waiting);
                        retained = None;
                        session = None;
                        output.send(Payload::Unavailable("Game process exited".into()))?;
                        retry = Instant::now();
                    }
                    Ok(false) => {}
                    Err(_) => status(GameStatus::Unknown),
                }
            }
            if session.is_none() {
                if Instant::now() < retry {
                    stop.wait_until(retry);
                    continue;
                }
                budget.renew(60000);
                match attach(directory, &mut retained, budget.clone(), &mut status).await {
                    Ok(s) => {
                        output.generation =
                            output.generation.checked_add(1).context("Generation exhausted")?;
                        output.process = Some(GameIdentity {
                            pid: s.process.identity.pid,
                            creation_ticks: s.process.identity.creation_ticks,
                            executable_sha256: BUILD_HASH,
                        });
                        asset_setup_attempted = false;
                        sessions += 1;
                        failures = 0;
                        recipe_binding = if tracker.is_some() && setup.is_none() {
                            budget.renew(60000);
                            let started = Instant::now();
                            match crate::recipe::Binding::discover(&s.live.context()).await {
                                Ok(binding) => {
                                    eprintln!(
                                        "Recipe bindings prepared in {} ms.",
                                        started.elapsed().as_millis()
                                    );
                                    Some(binding)
                                }
                                Err(error) => {
                                    eprintln!("Recipe binding preparation unavailable: {error:#}");
                                    None
                                }
                            }
                        } else {
                            None
                        };
                        session = Some(s);
                        recipe_setup_attempts = if tracker.is_some() {
                            1
                        } else {
                            0
                        };
                        recipe_setup_retry = Instant::now() + Duration::from_secs(2);

                        if let Some(t) = &tracker {
                            t.control.clear();
                        }
                        output.send(Payload::Ready)?;
                        last_menu = None;
                        pause_due = Instant::now();
                        world_due = Instant::now();
                    }
                    Err(e) => {
                        output.send(Payload::Unavailable(
                            format!("{e:#}").chars().take(240).collect(),
                        ))?;
                        failures = (failures + 1).min(3);
                        retry = Instant::now() + Duration::from_secs(1 << failures);
                        continue;
                    }
                }
            }
            budget.renew(5000);
            let s = session.as_mut().unwrap();
            let result: Result<()> = async {
                if Instant::now() >= pause_due {
                    let started = now();
                    let clock = Instant::now();
                    if let Some(menu) = s.live.menu(&s.process, &s.base).await? {
                        menu_reads += 1;
                        if last_menu != Some(menu)
                            || menu_sent.elapsed() >= Duration::from_millis(250)
                        {
                            output.send(Payload::Menu(MenuSample {
                                menu_open: menu,
                                started_ms: started,
                                finished_ms: now(),
                                guards_validated: true,
                            }))?;
                            last_menu = Some(menu);
                            menu_sent = Instant::now();
                        }
                    }
                    menu_max_micros = menu_max_micros.max(clock.elapsed().as_micros());
                    pause_due = Instant::now() + cadence.menu;
                }
                if Instant::now() >= world_due {
                    let cycle = Instant::now();
                    let started = now();
                    sample_context(&s.process, &s.base, s.live.context()).await?;
                    let live = s.live.snapshot(&s.process, &s.base).await?;
                    let mut sample = sample_context(&s.process, &s.base, s.live.context()).await?;
                    sample.observed_at_ms = now();
                    let process = output.process.context("Missing active process identity")?;
                    output.send(Payload::World(Box::new(WorldSample {
                        read_started_unix_ms: started as i64,
                        raw: RawObservation {
                            schema_version: 1,
                            contract_version: READER_CONTRACT_VERSION,
                            source_is_live_memory: true,
                            game_memory_writes: false,
                            process,
                            sample,
                            live,
                        },
                        statistics: Statistics {
                            menu_reads,
                            world_reads: world_reads + 1,
                            sessions,
                            world_read_millis: cycle.elapsed().as_millis() as u64,
                            menu_max_micros: menu_max_micros as u64,
                        },
                    })))?;
                    world_reads += 1;
                    world_due = cycle + cadence.world;
                }
                Ok(())
            }
            .await;
            if let Err(e) = result {
                if crate::error::is_retryable(&e) {
                    // Discard a torn wave read, preserving no new timestamp. The
                    // retained world/allocation guards are rechecked next cycle.
                    world_due = Instant::now() + Duration::from_millis(50);
                    continue;
                }
                output.send(Payload::Unavailable(format!("{e:#}").chars().take(240).collect()))?;
                session = None;
                last_menu = None;
                retry = Instant::now() + Duration::from_secs(1);
            }
            if setup.as_ref().is_some_and(|preparation| !preparation.enabled()) {
                asset_setup_attempted = false;
            }
            if !asset_setup_attempted
                && let (Some(preparation), Some(s)) = (setup.as_mut(), session.as_ref())
                && preparation.enabled()
            {
                asset_setup_attempted = true;
                let prepared: Result<_> = async {
                    let executable = Path::new(&s.process.identity.executable);
                    if let Some(cached) = preparation.cached(executable)? {
                        return Ok(cached);
                    }
                    preparation.preparing();
                    budget.renew(120000);
                    let started = Instant::now();
                    let mappings = crate::recipe::asset_mappings(&s.live.context()).await?;
                    eprintln!(
                        "Recipe asset schema prepared in {} ms.",
                        started.elapsed().as_millis()
                    );
                    preparation.complete(executable, &mappings, &stop)
                }
                .await;
                match prepared {
                    Ok(ready) => {
                        tracker = Some(ready);
                        recipe_binding = None;
                        recipe_setup_attempts = 0;
                        recipe_setup_retry = Instant::now();
                    }
                    Err(error) => {
                        preparation.failed(&format!("{error:#}"));
                        tracker = None;
                        eprintln!("Recipe setup unavailable: {error:#}");
                    }
                }
            }
            if let (Some(t), Some(s)) = (&tracker, session.as_mut()) {
                let request = t.control.request();
                // Recovery belongs to connection setup, never to a shortcut or
                // menu opening. Limit retries instead of repeatedly rescanning.
                if recipe_binding.is_none()
                    && recipe_setup_attempts < 3
                    && request.cursor.is_none()
                    && Instant::now() >= recipe_setup_retry
                {
                    recipe_setup_attempts += 1;
                    budget.renew(60000);
                    match crate::recipe::Binding::discover(&s.live.context()).await {
                        Ok(binding) => {
                            recipe_binding = Some(binding);
                            if let Some(preparation) = setup.as_mut() {
                                preparation.ready();
                            }
                            eprintln!("Recipe connection setup recovered.");
                        }
                        Err(error) => {
                            eprintln!("Recipe connection setup retry unavailable: {error:#}")
                        }
                    }
                    recipe_setup_retry = Instant::now() + Duration::from_secs(2);
                }
                if request.generation != recipe_generation {
                    recipe_generation = request.generation;
                    recipe_due = Instant::now();
                }
                if request.active && Instant::now() >= recipe_due {
                    budget.renew(5000);
                    let selection = request.cursor.is_some();
                    let started = Instant::now();
                    let read: Result<_> = async {
                        s.live.menu(&s.process, &s.base).await?;
                        let ctx = s.live.context();
                        let binding = recipe_binding.as_ref().context(
                            "Recipe connection setup is unavailable; see the local diagnostic log",
                        )?;
                        if selection {
                            crate::recipe::foreground_matches(s.process.identity.pid)?;
                            if !binding.in_station(&ctx, &s.base).await? {
                                s.live.menu(&s.process, &s.base).await?;
                                anyhow::ensure!(
                                    !binding.in_station(&ctx, &s.base).await?,
                                    "Recipe station opened during clear"
                                );
                                crate::recipe::foreground_matches(s.process.identity.pid)?;
                                return Ok(None);
                            }
                        }
                        let id = if selection {
                            crate::recipe::cursor_matches(request.cursor, s.process.identity.pid)?;
                            let id = binding.capture(&ctx, &s.base, &t.catalogue).await?;
                            crate::recipe::cursor_matches(request.cursor, s.process.identity.pid)?;
                            id
                        } else {
                            t.control.view().snapshot.context("No recipe selected")?.recipe_id
                        };
                        let sample = binding
                            .sample(
                                &ctx,
                                &s.base,
                                s.live.world_address(),
                                &id,
                                &t.catalogue,
                                request.generation,
                            )
                            .await?;
                        if selection {
                            crate::recipe::cursor_matches(request.cursor, s.process.identity.pid)?;
                            anyhow::ensure!(
                                binding.in_station(&ctx, &s.base).await?,
                                "Recipe station closed during capture"
                            );
                        }
                        s.live.menu(&s.process, &s.base).await?;
                        Ok(Some(sample))
                    }
                    .await;
                    if selection {
                        eprintln!(
                            "Recipe shortcut read completed in {} ms.",
                            started.elapsed().as_millis()
                        );
                        match read {
                            Ok(None) => t.control.clear_requested(request),
                            Ok(snapshot) => t.control.captured(request, snapshot),
                            Err(e) => {
                                eprintln!("Recipe shortcut ignored: {e:#}");
                                t.control.captured(request, None);
                            }
                        }
                    } else {
                        match read {
                            Ok(sample) => t.control.publish(request, sample, String::new()),
                            Err(e) => t
                                .control
                                .unavailable(request, format!("{e:#}").chars().take(220).collect()),
                        }
                    }
                    recipe_due = Instant::now() + Duration::from_millis(500);
                }
            }
            stop.wait_until(pause_due.min(world_due));
        }
        Ok::<(), anyhow::Error>(())
    })?;
    eprintln!(
        "Overlay reader stopped: {sessions} sessions, {menu_reads} menu reads, {world_reads} world reads"
    );
    Ok(())
}
