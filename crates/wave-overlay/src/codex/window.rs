//! Native, overlay-owned Codex window and input handling.
use super::{
    backdrop::Cursor,
    view::{self, Entry, LevelStatus, Row, Tab},
};
use anyhow::{Result, ensure};
use starrupture_assets::codex::{Icon, Prepared};
use starrupture_observation_model::recipe::{MAX_PINS, PIN_LIMIT_NOTICE};
use starrupture_overlay_reader::{codex::Progress, recipe::Tracker};
use std::{
    collections::{BTreeMap, VecDeque},
    ffi::c_void,
    mem::size_of,
    ptr::null_mut,
    sync::Arc,
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        Controls::{EM_SETCUEBANNER, EM_SETLIMITTEXT, WM_MOUSELEAVE},
        Input::KeyboardAndMouse::*,
        WindowsAndMessaging::*,
    },
};

pub(super) fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}
const INK: u32 = rgb(249, 248, 244);
pub(super) const DIM: u32 = rgb(164, 161, 157);
pub(super) const CYAN: u32 = rgb(125, 225, 232);
const ORANGE: u32 = rgb(252, 159, 62);
const BROWSER_BACK_COMMAND: u32 = 1; // APPCOMMAND_BROWSER_BACKWARD
const BROWSER_FORWARD_COMMAND: u32 = 2; // APPCOMMAND_BROWSER_FORWARD
const HISTORY_LIMIT: usize = 64;
pub(super) const FOOTER_CLICK: u32 = WM_APP + 4;
pub(super) const PANEL: u32 = rgb(12, 11, 10);
const TILE: u32 = rgb(39, 38, 37);
const SEARCH_BACKGROUND: u32 = rgb(33, 31, 29);
const DISABLED_SEARCH_BACKGROUND: u32 = rgb(24, 23, 22);
const SEARCH_HINT: &str = "Search this tab";

const WIDTH: i32 = 1140;
const HEIGHT: i32 = 756;
const CLOSE_HINT_WIDTH: i32 = 109;
const WINDOW_HEIGHT: i32 = HEIGHT + 33;
const TRANSPARENT_COLOUR: u32 = rgb(1, 2, 3);

#[derive(Clone, Copy)]
struct Scale(f64);

impl Scale {
    fn for_client(width: i32, height: i32) -> Option<Self> {
        // At 1920 × 1080 the Codex occupies about 60% of the width and 70% of the height.
        // Retain readable controls in smaller game windows when the extra space is available.
        let reference = (f64::from(width) / 1920.0).min(f64::from(height) / 1080.0);
        let fit = (0.9 * f64::from(width) / f64::from(WIDTH))
            .min(0.9 * f64::from(height) / f64::from(HEIGHT));
        let scale = Self(reference.clamp(0.61, 4.0).min(fit));
        (scale.pixels(WIDTH) >= 680 && scale.pixels(HEIGHT) >= 460).then_some(scale)
    }

    fn pixels(self, length: i32) -> i32 {
        (f64::from(length) * self.0).round() as i32
    }

    fn logical(self, pixel: i32) -> i32 {
        // Invert rounded coordinates at hit-test boundaries.
        ((f64::from(pixel) + 0.5) / self.0).ceil() as i32 - 1
    }

    fn rect(self, bounds: RECT) -> RECT {
        let left = self.pixels(bounds.left);
        let top = self.pixels(bounds.top);
        let right = self.pixels(bounds.right).max(left + i32::from(bounds.right > bounds.left));
        let bottom = self.pixels(bounds.bottom).max(top + i32::from(bounds.bottom > bounds.top));
        rect(left, top, right, bottom)
    }

    fn points<const N: usize>(self, points: [POINT; N]) -> [POINT; N] {
        points.map(|p| point(self.pixels(p.x), self.pixels(p.y)))
    }
}

const fn rgb(r: u32, g: u32, b: u32) -> u32 {
    r | g << 8 | b << 16
}

struct Brush(HBRUSH);
impl Brush {
    fn new(colour: u32) -> Self {
        // SAFETY: GDI returns an owned brush for this colour.
        Self(unsafe { CreateSolidBrush(colour) })
    }
}
impl Drop for Brush {
    fn drop(&mut self) {
        // SAFETY: The brush outlives its parent and child windows.
        unsafe { DeleteObject(self.0) };
    }
}
pub(super) struct Font(HFONT);
impl Font {
    pub(super) fn new(size: i32, weight: i32) -> Self {
        // SAFETY: The terminated face name lives through CreateFontW. The returned font is owned.
        Self(unsafe {
            CreateFontW(-size, 0, 0, 0, weight, 0, 0, 0, 1, 0, 0, 5, 0, wide("Oxanium").as_ptr())
        })
    }
}
impl Drop for Font {
    fn drop(&mut self) {
        // SAFETY: These fonts are restored after drawing and destroyed after their controls.
        unsafe { DeleteObject(self.0) };
    }
}

pub(super) struct Window {
    pub hwnd: HWND,
    state: Box<State>,
    game_focused: bool,
}

#[derive(Clone)]
struct Location {
    tab: Tab,
    query: String,
    entry: Option<Entry>,
    top: usize,
    scroll: i32,
    other_scroll: i32,
    detail_tab: usize,
    corporation_level: Option<u32>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Direction {
    Back,
    Forward,
}

impl Direction {
    fn bounds(self) -> RECT {
        match self {
            Self::Back => rect(125, 23, 203, 52),
            Self::Forward => rect(211, 23, 323, 52),
        }
    }
}

#[derive(Default)]
struct History {
    links: VecDeque<(Location, Location)>,
    position: usize,
}

impl History {
    fn follow(&mut self, source: Location, destination: Location) {
        self.links.truncate(self.position);
        if self.links.len() == HISTORY_LIMIT {
            self.links.pop_front();
        }
        self.links.push_back((source, destination));
        self.position = self.links.len();
    }

    fn can_step(&self, direction: Direction) -> bool {
        match direction {
            Direction::Back => self.position > 0,
            Direction::Forward => self.position < self.links.len(),
        }
    }

    fn step(&mut self, direction: Direction) -> Option<&Location> {
        if !self.can_step(direction) {
            return None;
        }
        match direction {
            Direction::Back => {
                self.position -= 1;
                Some(&self.links[self.position].0)
            }
            Direction::Forward => {
                let destination = &self.links[self.position].1;
                self.position += 1;
                Some(destination)
            }
        }
    }
}

struct State {
    edit: HWND,
    close_requested: bool,
    close_pressed: Option<Hit>,
    search_brush: Brush,
    disabled_search_brush: Brush,
    cursor: Cursor,
    tab: Tab,
    query: String,
    prepared: Arc<Prepared>,
    progress: Arc<Progress>,
    rows: Vec<Row>,
    history: History,
    selected: usize,
    top: usize,
    scroll: i32,
    other_scroll: i32,
    detail_tab: usize,
    detail_tabs: bool,
    corporation_level: Option<u32>,
    heights: Vec<DetailSize>,
    details: Vec<view::Detail>,
    producer: Option<view::Detail>,
    hover: Option<Hit>,
    tracking_mouse: bool,
    pinned: Vec<String>,
    pin_available: bool,
    pin_button: Option<&'static str>,
    pin_key: Option<String>,
    pin_action: Option<(String, bool)>,
    requested_pin: bool,
    pin_notice: String,
    images: Thumbnails,
    title: Font,
    text: Font,
    small: Font,
    scale: Scale,
}

impl State {
    fn location(&self) -> Location {
        Location {
            tab: self.tab,
            query: self.query.clone(),
            entry: self.rows.get(self.selected).map(|row| row.key.clone()),
            top: self.top,
            scroll: self.scroll,
            other_scroll: self.other_scroll,
            detail_tab: self.detail_tab,
            corporation_level: self.corporation_level,
        }
    }

    fn navigate(&mut self, direction: Direction) -> bool {
        let Some(previous) = self.history.step(direction).cloned() else {
            return false;
        };
        self.tab = previous.tab;
        self.query = previous.query;
        self.corporation_level = previous.corporation_level;
        self.top = 0;
        self.refresh(previous.entry.clone());
        if previous
            .entry
            .as_ref()
            .zip(self.rows.get(self.selected))
            .is_some_and(|(entry, row)| entry.same_location(&row.key))
        {
            self.restore_viewport(
                previous.top,
                previous.scroll,
                previous.other_scroll,
                previous.detail_tab,
            );
            if direction == Direction::Forward {
                self.reveal_selection();
            }
        }
        self.hover = None;
        true
    }

    fn restore_viewport(&mut self, top: usize, scroll: i32, other_scroll: i32, detail_tab: usize) {
        self.top = top.min(self.rows.len().saturating_sub(self.visible_rows()));
        if self.detail_tabs {
            self.detail_tab = 1 - detail_tab;
            self.other_scroll = other_scroll.min(self.scroll_max());
            self.detail_tab = detail_tab;
        }
        self.scroll = scroll.min(self.scroll_max());
    }

