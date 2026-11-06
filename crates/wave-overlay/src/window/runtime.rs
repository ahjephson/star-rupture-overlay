//! UI-owned reader lifetime. Changes cancel and join the old generation before resources restart.
use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Mask {
    pub wave: bool,
    pub compass: bool,
    pub recipe: bool,
}
impl From<Preferences> for Mask {
    fn from(value: Preferences) -> Self {
        Self {
            wave: value.wave_enabled,
            compass: value.compass_enabled,
            recipe: value.recipe_tracker.enabled,
        }
    }
}
impl Mask {
    fn any(self) -> bool {
        self.wave || self.compass || self.recipe
    }
}
pub(super) struct Runtime {
    presence_due: Instant,
    config: Config,
    active: Option<Mask>,
    stop: Arc<StopSignal>,
    worker: Option<std::thread::JoinHandle<()>>,
}
impl Runtime {
    pub fn new(config: Config) -> Self {
        Self {
            presence_due: Instant::now(),
            config,
            active: None,
            stop: Arc::new(StopSignal::default()),
            worker: None,
        }
    }
    fn presence(&mut self, app: &App) {
        if app.dialog_context.diagnostic || Instant::now() < self.presence_due {
            return;
        }
        self.presence_due = Instant::now() + Duration::from_secs(1);
        let status = starrupture_overlay_reader::overlay_stream::presence_without_features();
        app.shared.lock().unwrap_or_else(|e| e.into_inner()).connection.update(status);
    }
    pub fn sync(&mut self, app: &mut App, window: HWND) -> bool {
        let desired = Mask::from(app.dialog_context.preferences.get());
        if self.active == Some(desired) && !self.stop.is_requested() {
            if !desired.any() {
                self.presence(app);
            }
            return true;
        }
        self.stop.request();
        if self.worker.as_ref().is_some_and(|w| !w.is_finished()) {
            return false;
        }
        if let Some(worker) = self.worker.take()
            && worker.join().is_err()
        {
            eprintln!("Feature runtime failed during joined shutdown.");
        }
        // Old readers are fully joined. No late observation/setup result can repopulate these resources.
        if !desired.recipe {
            app.recipe = crate::recipe_tracker::Panel::new();
            app.recipe.status = "Recipe tracker is disabled.".into();
        }
        if !desired.compass {
            app.compass = crate::compass::Panel::default();
        }
        if !desired.wave {
            app.renderer = LayerRenderer::default();
            app.presenter = HudPresenter::default();
        }
        {
            let mut reading = app.shared.lock().unwrap_or_else(|e| e.into_inner());
            reading.value = None;
            reading.menu = None;
            reading.compass = None;
            reading.process = 0;
            reading.no_world = true;
            reading.navigation.configure(desired.compass);
            reading.navigation.set(false);
        }
        self.active = Some(desired);
        self.stop = Arc::new(StopSignal::default());
        if desired.any() {
            let tracker = desired.recipe.then(|| app.recipe.tracker()).flatten();
            let setup = app.recipe.asset_setup(desired.recipe);
            self.worker = Some(reader(
                self.config.clone(),
                app.shared.clone(),
                self.stop.clone(),
                window as usize,
                tracker,
                desired,
                setup,
            ));
        }
        if !desired.any() {
            self.presence(app);
        }
        eprintln!(
            "Feature runtime configured: wave={}, compass={}, recipes={}; disabled resources released.",
            desired.wave, desired.compass, desired.recipe
        );
        true
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        self.stop.request();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
/// Exercises the real UI lifetime owner using only a cancellable owned fixture worker.
pub(super) fn self_test_disabled(app: &mut App, window: HWND) -> Result<()> {
    use std::sync::atomic::{AtomicBool, Ordering};
    let original = app.dialog_context.preferences.get();
    let mut disabled = original;
    disabled.wave_enabled = false;
    disabled.compass_enabled = false;
    disabled.recipe_tracker.enabled = false;
    app.dialog_context.preferences.set(disabled);
    let weak_control = Arc::downgrade(&app.recipe.control);
    let control = app.recipe.control.clone();
    let released = Arc::new(AtomicBool::new(false));
    let finished = released.clone();
    let stop = Arc::new(StopSignal::default());
    let cancellation = stop.clone();
    let worker = std::thread::spawn(move || {
        let owned_buffer = vec![0u8; 1024 * 1024];
        cancellation.wait_until(Instant::now() + Duration::from_secs(10));
        drop((owned_buffer, control));
        finished.store(true, Ordering::SeqCst);
    });
    let mut runtime = Runtime {
        presence_due: Instant::now(),
        config: Config::default(),
        active: Some(Mask {
            wave: true,
            compass: true,
            recipe: true,
        }),
        stop,
        worker: Some(worker),
    };
    let deadline = Instant::now() + Duration::from_secs(2);
    while !runtime.sync(app, window) {
        if Instant::now() > deadline {
            return Err("Feature disable did not join fixture worker".into());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    if !released.load(Ordering::SeqCst)
        || weak_control.upgrade().is_some()
        || runtime.worker.is_some()
        || app.recipe.tracker().is_some()
        || app.recipe.asset_setup(false).is_some()
    {
        return Err("Disabled feature retained worker/control/setup resources".into());
    }
    // A stable disabled runtime remains idle, without opening the game or a cache.
    if !runtime.sync(app, window) || runtime.worker.is_some() {
        return Err("Disabled runtime started a reader".into());
    }
    app.dialog_context.preferences.set(original);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn feature_masks_are_independent_and_all_off_has_no_readers() {
        for bits in 0..8 {
            let preferences = Preferences {
                wave_enabled: bits & 1 != 0,
                compass_enabled: bits & 2 != 0,
                recipe_tracker: crate::preferences::RecipeSettings {
                    enabled: bits & 4 != 0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mask = Mask::from(preferences);
            assert_eq!(mask.any(), bits != 0);
            assert_eq!(mask.wave, bits & 1 != 0);
            assert_eq!(mask.compass, bits & 2 != 0);
            assert_eq!(mask.recipe, bits & 4 != 0);
        }
    }
}
