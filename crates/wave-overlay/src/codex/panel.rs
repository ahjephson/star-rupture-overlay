use super::{
    backdrop::{Backdrop, Cursor},
    banner::Banner,
    window::Window,
};
use crate::preferences::CodexShortcut;
use anyhow::{Result, ensure};
use starrupture_assets::codex::{Prepared, Store};
use starrupture_observation_model::shutdown::StopSignal;
use starrupture_overlay_reader::{codex::Progress, recipe::Tracker};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread::JoinHandle,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::HWND,
    UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};

pub(crate) const HOTKEY: usize = 1492;

#[derive(Clone)]
struct Observation {
    at: Instant,
    blocked: u64,
    gameplay: bool,
    catalogue: Arc<Prepared>,
    progress: Option<Arc<Progress>>,
}

struct Worker {
    pid: u32,
    stop: Arc<StopSignal>,
    thread: Option<JoinHandle<()>>,
    latest: Arc<Mutex<Option<Observation>>>,
}

impl Worker {
    fn start(directory: PathBuf, pid: u32) -> Result<Self> {
        let stop = Arc::new(StopSignal::default());
        let latest = Arc::new(Mutex::new(None));
        let worker_stop = stop.clone();
        let worker_latest = latest.clone();
        let thread = std::thread::Builder::new().name("codex".into()).spawn(move || {
            if let Err(error) = run(&directory, pid, &worker_stop, &worker_latest)
                && !worker_stop.is_requested()
            {
                eprintln!("Codex: {error:#}");
            }
        })?;
        Ok(Self {
            pid,
            stop,
            thread: Some(thread),
            latest,
        })
    }

    fn finished(&self) -> bool {
        self.thread.as_ref().is_none_or(JoinHandle::is_finished)
    }
}

