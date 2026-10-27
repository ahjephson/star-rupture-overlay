//! Native recipe panel and foreground-scoped shortcut. Owns only app resources.
use crate::{desktop::wide, preferences::RecipeSettings};
use anyhow::{Context, Result, ensure};
use starrupture_observation_model::recipe::{Catalogue, Control, View};
use std::{collections::BTreeMap, mem::size_of, path::Path, sync::Arc};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};

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
        let mut names: Vec<_> = catalogue.items.values().map(|i| i.image.clone()).collect();
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
            let b = std::fs::read(directory.join(&name))?;
            ensure!(b.len() >= 8 && b.len() <= 4 * 1024 * 1024 + 8, "Recipe image bound");
            let width = u32::from_le_bytes(b[0..4].try_into()?) as usize;
            let height = u32::from_le_bytes(b[4..8].try_into()?) as usize;
            ensure!(
                (1..=1024).contains(&width)
                    && (1..=1024).contains(&height)
                    && b.len() == 8 + width * height * 4,
                "Recipe image dimensions"
            );
            total += b.len();
            ensure!(total <= 64 * 1024 * 1024, "Recipe image pack bound");
            images.insert(
                name,
                Image {
                    width,
                    height,
                    pixels: b[8..].to_vec(),
                },
            );
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
            images,
            header_frame,
            header_background,
            font,
            _font_bytes: font_bytes,
        })
    }
}
pub struct Panel {
    window: HWND,
    hotkey_owner: HWND,
    assets: Option<Assets>,
    pub control: Arc<Control>,
    hotkey: Option<RecipeSettings>,
    pub status: String,
    last_settings: RecipeSettings,
}
impl Drop for Panel {
    fn drop(&mut self) {
        self.unregister();
        if !self.window.is_null() {
            unsafe {
                DestroyWindow(self.window);
            }
        }
    }
}
impl Panel {
    pub fn new() -> Self {
        let directory =
            std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.join("recipe-assets")));
        let loaded = directory
            .as_deref()
            .map(Assets::load)
            .unwrap_or_else(|| Err(anyhow::anyhow!("Recipe asset directory unavailable")));
        let (assets, status) = match loaded {
            Ok(a) => (Some(a), "Alt+X pins or clears a recipe.".into()),
            Err(e) => (None, format!("{e:#}")),
        };
        Self {
            window: std::ptr::null_mut(),
            hotkey_owner: std::ptr::null_mut(),
            assets,
            control: Arc::new(Control::default()),
            hotkey: None,
            status,
            last_settings: RecipeSettings::default(),
        }
    }
    pub fn tracker(&self) -> Option<starrupture_overlay_reader::recipe::Tracker> {
        self.assets.as_ref().map(|a| starrupture_overlay_reader::recipe::Tracker {
            catalogue: a.catalogue.clone(),
            control: self.control.clone(),
        })
    }
    fn unregister(&mut self) {
        if self.hotkey.take().is_some() {
            unsafe {
                UnregisterHotKey(self.hotkey_owner, HOTKEY);
            }
        }
    }
    fn window(&mut self) -> Result<()> {
        if !self.window.is_null() {
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
        self.window = w;
        Ok(())
    }
    pub fn shortcut(&mut self, owner: HWND, game: HWND, settings: RecipeSettings) {
        // Hotkeys are delivered to the main window, even though the panel is separate.
        let foreground = !game.is_null() && unsafe { GetForegroundWindow() } == game;
        let desired = foreground && settings.enabled && self.assets.is_some();
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
    pub fn hide(&self) {
        if !self.window.is_null() {
            unsafe {
                ShowWindow(self.window, SW_HIDE);
            }
        }
    }
    pub fn follow(&self, game: HWND) {
        if !self.window.is_null() && unsafe { IsWindowVisible(self.window) } != 0 {
            crate::stacking::follow_game(self.window, game);
        }
    }
    pub fn tick(
        &mut self,
        owner: HWND,
        game: HWND,
        visible: bool,
        settings: RecipeSettings,
        process: u32,
    ) {
        self.shortcut(owner, game, settings);
        if !settings.enabled || !visible || self.assets.is_none() {
            self.hide();
            return;
        }
        let view = self.control.view();
        if view.snapshot.is_none() {
            self.hide();
            return;
        }
        if let Some(s) = &view.snapshot
            && (s.process != process || s.generation != view.generation)
        {
            self.hide();
            return;
        }
        if let Err(e) = self.window() {
            self.status = e.to_string();
            return;
        }
        let mut rect = RECT::default();
        let mut origin = POINT::default();
        if unsafe { GetClientRect(game, &mut rect) } == 0
            || unsafe { ClientToScreen(game, &mut origin) } == 0
        {
            self.hide();
            return;
        }
        let factor = (rect.bottom as f64 / 1440.0).clamp(0.65, 2.0) * settings.scale as f64 / 100.0;
        let count = view.snapshot.as_ref().map_or(0, |s| s.ingredients.len());
        let width = (120.0 * factor).round() as i32;
        let natural = ((155 + count * 98) as f64 * factor).round() as i32;
        let height = natural.min(rect.bottom - 24).max(100);
        let x = origin.x + rect.right - width - 14;
        let y = origin.y
            + match settings.position {
                0 => 30,
                2 => (rect.bottom - height - 20).max(10),
                _ => (rect.bottom - height) / 2,
            };
        unsafe {
            SetWindowPos(
                self.window,
                std::ptr::null_mut(),
                x,
                y,
                width,
                height,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
        if let Some(a) = &self.assets
            && let Err(e) = render(self.window, a, &view, width, height)
        {
            self.status = e.to_string();
            self.hide();
            return;
        }
        self.follow(game);
        crate::stacking::follow_game(self.window, game);
    }
}

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