    fn open_row(&mut self, index: usize) {
        if self.selected != index {
            self.corporation_level = None;
            self.select(index);
        }
    }

    fn refresh(&mut self, keep: Option<Entry>) {
        let (rows, selected) = view::selection(
            &self.prepared.catalogue,
            &self.progress,
            self.tab,
            &self.query,
            keep.as_ref(),
        );
        self.rows = rows;
        self.select(selected);
    }

    fn select(&mut self, index: usize) {
        self.selected = index;
        self.pin_button = None;
        self.scroll = 0;
        self.other_scroll = 0;
        self.detail_tab = 0;
        self.reveal_selection();
        self.details = self
            .rows
            .get(index)
            .map(|row| {
                view::details(&self.prepared.catalogue, &self.progress, row, self.corporation_level)
            })
            .unwrap_or_default();
        self.producer = self
            .details
            .last()
            .is_some_and(|detail| detail.docked)
            .then(|| self.details.pop())
            .flatten();
        self.detail_tabs = self.details.iter().any(|row| row.produced)
            && self.rows.get(index).is_some_and(|row| {
                matches!(&row.key, Entry::Building(id) if self.prepared.catalogue.buildings[id]
                    .unlocks.iter().any(|rule| matches!(rule, starrupture_assets::codex::BuildingUnlock::Upgrade { .. })))
            });
        self.measure_details();
        if self.tab == Tab::Corporations
            && let Some(index) =
                view::corporation_level_index(&self.details, self.corporation_level)
        {
            let offset = self.heights[..index].iter().map(|size| size.height).sum::<i32>();
            self.scroll = offset.min(self.scroll_max());
        }
    }

    fn reveal_selection(&mut self) {
        let count = self.visible_rows();
        self.top = self
            .top
            .clamp((self.selected + 1).saturating_sub(count), self.selected)
            .min(self.rows.len().saturating_sub(count));
    }

    fn visible_rows(&self) -> usize {
        ((HEIGHT - 177) / 57) as usize
    }

