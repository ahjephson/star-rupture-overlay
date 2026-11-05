//! Latest-value, bounded feature mailboxes and one shared discovery/session authority.
use super::*;
use std::sync::Mutex;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Role {
    Wave,
    Compass,
    Recipe,
}
pub(super) struct Seed {
    pub epoch: u64,
    pub base: Arc<RuntimeCache>,
    pub live: Arc<live_context::Bindings>,
    pub proof: Arc<[(crate::ObjectStamp, i32, u64)]>,
}
struct State {
    epoch: u64,
    revision: u64,
    seed: Option<Arc<Seed>>,
    slots: [Option<Payload>; 3],
    invalid: Option<String>,
    no_world: bool,
    status: Option<GameStatus>,
    remembered: crate::recipe_pins::Remembered,
    control: Option<Arc<starrupture_observation_model::recipe::Control>>,
    fatal: Option<String>,
}
pub(super) struct Hub {
    state: Mutex<State>,
    pub definitions: Arc<crate::native_lookup::Lookup>,
}
pub(super) struct Batch {
    pub revision: u64,
    pub seed: Option<Arc<Seed>>,
    pub slots: [Option<Payload>; 3],
    pub invalid: Option<String>,
    pub no_world: bool,
    pub status: Option<GameStatus>,
    pub fatal: Option<String>,
}
impl State {
    fn reset_recipes(&mut self) {
        if let Some(control) = &self.control {
            if let Err(e) = self.remembered.flush(control) {
                eprintln!("Recipe choices could not be remembered: {e:#}");
            }
            control.reset_session();
        }
        self.remembered.detach();
    }
}
impl Hub {
    pub fn new(
        directory: &Path,
        control: Option<Arc<starrupture_observation_model::recipe::Control>>,
    ) -> Self {
        Self {
            state: Mutex::new(State {
                epoch: 0,
                revision: 0,
                seed: None,
                slots: [None, None, None],
                invalid: None,
                no_world: false,
                status: None,
                remembered: crate::recipe_pins::Remembered::new(directory),
                control,
                fatal: None,
            }),
            definitions: Arc::new(crate::native_lookup::Lookup::new(directory)),
        }
    }
    pub fn activate(
        &self,
        base: Arc<RuntimeCache>,
        live: Arc<live_context::Bindings>,
        proof: Vec<(crate::ObjectStamp, i32, u64)>,
    ) -> Result<u64> {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        s.reset_recipes();
        s.epoch = s.epoch.checked_add(1).context("Worker session exhausted")?;
        s.revision = s.revision.checked_add(1).context("Worker revision exhausted")?;
        s.seed = Some(Arc::new(Seed {
            epoch: s.epoch,
            base,
            live,
            proof: proof.into(),
        }));
        s.invalid = None;
        s.no_world = false;
        s.slots = [None, None, None];
        Ok(s.epoch)
    }
    pub fn seed(&self) -> Option<Arc<Seed>> {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).seed.clone()
    }
    pub fn current(&self, epoch: u64) -> bool {
        self.seed().is_some_and(|s| s.epoch == epoch)
    }
    pub fn with_epoch<T>(&self, epoch: u64, f: impl FnOnce() -> T) -> Option<T> {
        let s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if s.seed.as_ref().is_some_and(|v| v.epoch == epoch) {
            Some(f())
        } else {
            None
        }
    }
    pub fn register_control(
        &self,
        epoch: u64,
        control: &Arc<starrupture_observation_model::recipe::Control>,
    ) {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if s.seed.as_ref().is_some_and(|v| v.epoch == epoch)
            && s.control.as_ref().is_none_or(|c| !Arc::ptr_eq(c, control))
        {
            control.reset_session();
            s.control = Some(control.clone());
        }
    }
    pub fn recipes<T>(
        &self,
        epoch: u64,
        f: impl FnOnce(&mut crate::recipe_pins::Remembered) -> T,
    ) -> Option<T> {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if s.seed.as_ref().is_some_and(|v| v.epoch == epoch) {
            Some(f(&mut s.remembered))
        } else {
            None
        }
    }
    pub fn emit(&self, role: Role, message: Message) -> Result<()> {
        ensure!(message.within_bound(), "Feature message bound");
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if message.generation != s.epoch {
            return Ok(());
        }
        match message.payload {
            Payload::Ready => {}
            Payload::Unavailable(reason) if role == Role::Wave => {
                s.reset_recipes();
                s.seed = None;
                s.slots = [None, None, None];
                s.invalid = Some(reason);
                s.no_world = false;
                s.revision = s.revision.checked_add(1).context("Worker revision exhausted")?;
            }
            Payload::NoWorld if role == Role::Wave => {
                s.reset_recipes();
                s.seed = None;
                s.slots = [None, None, None];
                s.invalid = None;
                s.no_world = true;
                s.revision = s.revision.checked_add(1).context("Worker revision exhausted")?;
            }
            payload if s.seed.is_some() => {
                let slot = match (&role, &payload) {
                    (Role::Wave, Payload::Menu(_)) => 0,
                    (Role::Wave, Payload::World(_)) => 1,
                    (Role::Compass, Payload::Compass(_)) => 2,
                    _ => return Ok(()),
                };
                s.slots[slot] = Some(payload);
            }
            _ => {}
        }
        Ok(())
    }
    pub fn status(&self, value: GameStatus) {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).status = Some(value);
    }
    pub fn fail(&self, reason: String) {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).fatal = Some(reason);
    }
    pub fn batch(&self) -> Batch {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        Batch {
            revision: s.revision,
            seed: s.seed.clone(),
            slots: std::mem::take(&mut s.slots),
            invalid: s.invalid.clone(),
            no_world: s.no_world,
            status: s.status.take(),
            fatal: s.fatal.clone(),
        }
    }
    pub fn flush(&self) {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).reset_recipes();
    }
}
/// Publication is atomic with session replacement; late native reads cannot repopulate pins.
pub(super) struct EpochControl<'a> {
    pub hub: &'a Hub,
    pub epoch: u64,
    pub inner: &'a starrupture_observation_model::recipe::Control,
}
impl EpochControl<'_> {
    pub fn request(&self) -> starrupture_observation_model::recipe::Request {
        self.hub.with_epoch(self.epoch, || self.inner.request()).unwrap_or_default()
    }
    pub fn captured(
        &self,
        r: starrupture_observation_model::recipe::Request,
        s: Option<starrupture_observation_model::recipe::Snapshot>,
    ) {
        self.hub.with_epoch(self.epoch, || self.inner.captured(r, s));
    }
    pub fn unavailable(&self, r: starrupture_observation_model::recipe::Request, m: String) {
        self.hub.with_epoch(self.epoch, || self.inner.unavailable(r, m));
    }
    pub fn publish_for(
        &self,
        r: starrupture_observation_model::recipe::Request,
        id: &str,
        s: Option<starrupture_observation_model::recipe::Snapshot>,
        m: String,
    ) {
        self.hub.with_epoch(self.epoch, || self.inner.publish_for(r, id, s, m));
    }
    pub fn unavailable_for(
        &self,
        r: starrupture_observation_model::recipe::Request,
        id: &str,
        m: String,
    ) {
        self.hub.with_epoch(self.epoch, || self.inner.unavailable_for(r, id, m));
    }
}

