//! On-demand modeless native dialogs. Each window owns a separate stable state allocation.
use crate::{
    connection::Connection,
    desktop,
    preferences::{self, GameExit, Preferences},
};
use anyhow::{Result, ensure};
mod diagnostics;
pub use diagnostics::self_test;
use std::{
    cell::{Cell, RefCell},
    fs,
    io::Read,
    path::{Path, PathBuf},
    rc::Rc,
};
use windows_sys::Win32::{
    Foundation::*,
    System::{DataExchange::*, LibraryLoader::GetModuleHandleW, Memory::*},
    UI::{Controls::*, Input::KeyboardAndMouse::EnableWindow, WindowsAndMessaging::*},
};

pub const SETTINGS: u16 = 101;
pub const ABOUT: u16 = 102;
pub const NOTICES: u16 = 103;
pub const EXIT_PROMPT: u16 = 104;
pub const OPEN_NOTICES: u32 = WM_APP + 11;
const REFRESH_ICON: u32 = WM_APP + 12;
#[derive(Clone)]
pub struct Context {
    pub preferences: Rc<Cell<Preferences>>,
    pub connection: Rc<RefCell<Connection>>,
    pub preferences_path: PathBuf,
    pub config: Option<PathBuf>,
    pub diagnostic: bool,
    pub recipe_status: Rc<RefCell<String>>,
}
impl Context {
    pub fn diagnostic(path: PathBuf) -> Self {
        Self {
            preferences: Rc::new(Cell::new(Preferences::default())),
            connection: Rc::new(RefCell::new(Connection::default())),
            preferences_path: path,
            config: None,
            diagnostic: true,
            recipe_status: Rc::new(RefCell::new(String::new())),
        }
    }
}
struct State {
    heading_font: Cell<windows_sys::Win32::Graphics::Gdi::HFONT>,
    window: Cell<HWND>,
    owner: HWND,
    kind: u16,
    context: Context,
    initial_startup: Cell<bool>,
    startup_read: Cell<bool>,
    shortcut: Cell<desktop::ShortcutState>,
}
impl Drop for State {
    fn drop(&mut self) {
        let font = self.heading_font.get();
        if !font.is_null() {
            unsafe {
                windows_sys::Win32::Graphics::Gdi::DeleteObject(font);
            }
        }
    }
}
struct Dialog(Box<State>);
impl Drop for Dialog {
    fn drop(&mut self) {
        let window = self.0.window.get();
        if !window.is_null() {
            unsafe {
                DestroyWindow(window);
            }
        }
    }
}
#[derive(Default)]
pub struct Dialogs {
    windows: Vec<Dialog>,
}
impl Dialogs {
    pub fn open(&mut self, kind: u16, owner: HWND, context: Context) -> Result<()> {
        unsafe {
            InitCommonControlsEx(&INITCOMMONCONTROLSEX {
                dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
                dwICC: ICC_TAB_CLASSES,
            });
        }
        self.prune();
        if let Some(dialog) = self.windows.iter().find(|dialog| dialog.0.kind == kind) {
            let window = dialog.0.window.get();
            if !dialog.0.context.diagnostic {
                unsafe {
                    ShowWindow(window, SW_RESTORE);
                    SetForegroundWindow(window);
                }
            }
            return Ok(());
        }
        let mut state = Box::new(State {
            heading_font: Cell::new(std::ptr::null_mut()),
            window: Cell::new(std::ptr::null_mut()),
            owner,
            kind,
            context,
            initial_startup: Cell::new(false),
            startup_read: Cell::new(false),
            shortcut: Cell::new(desktop::ShortcutState::Missing),
        });
        let window = unsafe {
            CreateDialogParamW(
                GetModuleHandleW(std::ptr::null()),
                std::ptr::without_provenance(kind as usize),
                owner,
                Some(procedure),
                (&mut *state as *mut State) as isize,
            )
        };
        ensure!(!window.is_null(), "Could not open dialog: {}", std::io::Error::last_os_error());
        if let Ok(icons) = crate::icons::Icons::load() {
            icons.set_window(window);
        }
        let diagnostic = state.context.diagnostic;
        self.windows.push(Dialog(state));
        if !diagnostic {
            unsafe {
                ShowWindow(window, SW_SHOWNORMAL);
                SetForegroundWindow(window);
            }
        }
        Ok(())
    }
    pub fn prune(&mut self) {
        self.windows.retain(|dialog| !dialog.0.window.get().is_null());
    }
    pub fn translate(&self, message: &MSG) -> bool {
        self.windows.iter().any(|dialog| {
            let window = dialog.0.window.get();
            !window.is_null() && unsafe { IsDialogMessageW(window, message) } != 0
        })
    }
    pub fn refresh_about(&self) {
        for dialog in &self.windows {
            if dialog.0.kind == ABOUT && !dialog.0.window.get().is_null() {
                refresh_about(&dialog.0);
            }
        }
    }
    pub fn refresh_recipe_status(&self) {
        for d in &self.windows {
            if d.0.kind == SETTINGS && !d.0.window.get().is_null() {
                text(d.0.window.get(), 154, &d.0.context.recipe_status.borrow());
            }
        }
    }
}
fn text(window: HWND, id: i32, value: &str) {
    unsafe {
        SetDlgItemTextW(window, id, desktop::wide(value).as_ptr());
    }
}
fn error(state: &State, error: impl std::fmt::Display) {
    text(state.window.get(), 114, &error.to_string());
}
fn selected(state: &State) -> Preferences {
    let choice = unsafe { SendDlgItemMessageW(state.window.get(), 111, CB_GETCURSEL, 0, 0) };
    Preferences {
        game_exit: match choice {
            1 => GameExit::Ask,
            2 => GameExit::Exit,
            _ => GameExit::KeepRunning,
        },
        recipe_tracker: unsafe {
            let (key, modifiers) = preferences::parse_shortcut(&shortcut_text(state))
                .unwrap_or_else(|_| {
                    let previous = state.context.preferences.get().recipe_tracker;
                    (previous.key, previous.modifiers)
                });
            crate::preferences::RecipeSettings {
                enabled: SendDlgItemMessageW(state.window.get(), 150, BM_GETCHECK, 0, 0) as u32
                    == BST_CHECKED,
                key,
                modifiers,
                position: SendDlgItemMessageW(state.window.get(), 152, CB_GETCURSEL, 0, 0)
                    .clamp(0, 2) as u8,
                scale: [75, 100, 125, 150][SendDlgItemMessageW(
                    state.window.get(),
                    153,
                    CB_GETCURSEL,
                    0,
                    0,
                )
                .clamp(0, 3) as usize],
            }
        },
    }
}
fn checked_startup(state: &State) -> bool {
    unsafe { SendDlgItemMessageW(state.window.get(), 110, BM_GETCHECK, 0, 0) as u32 == BST_CHECKED }
}
fn shortcut_text(state: &State) -> String {
    let mut value = [0u16; 64];
    let n =
        unsafe { GetDlgItemTextW(state.window.get(), 151, value.as_mut_ptr(), value.len() as i32) };
    String::from_utf16_lossy(&value[..n as usize])
}
fn dirty(state: &State) {
    let changed = checked_startup(state) != state.initial_startup.get()
        || selected(state) != state.context.preferences.get()
        || shortcut_text(state) != state.context.preferences.get().recipe_tracker.shortcut_label();
    unsafe {
        EnableWindow(
            GetDlgItem(state.window.get(), IDOK),
            i32::from(changed && state.startup_read.get()),
        );
    }
    text(
        state.window.get(),
        115,
        if changed {
            "Unsaved changes"
        } else {
            "No unsaved changes"
        },
    );
}
fn settings_init(state: &State) {
    let window = state.window.get();
    // Match labels, checkbox backgrounds and the dialog to the native tab surface.
    // Windows owns the theme brush and updates it when the system theme changes.
    unsafe {
        EnableThemeDialogTexture(window, ETDT_ENABLE | ETDT_USETABTEXTURE);
    }
    for (index, label) in ["General", "Recipe tracker"].iter().enumerate() {
        let mut label = desktop::wide(label);
        let item = TCITEMW {
            mask: TCIF_TEXT,
            pszText: label.as_mut_ptr(),
            ..Default::default()
        };
        unsafe {
            SendDlgItemMessageW(
                window,
                155,
                TCM_INSERTITEMW,
                index,
                (&item as *const TCITEMW) as isize,
            );
        }
    }
    let recipe = state.context.preferences.get().recipe_tracker;
    unsafe {
        SendDlgItemMessageW(window, 150, BM_SETCHECK, usize::from(recipe.enabled), 0);
        SendDlgItemMessageW(window, 151, EM_SETLIMITTEXT, 48, 0);
        for (id, labels) in [
            (152, vec!["Upper right", "Middle right", "Lower right"]),
            (153, vec!["75%", "100%", "125%", "150%"]),
        ] {
            for label in labels {
                SendDlgItemMessageW(
                    window,
                    id,
                    CB_ADDSTRING,
                    0,
                    desktop::wide(label).as_ptr() as isize,
                );
            }
        }
        SendDlgItemMessageW(window, 152, CB_SETCURSEL, recipe.position as usize, 0);
        SendDlgItemMessageW(
            window,
            153,
            CB_SETCURSEL,
            [75, 100, 125, 150].iter().position(|s| *s == recipe.scale).unwrap_or(1),
            0,
        );
    }
    text(window, 154, &state.context.recipe_status.borrow());
    text(window, 151, &recipe.shortcut_label());
    for label in
        ["Keep running in the tray", "Ask before exiting", "Exit the overlay automatically"]
    {
        unsafe {
            SendDlgItemMessageW(
                window,
                111,
                CB_ADDSTRING,
                0,
                desktop::wide(label).as_ptr() as isize,
            );
        }
    }
    let index = match state.context.preferences.get().game_exit {
        GameExit::KeepRunning => 0,
        GameExit::Ask => 1,
        GameExit::Exit => 2,
    };
    unsafe {
        SendDlgItemMessageW(window, 111, CB_SETCURSEL, index, 0);
    }
    let startup = if state.context.diagnostic {
        Ok(None)
    } else {
        desktop::startup_value()
    };
    match startup {
        Ok(value) => {
            state.initial_startup.set(value.is_some());
            state.startup_read.set(true);
            unsafe {
                SendDlgItemMessageW(
                    window,
                    110,
                    BM_SETCHECK,
                    if value.is_some() {
                        BST_CHECKED as usize
                    } else {
                        BST_UNCHECKED as usize
                    },
                    0,
                );
            }
        }
        Err(value) => {
            error(state, value);
            unsafe {
                EnableWindow(GetDlgItem(window, 110), 0);
            }
        }
    }
    if state.context.diagnostic {
        unsafe {
            EnableWindow(GetDlgItem(window, 113), 0);
        }
    } else {
        match desktop::shortcut_path()
            .and_then(|path| desktop::shortcut_state(&path, state.context.config.as_deref()))
        {
            Ok(value) => state.shortcut.set(value),
            Err(value) => error(state, value),
        }
    }
    shortcut_labels(state);
    settings_tab(state);
    dirty(state);
}
fn settings_tab(state: &State) {
    let recipe = unsafe { SendDlgItemMessageW(state.window.get(), 155, TCM_GETCURSEL, 0, 0) } == 1;
    for id in [110, 111, 112, 113, 160, 161, 162, 163, 164, 165] {
        unsafe {
            ShowWindow(
                GetDlgItem(state.window.get(), id),
                if recipe {
                    SW_HIDE
                } else {
                    SW_SHOW
                },
            );
        }
    }
    for id in [150, 151, 152, 153, 154, 166, 167, 168, 169] {
        unsafe {
            ShowWindow(
                GetDlgItem(state.window.get(), id),
                if recipe {
                    SW_SHOW
                } else {
                    SW_HIDE
                },
            );
        }
    }
}
fn shortcut_labels(state: &State) {
    text(state.window.get(), 112, state.shortcut.get().label());
    text(state.window.get(), 113, state.shortcut.get().action());
}
fn save_settings(state: &State) -> Result<()> {
    ensure!(
        state.startup_read.get(),
        "Startup registration could not be read; reopen Settings to retry"
    );
    let preferences = selected(state);
    preferences::parse_shortcut(&shortcut_text(state))?;
    preferences.recipe_tracker.validate()?;
    let desired = checked_startup(state);
    let previous = if state.context.diagnostic {
        None
    } else {
        desktop::startup_value()?
    };
    let changed = desired != previous.is_some();
    if changed && !state.context.diagnostic {
        let command = desired
            .then(|| desktop::startup_command(state.context.config.as_deref()))
            .transpose()?;
        desktop::set_startup(command.as_deref())?;
    }
    if let Err(error) = preferences::save(&state.context.preferences_path, preferences) {
        if changed
            && !state.context.diagnostic
            && let Err(rollback) = desktop::set_startup(previous.as_deref())
        {
            anyhow::bail!(
                "Could not save preferences: {error}. Could not restore startup registration: {rollback}"
            );
        }
        return Err(error);
    }
    state.context.preferences.set(preferences);
    Ok(())
}
fn refresh_about(state: &State) {
    let window = state.window.get();
    let connection = state.context.connection.borrow();
    text(window, 121, connection.label());
    text(
        window,
        122,
        match connection.status {
            crate::connection::GameStatus::Running {
                supported: false,
                ..
            } => "A compatibility update is required for this game build.",
            crate::connection::GameStatus::Running {
                ..
            } if connection.connected => "The running game matches the supported build.",
            crate::connection::GameStatus::Running {
                ..
            } => "Waiting for a supported world and validated observations.",
            _ => "The overlay will connect when a supported game starts.",
        },
    );
    text(window, 124, connection.location());
    unsafe {
        windows_sys::Win32::Graphics::Gdi::InvalidateRect(
            GetDlgItem(window, 142),
            std::ptr::null(),
            1,
        );
    }
}
fn support_details(connection: &Connection) -> String {
    format!(
        "StarRupture Overlay {}\r\nSource revision: {}\r\nConnection: {}\r\nSupported executable SHA256: {}\r\nWindows x64",
        env!("CARGO_PKG_VERSION"),
        env!("OVERLAY_REVISION"),
        connection.label(),
        starrupture_observation_model::overlay_stream::GAME_HASH
    )
}
fn clipboard(owner: HWND, value: &str) -> Result<()> {
    let value = desktop::wide(value);
    let memory = unsafe { GlobalAlloc(GMEM_MOVEABLE, std::mem::size_of_val(value.as_slice())) };
    ensure!(!memory.is_null(), "Clipboard allocation failed");
    let mut transferred = false;
    let result = (|| -> Result<()> {
        let target = unsafe { GlobalLock(memory) };
        ensure!(!target.is_null(), "Clipboard memory unavailable");
        unsafe {
            std::ptr::copy_nonoverlapping(value.as_ptr(), target.cast(), value.len());
            GlobalUnlock(memory);
        }
        ensure!(unsafe { OpenClipboard(owner) } != 0, "Clipboard is busy; try again");
        let emptied = unsafe { EmptyClipboard() };
        let copied = emptied != 0 && !unsafe { SetClipboardData(13, memory) }.is_null();
        unsafe {
            CloseClipboard();
        }
        transferred = copied;
        ensure!(copied, "Could not copy support details");
        Ok(())
    })();
    if !transferred {
        unsafe {
            GlobalFree(memory);
        }
    }
    result
}
fn notices() -> Result<String> {
    let root = std::env::current_exe()?
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Application directory unavailable"))?
        .to_owned();
    if !root.join("LICENSE").is_file() {
        return Ok(format!("{}\r\n\r\n{}\r\n\r\n{}\r\n\r\nComplete dependency notices are included with release packages.",include_str!("../../../LICENSE"),include_str!("../../../THIRD-PARTY-NOTICES.md"),include_str!("../assets/fonts/OFL.txt")).replace('\n',"\r\n"));
    }
    let mut paths = vec![root.join("LICENSE"), root.join("THIRD-PARTY-NOTICES.md")];
    fn collect(root: &Path, paths: &mut Vec<PathBuf>) -> Result<()> {
        for entry in fs::read_dir(root)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            if kind.is_dir() {
                collect(&entry.path(), paths)?;
            } else if kind.is_file() {
                paths.push(entry.path());
            }
        }
        Ok(())
    }
    collect(&root.join("licenses"), &mut paths)?;
    paths.sort();
    let mut output = String::new();
    for path in paths {
        let remaining = 2_000_000usize.saturating_sub(output.len());
        let mut bytes = Vec::new();
        fs::File::open(&path)?.take(remaining as u64 + 1).read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= remaining, "Licence notices exceed the display limit");
        output.push_str(&format!("\r\n{}\r\n\r\n", path.strip_prefix(&root)?.display()));
        output.push_str(&String::from_utf8(bytes)?);
    }
    Ok(output.replace("\r\n", "\n").replace('\n', "\r\n"))
}
unsafe extern "system" fn procedure(window: HWND, message: u32, w: usize, l: isize) -> isize {
    if message == WM_INITDIALOG {
        let state = unsafe { &*(l as *const State) };
        state.window.set(window);
        unsafe {
            SetWindowLongPtrW(window, GWLP_USERDATA, l);
        }
        if matches!(state.kind, SETTINGS | ABOUT) {
            use windows_sys::Win32::Graphics::Gdi::*;
            let font = unsafe { SendMessageW(window, WM_GETFONT, 0, 0) } as HFONT;
            let mut description = LOGFONTW::default();
            if unsafe {
                GetObjectW(
                    font,
                    std::mem::size_of::<LOGFONTW>() as i32,
                    (&mut description as *mut LOGFONTW).cast(),
                )
            } != 0
            {
                description.lfHeight = description.lfHeight * 17 / 9;
                description.lfWeight = 600;
                let heading = unsafe { CreateFontIndirectW(&description) };
                if !heading.is_null() {
                    state.heading_font.set(heading);
                    unsafe {
                        SendDlgItemMessageW(window, 140, WM_SETFONT, heading as usize, 1);
                    }
                }
            }
        }
        match state.kind {
            SETTINGS => settings_init(state),
            ABOUT => {
                if let Err(error) = crate::icons::set_about(window) {
                    eprintln!("About icon unavailable: {error}");
                }
                text(
                    window,
                    120,
                    &format!("Version {} · Development build", env!("CARGO_PKG_VERSION")),
                );
                text(window, 123, env!("OVERLAY_REVISION"));
                refresh_about(state);
            }
            NOTICES => {
                unsafe {
                    SendDlgItemMessageW(window, 130, EM_SETLIMITTEXT, 2_100_000, 0);
                }
                text(
                    window,
                    130,
                    &notices()
                        .unwrap_or_else(|error| format!("Could not load licence notices: {error}")),
                );
            }
            _ => {}
        }
        if state.kind == ABOUT {
            unsafe {
                windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus(GetDlgItem(
                    window, IDCANCEL,
                ));
            }
            return 0;
        }
        return 1;
    }
    let pointer = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) } as *const State;
    if pointer.is_null() {
        return 0;
    }
    let state = unsafe { &*pointer };
    match message {
        WM_NOTIFY if state.kind == SETTINGS && l != 0 => {
            let notice = unsafe { &*(l as *const NMHDR) };
            if notice.idFrom == 155 && notice.code == TCN_SELCHANGE {
                settings_tab(state);
                return 1;
            }
            0
        }
        WM_DRAWITEM if w == 142 && l != 0 => {
            let item = unsafe { &*(l as *const DRAWITEMSTRUCT) };
            let font = unsafe { SendDlgItemMessageW(window, 121, WM_GETFONT, 0, 0) };
            let previous_font =
                unsafe { windows_sys::Win32::Graphics::Gdi::SelectObject(item.hDC, font as _) };
            unsafe {
                windows_sys::Win32::Graphics::Gdi::FillRect(
                    item.hDC,
                    &item.rcItem,
                    windows_sys::Win32::Graphics::Gdi::GetSysColorBrush(
                        windows_sys::Win32::Graphics::Gdi::COLOR_3DFACE,
                    ),
                );
            }
            crate::status::draw_dot(
                item.hDC,
                item.rcItem,
                state.context.connection.borrow().indicator_colour(),
                unsafe { windows_sys::Win32::UI::HiDpi::GetDpiForWindow(window) }.max(96),
                unsafe {
                    windows_sys::Win32::Graphics::Gdi::GetSysColor(
                        windows_sys::Win32::Graphics::Gdi::COLOR_3DFACE,
                    )
                },
            );
            unsafe {
                windows_sys::Win32::Graphics::Gdi::SelectObject(item.hDC, previous_font);
            }
            1
        }
        WM_DPICHANGED if state.kind == ABOUT => {
            unsafe {
                PostMessageW(window, REFRESH_ICON, 0, 0);
            }
            0
        }
        REFRESH_ICON if state.kind == ABOUT => {
            if let Err(error) = crate::icons::set_about(window) {
                eprintln!("About icon unavailable: {error}");
            }
            1
        }
        WM_COMMAND => {
            let id = (w & 0xffff) as i32;
            let notification = (w >> 16) as u32;
            if id == IDCANCEL {
                unsafe {
                    DestroyWindow(window);
                }
                return 1;
            }
            match (state.kind, id) {
                (SETTINGS, 110 | 111 | 150 | 151 | 152 | 153)
                    if notification == BN_CLICKED
                        || notification == CBN_SELCHANGE
                        || notification == EN_CHANGE =>
                {
                    dirty(state);
                }
                (SETTINGS, IDOK) => match save_settings(state) {
                    Ok(()) => unsafe {
                        DestroyWindow(window);
                    },
                    Err(value) => error(state, value),
                },
                (SETTINGS, 113) if !state.context.diagnostic => {
                    match desktop::toggle_shortcut(state.context.config.as_deref()) {
                        Ok(value) => {
                            state.shortcut.set(if value {
                                desktop::ShortcutState::Current
                            } else {
                                desktop::ShortcutState::Missing
                            });
                            shortcut_labels(state);
                            text(
                                window,
                                114,
                                "Shortcut updated. This action takes effect immediately.",
                            );
                        }
                        Err(value) => error(state, value),
                    }
                }
                (ABOUT, 125 | 126) => {
                    let target = if id == 125 {
                        "https://github.com/ahjephson/star-rupture-overlay"
                    } else {
                        "https://github.com/ahjephson/star-rupture-overlay/issues"
                    };
                    if let Err(value) = desktop::open(window, target) {
                        text(window, 122, &value.to_string());
                    }
                }
                (ABOUT, 127) => unsafe {
                    PostMessageW(state.owner, OPEN_NOTICES, 0, 0);
                },
                (ABOUT, 128) => {
                    let details = support_details(&state.context.connection.borrow());
                    match clipboard(window, &details) {
                        Ok(()) => text(
                            window,
                            122,
                            "Support details copied. Personal paths and game data are excluded.",
                        ),
                        Err(value) => text(window, 122, &value.to_string()),
                    }
                }
                (EXIT_PROMPT, 131) => unsafe {
                    PostMessageW(state.owner, WM_CLOSE, 0, 0);
                    DestroyWindow(window);
                },
                _ => return 0,
            }
            1
        }
        WM_CLOSE => {
            unsafe {
                DestroyWindow(window);
            }
            1
        }
        WM_NCDESTROY => {
            state.window.set(std::ptr::null_mut());
            unsafe {
                SetWindowLongPtrW(window, GWLP_USERDATA, 0);
            }
            0
        }
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn support_details_exclude_process_ids_paths_and_game_data() {
        let mut connection = Connection::default();
        connection.update(crate::connection::GameStatus::Running {
            pid: 987654,
            creation_ticks: 123456,
            executable: r"D:\Private user\Game.exe".into(),
            supported: true,
        });
        let text = support_details(&connection);
        assert!(!text.contains("Private user"));
        assert!(!text.contains("987654"));
        assert!(!text.contains("123456"));
        assert!(text.contains(starrupture_observation_model::overlay_stream::GAME_HASH));
    }
}
