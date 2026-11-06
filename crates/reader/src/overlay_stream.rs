//! Long-lived feature workers with shared discovery and bounded UI publication.
mod diagnostics;
mod workers;
use crate::{
    Cache, RuntimeCache, cache_matches, discover, live_context, platform, sample, sample_context,
};
use anyhow::{Context, Result, ensure};
pub use diagnostics::self_test as self_test_workers;
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
use workers::{EpochControl, Hub, Role};
// Outside-station clearing and an absent station target have different shortcut
// semantics: hovering a completed recipe must preserve another pinned recipe.
enum RecipeRead {
    OutsideStation,
    Research(Option<starrupture_observation_model::recipe::Snapshot>),
}
impl RecipeRead {
    fn apply(
        self,
        control: &starrupture_observation_model::recipe::Control,
        request: starrupture_observation_model::recipe::Request,
        selection: bool,
    ) {
        match self {
            Self::OutsideStation => control.clear_requested(request),
            Self::Research(snapshot) if selection => control.captured(request, snapshot),
            Self::Research(snapshot) => control.publish(request, snapshot, String::new()),
        }
    }
}
pub(crate) fn clock_ms() -> u64 {
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
            sampled_at_unix_ms: clock_ms(),
            process: self.process.clone(),
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
    base: Arc<RuntimeCache>,
    epoch: u64,
    live: live_context::resident::Resident,
}
/// Process presence is independent of whether a world can currently be sampled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GameStatus {
    Waiting,
    FeaturesDisabled {
        pid: u32,
        executable: String,
    },
    Running {
        pid: u32,
        creation_ticks: u64,
        executable: String,
        supported: bool,
    },
    Unknown,
}
/// OS process presence only: no memory-reading handle, executable hash or native discovery.
pub fn presence_without_features() -> GameStatus {
    platform::presence_without_features().unwrap_or(GameStatus::Unknown)
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
        supported: true,
    });

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
        base: Arc::new(base),
        epoch: 0,
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
    sink: impl FnMut(Message) -> Result<()>,
    status: impl FnMut(GameStatus),
    tracker: Option<crate::recipe::Tracker>,
    setup: Option<Box<dyn crate::recipe::AssetSetup>>,
) -> Result<()> {
    run_with_navigation(directory, stop, sink, status, tracker, setup, None)
}
pub fn run_with_navigation(
    directory: &Path,
    stop: Arc<StopSignal>,
    sink: impl FnMut(Message) -> Result<()>,
    status: impl FnMut(GameStatus),
    tracker: Option<crate::recipe::Tracker>,
    setup: Option<Box<dyn crate::recipe::AssetSetup>>,
    navigation: Option<Arc<starrupture_observation_model::compass::Control>>,
) -> Result<()> {
    run_with_features(
        directory,
        stop,
        sink,
        status,
        Features {
            wave: true,
            tracker,
            setup,
            navigation,
        },
    )
}
/// Requested feature resources. Absent features do not create workers or load assets.
pub struct Features {
    pub wave: bool,
    pub tracker: Option<crate::recipe::Tracker>,
    pub setup: Option<Box<dyn crate::recipe::AssetSetup>>,
    pub navigation: Option<Arc<starrupture_observation_model::compass::Control>>,
}
pub fn run_with_features(
    directory: &Path,
    stop: Arc<StopSignal>,
    sink: impl FnMut(Message) -> Result<()>,
    status: impl FnMut(GameStatus),
    features: Features,
) -> Result<()> {
    workers::run(directory, stop, sink, status, features)
}
async fn attach_shared(hub: &Hub, budget: Arc<platform::ReadBudget>) -> Result<Session> {
    let seed = hub.seed().context(crate::error::WorldNotLoaded)?;
    let process = platform::reopen(&seed.base.identity, budget)?;
    let live = live_context::resident::Resident::from_record(
        &process,
        &seed.base,
        seed.live.clone(),
        false,
    )
    .await?;
    ensure!(
        live.matches_proof(&seed.proof) && hub.current(seed.epoch),
        "Shared allocation/session changed during feature attach"
    );
    Ok(Session {
        process,
        base: seed.base.clone(),
        live,
        epoch: seed.epoch,
    })
}
struct Feature {
    wave: bool,
    role: Role,
    hub: Arc<Hub>,
    tracker: Option<crate::recipe::Tracker>,
    setup: Option<Box<dyn crate::recipe::AssetSetup>>,
    navigation: Option<Arc<starrupture_observation_model::compass::Control>>,
}
fn run_feature(
    feature: Feature,
    directory: &Path,
    stop: Arc<StopSignal>,
    mut sink: impl FnMut(Message) -> Result<()>,
    mut status: impl FnMut(GameStatus),
) -> Result<()> {
    let Feature {
        wave,
        role,
        hub,
        mut tracker,
        mut setup,
        navigation,
    } = feature;
    ensure!(!platform::is_elevated()?, "Do not run overlay elevated");
    ensure!(directory.is_absolute(), "Absolute private cache directory required");
    let cadence = crate::cadence::Cadence::new(50, 250).map_err(anyhow::Error::msg)?;
    let budget = platform::ReadBudget::new(stop.clone());
    let navigation_wait = navigation.as_ref().map(|_| platform::ShortWait::new());
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
        let mut compass_due = Instant::now();
        let mut compass_binding = None;
        let mut compass_attempted = false;
        let mut compass_error = None;
        let mut last_menu = None;
        let mut menu_sent = Instant::now();
        let mut presence_due = Instant::now();
        let mut asset_setup_attempted = false;
        let mut recipe_binding = None;
        let mut identity_binding = None;
        let mut identity_due = Instant::now();
        let mut identity_attempts = 0;
        let mut identity_error = None;
        let mut recipe_setup_attempts = 0u8;
        let mut recipe_setup_retry = Instant::now();

        let mut recipe_generation = 0;
        let mut recipe_due = Instant::now();
        while !stop.is_requested() {
            if let (Some(t), Some(s)) = (&tracker, &session)
                && let Some(Err(e)) = hub.recipes(s.epoch, |r| r.checkpoint(&t.control))
            {
                eprintln!("Recipe choices could not be remembered: {e:#}");
            }
            if role != Role::Connection && session.as_ref().is_some_and(|s| !hub.current(s.epoch)) {
                session = None;
                retry = Instant::now();
            }
            budget.renew(5000);
            if role == Role::Connection
                && Instant::now() >= presence_due
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
                let attached = if role == Role::Connection {
                    attach(directory, &mut retained, budget.clone(), &mut status).await
                } else {
                    attach_shared(&hub, budget.clone()).await
                };
                match attached {
                    Ok(mut s) => {
                        if role == Role::Connection {
                            s.epoch =
                                hub.activate(s.base.clone(), s.live.record(), s.live.proof())?;
                        }
                        output.generation = s.epoch;
                        output.process = Some(GameIdentity {
                            pid: s.process.identity.pid,
                            creation_ticks: s.process.identity.creation_ticks,
                            executable_sha256: s.process.identity.executable_sha256.clone().into(),
                        });
                        asset_setup_attempted = false;
                        sessions += 1;
                        failures = 0;
                        recipe_binding = None;
                        compass_binding = None;
                        compass_attempted = false;
                        compass_due = Instant::now();
                        session = Some(s);
                        recipe_setup_attempts = 0;
                        recipe_setup_retry = Instant::now();
                        identity_binding = None;
                        identity_attempts = 0;
                        identity_due = Instant::now();
                        identity_error = None;
                        output.send(Payload::Ready)?;
                        last_menu = None;
                        pause_due = Instant::now();
                        world_due = Instant::now();
                    }
                    Err(e) => {
                        if e.is::<crate::error::WorldNotLoaded>() {
                            output.send(Payload::NoWorld)?;
                        } else {
                            output.send(Payload::Unavailable(
                                format!("{e:#}").chars().take(240).collect(),
                            ))?;
                        }
                        failures = (failures + 1).min(3);
                        retry = Instant::now()
                            + if role == Role::Connection {
                                Duration::from_secs(1 << failures)
                            } else {
                                Duration::from_millis(100)
                            };
                        continue;
                    }
                }
            }
            if navigation.as_ref().is_some_and(|c| c.should_prepare()) && !compass_attempted {
                compass_attempted = true;
                budget.renew(60000);
                let ctx = session.as_ref().unwrap().live.context();
                let started = Instant::now();
                match crate::compass::Binding::discover(
                    &ctx,
                    &session.as_ref().unwrap().base,
                    session.as_ref().unwrap().live.world_address(),
                )
                .await
                {
                    Ok(b) => {
                        eprintln!(
                            "Compass bindings prepared in {} ms.",
                            started.elapsed().as_millis()
                        );
                        compass_binding = Some(b);
                    }
                    Err(e) => eprintln!("Compass setup unavailable: {e:#}"),
                }
            }
            if Instant::now() >= compass_due {
                budget.renew(100);
                let value = if navigation.as_ref().is_some_and(|c| c.enabled()) {
                    if let Some(binding) = &mut compass_binding {
                        let s = session.as_mut().unwrap();
                        match s.live.compass(&s.process, &s.base, binding).await {
                            Ok(value) if value.valid(clock_ms()) => {
                                compass_error = None;
                                Some(value)
                            }
                            result => {
                                let reason = match result {
                                    Err(e) => format!("{e:#}"),
                                    Ok(_) => "Navigation read exceeded freshness bound".into(),
                                };
                                if compass_error.as_ref() != Some(&reason) {
                                    eprintln!("Compass observation unavailable: {reason}");
                                    compass_error = Some(reason);
                                }
                                None
                            }
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };
                if navigation.is_some() {
                    output.send(Payload::Compass(value))?;
                }
                compass_due = Instant::now() + Duration::from_millis(33);
            }
            budget.renew(5000);
            let s = session.as_mut().unwrap();
            let result: Result<()> = if role == Role::Connection {
                async {
                    if Instant::now() >= pause_due {
                        let started = clock_ms();
                        let clock = Instant::now();
                        if let Some(menu) = s.live.menu(&s.process, &s.base).await? {
                            menu_reads += 1;
                            if last_menu != Some(menu)
                                || menu_sent.elapsed() >= Duration::from_millis(250)
                            {
                                output.send(Payload::Menu(MenuSample {
                                    menu_open: menu,
                                    started_ms: started,
                                    finished_ms: clock_ms(),
                                    guards_validated: true,
                                }))?;
                                last_menu = Some(menu);
                                menu_sent = Instant::now();
                            }
                        }
                        menu_max_micros = menu_max_micros.max(clock.elapsed().as_micros());
                        pause_due = Instant::now() + cadence.menu;
                    }
                    if wave && Instant::now() >= world_due {
                        let cycle = Instant::now();
                        let started = clock_ms();
                        sample_context(&s.process, &s.base, s.live.context()).await?;
                        let live = s.live.snapshot(&s.process, &s.base).await?;
                        let mut sample =
                            sample_context(&s.process, &s.base, s.live.context()).await?;
                        sample.observed_at_ms = clock_ms();
                        let process =
                            output.process.clone().context("Missing active process identity")?;
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
                .await
            } else {
                Ok(())
            };
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
                    if let Some(cached) =
                        preparation.cached(executable, &s.process.identity.executable_sha256)?
                    {
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
                    preparation.complete(
                        executable,
                        &s.process.identity.executable_sha256,
                        &mappings,
                        &stop,
                    )
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
                hub.register_control(s.epoch, &t.control);
                let control = EpochControl {
                    hub: &hub,
                    epoch: s.epoch,
                    inner: &t.control,
                };
                let request = control.request();
                // Recovery belongs to connection setup, never to a shortcut or
                // menu opening. Limit retries instead of repeatedly rescanning.
                if recipe_binding.is_none()
                    && recipe_setup_attempts < 3
                    && request.cursor.is_none()
                    && Instant::now() >= recipe_setup_retry
                {
                    recipe_setup_attempts += 1;
                    budget.renew(60000);
                    let began = Instant::now();
                    let scans_before = crate::recipe::registry_scans();
                    match crate::recipe::Binding::from_lookup(
                        &s.live.context(),
                        &s.base,
                        &hub.definitions,
                    )
                    .await
                    {
                        Ok(binding) => {
                            recipe_binding = Some(binding);
                            if let Some(preparation) = setup.as_mut() {
                                preparation.ready();
                            }
                            eprintln!(
                                "Recipe connection setup recovered in {} ms ({} registry recovery scans).",
                                began.elapsed().as_millis(),
                                crate::recipe::registry_scans() - scans_before
                            );
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
                if recipe_binding.is_some()
                    && !hub.recipes(s.epoch, |r| r.bound()).unwrap_or(true)
                    && Instant::now() >= identity_due
                {
                    identity_due = Instant::now() + Duration::from_secs(5);
                    budget.renew(60000);
                    let result: Result<_> = async {
                        s.live.menu(&s.process, &s.base).await?;
                        if identity_binding.is_none() && identity_attempts < 3 {
                            identity_attempts += 1;
                            identity_binding = Some(
                                crate::recipe_pins::Binding::discover(
                                    &s.live.context(),
                                    &s.base,
                                    s.live.world_address(),
                                )
                                .await?,
                            );
                        }
                        let identity = identity_binding
                            .as_ref()
                            .context("Recipe persistence identity setup unavailable")?;
                        let world = identity
                            .current(
                                &s.live.context(),
                                s.live.world_address(),
                                Path::new(&s.process.identity.executable),
                            )
                            .await?;
                        s.live.menu(&s.process, &s.base).await?;
                        Ok(world)
                    }
                    .await;
                    match result {
                        Ok(world) => {
                            if let Some(result) = hub.recipes(s.epoch, |r| r.begin(world)) {
                                result?;
                            }
                            identity_error = None;
                        }
                        Err(e) => {
                            let message = format!("{e:#}");
                            if identity_error.as_ref() != Some(&message) {
                                eprintln!("Recipe persistence unavailable: {message}");
                                identity_error = Some(message);
                            }
                        }
                    }
                }
                if let Some((restore_request, ids)) =
                    hub.recipes(s.epoch, |r| r.desired(&t.control)).flatten()
                    && let Some(binding) = &recipe_binding
                {
                    budget.renew(5000);
                    let result: Result<_> = async {
                        let mut snapshots = Vec::new();
                        for id in ids {
                            if !t.catalogue.recipes.contains_key(&id) {
                                continue;
                            }
                            s.live.menu(&s.process, &s.base).await?;
                            if let Some(snapshot) = binding
                                .sample(
                                    &s.live.context(),
                                    &s.base,
                                    s.live.world_address(),
                                    &id,
                                    &t.catalogue,
                                    restore_request.generation,
                                )
                                .await?
                            {
                                snapshots.push(snapshot);
                            }
                            s.live.menu(&s.process, &s.base).await?;
                        }
                        Ok(snapshots)
                    }
                    .await;
                    match result {
                        Ok(snapshots) => {
                            hub.recipes(s.epoch, |r| {
                                r.restored(&t.control, restore_request, snapshots)
                            });
                        }
                        Err(e) => eprintln!("Recipe restoration deferred: {e:#}"),
                    }
                }
                if request.active && request.cursor.is_some() && Instant::now() >= recipe_due {
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
                                return Ok(RecipeRead::OutsideStation);
                            }
                        }
                        let id = if selection {
                            crate::recipe::cursor_matches(request.cursor, s.process.identity.pid)?;
                            let id = if request.clear_outside {
                                binding.capture(&ctx, &s.base, &t.catalogue).await?
                            } else {
                                binding.capture_mouse(&ctx, &s.base, &t.catalogue).await?
                            };
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
                        Ok(RecipeRead::Research(sample))
                    }
                    .await;
                    if selection {
                        eprintln!(
                            "Recipe shortcut read completed in {} ms.",
                            started.elapsed().as_millis()
                        );
                        match read {
                            Ok(read) => {
                                hub.with_epoch(s.epoch, || read.apply(&t.control, request, true));
                            }
                            Err(e) => {
                                eprintln!("Recipe shortcut ignored: {e:#}");
                                control.captured(request, None);
                            }
                        }
                    } else {
                        match read {
                            Ok(read) => {
                                hub.with_epoch(s.epoch, || read.apply(&t.control, request, false));
                            }
                            Err(e) => control
                                .unavailable(request, format!("{e:#}").chars().take(220).collect()),
                        }
                    }
                    // Selection is settled immediately, including invalid/completed targets.
                    recipe_due = Instant::now();
                } else if request.active && Instant::now() >= recipe_due {
                    budget.renew(5000);
                    for view in t.control.views() {
                        let Some(snapshot) = view.snapshot else {
                            continue;
                        };
                        if t.control.request() != request {
                            break;
                        }
                        let id = snapshot.recipe_id;
                        let read: Result<_> = async {
                            s.live.menu(&s.process, &s.base).await?;
                            let ctx = s.live.context();
                            let binding = recipe_binding
                                .as_ref()
                                .context("Recipe connection is unavailable")?;
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
                            s.live.menu(&s.process, &s.base).await?;
                            Ok(sample)
                        }
                        .await;
                        match read {
                            Ok(sample) => control.publish_for(request, &id, sample, String::new()),
                            Err(error) => control.unavailable_for(
                                request,
                                &id,
                                format!("{error:#}").chars().take(220).collect(),
                            ),
                        }
                    }
                    recipe_due = Instant::now() + Duration::from_millis(500);
                }
            }
            let navigation_active = navigation.as_ref().is_some_and(|c| c.enabled());
            let deadline = match role {
                Role::Connection if wave => pause_due.min(world_due),
                Role::Connection => pause_due,
                Role::Compass if navigation_active => compass_due,
                Role::Compass => Instant::now() + Duration::from_millis(100),
                Role::Recipe => Instant::now() + Duration::from_millis(50),
            };
            if navigation_active && let Some(wait) = &navigation_wait {
                wait.until(&stop, deadline);
            } else {
                stop.wait_until(deadline);
            }
        }
        Ok::<(), anyhow::Error>(())
    })?;
    eprintln!(
        "Overlay reader stopped: {sessions} sessions, {menu_reads} menu reads, {world_reads} world reads"
    );
    Ok(())
}

#[cfg(test)]
mod recipe_read_tests {
    use super::*;
    use starrupture_observation_model::recipe::{Control, Ingredient, Snapshot};
    #[test]
    fn absent_hover_preserves_pin_but_absent_poll_and_outside_shortcut_clear() {
        let control = Control::default();
        let pin = || Snapshot {
            recipe_id: "A".into(),
            ingredients: vec![Ingredient {
                item_id: "item".into(),
                submitted: 0,
                required: 1,
                remaining: 1,
                bag_count: None,
            }],
            observed_at_ms: 1000,
            generation: 0,
            process: 1,
        };
        control.toggle_at(Some([20, 30]));
        control.captured(control.request(), Some(pin()));
        control.toggle_at(Some([20, 30]));
        RecipeRead::Research(None).apply(&control, control.request(), true);
        assert_eq!(control.view().snapshot.unwrap().recipe_id, "A");
        RecipeRead::Research(None).apply(&control, control.request(), false);
        assert!(control.view().snapshot.is_none());
        assert!(!control.request().active);
        control.toggle_at(Some([20, 30]));
        control.captured(control.request(), Some(pin()));
        control.toggle_at(Some([20, 30]));
        RecipeRead::OutsideStation.apply(&control, control.request(), true);
        assert!(control.view().snapshot.is_none());
        assert!(!control.request().active);
    }
}