fn run(
    directory: &Path,
    pid: u32,
    stop: &Arc<StopSignal>,
    latest: &Mutex<Option<Observation>>,
) -> Result<()> {
    let started = Instant::now();
    let store = Store::new(directory.join("catalogue"));
    let catalogue = Arc::new(starrupture_overlay_reader::codex::prepare(
        directory,
        stop.clone(),
        |paks, context| store.lookup(paks, context),
        |paks, context, mappings, stop| {
            super::build(directory, &store, paks, context, mappings, stop)
        },
    )?);
    ensure!(!stop.is_requested(), "Codex preparation cancelled");
    eprintln!("Codex catalogue prepared in {} ms.", started.elapsed().as_millis());
    let mut progress = None;
    let mut blocked = 0u64;
    let mut previous = true;
    starrupture_overlay_reader::codex::observe(
        directory,
        stop.clone(),
        Some(pid),
        &catalogue.catalogue.context,
        &super::pins::upgrade_recipes(&catalogue.catalogue),
        |gameplay, sample| {
            if !gameplay && previous {
                blocked = blocked.wrapping_add(1);
            }
            previous = gameplay;
            if let Some(sample) = sample
                && progress.as_deref() != Some(&sample)
            {
                progress = Some(Arc::new(sample));
            }
            let observation = Observation {
                at: Instant::now(),
                blocked,
                gameplay,
                catalogue: catalogue.clone(),
                progress: progress.clone(),
            };
            *latest.lock().unwrap_or_else(|e| e.into_inner()) = Some(observation);
        },
    )
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.stop.request();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[derive(Default)]
pub(crate) struct Panel {
    cache: Option<PathBuf>,
    worker: Option<Worker>,
    window: Option<Window>,
    banner: Option<Banner>,
    backdrop: Option<Backdrop>,
    requested: Option<Instant>,
    open: bool,
    registration: Option<CodexShortcut>,
    retry: Option<Instant>,
    attempts: u8,
    pid: u32,
    game: HWND,
    blocked: u64,
}

impl Panel {
    pub fn with_cache(cache: Option<PathBuf>) -> Self {
        Self {
            cache,
            ..Default::default()
        }
    }

    pub fn pin_context(&self) -> Option<(Arc<Prepared>, Arc<Progress>)> {
        let worker = self.worker.as_ref().filter(|worker| !worker.stop.is_requested())?;
        let latest = worker.latest.lock().unwrap_or_else(|error| error.into_inner());
        let reading = latest.as_ref()?;
        (reading.at.elapsed() <= Duration::from_secs(2))
            .then(|| {
                reading
                    .progress
                    .as_ref()
                    .map(|progress| (reading.catalogue.clone(), progress.clone()))
            })
            .flatten()
    }
    pub fn window_handle(&self) -> HWND {
        self.window
            .as_ref()
            .filter(|_| self.open)
            .map_or(std::ptr::null_mut(), |window| window.hwnd)
    }

    pub fn backdrop(&self) -> HWND {
        self.backdrop.as_ref().map_or(std::ptr::null_mut(), |backdrop| backdrop.hwnd)
    }

    pub fn close(&mut self) {
        self.backdrop = None;
        self.banner = None;
        self.requested = None;
        if self.open {
            if let Some(window) = &mut self.window {
                window.hide(self.game, self.pid);
            }
            self.open = false;
        }
    }

    fn register(&mut self, owner: HWND, requested: Option<CodexShortcut>) {
        if self.registration == requested {
            return;
        }
        if self.registration.take().is_some() {
            // SAFETY: This registration belongs to the supplied overlay HWND and its known ID.
            unsafe { UnregisterHotKey(owner, HOTKEY as i32) };
        }
        if let Some(shortcut) = requested {
            // SAFETY: This UI-thread hotkey uses scalar key data and the live overlay HWND.
            if unsafe {
                RegisterHotKey(
                    owner,
                    HOTKEY as i32,
                    u32::from(shortcut.modifiers) | MOD_NOREPEAT,
                    u32::from(shortcut.key),
                )
            } != 0
            {
                self.registration = Some(shortcut);
            }
        }
    }

    fn session(&mut self, game: HWND, pid: u32) {
        if self.game != game || self.pid != pid {
            self.close();
            self.window = None;
            self.retry = None;
            self.attempts = 0;
            self.pid = pid;
            self.game = game;
            if let Some(worker) = &self.worker {
                worker.stop.request();
            }
        }
        if let Some(worker) = &self.worker
            && worker.pid != pid
        {
            worker.stop.request();
            self.close();
            self.window = None;
        }
        if self.worker.as_ref().is_some_and(Worker::finished) {
            self.close();
            self.window = None;
            self.worker = None;
            self.blocked = 0;
            self.retry = Some(Instant::now() + Duration::from_secs(2));
        }
        if pid != 0
            && !game.is_null()
            && self.worker.is_none()
            && self.attempts < 3
            && self.retry.is_none_or(|at| Instant::now() >= at)
            && let Some(cache) = self.cache.clone()
        {
            self.attempts += 1;
            match Worker::start(cache, pid) {
                Ok(worker) => self.worker = Some(worker),
                Err(error) => {
                    eprintln!("Codex: {error:#}");
                    self.retry = Some(Instant::now() + Duration::from_secs(2));
                }
            }
        }
    }

    pub fn tick(
        &mut self,
        owner: HWND,
        session: (HWND, u32),
        shortcut: Option<CodexShortcut>,
        visible: bool,
        inputs: (Option<Tracker>, Cursor),
    ) {
        let (game, pid) = session;
        let (tracker, cursor) = inputs;
        self.session(game, pid);
        if self.open && self.window.as_ref().is_some_and(Window::close_requested) {
            self.close();
        }
        let in_game = visible && crate::compass::focused(game) && crate::compass::gameplay_cursor();
        self.register(
            owner,
            if in_game && !self.open {
                shortcut
            } else {
                None
            },
        );
        if !visible || pid == 0 {
            self.close();
            return;
        }
        let reading = self.worker.as_ref().and_then(|worker| {
            (!worker.stop.is_requested())
                .then(|| worker.latest.lock().unwrap_or_else(|e| e.into_inner()).clone())
                .flatten()
        });
        let Some(reading) = reading else {
            if self.open {
                self.close();
            }
            return;
        };
        self.attempts = 0;
        let entered_screen = reading.blocked != self.blocked;
        self.blocked = reading.blocked;
        if !reading.gameplay || entered_screen || reading.at.elapsed() > Duration::from_secs(2) {
            self.close();
            return;
        }
        let Some(progress) = reading.progress else {
            return;
        };
        if self.open && self.window.as_ref().is_some_and(|window| !window.visible()) {
            self.close();
        }
        if let Some(window) = &mut self.window
            && self.open
        {
            if let Some(backdrop) = &mut self.backdrop {
                backdrop.resize(game);
                if crate::compass::focused(game) {
                    crate::stacking::follow_game(backdrop.hwnd, game);
                }
            }
            window.follow_focus(game);
            window.update(progress.clone());
            window.pins(tracker.as_ref(), pid);
        }
        if self.requested.is_some() && !in_game {
            self.close();
            return;
        }
        if let Some(requested) = self.requested.take() {
            self.banner = None;
            self.register(owner, None);
            self.backdrop = Backdrop::open(game, cursor.clone());
            if self.backdrop.is_none() {
                return;
            }
            if self.window.as_ref().is_some_and(|window| !window.matches_game_scale(game)) {
                self.window = None;
            }
            if let Some(window) = &mut self.window {
                window.update(progress);
                window.pins(tracker.as_ref(), pid);
                window.show(game);
            } else {
                match Window::open(game, reading.catalogue, progress, cursor) {
                    Ok(mut window) => {
                        window.pins(tracker.as_ref(), pid);
                        self.window = Some(window);
                    }
                    Err(error) => {
                        eprintln!("Codex: {error:#}");
                        self.backdrop = None;
                        return;
                    }
                }
            }
            if let (Some(backdrop), Some(window)) = (&self.backdrop, &self.window) {
                backdrop.bind_codex(window.hwnd);
            }
            self.open = true;
            eprintln!("Codex opened in {} ms.", requested.elapsed().as_millis());
        }
    }

    pub fn pressed(&mut self, game: HWND, hotkey: isize) {
        let Some(shortcut) = self.registration else {
            return;
        };
        if ((hotkey >> 16) as u16) != shortcut.key
            || (hotkey as u16 & 7) != shortcut.modifiers
            || !crate::compass::focused(game)
            || !crate::compass::gameplay_cursor()
        {
            return;
        }
        if self.requested.is_some() {
            self.close();
        } else if let Some(worker) = &self.worker
            && !worker.stop.is_requested()
        {
            self.requested = Some(Instant::now());
            if worker.latest.lock().unwrap_or_else(|e| e.into_inner()).is_none() {
                self.banner = Banner::open(game);
            }
        }
    }

    pub fn translate(&mut self, message: &MSG) -> bool {
        let Some(window) = self.window.as_mut().filter(|_| self.open) else {
            return false;
        };
        if message.message == WM_KEYDOWN
            && matches!(message.wParam as u16, VK_ESCAPE | VK_TAB)
            && window.handles(message.hwnd)
        {
            self.close();
            return true;
        }
        window.key(message)
    }
}
