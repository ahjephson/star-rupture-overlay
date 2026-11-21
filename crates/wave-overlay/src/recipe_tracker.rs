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
    card_images: BTreeMap<String, BTreeMap<usize, Image>>,
    card_image_build: String,
    lock_icon: Option<String>,
    header_frame: Image,
    header_background: Image,
    font: HANDLE,
    _font_bytes: Vec<u8>,
}
impl Drop for Assets {
    fn drop(&mut self) {
        // SAFETY: This handle is an owned font registration from AddFontMemResourceEx; it is
        // removed before its backing font storage is dropped.
        unsafe {
            RemoveFontMemResourceEx(self.font);
        }
    }
}
impl Assets {
    pub fn load(directory: &Path) -> Result<Self> {
        let data = setup::read_bounded(&directory.join("catalogue.json"), 512 * 1024)
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
        let font_bytes = setup::read_bounded(&directory.join(&catalogue.font), 2 * 1024 * 1024)?;
        ensure!(
            font_bytes.len() <= 2 * 1024 * 1024 && font_bytes.starts_with(b"OTTO"),
            "Recipe font bound/type"
        );
        let mut count = 0;
        // SAFETY: The font byte slice is retained and its bounded length fits DWORD; count is
        // writable raw storage, and the returned registration is removed only by its owner.
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
            card_images: BTreeMap::new(),
            card_image_build: String::new(),
            lock_icon: None,
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
    fn prepare_cards(
        &mut self,
        prepared: &starrupture_assets::codex::Prepared,
        cards: &[crate::codex::pins::Card],
        geometry: layout::Layout,
    ) {
        let catalogue = &prepared.catalogue;
        if self.card_image_build != catalogue.context.game_hash {
            self.card_images.clear();
            self.card_image_build.clone_from(&catalogue.context.game_hash);
        }
        let mut needed = std::collections::BTreeSet::new();
        let header = layout::header_icon_size(geometry.width);
        for card in cards {
            needed.insert((card.icon.as_str(), header));
            let [_, _, tile] =
                layout::tile(geometry.width, geometry.height, card.materials.len(), 0);
            for material in &card.materials {
                if let Some(item) = catalogue.items.get(&material.amount.item) {
                    needed.insert((item.icon.as_str(), (tile - 4) as usize));
                }
            }
        }
        self.lock_icon.clone_from(&catalogue.lock_icon);
        if let Some(lock) = self.lock_icon.as_deref() {
            needed.insert((lock, layout::lock_icon_size(geometry.width)));
        }
        self.card_images.retain(|id, images| {
            images.retain(|side, _| needed.contains(&(id.as_str(), *side)));
            !images.is_empty()
        });
        let mut used = self
            .card_images
            .values()
            .flat_map(|images| images.values())
            .map(|image| image.pixels.len())
            .sum::<usize>();
        for (id, size) in needed.into_iter().take(64) {
            if self.card_image(id, size).is_some() || used + size * size * 4 > IMAGE_BUDGET {
                continue;
            }
            if let Ok(icon) = prepared.icon(id) {
                let image = scaled_pin_icon(&icon, size);
                used += image.pixels.len();
                self.card_images.entry(id.into()).or_default().insert(size, image);
            }
        }
    }

    fn card_image(&self, id: &str, size: usize) -> Option<&Image> {
        self.card_images.get(id)?.get(&size)
    }

    fn clear_images(&mut self) {
        self.images.retain(|name, _| {
            name == &self.catalogue.tick
                || name == &self.catalogue.frame
                || name == &self.catalogue.background
        });
        self.selected.clear();
        self.card_images.clear();
    }
}
// Filter premultiplied colours and alpha together, directly from the native icon.
fn scaled_pin_icon(icon: &starrupture_assets::codex::Icon, side: usize) -> Image {
    let width = icon.width as usize;
    let height = icon.height as usize;
    let longest = width.max(height);
    let draw_width = (width * side / longest).max(1);
    let draw_height = (height * side / longest).max(1);
    let inset = [(side - draw_width) / 2, (side - draw_height) / 2];
    let footprint = |pixel, source, output| {
        let ratio = source as f64 / output as f64;
        let center = (pixel as f64 + 0.5) * ratio;
        let radius = ratio.max(1.0) / 2.0;
        [(center - radius).max(0.0), (center + radius).min(source as f64)]
    };
    let coverage =
        |from: f64, to: f64, pixel: usize| to.min((pixel + 1) as f64) - from.max(pixel as f64);

    let mut pixels = vec![0; side * side * 4];
    for y in 0..draw_height {
        let [top, bottom] = footprint(y, height, draw_height);
        for x in 0..draw_width {
            let [left, right] = footprint(x, width, draw_width);
            let area = (right - left) * (bottom - top);
            let mut sum = [0.0; 4];
            for row in top.floor() as usize..bottom.ceil() as usize {
                let vertical = coverage(top, bottom, row);
                for column in left.floor() as usize..right.ceil() as usize {
                    let weight = vertical * coverage(left, right, column);
                    let offset = (row * width + column) * 4;
                    for (sum, pixel) in sum.iter_mut().zip(&icon.pixels[offset..offset + 4]) {
                        *sum += f64::from(*pixel) * weight;
                    }
                }
            }
            let target = ((inset[1] + y) * side + inset[0] + x) * 4;
            for (pixel, sum) in pixels[target..target + 4].iter_mut().zip(sum) {
                *pixel = (sum / area).round() as u8;
            }
        }
    }
    Image {
        width: side,
        height: side,
        pixels,
    }
}

#[cfg(test)]
mod pin_icon_tests {
    use super::*;