    fn split(&self) -> i32 {
        WIDTH * 42 / 100
    }
}

impl Window {
    pub fn open(
        game: HWND,
        prepared: Arc<Prepared>,
        progress: Arc<Progress>,
        cursor: Cursor,
    ) -> Result<Self> {
        let class = wide("StarRuptureOverlayCodex");
        // SAFETY: The current module and shared cursor are borrowed. The class callback lives for the process.
        let module = unsafe { GetModuleHandleW(std::ptr::null()) };
        let wc = WNDCLASSW {
            lpfnWndProc: Some(proc),
            hInstance: module,
            lpszClassName: class.as_ptr(),
            // SAFETY: The shared arrow cursor has process lifetime.
            hCursor: unsafe { LoadCursorW(null_mut(), IDC_ARROW) },
            ..Default::default()
        };
        // SAFETY: The class name, callback and module are valid for registration.
        if unsafe { RegisterClassW(&wc) } == 0 {
            let error = std::io::Error::last_os_error();
            ensure!(error.raw_os_error() == Some(ERROR_CLASS_ALREADY_EXISTS as i32), "{error}");
        }
        let mut client = RECT::default();
        let mut origin = POINT::default();
        // SAFETY: Win32 writes into initialized local storage using the checked game window.
        let geometry = unsafe {
            GetClientRect(game, &mut client) != 0 && ClientToScreen(game, &mut origin) != 0
        };
        ensure!(geometry, "Game client rectangle unavailable");
        let client_width = client.right - client.left;
        let client_height = client.bottom - client.top;
        let scale = Scale::for_client(client_width, client_height)
            .ok_or_else(|| anyhow::anyhow!("Game window is too small for the Codex"))?;
        let width = scale.pixels(WIDTH);
        let height = scale.pixels(HEIGHT);
        let mut state = Box::new(State {
            edit: null_mut(),
            close_requested: false,
            close_pressed: None,
            cursor,
            search_brush: Brush::new(SEARCH_BACKGROUND),
            disabled_search_brush: Brush::new(DISABLED_SEARCH_BACKGROUND),
            tab: Tab::Production,
            query: String::new(),
            prepared,
            progress,
            rows: Vec::new(),
            history: History::default(),
            selected: 0,
            top: 0,
            scroll: 0,
            other_scroll: 0,
            detail_tab: 0,
            detail_tabs: false,
            corporation_level: None,
            heights: Vec::new(),
            details: Vec::new(),
            producer: None,
            hover: None,
            tracking_mouse: false,
            pinned: Vec::new(),
            pin_available: false,
            pin_button: None,
            pin_key: None,
            pin_action: None,
            requested_pin: false,
            pin_notice: String::new(),
            images: Thumbnails::default(),
            title: Font::new(scale.pixels(25), 700),
            text: Font::new(scale.pixels(17), 600),
            small: Font::new(scale.pixels(14), 400),
            scale,
        });
        state.refresh(None);
        let x = origin.x + (client_width - width) / 2;
        let y = origin.y + (client_height - height) / 2;
        // SAFETY: Class/title strings remain live. State is boxed through HWND destruction.
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_LAYERED,
                class.as_ptr(),
                wide("Codex").as_ptr(),
                WS_POPUP | WS_CLIPCHILDREN,
                x,
                y,
                width,
                scale.pixels(WINDOW_HEIGHT),
                null_mut(),
                null_mut(),
                module,
                (&mut *state as *mut State).cast::<c_void>(),
            )
        };
        ensure!(!hwnd.is_null(), "Unable to create the Codex window");
        let result = Self {
            hwnd,
            state,
            game_focused: false,
        };
        // SAFETY: The child Edit lives as long as its parent. All message arguments are local or scalar.
        unsafe {
            let search = scale.rect(rect(22, 101, result.state.split() - 12, 129));
            let edit = CreateWindowExW(
                WS_EX_CLIENTEDGE,
                wide("EDIT").as_ptr(),
                wide("").as_ptr(),
                WS_CHILD | WS_VISIBLE | ES_AUTOHSCROLL as u32,
                search.left,
                search.top,
                search.right - search.left,
                search.bottom - search.top,
                hwnd,
                null_mut(),
                module,
                std::ptr::null(),
            );
            ensure!(!edit.is_null(), "Unable to create Codex search");
            SendMessageW(edit, WM_SETFONT, result.state.text.0 as usize, 0);
            SendMessageW(edit, EM_SETLIMITTEXT, 96, 0);
            let hint = wide(SEARCH_HINT);
            SendMessageW(edit, EM_SETCUEBANNER, 1, hint.as_ptr() as isize);
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
            (*ptr).edit = edit;
            SetLayeredWindowAttributes(hwnd, TRANSPARENT_COLOUR, 236, LWA_ALPHA | LWA_COLORKEY);
            let points = scale.points(outline(0, 0, WIDTH, HEIGHT));
            let region = CreatePolygonRgn(points.as_ptr(), points.len() as i32, WINDING);
            if !region.is_null() {
                let footer = CreateRectRgn(
                    scale.pixels(WIDTH - CLOSE_HINT_WIDTH),
                    scale.pixels(HEIGHT + 4),
                    width,
                    scale.pixels(WINDOW_HEIGHT),
                );
                if !footer.is_null() {
                    CombineRgn(region, region, footer, RGN_OR);
                    DeleteObject(footer);
                }
                if SetWindowRgn(hwnd, region, 0) == 0 {
                    DeleteObject(region);
                }
            }
            ShowWindow(hwnd, SW_SHOW);
            SetForegroundWindow(hwnd);
            SetFocus(edit);
        }
        Ok(result)
    }

    pub fn update(&mut self, progress: Arc<Progress>) {
        if Arc::ptr_eq(&self.state.progress, &progress) {
            return;
        }
        let keep = self.state.rows.get(self.state.selected).map(|row| row.key.clone());
        self.state.progress = progress;
        let top = self.state.top;
        let selection_visible =
            (top..top + self.state.visible_rows()).contains(&self.state.selected);
        let (scroll, other_scroll, tab) =
            (self.state.scroll, self.state.other_scroll, self.state.detail_tab);
        self.state.refresh(keep.clone());
        if keep
            .as_ref()
            .zip(self.state.rows.get(self.state.selected))
            .is_some_and(|(entry, row)| entry.same_location(&row.key))
        {
            self.state.restore_viewport(top, scroll, other_scroll, tab);
            if selection_visible {
                self.state.reveal_selection();
            }
        }
        // SAFETY: Invalidation queues paint for the owned window.
        unsafe { InvalidateRect(self.hwnd, std::ptr::null(), 0) };
    }

    pub fn visible(&self) -> bool {
        // SAFETY: The HWND is owned by this window.
        unsafe { IsWindowVisible(self.hwnd) != 0 }
    }

    pub fn close_requested(&self) -> bool {
        self.state.close_requested
    }

    pub fn hide(&mut self, game: HWND, pid: u32) {
        self.game_focused = false;
        self.state.close_requested = false;
        self.state.close_pressed = None;
        self.state.hover = None;
        self.state.pin_action = None;
        // SAFETY: Use normal Windows activation while the closing Codex still owns foreground.
        // Check the borrowed game's identity first. Only the owned Codex window is hidden.
        unsafe {
            let mut owner = 0;
            if GetForegroundWindow() == self.hwnd
                && pid != 0
                && GetWindowThreadProcessId(game, &mut owner) != 0
                && owner == pid
                && IsWindowVisible(game) != 0
                && IsIconic(game) == 0
            {
                SetForegroundWindow(game);
            }
            ShowWindow(self.hwnd, SW_HIDE);
        }
    }

    pub fn show(&mut self, game: HWND) {
        let focus = self.focus_target();
        // SAFETY: The parent and child HWNDs are live; native messages retain no Rust borrow.
        unsafe {
            let mut anchor = POINT::default();
            let mut client = RECT::default();
            if GetClientRect(game, &mut client) != 0 && ClientToScreen(game, &mut anchor) != 0 {
                let x = anchor.x + (client.right - self.state.scale.pixels(WIDTH)) / 2;
                let y = anchor.y + (client.bottom - self.state.scale.pixels(HEIGHT)) / 2;
                SetWindowPos(
                    self.hwnd,
                    null_mut(),
                    x,
                    y,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
            ShowWindow(self.hwnd, SW_SHOW);
            SetForegroundWindow(self.hwnd);
            SetFocus(focus);
        }
    }

    pub fn follow_focus(&mut self, game: HWND) {
        let focused = crate::compass::focused(game);
        if focused && !self.game_focused {
            // SAFETY: Only reclaim input after the game itself becomes foreground.
            unsafe {
                SetWindowPos(
                    self.hwnd,
                    HWND_TOP,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
                if SetForegroundWindow(self.hwnd) != 0 {
                    SetFocus(self.focus_target());
                }
            }
        }
        self.game_focused = focused;
    }

    fn focus_target(&self) -> HWND {
        if self.state.tab.search_enabled() {
            self.state.edit
        } else {
            self.hwnd
        }
    }

    pub fn matches_game_scale(&self, game: HWND) -> bool {
        let mut client = RECT::default();
        // SAFETY: The checked game HWND and initialized rectangle are borrowed for this query.
        if unsafe { GetClientRect(game, &mut client) } == 0 {
            return false;
        }
        Scale::for_client(client.right, client.bottom)
            .is_some_and(|scale| scale.0 == self.state.scale.0)
    }

    pub fn pins(&mut self, tracker: Option<&Tracker>, pid: u32) {
        let s = &mut self.state;
        let tracker = tracker.filter(|tracker| {
            tracker.catalogue.game_hash == s.prepared.catalogue.context.game_hash
        });
        let available = tracker.is_some();
        let mut pinned = Vec::new();
        let mut button = None;
        let mut notice = String::new();
        if let Some(tracker) = tracker {
            pinned = tracker.control.pinned_ids(pid);
            pinned.extend(tracker.control.card_ids(pid));
            let catalogue = &s.prepared.catalogue;
            let eligible = |id: &str| {
                s.progress.research.contains(id) && tracker.catalogue.recipes.contains_key(id)
            };
            let key = s.rows.get(s.selected).and_then(|row| super::pins::key(catalogue, &row.key));
            if let Some((id, add)) = s.pin_action.take() {
                if !add {
                    tracker.control.remove(&id);
                    pinned.retain(|pin| pin != &id);
                    s.requested_pin = false;
                } else if pinned.len() < MAX_PINS
                    && (key.as_deref() == Some(id.as_str())
                        || s.details.iter().any(|detail| detail.pin.as_ref() == Some(&id)))
                {
                    if eligible(&id) {
                        tracker.control.select_catalogue(&id);
                    } else {
                        tracker.control.pin_card(&id, pid, s.progress.research.contains(&id));
                        pinned = tracker.control.pinned_ids(pid);
                        pinned.extend(tracker.control.card_ids(pid));
                    }
                    s.requested_pin = true;
                }
            }
            if s.requested_pin {
                let feedback = tracker.control.notice();
                if feedback != PIN_LIMIT_NOTICE {
                    notice = feedback;
                }
            }
            button = key.as_ref().map(|id| {
                if pinned.contains(id) {
                    "Unpin"
                } else {
                    "Pin recipe"
                }
            });
            s.pin_key = key;
        } else {
            s.pin_action = None;
            s.requested_pin = false;
        }
        if (s.pinned.as_slice(), s.pin_button, s.pin_notice.as_str())
            != (pinned.as_slice(), button, notice.as_str())
            || s.pin_available != available
        {
            s.pin_available = available;
            s.pinned = pinned;
            s.pin_button = button;
            s.pin_notice = notice;
            s.scroll = s.scroll.min(s.scroll_max());
            // SAFETY: Repaint is queued for the live, owned window.
            unsafe { InvalidateRect(self.hwnd, std::ptr::null(), 0) };
        }
    }
    pub fn key(&mut self, message: &MSG) -> bool {
        if !self.handles(message.hwnd) {
            return false;
        }
        if message.message == WM_SYSKEYDOWN && message.lParam & (1 << 29) != 0 {
            let command = match message.wParam as u16 {
                VK_LEFT => BROWSER_BACK_COMMAND,
                VK_RIGHT => BROWSER_FORWARD_COMMAND,
                _ => return false,
            };
            // SAFETY: The command carries scalar values and targets the owned window.
            unsafe { PostMessageW(self.hwnd, WM_APPCOMMAND, 0, (command << 16) as isize) };
            return true;
        }
        if message.message != WM_KEYDOWN {
            return false;
        }
        let next = match message.wParam as u16 {
            VK_DOWN => self.state.selected.saturating_add(1),
            VK_UP => self.state.selected.saturating_sub(1),
            _ => return false,
        };
        if next < self.state.rows.len() {
            self.state.open_row(next);
            // SAFETY: The Codex owns the HWND.
            unsafe { InvalidateRect(self.hwnd, std::ptr::null(), 0) };
        }
        true
    }

    pub fn handles(&self, hwnd: HWND) -> bool {
        hwnd == self.hwnd || hwnd == self.state.edit
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        // SAFETY: The HWND still owns its native children and State outlives their destruction.
        unsafe {
            SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
            DestroyWindow(self.hwnd);
        }
    }
}

fn point(x: i32, y: i32) -> POINT {
    POINT {
        x,
        y,
    }
}
pub(super) fn rect(left: i32, top: i32, right: i32, bottom: i32) -> RECT {
    RECT {
        left,
        top,
        right,
        bottom,
    }
}

pub(super) fn outline(x: i32, y: i32, r: i32, b: i32) -> [POINT; 7] {
    [
        point(x, y),
        point(x + 154, y),
        point(x + 169, y + 12),
        point(r, y + 12),
        point(r, b - 17),
        point(r - 17, b),
        point(x, b),
    ]
}

type ThumbnailKey = (String, i32, bool, bool);

#[derive(Clone, Copy, Default)]
struct TileStyle {
    locked: bool,
    warm: bool,
    hovered: bool,
}

#[derive(Default)]
struct Thumbnails(BTreeMap<ThumbnailKey, Option<Vec<u8>>>);
impl Thumbnails {
    fn get(&mut self, prepared: &Prepared, id: &str, size: i32, style: TileStyle) -> Option<&[u8]> {
        let key = (id.to_owned(), size, style.locked, style.warm);
        if self.0.len() >= 96 && !self.0.contains_key(&key) {
            self.0.clear();
        }
        self.0
            .entry(key)
            .or_insert_with(|| {
                let icon = prepared.icon(id).ok()?;
                let mut tile = thumbnail(&icon, size as usize, style);
                if style.locked
                    && let Some(icon) = prepared
                        .catalogue
                        .lock_icon
                        .as_deref()
                        .and_then(|key| prepared.icon(key).ok())
                {
                    let side = size as usize;
                    let badge = (side / 2).max(1);
                    let offset = (side - badge) / 2;
                    for y in 0..badge {
                        for x in 0..badge {
                            let from = (y * icon.height as usize / badge * icon.width as usize
                                + x * icon.width as usize / badge)
                                * 4;
                            let to = ((offset + y) * side + offset + x) * 4;
                            let alpha = u32::from(icon.pixels[from + 3]);
                            for channel in 0..3 {
                                let front = u32::from(icon.pixels[from + channel]);
                                let back = u32::from(tile[to + channel]) * (255 - alpha) / 255;
                                tile[to + channel] = (front + back).min(255) as u8;
                            }
                        }
                    }
                }
                Some(tile)
            })
            .as_deref()
    }
}

fn thumbnail(icon: &Icon, size: usize, style: TileStyle) -> Vec<u8> {
    let width = icon.width as usize;
    let height = icon.height as usize;
    let longest = width.max(height);
    let w = (size * width / longest).max(1);
    let h = (size * height / longest).max(1);
    let inset = ((size - w) / 2, (size - h) / 2);
    let base = if style.warm {
        [94, 104, 117, 0]
    } else {
        [43, 42, 41, 0]
    };
    let mut result = base.repeat(size * size);
    let fade = if style.locked {
        95
    } else {
        255
    };
    for y in 0..h {
        for x in 0..w {
            let source = (y * height / h * width + x * width / w) * 4;
            let target = ((inset.1 + y) * size + inset.0 + x) * 4;
            let alpha = u32::from(icon.pixels[source + 3]) * fade / 255;
            for channel in 0..3 {
                let pixel = u32::from(icon.pixels[source + channel]) * fade / 255;
                let background = u32::from(result[target + channel]) * (255 - alpha) / 255;
                result[target + channel] = (pixel + background).min(255) as u8;
            }
        }
    }
    result
}

pub(super) struct Drawing(HDC, Scale);
impl Drawing {
    pub(super) fn unscaled(dc: HDC) -> Self {
        Self(dc, Scale(1.0))
    }
    fn fill(&self, r: RECT, colour: u32) {
        let r = self.1.rect(r);
        // SAFETY: The paint DC is live and we release the owned solid brush after use.
        unsafe {
            let brush = CreateSolidBrush(colour);
            FillRect(self.0, &r, brush);
            DeleteObject(brush);
        }
    }
    pub(super) fn polygon<const N: usize>(&self, points: &[POINT; N], colour: u32, edge: u32) {
        let points = self.1.points(*points);
        // SAFETY: Points are bounded stack/slice data. GDI objects are restored before release.
        unsafe {
            let brush = CreateSolidBrush(colour);
            let pen = CreatePen(PS_SOLID, self.1.pixels(1).max(1), edge);
            if brush.is_null() || pen.is_null() {
                DeleteObject(brush);
                DeleteObject(pen);
                return;
            }
            let old_brush = SelectObject(self.0, brush);
            let old_pen = SelectObject(self.0, pen);
            Polygon(self.0, points.as_ptr(), points.len() as i32);
            SelectObject(self.0, old_pen);
            SelectObject(self.0, old_brush);
            DeleteObject(pen);
            DeleteObject(brush);
        }
    }
    pub(super) fn label(&self, font: &Font, text: &str, bounds: RECT, colour: u32, flags: u32) {
        // SAFETY: Text and rectangle stay live for synchronous drawing. Font remains owned by State.
        let text = wide(text);
        let mut bounds = self.1.rect(bounds);
        // SAFETY: The UTF-16 slice and selected font remain live during drawing.
        unsafe {
            let old = SelectObject(self.0, font.0);
            SetBkMode(self.0, TRANSPARENT as i32);
            SetTextColor(self.0, colour);
            DrawTextW(
                self.0,
                text.as_ptr(),
                (text.len() - 1) as i32,
                &mut bounds,
                flags | DT_NOPREFIX,
            );
            SelectObject(self.0, old);
        }
    }
    fn label_width(&self, font: &Font, text: &str) -> i32 {
        let text = wide(text);
        let mut size = SIZE::default();
        // SAFETY: The font and UTF-16 text stay live until the original font is restored.
        unsafe {
            let old = SelectObject(self.0, font.0);
            GetTextExtentPoint32W(self.0, text.as_ptr(), (text.len() - 1) as i32, &mut size);
            SelectObject(self.0, old);
        }
        self.1.logical(size.cx)
    }

    fn padlock(&self, x: i32, y: i32, colour: u32, background: u32) {
        self.polygon(
            &[
                point(x - 6, y),
                point(x - 6, y - 6),
                point(x - 3, y - 9),
                point(x + 3, y - 9),
                point(x + 6, y - 6),
                point(x + 6, y),
            ],
            background,
            colour,
        );
        self.fill(rect(x - 8, y - 2, x + 8, y + 10), colour);
        self.fill(rect(x - 1, y + 2, x + 2, y + 7), background);
    }

    fn close_button(&self, hovered: bool) {
        let bounds = close_bounds();
        let x = (bounds.left + bounds.right) / 2;
        let y = (bounds.top + bounds.bottom) / 2;
        let colour = if hovered {
            rgb(117, 224, 230)
        } else {
            rgb(240, 240, 240)
        };
        let reach = self.1.pixels(5);
        let x = self.1.pixels(x);
        let y = self.1.pixels(y);
        // SAFETY: The live paint DC borrows the pen until it is deselected and released.
        unsafe {
            let pen = CreatePen(PS_SOLID, self.1.pixels(1).max(1), colour);
            if pen.is_null() {
                return;
            }
            let previous = SelectObject(self.0, pen);
            MoveToEx(self.0, x - reach, y - reach, null_mut());
            LineTo(self.0, x + reach + 1, y + reach + 1);
            MoveToEx(self.0, x - reach, y + reach, null_mut());
            LineTo(self.0, x + reach + 1, y - reach - 1);
            SelectObject(self.0, previous);
            DeleteObject(pen);
        }
    }

    fn icon(
        &self,
        cache: &mut Thumbnails,
        prepared: &Prepared,
        id: Option<&str>,
        position: (i32, i32),
        size: i32,
        style: TileStyle,
    ) {
        let (x, y) = position;
        self.fill(rect(x, y, x + size, y + size), TILE);
        let side = self.1.pixels(size);
        if let Some(tile) = id.and_then(|id| cache.get(prepared, id, side, style)) {
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: side,
                    biHeight: -side,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB,
                    ..Default::default()
                },
                ..Default::default()
            };
            // SAFETY: A complete BGRA tile of the advertised size lives through this blit.
            unsafe {
                SetDIBitsToDevice(
                    self.0,
                    self.1.pixels(x),
                    self.1.pixels(y),
                    side as u32,
                    side as u32,
                    0,
                    0,
                    0,
                    side as u32,
                    tile.as_ptr().cast(),
                    &info,
                    DIB_RGB_COLORS,
                );
            }
        }
        if style.hovered {
            let edge = rgb(213, 222, 220);
            self.fill(rect(x, y, x + size, y + 1), edge);
            self.fill(rect(x, y + size - 1, x + size, y + size), edge);
            self.fill(rect(x, y, x + 1, y + size), edge);
            self.fill(rect(x + size - 1, y, x + size, y + size), edge);
        }
        if style.locked && (prepared.catalogue.lock_icon.is_none() || id.is_none()) {
            self.padlock(x + size / 2, y + size / 2, INK, TILE);
        }
    }
}

