//! Native recipe panel and foreground-scoped shortcut. Owns only app resources.
use crate::{desktop::wide, preferences::RecipeSettings};
use anyhow::{Context, Result, ensure};
use starrupture_observation_model::recipe::{Catalogue, Control, View};
use std::{cell::RefCell, collections::BTreeMap, mem::size_of, path::Path, rc::Rc, sync::Arc};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};

pub const MOUSE_CLICK: u32 = mouse::MESSAGE;
const HOTKEY: i32 = 0x5243;
struct Image {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
}
impl Image {
    fn clockwise(&self, opacity: u32) -> Self {
        let mut pixels = vec![0; self.pixels.len()];
        for y in 0..self.height {
            for x in 0..self.width {
                let from = (y * self.width + x) * 4;
                let to = (x * self.height + self.height - 1 - y) * 4;
                for c in 0..4 {
                    pixels[to + c] = (self.pixels[from + c] as u32 * opacity / 255) as u8;
                }
            }
        }
        Self {
            width: self.height,
            height: self.width,
            pixels,
        }
    }
}
pub struct Assets {
    pub catalogue: Arc<Catalogue>,
    directory: std::path::PathBuf,
    selected: Vec<String>,
    images: BTreeMap<String, Image>,
    header_frame: Image,
    header_background: Image,
    font: HANDLE,
    _font_bytes: Vec<u8>,
}
impl Drop for Assets {
    fn drop(&mut self) {
        unsafe {
            RemoveFontMemResourceEx(self.font);
        }
    }
}
impl Assets {
    pub fn load(directory: &Path) -> Result<Self> {
        let data = std::fs::read(directory.join("catalogue.json"))
            .context("Recipe assets are missing. Import a local game asset pack.")?;
        ensure!(data.len() <= 512 * 1024, "Recipe catalogue bound");
        let catalogue: Catalogue = serde_json::from_slice(&data)?;
        catalogue.validate().map_err(anyhow::Error::msg)?;
        let mut names = Vec::new();
        names.extend([
            catalogue.tick.clone(),
            catalogue.frame.clone(),
            catalogue.background.clone(),
        ]);
        names.sort();
        names.dedup();
        let mut images = BTreeMap::new();
        let mut total = 0;
        for name in names {
            let image = Image::load(directory, &name)?;
            total += image.pixels.len();
            ensure!(total <= IMAGE_BUDGET, "Recipe image cache bound");
            images.insert(name, image);
        }
        let font_bytes = std::fs::read(directory.join(&catalogue.font))?;
        ensure!(
            font_bytes.len() <= 2 * 1024 * 1024 && font_bytes.starts_with(b"OTTO"),
            "Recipe font bound/type"
        );
        let mut count = 0;
        let font = unsafe {
            AddFontMemResourceEx(
                font_bytes.as_ptr().cast(),
                font_bytes.len() as u32,
                std::ptr::null(),
                &raw mut count,
            )
        };
        ensure!(!font.is_null() && count > 0, "Recipe font could not be loaded");
        let header_frame = images[&catalogue.frame].clockwise(127);
        let header_background = images[&catalogue.background].clockwise(255);
        Ok(Self {
            catalogue: Arc::new(catalogue),
            directory: directory.into(),
            selected: Vec::new(),
            images,
            header_frame,
            header_background,
            font,
            _font_bytes: font_bytes,
        })
    }
}
const IMAGE_BUDGET: usize = 8 * 1024 * 1024;
impl Image {
    fn load(directory: &Path, name: &str) -> Result<Self> {
        use std::io::Read;
        ensure!(starrupture_observation_model::recipe::safe_file(name), "Unsafe recipe image name");
        let mut file = std::fs::File::open(directory.join(name))?;
        let length = file.metadata()?.len() as usize;
        ensure!((8..=4 * 1024 * 1024 + 8).contains(&length), "Recipe image bound");
        let mut header = [0; 8];
        file.read_exact(&mut header)?;
        let width = u32::from_le_bytes(header[0..4].try_into()?) as usize;
        let height = u32::from_le_bytes(header[4..8].try_into()?) as usize;
        ensure!(
            (1..=1024).contains(&width)
                && (1..=1024).contains(&height)
                && length == 8 + width * height * 4,
            "Recipe image dimensions"
        );
        let mut b = vec![0; length - 8];
        file.read_exact(&mut b)?;
        ensure!(file.read(&mut [0])? == 0, "Recipe image changed size");
        Ok(Self {
            width,
            height,
            pixels: b,
        })
    }
}
impl Assets {
    fn prepare(&mut self, view: &View) -> Result<()> {
        self.prepare_views(std::slice::from_ref(view))
    }
    fn prepare_views(&mut self, views: &[View]) -> Result<()> {
        let selected: Vec<_> =
            views.iter().filter_map(|v| v.snapshot.as_ref().map(|s| s.recipe_id.clone())).collect();
        if self.selected == selected {
            return Ok(());
        }
        let mut needed = vec![
            self.catalogue.tick.clone(),
            self.catalogue.frame.clone(),
            self.catalogue.background.clone(),
        ];
        for id in &selected {
            let recipe = self.catalogue.recipes.get(id).context("Recipe reference missing")?;
            needed.push(self.catalogue.items[&recipe.item_id].image.clone());
            for id in recipe.requirements.keys() {
                needed.push(self.catalogue.items[id].image.clone());
            }
        }
        needed.sort();
        needed.dedup();
        self.selected.clear();
        self.images.retain(|name, _| needed.binary_search(name).is_ok());
        let mut total: usize = self.images.values().map(|i| i.pixels.capacity()).sum();
        for name in needed {
            if !self.images.contains_key(&name) {
                let image = Image::load(&self.directory, &name)?;
                total += image.pixels.capacity();
                ensure!(total <= IMAGE_BUDGET, "Recipe image cache bound");
                self.images.insert(name, image);
            }
        }
        self.selected = selected;
        Ok(())
    }
    fn clear_images(&mut self) {
        self.images.retain(|name, _| {
            name == &self.catalogue.tick
                || name == &self.catalogue.frame
                || name == &self.catalogue.background
        });
        self.selected.clear();
    }
}
pub struct Panel {
    windows: [HWND; 3],
    buttons: Vec<close::Button>,
    mouse: mouse::Input,
    cursor: Rc<RefCell<cursor::GameCursor>>,
    hotkey_owner: HWND,
    assets: Option<Assets>,
    pub control: Arc<Control>,
    hotkey: Option<RecipeSettings>,
    pub status: String,
    last_settings: RecipeSettings,
    setup_state: setup::Shared,
    setup_revision: u64,
    setup_enabled: Arc<std::sync::atomic::AtomicBool>,
}
impl Drop for Panel {
    fn drop(&mut self) {
        self.mouse.configure(std::ptr::null_mut(), std::ptr::null_mut(), false);
        self.unregister();
        for window in self.windows {
            if !window.is_null() {
                unsafe {
                    DestroyWindow(window);
                }
            }
        }
    }
}
impl Panel {
    pub fn new() -> Self {
        let assets = None;
        let status = "Start StarRupture and load your world to prepare the recipe tracker.".into();
        let setup_state = Arc::new(std::sync::Mutex::new(setup::State::default()));
        Self {
            windows: [std::ptr::null_mut(); 3],
            buttons: Vec::new(),
            mouse: mouse::Input::default(),
            cursor: Rc::new(RefCell::new(cursor::GameCursor::default())),
            hotkey_owner: std::ptr::null_mut(),
            assets,
            control: Arc::new(Control::default()),
            hotkey: None,
            status,
            last_settings: RecipeSettings::default(),
            setup_state,
            setup_revision: 0,
            setup_enabled: Arc::new(std::sync::atomic::AtomicBool::new(true)),
        }
    }
    pub fn tracker(&self) -> Option<starrupture_overlay_reader::recipe::Tracker> {
        self.assets.as_ref().map(|a| starrupture_overlay_reader::recipe::Tracker {
            catalogue: a.catalogue.clone(),
            control: self.control.clone(),
        })
    }
    pub fn asset_setup(
        &self,
        enabled: bool,
    ) -> Option<Box<dyn starrupture_overlay_reader::recipe::AssetSetup>> {
        self.setup_enabled.store(enabled, std::sync::atomic::Ordering::Relaxed);
        let root = std::env::var_os("LOCALAPPDATA")
            .map(std::path::PathBuf::from)?
            .join("StarRuptureWaveOverlay/recipe-cache");
        Some(Box::new(setup::Preparation {
            root,
            state: self.setup_state.clone(),
            control: self.control.clone(),
            enabled: self.setup_enabled.clone(),
            pending_directory: None,
        }))
    }
    fn refresh_setup(&mut self) {
        let state = self.setup_state.lock().unwrap_or_else(|p| p.into_inner()).clone();
        if state.revision != self.setup_revision {
            self.setup_revision = state.revision;
            self.assets = None;
            self.status = state.message;
            if let Some(directory) = state.ready {
                match Assets::load(&directory) {
                    Ok(assets) => self.assets = Some(assets),
                    Err(error) => {
                        self.status = format!("Recipe tracker could not load its cache: {error:#}")
                    }
                }
            }
        }
        if self.assets.is_none()
            && let Some(path) = state.progress
            && let Ok(message) = std::fs::read_to_string(path)
            && message.len() <= 512
        {
            self.status = message;
        }
    }
    fn unregister(&mut self) {
        if self.hotkey.take().is_some() {
            unsafe {
                UnregisterHotKey(self.hotkey_owner, HOTKEY);
            }
        }
    }
    fn window(&mut self, index: usize) -> Result<()> {
        if !self.windows[index].is_null() {
            return Ok(());
        }
        let class = wide("StarRuptureRecipeTracker");
        let module = unsafe { GetModuleHandleW(std::ptr::null()) };
        let wc = WNDCLASSW {
            lpfnWndProc: Some(DefWindowProcW),
            hInstance: module,
            lpszClassName: class.as_ptr(),
            ..Default::default()
        };
        unsafe {
            RegisterClassW(&wc);
        };
        let w = unsafe {
            CreateWindowExW(
                crate::stacking::EXTENDED_STYLE,
                class.as_ptr(),
                wide("Recipe tracker").as_ptr(),
                WS_POPUP,
                0,
                0,
                1,
                1,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                module,
                std::ptr::null(),
            )
        };
        ensure!(!w.is_null(), "Could not create recipe panel");
        self.windows[index] = w;
        if self.buttons.len() <= index {
            self.buttons.push(close::Button::new(self.control.clone(), self.cursor.clone())?);
        }
        Ok(())
    }
    pub fn shortcut(&mut self, owner: HWND, game: HWND, settings: RecipeSettings) {
        self.setup_enabled.store(settings.enabled, std::sync::atomic::Ordering::Relaxed);
        self.refresh_setup();
        // Hotkeys are delivered to the main window, even though the panel is separate.
        let foreground = !game.is_null() && unsafe { GetForegroundWindow() } == game;
        let desired = foreground && settings.enabled && self.assets.is_some();
        self.mouse.configure(owner, game, desired && settings.double_click_pin);
        if (self.hotkey.is_some_and(|s| s != settings) || !desired) && self.hotkey.take().is_some()
        {
            unsafe {
                UnregisterHotKey(owner, HOTKEY);
            }
        }
        if desired && self.hotkey.is_none() {
            if unsafe {
                RegisterHotKey(
                    owner,
                    HOTKEY,
                    settings.modifiers as u32 | MOD_NOREPEAT,
                    settings.key as u32,
                )
            } != 0
            {
                self.hotkey = Some(settings);
                self.hotkey_owner = owner;
                self.status = "Shortcut ready while StarRupture has focus.".into()
            } else {
                self.status = "Shortcut unavailable. Choose another in Settings.".into()
            }
        }
        if self.last_settings.enabled && !settings.enabled {
            self.control.clear();
            if let Some(assets) = self.assets.as_mut() {
                assets.clear_images();
            }
        }
        self.last_settings = settings;
    }
    pub fn pressed(&self, id: usize, game: HWND) -> bool {
        if id == HOTKEY as usize
            && self.hotkey.is_some()
            && !game.is_null()
            && unsafe { GetForegroundWindow() } == game
        {
            let mut cursor = POINT::default();
            if unsafe { GetCursorPos(&mut cursor) } == 0 {
                return false;
            }
            self.control.toggle_at(Some([cursor.x, cursor.y]));
            true
        } else {
            false
        }
    }
    pub fn mouse_pressed(&mut self, game: HWND, time: u32, point: [i32; 2]) {
        if self.mouse.accept(game, time, point) {
            self.control.mouse_at(point);
        }
    }
    pub fn hide(&self) {
        for window in self.windows {
            if !window.is_null() {
                unsafe {
                    ShowWindow(window, SW_HIDE);
                }
            }
        }
        for button in &self.buttons {
            button.hide();
        }
    }
    pub fn follow(&self, game: HWND) {
        for window in self.windows {
            if !window.is_null() && unsafe { IsWindowVisible(window) } != 0 {
                crate::stacking::follow_game(window, game);
            }
        }
        for button in &self.buttons {
            button.follow(game);
        }
    }
    pub fn tick(
        &mut self,
        owner: HWND,
        game: HWND,
        visible: bool,
        settings: RecipeSettings,
        process: u32,
        pause: bool,
    ) {
        self.cursor.borrow_mut().observe(game, process);
        self.shortcut(owner, game, settings);
        if !settings.enabled || !visible || self.assets.is_none() {
            self.hide();
            return;
        }
        let views = self.control.views();
        if views.is_empty() {
            if let Some(assets) = self.assets.as_mut() {
                assets.clear_images();
            }
            self.hide();
            return;
        }
        if views.iter().any(|v| {
            v.snapshot.as_ref().is_none_or(|s| s.process != process || s.generation != v.generation)
        }) {
            self.hide();
            return;
        }
        let notice = self.control.notice();
        if !notice.is_empty() {
            self.status = notice;
        }
        let mut rect = RECT::default();
        let mut origin = POINT::default();
        if unsafe { GetClientRect(game, &mut rect) } == 0
            || unsafe { ClientToScreen(game, &mut origin) } == 0
        {
            self.hide();
            return;
        }
        let count = views
            .iter()
            .filter_map(|v| v.snapshot.as_ref().map(|s| s.ingredients.len()))
            .max()
            .unwrap_or(0);
        let Some(geometry) =
            layout::fit([rect.right, rect.bottom], views.len(), count, settings.scale, pause)
        else {
            self.hide();
            return;
        };
        if let Err(error) = self.assets.as_mut().unwrap().prepare_views(&views) {
            self.status = error.to_string();
            self.hide();
            return;
        }
        let group_y = origin.y
            + match settings.position {
                0 => 20,
                2 => (rect.bottom - geometry.height - 20).max(10),
                _ => (rect.bottom - geometry.height) / 2,
            };
        let interactive = mouse::cursor_visible() && unsafe { GetForegroundWindow() } == game;
        for (index, view) in views.iter().enumerate() {
            if let Err(error) = self.window(index) {
                self.status = error.to_string();
                self.hide();
                return;
            }
            let x = origin.x + rect.right
                - geometry.right_margin
                - geometry.width
                - (geometry.width + geometry.gap) * index as i32;
            let height = geometry.height;
            unsafe {
                SetWindowPos(
                    self.windows[index],
                    std::ptr::null_mut(),
                    x,
                    group_y,
                    geometry.width,
                    height,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
            if let Err(error) = render(
                self.windows[index],
                self.assets.as_ref().unwrap(),
                view,
                geometry.width,
                height,
                layout::frame(index, views.len()),
            ) {
                self.status = error.to_string();
                self.hide();
                return;
            }
            crate::stacking::follow_game(self.windows[index], game);
            self.buttons[index].show(
                game,
                self.windows[index],
                &view.snapshot.as_ref().unwrap().recipe_id,
                [x + geometry.width - close::SIZE - 6, group_y + layout::control_y(geometry.width)],
                interactive,
            );
        }
        for index in views.len()..3 {
            if !self.windows[index].is_null() {
                unsafe {
                    ShowWindow(self.windows[index], SW_HIDE);
                }
            }
            if let Some(button) = self.buttons.get(index) {
                button.hide();
            }
        }
    }
}

mod close;
mod cursor;
mod layout;
mod mouse;
mod setup;
pub use setup::{
    self_test as self_test_asset_setup, self_test_cancel as self_test_asset_cancel,
    self_test_disabled as self_test_asset_disabled,
};
mod render;
use render::render;
pub use render::self_test;

fn self_test_shortcut() -> Result<()> {
    struct Window(HWND);
    impl Drop for Window {
        fn drop(&mut self) {
            unsafe {
                DestroyWindow(self.0);
            }
        }
    }
    let foreground = unsafe { GetForegroundWindow() };
    let class = wide("StarRuptureRecipeShortcutTest");
    let module = unsafe { GetModuleHandleW(std::ptr::null()) };
    let wc = WNDCLASSW {
        lpfnWndProc: Some(DefWindowProcW),
        hInstance: module,
        lpszClassName: class.as_ptr(),
        ..Default::default()
    };
    unsafe {
        RegisterClassW(&wc);
    }
    let make = || {
        Window(unsafe {
            CreateWindowExW(
                WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
                class.as_ptr(),
                class.as_ptr(),
                WS_POPUP,
                -30000,
                -30000,
                1,
                1,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                module,
                std::ptr::null(),
            )
        })
    };
    let a = make();
    let b = make();
    ensure!(!a.0.is_null() && !b.0.is_null(), "Shortcut diagnostic windows");
    let modifiers = MOD_ALT | MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT;
    let key = (0x70..=0x7a)
        .find(|key| unsafe { RegisterHotKey(a.0, 1, modifiers, *key) } != 0)
        .context("No free diagnostic shortcut")?;
    let conflict = unsafe { RegisterHotKey(b.0, 2, modifiers, key) } == 0;
    unsafe {
        UnregisterHotKey(a.0, 1);
    }
    ensure!(conflict, "Duplicate shortcut was not rejected");
    let released = unsafe { RegisterHotKey(b.0, 2, modifiers, key) } != 0;
    unsafe {
        UnregisterHotKey(b.0, 2);
    }
    ensure!(released, "Shortcut was not released");
    ensure!(unsafe { GetForegroundWindow() } == foreground, "Recipe diagnostics changed focus");
    Ok(())
}

pub fn diagnostic_catalogue() -> Result<std::path::PathBuf> {
    Ok(setup::diagnostic_assets()?.join("catalogue.json"))
}

pub fn asset_wait_fixture(path: &std::path::Path, child: bool) -> anyhow::Result<()> {
    setup::wait_fixture(path, child)
}

#[cfg(test)]
mod image_tests {
    use super::*;
    #[test]
    fn pixel_reader_rejects_truncation_dimensions_and_oversize() {
        let directory = std::env::temp_dir().join(format!(
            "starrupture-image-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("icon.bgra");
        let mut valid = Vec::new();
        valid.extend(2u32.to_le_bytes());
        valid.extend(1u32.to_le_bytes());
        valid.extend([1, 2, 3, 4, 5, 6, 7, 8]);
        std::fs::write(&path, &valid).unwrap();
        let image = Image::load(&directory, "icon.bgra").unwrap();
        assert_eq!((image.width, image.height), (2, 1));
        assert_eq!(image.pixels, &valid[8..]);
        assert_eq!(image.pixels.capacity(), 8);
        for length in [0, 7, 15] {
            std::fs::write(&path, &valid[..length]).unwrap();
            assert!(Image::load(&directory, "icon.bgra").is_err());
        }
        valid[..4].copy_from_slice(&1025u32.to_le_bytes());
        std::fs::write(&path, &valid).unwrap();
        assert!(Image::load(&directory, "icon.bgra").is_err());
        std::fs::File::create(&path).unwrap().set_len(4 * 1024 * 1024 + 9).unwrap();
        assert!(Image::load(&directory, "icon.bgra").is_err());
        assert!(Image::load(&directory, "../icon.bgra").is_err());
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir(&directory).unwrap();
    }
}