    #[test]
    fn downsampling_keeps_partial_coverage_and_preserves_alpha() {
        let icon = starrupture_assets::codex::Icon {
            width: 2,
            height: 2,
            pixels: vec![200, 0, 0, 200, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        };
        assert_eq!(scaled_pin_icon(&icon, 1).pixels, [50, 0, 0, 50]);
        assert_eq!(scaled_pin_icon(&icon, 2).pixels, icon.pixels);
    }
}

type CardContext =
    (Arc<starrupture_assets::codex::Prepared>, Arc<starrupture_overlay_reader::codex::Progress>);
pub(crate) struct Presentation {
    pub pause: bool,
    pub modal: HWND,
    pub codex_window: HWND,
    pub codex: Option<CardContext>,
}
pub struct Panel {
    windows: [HWND; 3],
    buttons: Vec<close::Button>,
    mouse: mouse::Input,
    cursor: Rc<RefCell<cursor::GameCursor>>,
    item_hover: RefCell<items::Hover>,
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
                // SAFETY: Only this UI/fixture owner destroys its HWND; boxed callback state
                // remains alive until DestroyWindow completes, and stale handles are validated by
                // Win32.
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
            item_hover: RefCell::new(items::Hover::default()),
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
    pub fn suspend(&mut self) {
        self.hide();
        self.unregister();
        self.mouse.configure(std::ptr::null_mut(), std::ptr::null_mut(), false);
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
        if !enabled {
            return None;
        }
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
            && let Ok(bytes) = setup::read_bounded(&path, 512)
            && let Ok(message) = String::from_utf8(bytes)
        {
            self.status = message;
        }
    }
    fn unregister(&mut self) {
        if self.hotkey.take().is_some() {
            // SAFETY: The UI thread owns this window and shortcut ID; registration uses scalar
            // key/modifier values and is released by the same owner before its HWND is destroyed.
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
        // SAFETY: These Win32 queries use scalar values or borrowed handles, validate native handle
        // identity themselves, and neither dereference Rust memory nor transfer ownership.
        let module = unsafe { GetModuleHandleW(std::ptr::null()) };
        let wc = WNDCLASSW {
            lpfnWndProc: Some(DefWindowProcW),
            hInstance: module,
            lpszClassName: class.as_ptr(),
            ..Default::default()
        };
        // SAFETY: The class structure is initialized, its terminated name is live during
        // registration, and the callback has the Win32 ABI and remains in this module.
        unsafe {
            RegisterClassW(&wc);
        };
        // SAFETY: The registered callback has the Win32 ABI, class/title strings remain live during
        // creation, and any creation-state box stays at a stable address until after the new HWND
        // is destroyed.
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
        // SAFETY: These Win32 queries use scalar values or borrowed handles, validate native handle
        // identity themselves, and neither dereference Rust memory nor transfer ownership.
        let foreground = !game.is_null() && unsafe { GetForegroundWindow() } == game;
        let desired = foreground && settings.enabled && self.assets.is_some();
        self.mouse.configure(owner, game, desired && settings.double_click_pin);
        if (self.hotkey.is_some_and(|s| s != settings) || !desired) && self.hotkey.take().is_some()
        {
            // SAFETY: The UI thread owns this window and shortcut ID; registration uses scalar
            // key/modifier values and is released by the same owner before its HWND is destroyed.
            unsafe {
                UnregisterHotKey(owner, HOTKEY);
            }
        }
        if desired && self.hotkey.is_none() {
            // SAFETY: The UI thread owns this window and shortcut ID; registration uses scalar
            // key/modifier values and is released by the same owner before its HWND is destroyed.
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
            // SAFETY: These Win32 queries use scalar values or borrowed handles, validate native
            // handle identity themselves, and neither dereference Rust memory nor transfer
            // ownership.
            && unsafe { GetForegroundWindow() } == game
        {
            let mut cursor = POINT::default();
            // SAFETY: Win32 validates the borrowed handle where required; every output points to an
            // initialized local struct or bounded writable slice that stays live for the
            // synchronous query.
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
        self.item_hover.borrow_mut().hide();
        for window in self.windows {
            if !window.is_null() {
                // SAFETY: The UI thread uses owned overlay/menu HWNDs and borrowed comparison
                // handles; native position/style values are scalar, and referenced menu/state
                // objects remain alive through synchronous dispatch.
                unsafe {
                    ShowWindow(window, SW_HIDE);
                }
            }
        }
        for button in &self.buttons {
            button.hide();
        }
    }
    pub(crate) fn game_cursor(&self) -> Rc<RefCell<cursor::GameCursor>> {
        self.cursor.clone()
    }
    pub fn follow(&self, game: HWND, modal: HWND, codex_window: HWND) {
        for window in self.windows {
            // SAFETY: These Win32 queries use scalar values or borrowed handles, validate native
            // handle identity themselves, and neither dereference Rust memory nor transfer
            // ownership.
            if !window.is_null() && unsafe { IsWindowVisible(window) } != 0 {
                if modal.is_null() {
                    crate::stacking::follow_game(window, game);
                } else {
                    crate::stacking::follow_companion(window, modal, game);
                }
            }
        }
        for button in &self.buttons {
            button.follow(game);
        }
        self.item_hover.borrow_mut().follow(game, codex_window);
    }
    pub fn tick(
        &mut self,
        owner: HWND,
        game: HWND,
        visible: bool,
        settings: RecipeSettings,
        process: u32,
        presentation: Presentation,
    ) {
        let Presentation {
            pause,
            modal,
            codex_window,
            codex,
        } = presentation;
        self.cursor.borrow_mut().observe(game, process);
        self.shortcut(owner, game, settings);
        if let Some((prepared, progress)) = &codex {
            let catalogue = &prepared.catalogue;
            self.control.discard_cards(process, |id, research| {
                crate::codex::pins::delivery_complete(catalogue, progress, id)
                    || (catalogue.recipes.contains_key(id)
                        && (!crate::codex::pins::pinnable_recipe(catalogue, id)
                            || (research
                                && !progress.research.contains(id)
                                && progress.recipes.contains(id))))
            });
        }
        if !settings.enabled || !visible || self.assets.is_none() {
            self.hide();
            return;
        }
        let views = self.control.views();
        let cards: Vec<_> = codex
            .as_ref()
            .map(|(prepared, progress)| {
                self.control
                    .card_ids(process)
                    .into_iter()
                    .filter_map(|key| crate::codex::pins::card(&prepared.catalogue, progress, key))
                    .collect()
            })
            .unwrap_or_default();
        let total_pins = views.len() + cards.len();
        if total_pins == 0 {
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
        // SAFETY: Win32 validates the borrowed handle where required; every output points to an
        // initialized local struct or bounded writable slice that stays live for the synchronous
        // query.
        if unsafe { GetClientRect(game, &mut rect) } == 0
            // SAFETY: Win32 validates the borrowed handle where required; every output points to an
            // initialized local struct or bounded writable slice that stays live for the
            // synchronous query.
            || unsafe { ClientToScreen(game, &mut origin) } == 0
        {
            self.hide();
            return;
        }
        let count = views
            .iter()
            .filter_map(|v| v.snapshot.as_ref().map(|s| s.ingredients.len()))
            .chain(cards.iter().map(|card| card.materials.len()))
            .max()
            .unwrap_or(0);
        let Some(geometry) =
            layout::fit([rect.right, rect.bottom], total_pins, count, settings.scale, pause)
        else {
            self.hide();
            return;
        };
        if let Err(error) = self.assets.as_mut().unwrap().prepare_views(&views) {
            self.status = error.to_string();
            self.hide();
            return;
        }
        if let Some((prepared, _)) = &codex {
            self.assets.as_mut().unwrap().prepare_cards(prepared, &cards, geometry);
        } else {
            self.assets.as_mut().unwrap().card_images.clear();
        }
        let group_y = origin.y
            + match settings.position {
                0 => 20,
                2 => (rect.bottom - geometry.height - 20).max(10),
                _ => (rect.bottom - geometry.height) / 2,
            };
        // SAFETY: These Win32 queries use scalar values or borrowed handles, validate native handle
        // identity themselves, and neither dereference Rust memory nor transfer ownership.
        let foreground = unsafe { GetForegroundWindow() };
        let codex_focused = !modal.is_null() && foreground == codex_window;
        let interactive =
            codex_focused || (foreground == game && (!modal.is_null() || mouse::cursor_visible()));
        let mut pointer = POINT::default();
        // SAFETY: Win32 validates the borrowed handle where required; every output points to an
        // initialized local struct or bounded writable slice that stays live for the synchronous
        // query.
        let cursor_valid = interactive && unsafe { GetCursorPos(&mut pointer) } != 0;
        let pointer_root = if cursor_valid {
            // SAFETY: These Win32 queries use scalar values or borrowed handles, validate native
            // handle identity themselves, and neither dereference Rust memory nor transfer
            // ownership.
            unsafe { GetAncestor(WindowFromPoint(pointer), GA_ROOT) }
        } else {
            std::ptr::null_mut()
        };
        let pointer_valid = cursor_valid
            && !pointer_root.is_null()
            && (pointer_root == game
                || pointer_root == modal
                || self.windows.contains(&pointer_root)
                || self.item_hover.borrow().owns(pointer_root));
        let point = [pointer.x, pointer.y];
        let target = if pointer_valid {
            items::hit(
                &views,
                &cards,
                geometry,
                origin.x + rect.right - geometry.right_margin,
                group_y,
                point,
            )
        } else {
            None
        };
        for index in 0..total_pins {
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
            // SAFETY: The UI thread uses owned overlay/menu HWNDs and borrowed comparison handles;
            // native position/style values are scalar, and referenced menu/state objects remain
            // alive through synchronous dispatch.
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
            let frame = layout::frame(index, total_pins);
            let (id, result) = if let Some(view) = views.get(index) {
                let result = render(
                    self.windows[index],
                    self.assets.as_ref().unwrap(),
                    view,
                    geometry.width,
                    height,
                    frame,
                    target.as_ref().filter(|t| t.panel == index).map(|t| t.ingredient),
                );
                (view.snapshot.as_ref().unwrap().recipe_id.as_str(), result)
            } else {
                let card = &cards[index - views.len()];
                let result = render::render_card(
                    self.windows[index],
                    self.assets.as_ref().unwrap(),
                    card,
                    [geometry.width, height],
                    frame,
                    &codex.as_ref().unwrap().0.catalogue,
                    target.as_ref().filter(|t| t.panel == index).map(|t| t.ingredient),
                );
                (card.key.as_str(), result)
            };
            if let Err(error) = result {
                self.status = error.to_string();
                self.hide();
                return;
            }
            if modal.is_null() {
                crate::stacking::follow_game(self.windows[index], game);
            } else {
                crate::stacking::follow_companion(self.windows[index], modal, game);
            }
            if let Err(error) = self.buttons[index].show(
                game,
                self.windows[index],
                id,
                [x + geometry.width - close::SIZE - 6, group_y + layout::control_y(geometry.width)],
                interactive,
            ) {
                self.status = error.to_string();
                self.buttons[index].hide();
            }
        }
        for index in total_pins..3 {
            if !self.windows[index].is_null() {
                // SAFETY: The UI thread uses owned overlay/menu HWNDs and borrowed comparison
                // handles; native position/style values are scalar, and referenced menu/state
                // objects remain alive through synchronous dispatch.
                unsafe {
                    ShowWindow(self.windows[index], SW_HIDE);
                }
            }
            if let Some(button) = self.buttons.get(index) {
                button.hide();
            }
        }
        let hover = self.item_hover.get_mut();
        if hover.select(target.clone(), std::time::Instant::now())
            && let Some(target) = target
            && let Some(name) = if target.panel < views.len() {
                self.assets
                    .as_ref()
                    .unwrap()
                    .catalogue
                    .items
                    .get(&target.item)
                    .map(|item| item.name.as_str())
            } else {
                codex
                    .as_ref()
                    .and_then(|(prepared, _)| prepared.catalogue.items.get(&target.item))
                    .map(|item| item.name.as_str())
            }
        {
            let font =
                (18.0 * (rect.bottom as f64 / 1440.0).clamp(0.65, 2.0)).round().max(12.0) as i32;
            if let Err(error) = hover.show(
                name,
                target.center,
                [origin.x, origin.y, origin.x + rect.right, origin.y + rect.bottom],
                font,
                game,
                // Panels stack from right to left. Anchor above the group, keeping the hovered tile's position.
                self.windows[0],
            ) {
                self.status = format!("Item tooltip unavailable: {error:#}");
                hover.hide();
            }
        }
    }
}

mod close;
pub(crate) mod cursor;
mod items;
mod layout;
mod mouse;
mod setup;
pub(crate) use setup::read_bounded as read_asset_input;
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
            // SAFETY: Only this UI/fixture owner destroys its HWND; boxed callback state remains
            // alive until DestroyWindow completes, and stale handles are validated by Win32.
            unsafe {
                DestroyWindow(self.0);
            }
        }
    }
    // SAFETY: These Win32 queries use scalar values or borrowed handles, validate native handle
    // identity themselves, and neither dereference Rust memory nor transfer ownership.
    let foreground = unsafe { GetForegroundWindow() };
    let class = wide("StarRuptureRecipeShortcutTest");
    // SAFETY: These Win32 queries use scalar values or borrowed handles, validate native handle
    // identity themselves, and neither dereference Rust memory nor transfer ownership.
    let module = unsafe { GetModuleHandleW(std::ptr::null()) };
    let wc = WNDCLASSW {
        lpfnWndProc: Some(DefWindowProcW),
        hInstance: module,
        lpszClassName: class.as_ptr(),
        ..Default::default()
    };
    // SAFETY: The class structure is initialized, its terminated name is live during registration,
    // and the callback has the Win32 ABI and remains in this module.
    unsafe {
        RegisterClassW(&wc);
    }
    let make = || {
        // SAFETY: The registered callback has the Win32 ABI, class/title strings remain live during
        // creation, and any creation-state box stays at a stable address until after the new HWND
        // is destroyed.
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
        // SAFETY: The UI thread owns this window and shortcut ID; registration uses scalar
        // key/modifier values and is released by the same owner before its HWND is destroyed.
        .find(|key| unsafe { RegisterHotKey(a.0, 1, modifiers, *key) } != 0)
        .context("No free diagnostic shortcut")?;
    // SAFETY: The UI thread owns this window and shortcut ID; registration uses scalar key/modifier
    // values and is released by the same owner before its HWND is destroyed.
    let conflict = unsafe { RegisterHotKey(b.0, 2, modifiers, key) } == 0;
    // SAFETY: The UI thread owns this window and shortcut ID; registration uses scalar key/modifier
    // values and is released by the same owner before its HWND is destroyed.
    unsafe {
        UnregisterHotKey(a.0, 1);
    }
    ensure!(conflict, "Duplicate shortcut was not rejected");
    // SAFETY: The UI thread owns this window and shortcut ID; registration uses scalar key/modifier
    // values and is released by the same owner before its HWND is destroyed.
    let released = unsafe { RegisterHotKey(b.0, 2, modifiers, key) } != 0;
    // SAFETY: The UI thread owns this window and shortcut ID; registration uses scalar key/modifier
    // values and is released by the same owner before its HWND is destroyed.
    unsafe {
        UnregisterHotKey(b.0, 2);
    }
    ensure!(released, "Shortcut was not released");
    // SAFETY: These Win32 queries use scalar values or borrowed handles, validate native handle
    // identity themselves, and neither dereference Rust memory nor transfer ownership.
    ensure!(unsafe { GetForegroundWindow() } == foreground, "Recipe diagnostics changed focus");
    Ok(())
}

pub fn diagnostic_catalogue() -> Result<std::path::PathBuf> {
    Ok(setup::diagnostic_assets()?.join("catalogue.json"))
}

pub fn asset_wait_fixture(path: &std::path::Path, child: bool) -> anyhow::Result<()> {
    setup::wait_fixture(path, child)
}
pub(crate) use setup::run_catalogue_child;

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