const DETAILS_TOP: i32 = 201;
const HOVER: u32 = rgb(51, 49, 47);
const TILE_GAP: i32 = 8;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Hit {
    Navigate(Direction),
    Row(usize),
    Detail(usize),
    Tile(usize, usize),
    Producer,
    Pin,
    DetailPin(usize),
    DetailTab(usize),
    Close,
    CloseHint,
}

impl State {
    fn detail_end(&self) -> i32 {
        HEIGHT
            - if self.pin_notice.is_empty() {
                14
            } else {
                36
            }
    }

    fn producer_top(&self) -> i32 {
        self.detail_end() - 58
    }

    fn detail_bottom(&self) -> i32 {
        if self.producer.is_some() {
            self.producer_top() - 10
        } else {
            self.detail_end()
        }
    }

    fn detail_range(&self) -> std::ops::Range<usize> {
        if !self.detail_tabs {
            return 0..self.details.len();
        }
        let produced = self.details.iter().position(|row| row.produced).unwrap();
        if self.detail_tab == 0 {
            produced + 1..self.details.len()
        } else {
            0..produced
        }
    }

    fn detail_top(&self) -> i32 {
        DETAILS_TOP
            + if self.detail_tabs {
                32
            } else {
                0
            }
    }

    fn fixed_detail_count(&self) -> usize {
        if self.detail_tabs {
            return 0;
        }
        // Keep the overview and Produced items heading above its scrolling rows.
        self.details.iter().position(|row| row.produced).map_or(0, |start| {
            let fixed = start + 1;
            let height = self.heights[..fixed].iter().map(|size| size.height).sum::<i32>();
            if DETAILS_TOP + height + 58 <= self.detail_bottom() {
                fixed
            } else {
                0
            }
        })
    }

    fn scroll_top(&self) -> i32 {
        self.detail_top()
            + self.heights[..self.fixed_detail_count()].iter().map(|size| size.height).sum::<i32>()
    }

    fn scroll_max(&self) -> i32 {
        let range = self.detail_range();
        let start = range.start.max(self.fixed_detail_count());
        let content = self.heights[start..range.end].iter().map(|size| size.height).sum::<i32>();
        (content - (self.detail_bottom() - self.scroll_top())).max(0)
    }