pub(super) struct Group {
    stop: Arc<StopSignal>,
    handles: Vec<std::thread::JoinHandle<()>>,
}
impl Group {
    fn new(stop: Arc<StopSignal>) -> Self {
        Self {
            stop,
            handles: Vec::new(),
        }
    }
    fn spawn(
        &mut self,
        role: Role,
        directory: &Path,
        hub: Arc<Hub>,
        tracker: Option<crate::recipe::Tracker>,
        setup: Option<Box<dyn crate::recipe::AssetSetup>>,
        navigation: Option<Arc<starrupture_observation_model::compass::Control>>,
    ) -> Result<()> {
        let directory = directory.to_path_buf();
        let stop = self.stop.clone();
        self.handles.push(std::thread::Builder::new().name(format!("overlay-{role:?}")).spawn(
            move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    super::run_feature(
                        super::Feature {
                            role,
                            hub: hub.clone(),
                            tracker,
                            setup,
                            navigation,
                        },
                        &directory,
                        stop.clone(),
                        |message| hub.emit(role, message),
                        |value| {
                            if role == Role::Wave {
                                hub.status(value)
                            }
                        },
                    )
                }));
                if !stop.is_requested() {
                    match result {
                        Ok(Ok(())) => {}
                        Ok(Err(e)) => hub.fail(format!("{role:?} worker: {e:#}")),
                        Err(_) => hub.fail(format!("{role:?} worker failed; no fallback")),
                    }
                }
                eprintln!("{role:?} feature worker stopped.");
            },
        )?);
        Ok(())
    }
    fn join(&mut self) -> Result<()> {
        self.stop.request();
        let mut panicked = false;
        for handle in self.handles.drain(..) {
            panicked |= handle.join().is_err();
        }
        ensure!(!panicked, "Feature worker join failed");
        Ok(())
    }
}
impl Drop for Group {
    fn drop(&mut self) {
        let _ = self.join();
    }
}
pub(super) fn run(
    directory: &Path,
    stop: Arc<StopSignal>,
    mut sink: impl FnMut(Message) -> Result<()>,
    mut status: impl FnMut(GameStatus),
    tracker: Option<crate::recipe::Tracker>,
    setup: Option<Box<dyn crate::recipe::AssetSetup>>,
    navigation: Option<Arc<starrupture_observation_model::compass::Control>>,
) -> Result<()> {
    ensure!(!platform::is_elevated()?, "Do not run overlay elevated");
    ensure!(directory.is_absolute(), "Absolute private cache directory required");
    std::fs::create_dir_all(directory)?;
    let hub = Arc::new(Hub::new(directory, tracker.as_ref().map(|t| t.control.clone())));
    let mut group = Group::new(stop.clone());
    group.spawn(Role::Wave, directory, hub.clone(), None, None, None)?;
    if navigation.is_some() {
        group.spawn(Role::Compass, directory, hub.clone(), None, None, navigation)?;
    }
    if tracker.is_some() || setup.is_some() {
        group.spawn(Role::Recipe, directory, hub.clone(), tracker, setup, None)?;
    }
    let mut output = Output {
        sink: &mut sink,
        sequence: 0,
        generation: 0,
        process: None,
    };
    let mut revision = 0;
    let result = (|| -> Result<()> {
        while !stop.is_requested() {
            let batch = hub.batch();
            if let Some(error) = batch.fatal {
                anyhow::bail!(error);
            }
            if let Some(value) = batch.status {
                status(value);
            }
            if batch.revision != revision {
                revision = batch.revision;
                if let Some(seed) = &batch.seed {
                    output.generation = seed.epoch;
                    output.process = Some(GameIdentity {
                        pid: seed.base.identity.pid,
                        creation_ticks: seed.base.identity.creation_ticks,
                        executable_sha256: seed.base.identity.executable_sha256.clone().into(),
                    });
                    output.send(Payload::Ready)?;
                } else if batch.no_world {
                    output.send(Payload::NoWorld)?;
                } else if let Some(reason) = batch.invalid {
                    output.send(Payload::Unavailable(reason))?;
                }
            }
            if let Some(seed) = &batch.seed {
                for payload in batch.slots.into_iter().flatten() {
                    if hub.current(seed.epoch) {
                        output.send(payload)?;
                    }
                }
            }
            stop.wait_until(Instant::now() + Duration::from_millis(10));
        }
        Ok(())
    })();
    let joined = group.join();
    hub.flush();
    result.and(joined)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn metadata() -> (Arc<RuntimeCache>, Arc<live_context::Bindings>) {
        let id = crate::Identity {
            pid: 12,
            creation_ticks: 42,
            executable: "game.exe".into(),
            executable_sha256: starrupture_observation_model::overlay_stream::GAME_HASH.into(),
        };
        let stamp = crate::ObjectStamp {
            address: 65536,
            index: 1,
            class: 65536,
            name: "synthetic".into(),
        };
        let base = RuntimeCache::new(Cache {
            version: 1,
            identity: id.clone(),
            config: jmap_dumper::Config {
                guobject_array: 1,
                fname_pool: 2,
                engine_version: (5, 6),
                image_base: 0,
                build_change_list: None,
                build_config: Default::default(),
                target_triplet: jmap_dumper::structs::default_target_triplet(),
            },
            controller: stamp.clone(),
            offsets: crate::FIELDS.iter().map(|(c, f, _)| (crate::key(c, f), 320)).collect(),
        })
        .unwrap();
        let live =
            serde_json::from_value(serde_json::json!({"version":2,"identity":id,"world":stamp,
            "timer_subsystem":stamp,"wave_subsystem":stamp,"offsets":{},"masks":{},"getters":{}}))
            .unwrap();
        (Arc::new(base), Arc::new(live))
    }
    fn message(epoch: u64, payload: Payload) -> Message {
        Message {
            version: 1,
            reader_pid: std::process::id(),
            sequence: 1,
            generation: epoch,
            sampled_at_unix_ms: clock_ms(),
            process: None,
            read_only: true,
            game_code_invoked: false,
            payload,
        }
    }
    fn compass(heading: f64) -> Payload {
        Payload::Compass(Some(starrupture_observation_model::compass::Sample {
            heading,
            started_ms: clock_ms(),
            finished_ms: clock_ms(),
            pin: None,
            pin_known: true,
            menu_open: false,
        }))
    }
    #[test]
    fn mailboxes_are_bounded_latest_values_and_reject_old_worlds() {
        let hub = Hub::new(Path::new("unused"), None);
        let (base, live) = metadata();
        let epoch = hub.activate(base.clone(), live.clone(), Vec::new()).unwrap();
        assert!(Arc::ptr_eq(&hub.seed().unwrap().base, &base));
        assert!(Arc::ptr_eq(&hub.seed().unwrap().live, &live));
        for i in 0..10000 {
            hub.emit(Role::Compass, message(epoch, compass((i % 360) as f64))).unwrap();
        }
        let batch = hub.batch();
        assert_eq!(batch.slots.iter().flatten().count(), 1);
        assert!(
            matches!(batch.slots[2], Some(Payload::Compass(Some(s))) if s.heading == (9999%360) as f64)
        );
        let next = hub.activate(base, live, Vec::new()).unwrap();
        hub.emit(Role::Compass, message(epoch, compass(20.))).unwrap();
        hub.emit(Role::Wave, message(epoch, Payload::Unavailable("old error".into()))).unwrap();
        assert!(hub.current(next));
        assert!(hub.batch().slots.iter().all(Option::is_none));
        hub.emit(Role::Wave, message(next, Payload::NoWorld)).unwrap();
        assert!(hub.seed().is_none());
        hub.emit(Role::Compass, message(next, compass(30.))).unwrap();
        assert!(hub.batch().slots.iter().all(Option::is_none));
    }
    #[test]
    fn old_recipe_completion_cannot_repopulate_a_replaced_session() {
        let control = Arc::new(starrupture_observation_model::recipe::Control::default());
        let hub = Arc::new(Hub::new(Path::new("unused"), Some(control.clone())));
        let (base, live) = metadata();
        let old = hub.activate(base.clone(), live.clone(), Vec::new()).unwrap();
        hub.activate(base, live, Vec::new()).unwrap();
        control.toggle();
        let request = control.request();
        let publication = EpochControl {
            hub: &hub,
            epoch: old,
            inner: &control,
        };
        publication.captured(
            request,
            Some(starrupture_observation_model::recipe::Snapshot {
                recipe_id: "/Game/Old".into(),
                ingredients: vec![starrupture_observation_model::recipe::Ingredient {
                    item_id: "item".into(),
                    required: 1,
                    submitted: 0,
                    remaining: 1,
                    bag_count: None,
                }],
                process: 12,
                observed_at_ms: clock_ms(),
                generation: request.generation,
            }),
        );
        assert!(control.views().is_empty());
        assert_eq!(control.request(), request);
    }
    #[test]
    fn stalled_recipe_does_not_block_compass_and_group_drop_joins_waiters() {
        let stop = Arc::new(StopSignal::default());
        let hub = Arc::new(Hub::new(Path::new("unused"), None));
        let (base, live) = metadata();
        let epoch = hub.activate(base, live, Vec::new()).unwrap();
        let entered = Arc::new(std::sync::Barrier::new(2));
        let completed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut group = Group::new(stop.clone());
        for _ in 0..2 {
            let (stop, worker_entered, completed) =
                (stop.clone(), entered.clone(), completed.clone());
            group.handles.push(std::thread::spawn(move || {
                worker_entered.wait();
                stop.wait_until(Instant::now() + Duration::from_secs(10));
                completed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }));
            entered.wait();
        }
        hub.emit(Role::Compass, message(epoch, compass(90.))).unwrap();
        hub.emit(
            Role::Wave,
            message(
                epoch,
                Payload::Menu(MenuSample {
                    menu_open: false,
                    started_ms: clock_ms(),
                    finished_ms: clock_ms(),
                    guards_validated: true,
                }),
            ),
        )
        .unwrap();
        assert_eq!(hub.batch().slots.iter().flatten().count(), 2);
        let start = Instant::now();
        drop(group);
        assert_eq!(completed.load(std::sync::atomic::Ordering::SeqCst), 2);
        assert!(start.elapsed() < Duration::from_secs(2));
    }
}