    fn detail_rows(&self) -> impl Iterator<Item = (usize, i32, i32)> + '_ {
        let fixed = self.fixed_detail_count();
        let range = self.detail_range();
        let start = fixed.max(range.start);
        let scroll_top = self.scroll_top();
        let mut next = self.detail_top();
        range.filter_map(move |index| {
            if index == start {
                next -= self.scroll;
            }
            let y = next;
            let height = self.heights[index].height;
            next += height;
            let clip = if index < fixed {
                self.detail_top()
            } else {
                scroll_top
            };
            (next > clip && y < self.detail_bottom()).then_some((index, y, height))
        })
    }

    fn detail_tab_bounds(&self, tab: usize) -> RECT {
        let x = self.split() + 20 + tab as i32 * 204;
        rect(x, DETAILS_TOP, x + 196, DETAILS_TOP + 25)
    }

    fn scrollbars(&self) -> [Option<Scrollbar>; 2] {
        let height = self.visible_rows() as i32 * 57 - 4;
        [
            Scrollbar::new(
                self.split() - 9,
                140,
                140 + height,
                self.top as i32,
                self.rows.len().saturating_sub(self.visible_rows()) as i32,
                self.visible_rows() as i32,
            ),
            Scrollbar::new(
                WIDTH - 19,
                self.scroll_top(),
                self.detail_bottom(),
                self.scroll,
                self.scroll_max(),
                self.detail_bottom() - self.scroll_top(),
            ),
        ]
    }

    fn measure_details(&mut self) {
        // SAFETY: The private memory DC is used only for text metrics, then released.
        unsafe {
            let dc = CreateCompatibleDC(null_mut());
            self.heights =
                self.details.iter().map(|detail| self.measure_detail(dc, detail)).collect();
            if !dc.is_null() {
                DeleteDC(dc);
            }
        }
    }

    fn measure_detail(&self, dc: HDC, detail: &view::Detail) -> DetailSize {
        let x = self.split() + 20;
        let measure = |font: &Font, label: &str, width| {
            if label.is_empty() {
                return 0;
            }
            if dc.is_null() {
                return 24;
            }
            let mut bounds = rect(0, 0, self.scale.pixels(width), 0);
            let label = wide(label);
            // SAFETY: The selected font, text and bounds live through DrawTextW.
            unsafe {
                let previous = SelectObject(dc, font.0);
                DrawTextW(
                    dc,
                    label.as_ptr(),
                    -1,
                    &mut bounds,
                    DT_CALCRECT | DT_WORDBREAK | DT_NOPREFIX,
                );
                SelectObject(dc, previous);
            }
            (f64::from(bounds.bottom) / self.scale.0).ceil() as i32
        };
        if detail.section {
            return DetailSize {
                height: 30,
                title: 0,
            };
        }
        if !detail.tiles.is_empty() {
            let width = tile_width(x) - 12;
            let title = detail
                .tiles
                .iter()
                .map(|tile| measure(&self.small, &tile.title, width))
                .max()
                .unwrap_or(0);
            let material = detail
                .tiles
                .iter()
                .map(|tile| {
                    if tile.materials.is_empty() {
                        0
                    } else {
                        measure(&self.small, &format!("Equip · {}", tile.materials), width)
                    }
                })
                .max()
                .unwrap_or(0);
            return DetailSize {
                height: 62 + title + 24 + material + 14,
                title,
            };
        }
        let width = detail_text_right(detail, detail.pin.is_some()) - detail_text_x(x, detail);
        let heading = if detail.heading.is_empty() {
            0
        } else {
            19
        };
        let body = measure(&self.text, &detail.body, width);
        DetailSize {
            height: (heading + body + 16).max(if detail.icon.is_some() {
                58
            } else {
                36
            }),
            title: 0,
        }
    }

    fn pin_bounds(&self) -> RECT {
        rect(WIDTH - 134, 114, WIDTH - 23, 145)
    }

    fn pin_enabled(&self, key: &str) -> bool {
        self.pinned.len() < MAX_PINS || self.pinned.iter().any(|pinned| pinned == key)
    }

    fn hit(&self, x: i32, y: i32) -> Option<Hit> {
        let within = |r: RECT| (r.left..r.right).contains(&x) && (r.top..r.bottom).contains(&y);
        for direction in [Direction::Back, Direction::Forward] {
            if within(direction.bounds()) && self.history.can_step(direction) {
                return Some(Hit::Navigate(direction));
            }
        }
        if within(close_bounds()) {
            return Some(Hit::Close);
        }
        if within(close_hint_bounds()) {
            return Some(Hit::CloseHint);
        }
        if self.pin_button.is_some() && within(self.pin_bounds()) {
            return self.pin_key.as_deref().filter(|key| self.pin_enabled(key)).map(|_| Hit::Pin);
        }
        if within(rect(22, 140, self.split() - 12, 140 + self.visible_rows() as i32 * 57)) {
            let row = self.top + ((y - 140) / 57) as usize;
            return (row < self.rows.len() && (y - 140) % 57 < 53).then_some(Hit::Row(row));
        }
        let left = self.split() + 20;
        if !(left..WIDTH - 22).contains(&x) {
            return None;
        }
        if self.detail_tabs {
            for tab in 0..2 {
                if within(self.detail_tab_bounds(tab)) {
                    return Some(Hit::DetailTab(tab));
                }
            }
        }
        if self.producer.is_some() && (self.producer_top()..self.detail_end()).contains(&y) {
            return Some(Hit::Producer);
        }
        for (index, top, height) in self.detail_rows() {
            let clip = if index < self.fixed_detail_count() {
                self.detail_top()
            } else {
                self.scroll_top()
            };
            if (top.max(clip)..(top + height).min(self.detail_bottom())).contains(&y) {
                let detail = &self.details[index];
                if self.pin_available
                    && within(detail_pin_bounds(top))
                    && let Some(key) = &detail.pin
                {
                    return self.pin_enabled(key).then_some(Hit::DetailPin(index));
                }
                if !detail.tiles.is_empty() {
                    let pitch = tile_width(left) + TILE_GAP;
                    let slot = ((x - left) / pitch) as usize;
                    return (slot < detail.tiles.len() && (x - left) % pitch < pitch - TILE_GAP)
                        .then_some(Hit::Tile(index, slot));
                }
                return (detail.link.is_some() || detail.warm_icon).then_some(Hit::Detail(index));
            }
        }
        None
    }

    fn activate(&mut self, hit: Hit) -> bool {
        let link = match hit {
            Hit::Navigate(direction) => return self.navigate(direction),
            Hit::Row(index) => {
                self.open_row(index);
                return false;
            }
            Hit::Pin => {
                if let Some(key) = &self.pin_key {
                    self.pin_action = Some((key.clone(), self.pin_button != Some("Unpin")));
                }
                return false;
            }
            Hit::DetailPin(index) => {
                if let Some(key) = &self.details[index].pin {
                    self.pin_action = Some((key.clone(), !self.pinned.contains(key)));
                }
                return false;
            }
            Hit::Detail(index) => self.details[index].link.clone(),
            Hit::Producer => self.producer.as_ref().and_then(|detail| detail.link.clone()),
            Hit::DetailTab(tab) => {
                if self.detail_tab != tab {
                    std::mem::swap(&mut self.scroll, &mut self.other_scroll);
                    self.detail_tab = tab;
                    self.scroll = self.scroll.min(self.scroll_max());
                    self.hover = None;
                }
                return false;
            }
            Hit::Tile(..) | Hit::Close | Hit::CloseHint => None,
        };
        if let Some((tab, entry)) = link {
            let corporation_level = match hit {
                Hit::Detail(index) => self.details[index].corporation_level,
                _ => None,
            };
            if self.tab == tab
                && self.rows.get(self.selected).is_some_and(|row| row.key == entry)
                && self.corporation_level == corporation_level
            {
                return false;
            }
            let source = self.location();
            self.corporation_level = corporation_level;
            self.tab = tab;
            if tab.search_enabled() {
                self.query.clear();
            }
            self.top = 0;
            self.refresh(Some(entry));
            self.history.follow(source, self.location());
            self.hover = None;
            return true;
        }
        false
    }
}

struct DetailSize {
    height: i32,
    title: i32,
}

struct Scrollbar {
    bounds: RECT,
    thumb: RECT,
    max: i32,
}

impl Scrollbar {
    fn new(x: i32, top: i32, bottom: i32, offset: i32, max: i32, visible: i32) -> Option<Self> {
        let length = bottom - top;
        if max == 0 || length < 16 {
            return None;
        }
        let height = (length * visible / (visible + max)).max(12);
        let y = top + (length - height) * offset / max;
        Some(Self {
            bounds: rect(x - 3, top, x + 6, bottom),
            thumb: rect(x, y, x + 3, y + height),
            max,
        })
    }

    fn offset(&self, y: i32) -> i32 {
        let height = self.thumb.bottom - self.thumb.top;
        let travel = self.bounds.bottom - self.bounds.top - height;
        if travel == 0 {
            return 0;
        }
        ((y - self.bounds.top - height / 2) * self.max / travel).clamp(0, self.max)
    }
}

fn detail_text_x(left: i32, detail: &view::Detail) -> i32 {
    left + if detail.icon.is_some() {
        60
    } else {
        3
    }
}

fn detail_text_right(detail: &view::Detail, pin: bool) -> i32 {
    if pin {
        WIDTH - 142
    } else if detail.link.is_some() {
        WIDTH - 46
    } else {
        WIDTH - 28
    }
}

fn tile_width(left: i32) -> i32 {
    (WIDTH - 22 - left - TILE_GAP * 2) / 3
}

fn close_hint_bounds() -> RECT {
    rect(WIDTH - CLOSE_HINT_WIDTH, HEIGHT + 4, WIDTH, WINDOW_HEIGHT)
}

fn close_bounds() -> RECT {
    let size = 28;
    rect(WIDTH - 22 - size, 23, WIDTH - 22, 23 + size)
}

fn detail_pin_bounds(top: i32) -> RECT {
    rect(WIDTH - 134, top + 10, WIDTH - 23, top + 41)
}

fn paint_detail(
    d: &Drawing,
    s: &mut State,
    detail: &view::Detail,
    top: i32,
    height: i32,
    hover: bool,
    pin_hover: bool,
) {
    let x = s.split() + 20;
    let right = WIDTH - 23;
    let inner = DT_END_ELLIPSIS | DT_SINGLELINE;
    if detail.section {
        let bounds = rect(x, top + 6, right, top + 29);
        let colour = if detail.level_status == Some(LevelStatus::Locked) {
            DIM
        } else {
            CYAN
        };
        d.label(&s.text, &detail.heading, bounds, colour, inner);
        if let Some(status) = detail.level_status {
            let badge = x + d.label_width(&s.text, &detail.heading) + 10;
            let y = top + 16;
            match status {
                LevelStatus::Unlocked => d.polygon(
                    &[
                        point(badge, y),
                        point(badge + 2, y - 2),
                        point(badge + 6, y + 2),
                        point(badge + 13, y - 5),
                        point(badge + 15, y - 3),
                        point(badge + 6, y + 6),
                    ],
                    CYAN,
                    CYAN,
                ),
                LevelStatus::Next => d.label(
                    &s.small,
                    "Next level",
                    rect(badge, bounds.top, right, bounds.bottom),
                    ORANGE,
                    inner | DT_VCENTER,
                ),
                LevelStatus::Locked => d.padlock(badge + 8, y, DIM, PANEL),
            }
        }
        return;
    }
    if hover {
        d.fill(rect(x, top, right, top + height - 4), HOVER);
    }
    if let Some(icon) = &detail.icon {
        d.icon(
            &mut s.images,
            &s.prepared,
            Some(icon),
            (x + 6, top + (height - 4 - 42) / 2),
            42,
            TileStyle {
                locked: detail.locked,
                warm: detail.warm_icon,
                hovered: hover,
            },
        );
    }
    let text_x = detail_text_x(x, detail);
    let pin = detail.pin.as_ref().filter(|_| s.pin_available);
    if let Some(key) = pin {
        let bounds = detail_pin_bounds(top);
        let label = if s.pinned.contains(key) {
            "Unpin"
        } else {
            "Pin upgrade"
        };
        paint_pin_button(d, &s.small, label, bounds, s.pin_enabled(key), pin_hover);
    }
    let text_right = detail_text_right(detail, pin.is_some());
    let body_top = if detail.heading.is_empty() {
        if detail.icon.is_some() {
            top + (height - 24) / 2
        } else {
            top + 5
        }
    } else {
        d.label(
            &s.small,
            &detail.heading,
            rect(text_x, top + 5, text_right, top + 24),
            CYAN,
            inner,
        );
        top + 24
    };
    d.label(
        &s.text,
        &detail.body,
        rect(text_x, body_top, text_right, top + height - 4),
        INK,
        DT_WORDBREAK,
    );
    if detail.link.is_some() {
        d.label(
            &s.text,
            "›",
            rect(right - 18, top, right - 5, top + height - 4),
            CYAN,
            DT_SINGLELINE | DT_VCENTER | DT_CENTER,
        );
    }
}

fn paint_tiles(d: &Drawing, s: &mut State, index: usize, detail: &view::Detail, y: i32) {
    let left = s.split() + 20;
    let width = tile_width(left);
    let size = &s.heights[index];
    let status_top = y + 64 + size.title;
    let bottom = y + size.height - 5;
    for (slot, tile) in detail.tiles.iter().enumerate() {
        let x = left + slot as i32 * (width + TILE_GAP);
        let right = x + width;
        let hovered = s.hover == Some(Hit::Tile(index, slot));
        d.fill(
            rect(x, y, right, bottom),
            if hovered {
                HOVER
            } else {
                rgb(28, 27, 26)
            },
        );
        d.icon(
            &mut s.images,
            &s.prepared,
            tile.icon.as_deref(),
            (x + (width - 54) / 2, y + 5),
            54,
            TileStyle {
                locked: tile.locked,
                hovered,
                ..TileStyle::default()
            },
        );
        d.label(
            &s.small,
            &tile.title,
            rect(x + 6, y + 62, right - 6, status_top),
            INK,
            DT_CENTER | DT_WORDBREAK,
        );
        d.label(
            &s.small,
            &tile.status,
            rect(x + 6, status_top, right - 6, status_top + 19),
            if tile.locked {
                ORANGE
            } else {
                CYAN
            },
            DT_CENTER | DT_SINGLELINE,
        );
        if !tile.materials.is_empty() {
            d.label(
                &s.small,
                &format!("Equip · {}", tile.materials),
                rect(x + 6, status_top + 20, right - 6, bottom),
                DIM,
                DT_CENTER | DT_WORDBREAK,
            );
        }
    }
}

fn paint_pin_button(
    d: &Drawing,
    font: &Font,
    label: &str,
    bounds: RECT,
    enabled: bool,
    hovered: bool,
) {
    let (background, foreground) = if !enabled {
        (rgb(37, 35, 33), rgb(138, 135, 131))
    } else if hovered {
        (rgb(42, 78, 82), CYAN)
    } else {
        (HOVER, CYAN)
    };
    d.fill(bounds, background);
    d.label(font, label, bounds, foreground, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
}

fn paint(dc: HDC, s: &mut State) {
    let d = Drawing(dc, s.scale);
    let (w, h) = (WIDTH, HEIGHT);
    d.fill(rect(0, 0, w, WINDOW_HEIGHT), TRANSPARENT_COLOUR);
    let split = s.split();
    d.polygon(&outline(0, 0, w - 1, h - 1), PANEL, DIM);
    d.label(&s.title, "CODEX", rect(22, 23, 180, 61), INK, DT_SINGLELINE);
    for (direction, label) in [(Direction::Back, "‹ Back"), (Direction::Forward, "Forward ›")] {
        let bounds = direction.bounds();
        let colour = if !s.history.can_step(direction) {
            rgb(99, 96, 92)
        } else if s.hover == Some(Hit::Navigate(direction)) {
            d.fill(bounds, rgb(33, 67, 71));
            CYAN
        } else {
            INK
        };
        d.label(&s.text, label, bounds, colour, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
    }
    d.close_button(s.hover == Some(Hit::Close));
    d.fill(rect(22, 88, w - 22, 89), DIM);
    for (i, tab) in Tab::ALL.into_iter().enumerate() {
        let x = 22 + i as i32 * 136;
        if s.tab == tab {
            d.fill(rect(x, 65, x + 128, 87), rgb(33, 67, 71));
            d.fill(rect(x, 86, x + 128, 88), CYAN);
        }
        d.label(
            &s.text,
            tab.label(),
            rect(x, 66, x + 128, 88),
            if s.tab == tab {
                CYAN
            } else {
                DIM
            },
            DT_CENTER | DT_SINGLELINE,
        );
    }
    d.fill(rect(split, 100, split + 1, h - 38), rgb(94, 91, 88));
    let visible = s.visible_rows();
    let inner = DT_END_ELLIPSIS | DT_SINGLELINE;
    for (i, row) in s.rows.iter().enumerate().skip(s.top).take(visible) {
        let y = 140 + (i - s.top) as i32 * 57;
        let selected = s.selected == i;
        let fill = if selected {
            rgb(31, 61, 66)
        } else if s.hover == Some(Hit::Row(i)) {
            HOVER
        } else {
            rgb(28, 27, 26)
        };
        d.fill(rect(22, y, split - 12, y + 53), fill);
        if selected {
            d.fill(rect(22, y, 24, y + 53), CYAN);
        }
        d.icon(
            &mut s.images,
            &s.prepared,
            row.icon.as_deref(),
            (28, y + 4),
            45,
            TileStyle {
                locked: row.locked,
                warm: matches!(&row.key, Entry::Recipe { .. } | Entry::Item(_)),
                hovered: s.hover == Some(Hit::Row(i)),
            },
        );
        let pinned = super::pins::key(&s.prepared.catalogue, &row.key)
            .is_some_and(|key| s.pinned.contains(&key));
        if pinned {
            d.label(
                &s.small,
                "Pinned",
                rect(split - 69, y + 6, split - 20, y + 27),
                CYAN,
                inner | DT_RIGHT,
            );
        }
        d.label(
            &s.text,
            &row.title,
            rect(
                83,
                y + 5,
                split
                    - if pinned {
                        76
                    } else {
                        18
                    },
                y + 28,
            ),
            INK,
            inner,
        );
        d.label(
            &s.small,
            &row.summary,
            rect(83, y + 28, split - 18, y + 51),
            if selected {
                CYAN
            } else {
                DIM
            },
            inner,
        );
    }
    if s.rows.is_empty() {
        d.label(&s.text, "No entries available", rect(23, 155, split - 10, 201), DIM, inner);
    }
    if let Some(row) = s.rows.get(s.selected) {
        let x = split + 20;
        d.icon(
            &mut s.images,
            &s.prepared,
            row.icon.as_deref(),
            (x, 111),
            70,
            TileStyle {
                locked: row.locked,
                warm: matches!(&row.key, Entry::Recipe { .. } | Entry::Item(_)),
                hovered: false,
            },
        );
        let title_right = if s.pin_button.is_some() {
            s.pin_bounds().left - 8
        } else {
            w - 21
        };
        let title_top = if s.producer.is_some() {
            131
        } else {
            116
        };
        d.label(
            &s.title,
            &row.title,
            rect(x + 86, title_top, title_right, title_top + 35),
            INK,
            inner,
        );
        if s.producer.is_none() {
            d.label(&s.small, &row.summary, rect(x + 86, 153, w - 21, 173), DIM, inner);
            if let Entry::Corporation(id) = &row.key
                && let Some((current, target)) =
                    super::view::corporation_progress(&s.prepared.catalogue, &s.progress, id)
            {
                let label = format!(
                    "{} / {} reputation",
                    super::view::reputation_text(current),
                    super::view::reputation_text(target),
                );
                d.label(
                    &s.small,
                    &label,
                    rect(x + 86, 153, w - 23, 173),
                    CYAN,
                    DT_RIGHT | DT_VCENTER | DT_SINGLELINE,
                );
                let left = x + 86;
                let right = w - 23;
                d.fill(rect(left, 176, right, 182), rgb(53, 59, 63));
                let width =
                    i64::from(right - left) * i64::from(current.min(target)) / i64::from(target);
                if width > 0 {
                    d.fill(rect(left, 176, left + width as i32, 182), CYAN);
                }
            }
        }
        if let Some(button) = s.pin_button {
            let bounds = s.pin_bounds();
            let enabled = s.pin_key.as_deref().is_some_and(|key| s.pin_enabled(key));
            paint_pin_button(&d, &s.small, button, bounds, enabled, s.hover == Some(Hit::Pin));
        }
        d.fill(rect(x, 191, w - 23, 192), DIM);

        if s.detail_tabs {
            for (tab, title) in ["Produced items", "Upgrade requirements"].into_iter().enumerate() {
                let bounds = s.detail_tab_bounds(tab);
                if tab == s.detail_tab {
                    d.fill(bounds, rgb(42, 78, 82));
                    d.fill(rect(bounds.left, bounds.bottom - 2, bounds.right, bounds.bottom), CYAN);
                } else if s.hover == Some(Hit::DetailTab(tab)) {
                    d.fill(bounds, HOVER);
                }
                d.label(
                    &s.small,
                    title,
                    bounds,
                    if tab == s.detail_tab {
                        CYAN
                    } else {
                        DIM
                    },
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                );
            }
        }
        let fixed = s.fixed_detail_count();
        let scroll_top = s.scroll_top();
        let bottom = s.detail_bottom();
        let visible: Vec<_> = s.detail_rows().collect();
        let details = std::mem::take(&mut s.details);
        for (index, y, height) in visible {
            let detail = &details[index];
            let clip = s.scale.rect(rect(
                x,
                if index < fixed {
                    s.detail_top()
                } else {
                    scroll_top
                },
                w - 22,
                bottom,
            ));
            // SAFETY: The paint DC is live; save/restore confines clipping to this row.
            let saved = unsafe { SaveDC(dc) };
            if saved != 0 {
                // SAFETY: Clipping uses the saved live DC and initialized coordinates.
                unsafe { IntersectClipRect(dc, clip.left, clip.top, clip.right, clip.bottom) };
            }
            // With no description or tabs above the first section, the header divider suffices.
            if detail.section && (index != 0 || s.detail_tabs) {
                d.fill(rect(x, y, w - 23, y + 1), DIM);
            }
            if detail.tiles.is_empty() {
                paint_detail(
                    &d,
                    s,
                    detail,
                    y,
                    height,
                    s.hover == Some(Hit::Detail(index)),
                    s.hover == Some(Hit::DetailPin(index)),
                );
            } else {
                paint_tiles(&d, s, index, detail, y);
            }
            if saved != 0 {
                // SAFETY: The saved state belongs to this DC and has not been restored.
                unsafe { RestoreDC(dc, saved) };
            }
        }
        if let Some(producer) = s.producer.take() {
            let producer_top = s.producer_top();
            d.fill(rect(x, producer_top - 5, w - 23, producer_top - 4), DIM);
            paint_detail(&d, s, &producer, producer_top, 58, s.hover == Some(Hit::Producer), false);
            s.producer = Some(producer);
        }
        s.details = details;
        d.label(&s.small, &s.pin_notice, rect(x, h - 31, w - 188, h - 8), ORANGE, inner);
    }
    for scrollbar in s.scrollbars().into_iter().flatten() {
        d.fill(
            rect(
                scrollbar.thumb.left + 1,
                scrollbar.bounds.top,
                scrollbar.thumb.right - 1,
                scrollbar.bounds.bottom,
            ),
            HOVER,
        );
        d.fill(scrollbar.thumb, INK);
    }
    let x = close_hint_bounds().left + 3;
    let y = h + 7;
    let key_right = x + 37;
    let key_bottom = y + 22;
    d.polygon(
        &[
            point(x, y),
            point(key_right, y),
            point(key_right, key_bottom - 5),
            point(key_right - 5, key_bottom),
            point(x, key_bottom),
        ],
        PANEL,
        ORANGE,
    );
    let hint = DT_SINGLELINE | DT_VCENTER;
    d.label(&s.small, "ESC", rect(x, y, key_right, key_bottom), ORANGE, hint | DT_CENTER);
    let label = rect(key_right + 8, y, w - 1, key_bottom);
    d.label(
        &s.text,
        "CLOSE",
        rect(label.left + 1, label.top + 1, w, label.bottom + 1),
        PANEL,
        hint,
    );
    d.label(&s.text, "CLOSE", label, INK, hint);
}

unsafe extern "system" fn proc(hwnd: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    // SAFETY: The synchronous creation structure and boxed State are owned by Window::open.
    if message == WM_NCCREATE {
        // SAFETY: Win32 supplies a live CREATESTRUCTW with our stable State pointer.
        unsafe {
            let cs = &*(l as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
        }
        return 1;
    }
    // SAFETY: State is installed only for this HWND and accessed exclusively on the UI thread.
    let pointer = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut State;
    if pointer.is_null() {
        // SAFETY: Win32 owns native default dispatch; no State is installed.
        return unsafe { DefWindowProcW(hwnd, message, w, l) };
    }
    // SAFETY: The valid State belongs to this HWND and is accessed exclusively on its UI thread.
    let s = unsafe { &mut *pointer };
    let mut sync_search = false;
    let mut result = 0;
    match message {
        WM_PAINT => {
            // SAFETY: Paint buffers/DCs live until EndPaint. Clip children excludes the search control.
            unsafe {
                let mut ps = PAINTSTRUCT::default();
                let dc = BeginPaint(hwnd, &mut ps);
                let mem = CreateCompatibleDC(dc);
                let (width, height) = (s.scale.pixels(WIDTH), s.scale.pixels(WINDOW_HEIGHT));
                let bitmap = CreateCompatibleBitmap(dc, width, height);
                if !mem.is_null() && !bitmap.is_null() {
                    let old = SelectObject(mem, bitmap);
                    paint(mem, s);
                    BitBlt(dc, 0, 0, width, height, mem, 0, 0, SRCCOPY);
                    SelectObject(mem, old);
                } else {
                    paint(dc, s);
                }
                DeleteObject(bitmap);
                DeleteDC(mem);
                EndPaint(hwnd, &ps);
            }
        }
        WM_SETCURSOR => {
            // SAFETY: The shared cursor remains owned on this UI thread.
            unsafe { SetCursor(s.cursor.borrow().handle()) };
            return 1;
        }
        WM_CTLCOLORSTATIC if l as HWND == s.edit && !s.tab.search_enabled() => {
            // SAFETY: The Edit control supplies a live drawing context.
            unsafe { SetBkColor(w as HDC, DISABLED_SEARCH_BACKGROUND) };
            return s.disabled_search_brush.0 as isize;
        }
        WM_CTLCOLOREDIT if l as HWND == s.edit => {
            // SAFETY: The Edit control supplies a live drawing context.
            unsafe {
                SetTextColor(w as HDC, INK);
                SetBkColor(w as HDC, SEARCH_BACKGROUND);
            }
            return s.search_brush.0 as isize;
        }
        WM_ERASEBKGND => return 1,
        WM_COMMAND
            if ((w >> 16) & 0xffff) == EN_CHANGE as usize
                && l as HWND == s.edit
                && s.tab.search_enabled() =>
        {
            let mut buffer = [0u16; 97];
            // SAFETY: The bounded writable buffer matches the search limit.
            let read = unsafe { GetWindowTextW(s.edit, buffer.as_mut_ptr(), buffer.len() as i32) };
            let query = String::from_utf16_lossy(&buffer[..read.max(0) as usize]);
            if s.query != query {
                s.query = query;
                s.top = 0;
                s.corporation_level = None;
                s.refresh(None);
            }
        }
        WM_APPCOMMAND => {
            let direction = match (l as u32 >> 16) & !FAPPCOMMAND_MASK {
                BROWSER_BACK_COMMAND => Direction::Back,
                BROWSER_FORWARD_COMMAND => Direction::Forward,
                // SAFETY: Win32 dispatches commands the Codex does not handle.
                _ => return unsafe { DefWindowProcW(hwnd, message, w, l) },
            };
            sync_search = s.navigate(direction);
            result = 1;
        }
        WM_LBUTTONDOWN => {
            let x = s.scale.logical(l as i16 as i32);
            let y = s.scale.logical((l >> 16) as i16 as i32);
            s.close_pressed = s.hit(x, y).filter(|hit| matches!(hit, Hit::Close | Hit::CloseHint));
            if s.close_pressed.is_some() {
                return 0;
            } else if (65..89).contains(&y) && x >= 22 {
                if let Some(&tab) = Tab::ALL.get(((x - 22) / 136) as usize)
                    && s.tab != tab
                {
                    s.tab = tab;
                    s.top = 0;
                    s.corporation_level = None;
                    s.refresh(None);
                    sync_search = true;
                }
            } else if let Some((index, scrollbar)) = s
                .scrollbars()
                .into_iter()
                .enumerate()
                .filter_map(|(index, bar)| Some((index, bar?)))
                .find(|(_, bar)| {
                    (bar.bounds.left..bar.bounds.right).contains(&x)
                        && (bar.bounds.top..bar.bounds.bottom).contains(&y)
                })
            {
                if index == 0 {
                    s.top = scrollbar.offset(y) as usize;
                } else {
                    s.scroll = scrollbar.offset(y);
                }
                s.hover = None;
            } else if let Some(hit) = s.hit(x, y) {
                sync_search = s.activate(hit);
            }
        }
        WM_MOUSEMOVE => {
            if !s.tracking_mouse {
                let mut event = TRACKMOUSEEVENT {
                    cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                // SAFETY: The short-lived tracking structure refers to our live window.
                s.tracking_mouse = unsafe { TrackMouseEvent(&mut event) } != 0;
            }
            let hover = s
                .hit(s.scale.logical(l as i16 as i32), s.scale.logical((l >> 16) as i16 as i32))
                .filter(|hit| *hit != Hit::CloseHint);
            if hover == s.hover {
                return 0;
            }
            s.hover = hover;
        }
        WM_MOUSELEAVE => {
            s.tracking_mouse = false;
            if s.close_pressed == Some(Hit::Close) {
                s.close_pressed = None;
            }
            s.hover = None;
        }
        WM_MOUSEWHEEL => {
            let mut point = point(l as i16 as i32, (l >> 16) as i16 as i32);
            // SAFETY: Converts an initialized wheel position for the owned window.
            unsafe { ScreenToClient(hwnd, &mut point) };
            let delta = -((w >> 16) as i16).signum() as isize * 3;
            if s.scale.logical(point.x) < s.split() {
                s.top = s
                    .top
                    .saturating_add_signed(delta)
                    .min(s.rows.len().saturating_sub(s.visible_rows()));
            } else {
                let y = s.scale.logical(point.y);
                if (s.scroll_top()..s.detail_bottom()).contains(&y) {
                    s.scroll = (s.scroll + delta as i32 * 36).clamp(0, s.scroll_max());
                }
            }
            s.hover = s.hit(s.scale.logical(point.x), s.scale.logical(point.y));
        }
        FOOTER_CLICK => {
            let x = s.scale.logical(l as i16 as i32);
            let y = s.scale.logical((l >> 16) as i16 as i32);
            let within_footer = s.hit(x, y) == Some(Hit::CloseHint);
            if w == WM_LBUTTONDOWN as usize {
                s.close_pressed = within_footer.then_some(Hit::CloseHint);
            } else if s.close_pressed.take() == Some(Hit::CloseHint) && within_footer {
                s.close_requested = true;
            }
            return 0;
        }
        WM_LBUTTONUP => {
            let x = s.scale.logical(l as i16 as i32);
            let y = s.scale.logical((l >> 16) as i16 as i32);
            if let Some(pressed) = s.close_pressed.take()
                && s.hit(x, y) == Some(pressed)
            {
                s.close_requested = true;
            }
            return 0;
        }
        WM_CLOSE => {
            s.close_requested = true;
            return 0;
        }
        // SAFETY: Untreated messages go through ordinary native default dispatch.
        _ => return unsafe { DefWindowProcW(hwnd, message, w, l) },
    }
    // SAFETY: Invalidation queues paint; there is no synchronous callback into State here.
    if message != WM_PAINT {
        // SAFETY: This queues repaint for the live Codex window.
        unsafe { InvalidateRect(hwnd, std::ptr::null(), 0) };
    }
    if sync_search {
        let edit = s.edit;
        let enabled = s.tab.search_enabled();
        let query = wide(if enabled {
            &s.query
        } else {
            ""
        });
        let hint = wide(if enabled {
            SEARCH_HINT
        } else {
            ""
        });
        // SAFETY: The State borrow has ended. The owned windows receive only scalars and
        // live UTF-16 buffers. Synchronous edit notifications see the selected tab.
        unsafe {
            let was_enabled = IsWindowEnabled(edit) != 0;
            if !enabled && GetFocus() == edit {
                SetFocus(hwnd);
            }
            EnableWindow(edit, i32::from(enabled));
            SendMessageW(edit, EM_SETCUEBANNER, 1, hint.as_ptr() as isize);
            SetWindowTextW(edit, query.as_ptr());
            if enabled && !was_enabled && GetFocus() == hwnd {
                SetFocus(edit);
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_retraces_link_endpoints_and_discards_forward_after_a_new_link() {
        use Direction::{Back, Forward};

        let location = |item: &str, scroll| Location {
            tab: Tab::Production,
            query: item.into(),
            entry: Some(Entry::Item(item.into())),
            top: 0,
            scroll,
            other_scroll: 0,
            detail_tab: 0,
            corporation_level: None,
        };
        let mut history = History::default();
        assert!(history.step(Back).is_none());
        history.follow(location("recipe", 120), location("ingredient", 0));
        history.follow(location("building", 36), location("corporation", 54));
        assert_eq!(history.step(Back).unwrap().query, "building");
        assert_eq!(history.step(Back).unwrap().scroll, 120);
        assert!(history.step(Back).is_none());
        assert_eq!(history.step(Forward).unwrap().query, "ingredient");
        assert_eq!(history.step(Forward).unwrap().scroll, 54);
        assert!(history.step(Forward).is_none());

        assert_eq!(history.step(Back).unwrap().query, "building");
        history.follow(location("building", 36), location("material", 0));
        assert!(history.step(Forward).is_none());
        assert_eq!(history.step(Back).unwrap().query, "building");
        assert_eq!(history.step(Forward).unwrap().query, "material");
    }

    #[test]
    fn codex_matches_game_menu_proportions_across_resolutions() {
        for (width, height, pane_width, pane_height) in [
            (1280, 720, 760, 504),
            (1920, 1080, 1140, 756),
            (2560, 1440, 1520, 1008),
            (3440, 1440, 1520, 1008),
            (3840, 2160, 2280, 1512),
        ] {
            let scale = Scale::for_client(width, height).unwrap();
            assert_eq!((scale.pixels(WIDTH), scale.pixels(HEIGHT)), (pane_width, pane_height));
            assert!((height - pane_height) / 2 + scale.pixels(WINDOW_HEIGHT) <= height);
            for boundary in [22, 55, 65, 89, 140, 201, WIDTH * 42 / 100, HEIGHT - 34] {
                let pixel = scale.pixels(boundary);
                assert!(scale.logical(pixel - 1) < boundary);
                assert!(scale.logical(pixel) >= boundary);
            }
        }
        let scale = Scale::for_client(800, 600).unwrap();
        assert!(scale.pixels(WIDTH) <= 720 && scale.pixels(HEIGHT) <= 540);
        assert!(Scale::for_client(640, 480).is_none());
    }
}
