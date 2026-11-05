//! Owns the native window, GDI rendering and tray controls on the UI thread.
//!
//! Win32 handles and the boxed App remain alive through the message loop. The
//! reader only publishes locked data and posts a notification; it never touches
//! GDI resources. Shutdown cancels and joins it before the shared state drops.
use crate::{
    connection::Connection,
    dialogs::{self, Dialogs},
    preferences::{self, GameExit, Preferences},
};
use starrupture_observation_model::shutdown::StopSignal;
mod diagnostics;
mod render;

use diagnostics::{self_test, window_events_self_test, window_self_test};
use render::{LayerRenderer, STAGE_TOP_PADDING, panel_points};
use starrupture_observation_model::observation::{Observation, PauseState};
use starrupture_observation_model::wave::{HudFrame, HudPresenter, style, waves_stopped};
use starrupture_wave_core::{config::Config, formatting::now_ms};
use std::{
    cell::{Cell, RefCell},
    ffi::c_void,
    mem::size_of,
    path::PathBuf,
    rc::Rc,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::{
        LibraryLoader::GetModuleHandleW,
        Threading::{CreateMutexW, GetCurrentProcessId},
    },
    UI::{HiDpi::*, Input::KeyboardAndMouse::ReleaseCapture, Shell::*, WindowsAndMessaging::*},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const TRAY: u32 = WM_APP + 1;
const READING_READY: u32 = WM_APP + 2;
const COMPASS_READY: u32 = WM_APP + 40;
const COMPASS_TIMER: usize = 2;
const LAUNCH_GAME: u32 = WM_APP + 10;
fn pause_menu_layout(pause: Option<&PauseState>, previous: bool) -> bool {
    pause.map_or(previous, |p| p.menu_open)
}
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}
fn rgb(c: [u8; 3]) -> u32 {
    u32::from(c[0]) | (u32::from(c[1]) << 8) | (u32::from(c[2]) << 16)
}
struct Reading {
    compass: Option<starrupture_observation_model::compass::Sample>,
    navigation: Arc<starrupture_observation_model::compass::Control>,
    no_world: bool,
    value: Option<Box<Observation>>,
    menu: Option<(bool, u64)>,
    process: u32,
    connection: Connection,
}
struct App {
    compass: crate::compass::Panel,
    recipe: crate::recipe_tracker::Panel,
    dialogs: Dialogs,
    dialog_context: dialogs::Context,
    closed_sessions: u64,
    launch: crate::connection::Launch,
    tray_status: String,
    shared: Arc<Mutex<Reading>>,
    stop: Arc<StopSignal>,
    frame: HudFrame,
    presenter: HudPresenter,
    renderer: LayerRenderer,
    preview: Option<String>,
    hidden: bool,
    countdown_eligible: bool,
    unlocked: bool,
    user_moved: bool,
    pause_position: bool,
    last_rect: RECT,
    game: HWND,
}
/// # Safety
/// The window user data must point to a live, uniquely borrowed `App`. Callers
/// must preserve exclusive access across synchronous native calls and message
/// dispatch, and must not retain the reference beyond the window/state lifetime.
unsafe fn app(window: HWND) -> Option<&'static mut App> {
    // SAFETY: These Win32 queries use scalar values or borrowed handles, validate native handle
    // identity themselves, and neither dereference Rust memory nor transfer ownership.
    let p = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) } as *mut App;
    if p.is_null() {
        None
    } else {
        // SAFETY: The window stores the live boxed App installed at WM_NCCREATE; this UI-thread
        // access obeys app()'s exclusive-borrow contract and does not let the reference escape.
        Some(unsafe { &mut *p })
    }
}
/// # Safety
/// EnumWindows must call this synchronously with param pointing to a live mutable (u32, HWND)
/// tuple; the tuple must not be borrowed elsewhere during enumeration.
unsafe extern "system" fn find_game(window: HWND, param: isize) -> i32 {
    // SAFETY: EnumWindows invokes this callback synchronously with the pointer to the caller's live
    // mutable (pid, HWND) tuple; no reference is retained.
    let tuple = unsafe { &mut *(param as *mut (u32, HWND)) };
    let mut pid = 0;
    // SAFETY: Win32 validates the borrowed handle where required; every output points to an
    // initialized local struct or bounded writable slice that stays live for the synchronous query.
    unsafe { GetWindowThreadProcessId(window, &mut pid) };
    // SAFETY: These Win32 queries use scalar values or borrowed handles, validate native handle
    // identity themselves, and neither dereference Rust memory nor transfer ownership.
    if pid == tuple.0 && unsafe { IsWindowVisible(window) } != 0 {
        let mut name = [0u16; 64];
        // SAFETY: Win32 validates the borrowed handle where required; every output points to an
        // initialized local struct or bounded writable slice that stays live for the synchronous
        // query.
        let n = unsafe { GetClassNameW(window, name.as_mut_ptr(), 64) };
        if n > 0 && String::from_utf16_lossy(&name[..n as usize]) == "UnrealWindow" {
            tuple.1 = window;
            return 0;
        }
    }
    1
}
fn preview(phase: &str) -> HudFrame {
    let (stage, colour) = match phase {
        "waiting-hours" => style("waiting"),
        "FadeoutEarly" => ("INCINERATING", [0xff, 0x71, 0x69]),
        "FadeoutCooling" => ("COOLING", [0xff, 0xaa, 0x66]),
        _ => style(phase),
    };
    HudFrame {
        stage,
        colour,
        next_label: if phase == "waiting" || phase == "waiting-hours" || phase == "PreWave" {
            "Rupture in"
        } else {
            "Next stage in"
        },
        countdown: if phase == "waiting-hours" {
            "1:20:00".into()
        } else if stage == "UNAVAILABLE" {
            "--:--".into()
        } else {
            "12:34".into()
        },
    }
}
/// # Safety
/// window must be the live overlay HWND with boxed App user data; the caller must provide
/// exclusive App access for this UI-thread operation.
unsafe fn tick(window: HWND) {
    // Release App borrows before any operation that can dispatch another message.
    // SAFETY: The window stores the live boxed App installed at WM_NCCREATE; this UI-thread access
    // obeys app()'s exclusive-borrow contract and does not let the reference escape.
    unsafe { update_connection(window) };
    // SAFETY: The window stores the live boxed App installed at WM_NCCREATE; this UI-thread access
    // obeys app()'s exclusive-borrow contract and does not let the reference escape.
    let Some(a) = (unsafe { app(window) }) else {
        return;
    };
    let (frame, pid, stopped, no_world) = if let Some(phase) = &a.preview {
        (preview(phase), 0, false, false)
    } else {
        let r = a.shared.lock().unwrap_or_else(|e| e.into_inner());
        let paused = match r.menu {
            Some((menu, at)) if (now_ms().max(0) as u64).saturating_sub(at) <= 500 => menu,
            Some(_) => a.pause_position,
            None => pause_menu_layout(r.value.as_ref().map(|v| &v.pause), a.pause_position),
        };
        if paused != a.pause_position {
            a.pause_position = paused;
            eprintln!(
                "Overlay layout: {}",
                if paused {
                    "paused"
                } else {
                    "gameplay"
                }
            );
        }
        (
            a.presenter.update_with_format(
                r.value.as_deref(),
                now_ms().max(0) as u64,
                5000,
                a.dialog_context.preferences.get().countdown_format,
            ),
            r.process,
            waves_stopped(r.value.as_deref(), now_ms().max(0) as u64, 5000),
            r.no_world,
        )
    };
    if a.frame != frame {
        a.frame = frame;
        // SAFETY: The UI/fixture owns the dialog and its resource-template child handles; only
        // scalar enabled/theme/invalidation state is changed or queried, with no Rust pointer
        // retained.
        unsafe { InvalidateRect(window, std::ptr::null(), 0) };
    }
    if a.preview.is_some() {
        if !a.hidden {
            // SAFETY: The UI thread uses owned overlay/menu HWNDs and borrowed comparison handles;
            // native position/style values are scalar, and referenced menu/state objects remain
            // alive through synchronous dispatch.
            unsafe { ShowWindow(window, SW_SHOWNOACTIVATE) };
        }
        return;
    }
    let mut found = (pid, std::ptr::null_mut());
    if pid != 0 {
        // SAFETY: The callback has the Win32 ABI; its context points to the live mutable result
        // tuple for this synchronous enumeration and is not retained.
        unsafe { EnumWindows(Some(find_game), (&mut found as *mut (u32, HWND)) as isize) };
    }
    a.game = found.1;
    // SAFETY: These Win32 queries use scalar values or borrowed handles, validate native handle
    // identity themselves, and neither dereference Rust memory nor transfer ownership.
    let visible = !no_world && !a.hidden && !a.game.is_null() && unsafe { IsIconic(a.game) } == 0;
    a.recipe.tick(
        window,
        a.game,
        visible,
        a.dialog_context.preferences.get().recipe_tracker,
        pid,
        a.pause_position,
    );
    *a.dialog_context.recipe_status.borrow_mut() = a.recipe.status.clone();
    a.dialogs.refresh_recipe_status();
    a.countdown_eligible = visible && !stopped;
    if !a.countdown_eligible {
        // SAFETY: The UI thread uses owned overlay/menu HWNDs and borrowed comparison handles;
        // native position/style values are scalar, and referenced menu/state objects remain alive
        // through synchronous dispatch.
        unsafe { ShowWindow(window, SW_HIDE) };
        return;
    }
    let mut rect = RECT::default();
    // SAFETY: Win32 validates the borrowed handle where required; every output points to an
    // initialized local struct or bounded writable slice that stays live for the synchronous query.
    if unsafe { GetClientRect(a.game, &mut rect) } == 0 {
        a.countdown_eligible = false;
        // SAFETY: The UI thread uses owned overlay/menu HWNDs and borrowed comparison handles;
        // native position/style values are scalar, and referenced menu/state objects remain alive
        // through synchronous dispatch.
        unsafe { ShowWindow(window, SW_HIDE) };
        return;
    }
    let mut origin = POINT {
        x: 0,
        y: 0,
    };
    // SAFETY: Win32 validates the borrowed handle where required; every output points to an
    // initialized local struct or bounded writable slice that stays live for the synchronous query.
    unsafe { ClientToScreen(a.game, &mut origin) };
    let w = rect.right.max(1);
    let h = rect.bottom.max(1);
    let width = ((w as f64 * 0.087).round() as i32).max(180);
    let height = ((h as f64 * 0.059).round() as i32).max(60) + STAGE_TOP_PADDING;
    if !a.user_moved {
        let mut current = RECT::default();
        // SAFETY: Win32 validates the borrowed handle where required; every output points to an
        // initialized local struct or bounded writable slice that stays live for the synchronous
        // query.
        unsafe { GetWindowRect(window, &mut current) };
        let gap = (w as f64 * 0.003).round() as i32;
        let x = origin.x + gap;
        // Menu mode offsets the HUD below the border by 80/1440 of client height.
        // Play mode uses the same inset from the top and left edges.
        let y = origin.y
            + gap
            + if a.pause_position {
                (h as f64 * (80.0 / 1440.0)).round() as i32
            } else {
                0
            };
        if current.left != x
            || current.top != y
            || current.right - current.left != width
            || current.bottom - current.top != height
            // SAFETY: These Win32 queries use scalar values or borrowed handles, validate native
            // handle identity themselves, and neither dereference Rust memory nor transfer
            // ownership.
            || unsafe { IsWindowVisible(window) } == 0
        {
            // SAFETY: The UI thread uses owned overlay/menu HWNDs and borrowed comparison handles;
            // native position/style values are scalar, and referenced menu/state objects remain
            // alive through synchronous dispatch.
            unsafe {
                SetWindowPos(
                    window,
                    std::ptr::null_mut(),
                    x,
                    y,
                    width,
                    height,
                    SWP_NOACTIVATE | SWP_NOZORDER,
                )
            };
        }
    }
    crate::stacking::follow_game(window, a.game);
    if rect.right != a.last_rect.right || rect.bottom != a.last_rect.bottom {
        a.last_rect = rect;
        // SAFETY: The UI/fixture owns the dialog and its resource-template child handles; only
        // scalar enabled/theme/invalidation state is changed or queried, with no Rust pointer
        // retained.
        unsafe { InvalidateRect(window, std::ptr::null(), 0) };
    }
}
/// # Safety
/// window must retain the boxed App on its owning UI thread; no other App borrow may be live
/// when this function retrieves it.
unsafe fn update_connection(window: HWND) {
    // SAFETY: The window stores the live boxed App installed at WM_NCCREATE; this UI-thread access
    // obeys app()'s exclusive-borrow contract and does not let the reference escape.
    let Some(a) = (unsafe { app(window) }) else {
        return;
    };
    if a.preview.is_some() {
        return;
    }
    let changed = {
        let reading = a.shared.lock().unwrap_or_else(|error| error.into_inner());
        let mut previous = a.dialog_context.connection.borrow_mut();
        if *previous != reading.connection {
            *previous = reading.connection.clone();
            true
        } else {
            false
        }
    };
    if changed {
        a.dialogs.refresh_about();
    }
    let connection = a.dialog_context.connection.borrow();
    let closed = connection.closed_sessions > a.closed_sessions;
    a.closed_sessions = connection.closed_sessions;
    let exit = if closed {
        a.dialog_context.preferences.get().game_exit
    } else {
        GameExit::KeepRunning
    };
    let action = a.launch.advance(&connection, Instant::now());
    let launch = action == crate::connection::LaunchAction::OpenSteam;
    let timeout = action == crate::connection::LaunchAction::TimedOut;
    let label = if a.launch.pending() {
        "Starting StarRupture…"
    } else {
        connection.label()
    };
    if a.tray_status != label {
        a.tray_status = label.into();
        let mut icon = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: window,
            uID: 1,
            uFlags: NIF_TIP,
            ..Default::default()
        };
        let tip = wide(&format!("StarRupture Overlay\n{label}"));
        let count = tip.len().min(icon.szTip.len() - 1);
        icon.szTip[..count].copy_from_slice(&tip[..count]);
        // SAFETY: NOTIFYICONDATAW is initialized with its exact cbSize and bounded terminated
        // tooltip; its icon/window handles remain live for this synchronous shell call.
        unsafe {
            Shell_NotifyIconW(NIM_MODIFY, &icon);
        }
    }
    // Release the connection borrow before opening windows or invoking the shell.
    drop(connection);
    if launch && let Err(error) = crate::desktop::launch_game(window) {
        // SAFETY: The window stores the live boxed App installed at WM_NCCREATE; this UI-thread
        // access obeys app()'s exclusive-borrow contract and does not let the reference escape.
        if let Some(a) = unsafe { app(window) } {
            a.launch.failed();
        }
        show_error(window, &error.to_string());
    }
    if timeout {
        show_error(
            window,
            "StarRupture was not detected within two minutes. Check Steam, then use Start StarRupture to try again.",
        );
    }
    match exit {
        GameExit::KeepRunning => {}
        // SAFETY: Only scalar event data/IDs are queued to the retained UI HWND; no pointer to a
        // Rust stack value or borrow crosses the asynchronous message boundary.
        GameExit::Exit => unsafe {
            PostMessageW(window, WM_CLOSE, 0, 0);
        },
        // SAFETY: The window stores the live boxed App installed at WM_NCCREATE; this UI-thread
        // access obeys app()'s exclusive-borrow contract and does not let the reference escape.
        GameExit::Ask => unsafe {
            if let Some(context) = app(window).map(|a| a.dialog_context.clone()) {
                open_dialog(window, dialogs::EXIT_PROMPT, context);
            }
        },
    }
}
fn show_error(window: HWND, message: &str) {
    // SAFETY: Title/message buffers are terminated UTF-16 and live for the modal call; the borrowed
    // owner HWND is valid and App/RefCell borrows have been released before message dispatch.
    unsafe {
        MessageBoxW(
            window,
            wide(message).as_ptr(),
            wide("StarRupture Overlay").as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}
/// # Safety
/// window must be a live overlay with valid boxed App state on its UI thread; callers must
/// release previous App borrows before opening the dialog.
unsafe fn open_dialog(window: HWND, kind: u16, context: dialogs::Context) {
    // SAFETY: The window stores the live boxed App installed at WM_NCCREATE; this UI-thread access
    // obeys app()'s exclusive-borrow contract and does not let the reference escape.
    let result = if let Some(a) = unsafe { app(window) } {
        a.dialogs.open(kind, window, context)
    } else {
        return;
    };
    if let Err(error) = result {
        show_error(window, &error.to_string());
    }
}
/// # Safety
/// Call only for WM_PAINT on the live overlay HWND with exclusive App access and a retained
/// UI-thread renderer.
unsafe fn paint(window: HWND) {
    // SAFETY: The window stores the live boxed App installed at WM_NCCREATE; this UI-thread access
    // obeys app()'s exclusive-borrow contract and does not let the reference escape.
    let Some(a) = (unsafe { app(window) }) else {
        return;
    };
    let mut ps = PAINTSTRUCT::default();
    // SAFETY: This WM_PAINT handler owns the live window; PAINTSTRUCT is writable local storage and
    // BeginPaint/EndPaint are paired on the same UI thread.
    let dc = unsafe { BeginPaint(window, &mut ps) };
    // SAFETY: This WM_PAINT handler owns the live window; PAINTSTRUCT is writable local storage and
    // BeginPaint/EndPaint are paired on the same UI thread.
    unsafe {
        if !a.renderer.render(window, dc, &a.frame) {
            eprintln!("Overlay: layered frame update failed: {}", std::io::Error::last_os_error());
        }
        EndPaint(window, &ps)
    };
}
/// # Safety
/// window must retain its boxed App on the owning UI thread; callers must not retain App
/// borrows while the popup dispatches messages.
unsafe fn menu(window: HWND) {
    // SAFETY: The window stores the live boxed App installed at WM_NCCREATE; this UI-thread access
    // obeys app()'s exclusive-borrow contract and does not let the reference escape.
    let Some(a) = (unsafe { app(window) }) else {
        return;
    };
    let connection = a.shared.lock().unwrap_or_else(|error| error.into_inner()).connection.clone();
    let running = connection.running() || a.preview.is_some();
    let hidden = a.hidden;
    let unlocked = a.unlocked;
    let launching = a.launch.pending();
    let context = a.dialog_context.clone();
    // SAFETY: The popup/menu header are retained by this UI-thread owner; text and MENUITEMINFO
    // buffers are initialized and live through each call, and the menu is destroyed before its
    // item-data owner.
    let m = unsafe { CreatePopupMenu() };
    if m.is_null() {
        return;
    }
    let header = Box::new(crate::status::MenuHeader::new(window, &connection, launching));
    // SAFETY: The popup/menu header are retained by this UI-thread owner; text and MENUITEMINFO
    // buffers are initialized and live through each call, and the menu is destroyed before its
    // item-data owner.
    unsafe {
        header.append(m);
        AppendMenuW(m, MF_SEPARATOR, 0, std::ptr::null());
        if running {
            AppendMenuW(
                m,
                MF_STRING,
                1,
                wide(if hidden {
                    "Show overlay"
                } else {
                    "Hide overlay"
                })
                .as_ptr(),
            );
            AppendMenuW(
                m,
                MF_STRING,
                2,
                wide(if unlocked {
                    "Lock position"
                } else {
                    "Unlock to move"
                })
                .as_ptr(),
            );
            AppendMenuW(m, MF_STRING, 3, wide("Reset position").as_ptr());
        } else if connection.can_launch() {
            AppendMenuW(
                m,
                MF_STRING
                    | if launching {
                        MF_GRAYED
                    } else {
                        0
                    },
                5,
                wide(if launching {
                    "Starting StarRupture…"
                } else {
                    "Start StarRupture"
                })
                .as_ptr(),
            );
        }
        if running || connection.can_launch() {
            AppendMenuW(m, MF_SEPARATOR, 0, std::ptr::null());
        }
        AppendMenuW(m, MF_STRING, 6, wide("Settings…").as_ptr());
        AppendMenuW(m, MF_STRING, 7, wide("About…").as_ptr());
        AppendMenuW(m, MF_SEPARATOR, 0, std::ptr::null());
        AppendMenuW(m, MF_STRING, 4, wide("Exit").as_ptr());
    }
    let mut cursor = POINT::default();
    // SAFETY: The UI thread uses owned overlay/menu HWNDs and borrowed comparison handles; native
    // position/style values are scalar, and referenced menu/state objects remain alive through
    // synchronous dispatch.
    unsafe {
        GetCursorPos(&mut cursor);
        SetForegroundWindow(window);
    }
    // SAFETY: The UI thread uses owned overlay/menu HWNDs and borrowed comparison handles; native
    // position/style values are scalar, and referenced menu/state objects remain alive through
    // synchronous dispatch.
    let choice = unsafe {
        TrackPopupMenu(
            m,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            cursor.x,
            cursor.y,
            0,
            window,
            std::ptr::null(),
        )
    };
    // SAFETY: Only scalar event data/IDs are queued to the retained UI HWND; no pointer to a Rust
    // stack value or borrow crosses the asynchronous message boundary.
    unsafe {
        DestroyMenu(m);
        PostMessageW(window, WM_NULL, 0, 0);
    }
    match choice {
        1 => {
            // SAFETY: The window stores the live boxed App installed at WM_NCCREATE; this UI-thread
            // access obeys app()'s exclusive-borrow contract and does not let the reference escape.
            if let Some(a) = unsafe { app(window) } {
                a.hidden = !a.hidden;
            }
        }
        2 => {
            // SAFETY: The window stores the live boxed App installed at WM_NCCREATE; this UI-thread
            // access obeys app()'s exclusive-borrow contract and does not let the reference escape.
            if let Some(a) = unsafe { app(window) } {
                a.unlocked = !a.unlocked;
                let unlocked = a.unlocked;
                // SAFETY: These Win32 queries use scalar values or borrowed handles, validate
                // native handle identity themselves, and neither dereference Rust memory nor
                // transfer ownership.
                let style = unsafe { GetWindowLongPtrW(window, GWL_EXSTYLE) };
                // SAFETY: This is an owned window on its UI thread; a stored creation-state pointer
                // remains boxed through HWND teardown, and changing native style bits does not
                // transfer Rust ownership.
                unsafe {
                    SetWindowLongPtrW(
                        window,
                        GWL_EXSTYLE,
                        if unlocked {
                            style & !(WS_EX_TRANSPARENT as isize)
                        } else {
                            style | WS_EX_TRANSPARENT as isize
                        },
                    );
                }
            }
        }
        3 => {
            // SAFETY: The window stores the live boxed App installed at WM_NCCREATE; this UI-thread
            // access obeys app()'s exclusive-borrow contract and does not let the reference escape.
            if let Some(a) = unsafe { app(window) } {
                a.user_moved = false;
            }
        }
        // SAFETY: Only this UI/fixture owner destroys its HWND; boxed callback state remains alive
        // until DestroyWindow completes, and stale handles are validated by Win32.
        4 => unsafe {
            DestroyWindow(window);
        },
        // SAFETY: Only scalar event data/IDs are queued to the retained UI HWND; no pointer to a
        // Rust stack value or borrow crosses the asynchronous message boundary.
        5 => unsafe {
            PostMessageW(window, LAUNCH_GAME, 0, 0);
        },
        // SAFETY: The window stores the live boxed App installed at WM_NCCREATE; this UI-thread
        // access obeys app()'s exclusive-borrow contract and does not let the reference escape.
        6 => unsafe {
            open_dialog(window, dialogs::SETTINGS, context);
        },
        // SAFETY: The window stores the live boxed App installed at WM_NCCREATE; this UI-thread
        // access obeys app()'s exclusive-borrow contract and does not let the reference escape.
        7 => unsafe {
            open_dialog(window, dialogs::ABOUT, context);
        },
        _ => {}
    }
}
/// # Safety
/// Windows must supply message-specific valid parameters for this registered class;
/// WM_NCCREATE must receive the App box retained through the message loop.
unsafe extern "system" fn procedure(window: HWND, message: u32, w: usize, l: isize) -> isize {
    // SAFETY: The message uses a live native menu structure and item data backed by the retained
    // MenuHeader; the synchronous handler does not retain either pointer.
    if let Some(result) = unsafe { crate::status::menu_message(message, l) } {
        return result;
    }
    match message {
        LAUNCH_GAME => {
            // SAFETY: The window stores the live boxed App installed at WM_NCCREATE; this UI-thread
            // access obeys app()'s exclusive-borrow contract and does not let the reference escape.
            if let Some(a) = unsafe { app(window) } {
                a.launch.request();
            }
            // SAFETY: The window stores the live boxed App installed at WM_NCCREATE; this UI-thread
            // access obeys app()'s exclusive-borrow contract and does not let the reference escape.
            unsafe {
                update_connection(window);
            }
            0
        }
        dialogs::OPEN_NOTICES => {
            // SAFETY: The window stores the live boxed App installed at WM_NCCREATE; this UI-thread
            // access obeys app()'s exclusive-borrow contract and does not let the reference escape.
            if let Some(context) = unsafe { app(window) }.map(|a| a.dialog_context.clone()) {
                // SAFETY: The window stores the live boxed App installed at WM_NCCREATE; this UI-
                // thread access obeys app()'s exclusive-borrow contract and does not let the
                // reference escape.
                unsafe {
                    open_dialog(window, dialogs::NOTICES, context);
                }
            }
            0
        }
        WM_NCCREATE => {
            // SAFETY: WM_NCCREATE supplies a valid CREATESTRUCTW for this callback; lpCreateParams
            // points to the App box retained through the message loop.
            let create = unsafe { &*(l as *const CREATESTRUCTW) };
            // SAFETY: This is an owned window on its UI thread; a stored creation-state pointer
            // remains boxed through HWND teardown, and changing native style bits does not transfer
            // Rust ownership.
            unsafe { SetWindowLongPtrW(window, GWLP_USERDATA, create.lpCreateParams as isize) };
            1
        }
        COMPASS_READY => {
            // SAFETY: Message dispatch owns exclusive UI-thread access to the retained App.
            unsafe { compass_tick(window) };
            0
        }
        WM_TIMER if w == COMPASS_TIMER => {
            // SAFETY: This timer is scoped to the owned overlay and retained App.
            unsafe { compass_tick(window) };
            0
        }
        crate::window_events::CHANGED => {
            crate::window_events::acknowledge();
            // Focus recovery must not wait for the reader mutex, timing/geometry
            // work, or the next low-priority WM_TIMER. Reuse the checked game HWND.
            // SAFETY: The window stores the live boxed App installed at WM_NCCREATE; this UI-thread
            // access obeys app()'s exclusive-borrow contract and does not let the reference escape.
            if let Some(a) = unsafe { app(window) }
                && !a.hidden
                && a.preview.is_none()
            {
                if a.countdown_eligible {
                    crate::stacking::follow_game(window, a.game);
                }
                a.recipe.shortcut(
                    window,
                    a.game,
                    a.dialog_context.preferences.get().recipe_tracker,
                );
                a.recipe.follow(a.game);
                a.compass.follow(a.game);
            }
            0
        }
        WM_TIMER | READING_READY => {
            // SAFETY: The window stores the live boxed App installed at WM_NCCREATE; this UI-thread
            // access obeys app()'s exclusive-borrow contract and does not let the reference escape.
            unsafe { tick(window) };
            0
        }
        crate::recipe_tracker::MOUSE_CLICK => {
            // SAFETY: The window stores the live boxed App installed at WM_NCCREATE; this UI-thread
            // access obeys app()'s exclusive-borrow contract and does not let the reference escape.
            if let Some(a) = unsafe { app(window) } {
                a.recipe.mouse_pressed(
                    a.game,
                    w as u32,
                    [l as u32 as i32, ((l as u64) >> 32) as u32 as i32],
                );
            }
            0
        }
        WM_HOTKEY => {
            // SAFETY: The window stores the live boxed App installed at WM_NCCREATE; this UI-thread
            // access obeys app()'s exclusive-borrow contract and does not let the reference escape.
            if let Some(a) = unsafe { app(window) } {
                a.recipe.pressed(w, a.game);
            }
            0
        }
        WM_PAINT => {
            // SAFETY: The window stores the live boxed App installed at WM_NCCREATE; this UI-thread
            // access obeys app()'s exclusive-borrow contract and does not let the reference escape.
            unsafe { paint(window) };
            0
        }
        WM_SIZE => {
            let width = (l as u32 & 0xffff) as i32;
            let height = ((l as u32 >> 16) & 0xffff) as i32;
            let points = panel_points(width, height, 0);
            // SAFETY: The point array and dimensions are local bounded geometry; a created region
            // is deleted on failure or transferred to Windows only after SetWindowRgn succeeds.
            let region = unsafe { CreatePolygonRgn(points.as_ptr(), points.len() as i32, WINDING) };
            // SAFETY: The point array and dimensions are local bounded geometry; a created region
            // is deleted on failure or transferred to Windows only after SetWindowRgn succeeds.
            if !region.is_null() && unsafe { SetWindowRgn(window, region, 1) } == 0 {
                // SAFETY: The drawing DC is live on this UI thread; local text/geometry buffers
                // match their lengths. Owned GDI objects are restored/deselected before deletion;
                // stock/system objects remain borrowed.
                unsafe { DeleteObject(region) };
            }
            0
        }
        WM_ERASEBKGND => 1,
        WM_MOUSEACTIVATE => MA_NOACTIVATE as isize,
        WM_NCHITTEST => {
            // SAFETY: The window stores the live boxed App installed at WM_NCCREATE; this UI-thread
            // access obeys app()'s exclusive-borrow contract and does not let the reference escape.
            if unsafe { app(window) }.is_some_and(|a| !a.unlocked) {
                HTTRANSPARENT as isize
            } else {
                HTCLIENT as isize
            }
        }
        WM_LBUTTONDOWN => {
            // SAFETY: The window stores the live boxed App installed at WM_NCCREATE; this UI-thread
            // access obeys app()'s exclusive-borrow contract and does not let the reference escape.
            if let Some(a) = unsafe { app(window) }
                && a.unlocked
            {
                a.user_moved = true;
                // SAFETY: These synchronous messages target owned UI controls/windows; scalar
                // parameters or initialized text/message buffers remain live until the call
                // returns, with no retained Rust pointers.
                unsafe {
                    ReleaseCapture();
                    SendMessageW(window, WM_NCLBUTTONDOWN, HTCAPTION as usize, 0)
                };
            }
            0
        }
        TRAY => {
            if l as u32 == WM_RBUTTONUP {
                // SAFETY: The window stores the live boxed App installed at WM_NCCREATE; this UI-
                // thread access obeys app()'s exclusive-borrow contract and does not let the
                // reference escape.
                unsafe { menu(window) }
            } else if l as u32 == WM_LBUTTONDBLCLK
                // SAFETY: The window stores the live boxed App installed at WM_NCCREATE; this UI-
                // thread access obeys app()'s exclusive-borrow contract and does not let the
                // reference escape.
                && let Some(context) = unsafe { app(window) }.map(|a| a.dialog_context.clone())
            {
                // SAFETY: The window stores the live boxed App installed at WM_NCCREATE; this UI-
                // thread access obeys app()'s exclusive-borrow contract and does not let the
                // reference escape.
                unsafe {
                    open_dialog(window, dialogs::SETTINGS, context);
                }
            };
            0
        }
        WM_DESTROY => {
            // SAFETY: The window stores the live boxed App installed at WM_NCCREATE; this UI-thread
            // access obeys app()'s exclusive-borrow contract and does not let the reference escape.
            if let Some(a) = unsafe { app(window) } {
                a.stop.request()
            };
            let icon = NOTIFYICONDATAW {
                cbSize: size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: window,
                uID: 1,
                ..Default::default()
            };
            // SAFETY: NOTIFYICONDATAW is initialized with its exact cbSize and bounded terminated
            // tooltip; its icon/window handles remain live for this synchronous shell call.
            unsafe {
                Shell_NotifyIconW(NIM_DELETE, &icon);
                KillTimer(window, 1);
                PostQuitMessage(0)
            };
            0
        }
        // SAFETY: This callback forwards the original Win32 message parameters unchanged; message-
        // owned pointers remain valid for the synchronous default handling.
        _ => unsafe { DefWindowProcW(window, message, w, l) },
    }
}
// One normal HUD instance per interactive Windows session; tests bypass this.
struct Instance(HANDLE);
impl Instance {
    fn acquire() -> Result<Self> {
        // SAFETY: The terminated instance name stays live through creation, optional security
        // fields are null, and the returned mutex handle is retained by Instance.
        let handle = unsafe {
            CreateMutexW(std::ptr::null(), 0, wide("Local\\StarRuptureWaveOverlay").as_ptr())
        };
        if handle.is_null() {
            return Err(std::io::Error::last_os_error().into());
        }
        // SAFETY: These Win32 queries use scalar values or borrowed handles, validate native handle
        // identity themselves, and neither dereference Rust memory nor transfer ownership.
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            // SAFETY: This guard owns the non-shared Win32 handle returned by its creation/open API
            // and closes it once without transferring Rust memory.
            unsafe { CloseHandle(handle) };
            return Ok(Self(std::ptr::null_mut()));
        }
        Ok(Self(handle))
    }
}
impl Drop for Instance {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: This guard owns the non-shared Win32 handle returned by its creation/open API
            // and closes it once without transferring Rust memory.
            unsafe { CloseHandle(self.0) };
        }
    }
}
pub fn run() -> Result<()> {
    let _fonts = crate::fonts::Fonts::load()?;
    let mut args = std::env::args().skip(1);
    let mut path: Option<PathBuf> = None;
    let mut preview_phase = None;
    let mut launch_game = false;
    if let Some(first) = args.next() {
        if first == "--prepare-recipe-assets"
            || first == "--self-test-asset-wait"
            || first == "--self-test-asset-wait-child"
        {
            let path = args.next().ok_or("Missing asset operation path")?;
            if args.next().is_some() {
                return Err("Unexpected asset operation argument".into());
            }
            if first == "--prepare-recipe-assets" {
                return Ok(starrupture_assets::run(std::path::Path::new(&path))?);
            }
            return Ok(crate::recipe_tracker::asset_wait_fixture(
                std::path::Path::new(&path),
                first == "--self-test-asset-wait-child",
            )?);
        }
        if first == "--self-test-asset-disabled" {
            let dir = args.next().ok_or("Missing disabled asset setup directory")?;
            return Ok(crate::recipe_tracker::self_test_asset_disabled(std::path::Path::new(
                &dir,
            ))?);
        }
        if first == "--self-test-wave-layout" {
            let dir = args.next().ok_or("Missing wave diagnostic directory")?;
            return Ok(starrupture_overlay_reader::self_test_wave_layout(std::path::Path::new(
                &dir,
            ))?);
        }
        if first == "--self-test-asset-cancel" {
            let dir = args.next().ok_or("Missing asset cancellation directory")?;
            return Ok(crate::recipe_tracker::self_test_asset_cancel(std::path::Path::new(&dir))?);
        }
        if first == "--self-test-asset-setup" {
            let dir = args.next().ok_or("Missing asset setup directory")?;
            return Ok(crate::recipe_tracker::self_test_asset_setup(std::path::Path::new(&dir))?);
        }
        if first == "--self-test-recipes" {
            let dir = args.next().ok_or("Missing recipe render directory")?;
            return Ok(crate::recipe_tracker::self_test(std::path::Path::new(&dir))?);
        }
        if first == "--self-test-recipe-reader" {
            let dir = args.next().ok_or("Missing recipe reader directory")?;
            let recipe = args.next();
            if args.next().is_some() {
                return Err("Unexpected recipe reader argument".into());
            }
            return Ok(starrupture_overlay_reader::recipe::self_test_recipe(
                std::path::Path::new(&dir),
                &crate::recipe_tracker::diagnostic_catalogue()?,
                recipe.as_deref().unwrap_or("/Game/Chimera/Crafting/CR_Biofilament"),
            )?);
        }
        if first == "--self-test-recipe-warmup" {
            let dir = args.next().ok_or("Missing recipe warmup directory")?;
            return Ok(starrupture_overlay_reader::recipe::self_test_warmup(
                std::path::Path::new(&dir),
                &crate::recipe_tracker::diagnostic_catalogue()?,
            )?);
        }
        #[cfg(debug_assertions)]
        if first == "--self-test-recipe-probe" {
            let dir = args.next().ok_or("Missing recipe diagnostic directory")?;
            return Ok(starrupture_overlay_reader::recipe::probe(std::path::Path::new(&dir))?);
        }
        if first == "--self-test-dialogs" {
            let directory = args.next().ok_or("Missing dialog test directory")?;
            if args.next().is_some() {
                return Err("Unexpected dialog test argument".into());
            }
            return Ok(crate::dialogs::self_test(std::path::Path::new(&directory))?);
        }
        if first == "--self-test-compass-reader" {
            let directory =
                PathBuf::from(args.next().ok_or("Compass diagnostic directory required")?);
            if args.next().is_some() {
                return Err("Unexpected compass diagnostic argument".into());
            }
            return Ok(starrupture_overlay_reader::compass::self_test(&directory)?);
        }
        if first == "--self-test-compass" {
            let directory = PathBuf::from(args.next().ok_or("Compass render directory required")?);
            if args.next().is_some() {
                return Err("Unexpected compass render argument".into());
            }
            return crate::compass::self_test(&directory);
        }
        if first == "--self-test-icons" {
            if args.next().is_some() {
                return Err("Unexpected icon-test argument".into());
            }
            return Ok(crate::icons::self_test()?);
        }
        if first == "--self-test-window-events" {
            if args.next().is_some() {
                return Err("Unexpected event-test argument".into());
            }
            return window_events_self_test();
        }
        if first == "--self-test-stacking" {
            if args.next().is_some() {
                return Err("Unexpected stacking test argument".into());
            }
            return crate::stacking::self_test();
        }
        if first == "--self-test-reader" {
            let dir = args.next().ok_or("Missing reader test directory")?;
            if args.next().is_some() {
                return Err("Unexpected reader test argument".into());
            }
            return crate::stream_client::self_test(std::path::Path::new(&dir));
        }
        if first == "--self-test-window" {
            if args.next().is_some() {
                return Err("Unexpected window self-test argument".into());
            }
            return window_self_test();
        }
        if first == "--self-test-dir" {
            let dir = args.next().ok_or("Missing self-test directory")?;
            if args.next().is_some() {
                return Err("Unexpected self-test argument".into());
            }
            return self_test(std::path::Path::new(&dir));
        }
        args = std::env::args().skip(1);
    }
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--launch-game" => launch_game = true,
            "--config" => path = Some(args.next().ok_or("Missing configuration path")?.into()),
            "--preview-stage" => preview_phase = Some(args.next().ok_or("Missing preview phase")?),
            _ => return Err("Usage: starrupture-overlay [--config FILE] [--launch-game] [--preview-stage waiting|PreWave|Moving|FadeoutEarly|FadeoutCooling|Fadeout|Growback|unavailable]".into()),
        }
    }
    if let Some(phase) = &preview_phase
        && ![
            "waiting",
            "PreWave",
            "Moving",
            "FadeoutEarly",
            "FadeoutCooling",
            "Fadeout",
            "Growback",
            "unavailable",
        ]
        .contains(&phase.as_str())
    {
        return Err("Unknown preview stage".into());
    }
    if launch_game && preview_phase.is_some() {
        return Err("Game launch cannot be combined with a preview".into());
    }
    let path = path.map(std::path::absolute).transpose()?;
    let mut config = Config::load(path.as_deref())?;
    if preview_phase.is_none() && !config.live_enabled {
        return Err("Live reading is disabled by the supplied configuration".into());
    }
    if config.cache_directory.is_none() {
        let local = std::env::var_os("LOCALAPPDATA")
            .ok_or("Local application-data directory unavailable")?;
        config.cache_directory =
            Some(PathBuf::from(local).join("StarRuptureWaveOverlay").join("cache"));
    }
    let _instance = Instance::acquire()?;
    if _instance.0.is_null() {
        if launch_game {
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                let existing =
                    // SAFETY: The message buffers/class strings are initialized and live for these
                    // UI-thread calls; callback-owned data remains retained until synchronous
                    // dispatch finishes.
                    unsafe { FindWindowW(wide("StarRuptureWaveTimer").as_ptr(), std::ptr::null()) };
                if !existing.is_null() {
                    // SAFETY: Only scalar event data/IDs are queued to the retained UI HWND; no
                    // pointer to a Rust stack value or borrow crosses the asynchronous message
                    // boundary.
                    if unsafe { PostMessageW(existing, LAUNCH_GAME, 0, 0) } == 0 {
                        return Err(std::io::Error::last_os_error().into());
                    }
                    break;
                }
                if Instant::now() >= deadline {
                    return Err(
                        "The existing overlay is still starting. Try the shortcut again.".into()
                    );
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        return Ok(());
    }

    let preferences_path = preferences::directory()?.join("preferences.json");
    let preferences = match preferences::load(&preferences_path) {
        Ok(value) => value,
        Err(error) => {
            show_error(
                std::ptr::null_mut(),
                &format!(
                    "Could not load preferences: {error}. Default preferences will be used until you save Settings."
                ),
            );
            Preferences::default()
        }
    };
    let dialog_context = dialogs::Context {
        preferences: Rc::new(Cell::new(preferences)),
        connection: Rc::new(RefCell::new(Connection::default())),
        preferences_path,
        config: path,
        diagnostic: false,
        recipe_status: Rc::new(RefCell::new(String::new())),
    };

    let stop = Arc::new(StopSignal::default());
    let shared = Arc::new(Mutex::new(Reading {
        compass: None,
        navigation: Arc::new(Default::default()),
        no_world: false,
        value: None,
        menu: None,
        process: 0,
        connection: Connection::default(),
    }));
    // SAFETY: These UI-thread APIs use scalar IDs or documented Win32 pseudo-handles;
    // callback/state ownership is retained separately and no Rust pointer is transferred.
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    let class = wide("StarRuptureWaveTimer");
    // SAFETY: These Win32 queries use scalar values or borrowed handles, validate native handle
    // identity themselves, and neither dereference Rust memory nor transfer ownership.
    let module = unsafe { GetModuleHandleW(std::ptr::null()) };
    let app_icons = crate::icons::Icons::load()?;
    let wc = WNDCLASSW {
        hIcon: app_icons.large,
        lpfnWndProc: Some(procedure),
        hInstance: module,
        lpszClassName: class.as_ptr(),
        // SAFETY: The module/resource identifiers or terminated path are valid for the synchronous
        // lookup; shared image handles are borrowed, while explicitly extracted copies retain their
        // documented owners.
        hCursor: unsafe { LoadCursorW(std::ptr::null_mut(), IDC_ARROW) },
        ..Default::default()
    };
    // SAFETY: The class structure is initialized, its terminated name is live during registration,
    // and the callback has the Win32 ABI and remains in this module.
    if unsafe { RegisterClassW(&wc) } == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let mut app = Box::new(App {
        compass: crate::compass::Panel::default(),
        recipe: crate::recipe_tracker::Panel::new(),
        dialogs: Dialogs::default(),
        dialog_context,
        closed_sessions: 0,
        launch: {
            let mut launch = crate::connection::Launch::default();
            if launch_game {
                launch.request();
            }
            launch
        },
        tray_status: String::new(),
        shared,
        stop: stop.clone(),
        frame: preview("unavailable"),
        presenter: HudPresenter::default(),
        renderer: LayerRenderer::default(),
        preview: preview_phase,
        hidden: false,
        countdown_eligible: false,
        unlocked: false,
        user_moved: false,
        pause_position: false,
        last_rect: RECT::default(),
        game: std::ptr::null_mut(),
    });
    // SAFETY: The registered callback has the Win32 ABI, class/title strings remain live during
    // creation, and any creation-state box stays at a stable address until after the new HWND is
    // destroyed.
    let window = unsafe {
        CreateWindowExW(
            crate::stacking::EXTENDED_STYLE,
            class.as_ptr(),
            wide("StarRupture Overlay").as_ptr(),
            WS_POPUP,
            100,
            100,
            220,
            80 + STAGE_TOP_PADDING,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            module,
            (&mut *app as *mut App).cast::<c_void>(),
        )
    };
    if window.is_null() {
        stop.request();
        return Err(std::io::Error::last_os_error().into());
    }
    app_icons.set_window(window);
    let mut icon = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: window,
        uID: 1,
        uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
        uCallbackMessage: TRAY,
        hIcon: app_icons.small,
        ..Default::default()
    };
    let tip = wide("StarRupture Overlay — right-click for controls");
    icon.szTip[..tip.len()].copy_from_slice(&tip);
    // SAFETY: NOTIFYICONDATAW is initialized with its exact cbSize and bounded terminated tooltip;
    // its icon/window handles remain live for this synchronous shell call.
    if unsafe { Shell_NotifyIconW(NIM_ADD, &icon) } == 0 {
        // SAFETY: Only this UI/fixture owner destroys its HWND; boxed callback state remains alive
        // until DestroyWindow completes, and stale handles are validated by Win32.
        unsafe { DestroyWindow(window) };
        return Err("Could not create overlay exit/control tray icon".into());
    }
    let window_events = if app.preview.is_none() {
        match crate::window_events::WindowEvents::install(window) {
            Ok(events) => {
                eprintln!("OS window notifications active (out of context, no injection).");
                Some(events)
            }
            Err(error) => {
                eprintln!("Window notifications unavailable; UI timer fallback: {error}");
                None
            }
        }
    } else {
        None
    };
    let worker = if app.preview.is_none() {
        Some(reader(
            config,
            app.shared.clone(),
            stop.clone(),
            window as usize,
            app.recipe.tracker(),
            app.recipe.asset_setup(app.dialog_context.preferences.get().recipe_tracker.enabled),
        ))
    } else {
        None
    };
    // SAFETY: These UI-thread APIs use scalar IDs or documented Win32 pseudo-handles;
    // callback/state ownership is retained separately and no Rust pointer is transferred.
    unsafe {
        SetTimer(window, 1, 100, None);
        SetTimer(window, COMPASS_TIMER, 33, None);
        tick(window)
    };
    eprintln!(
        "StarRupture Overlay running (PID{}); right-click its tray icon to hide, move or exit. Preview values are illustrative.",
        // SAFETY: These Win32 queries use scalar values or borrowed handles, validate native handle
        // identity themselves, and neither dereference Rust memory nor transfer ownership.
        unsafe { GetCurrentProcessId() }
    );
    let mut message = MSG::default();
    loop {
        // SAFETY: The message buffers/class strings are initialized and live for these UI-thread
        // calls; callback-owned data remains retained until synchronous dispatch finishes.
        let state = unsafe { GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) };
        if state <= 0 {
            break;
        }
        app.dialogs.prune();
        if app.dialogs.translate(&message) {
            continue;
        }
        // SAFETY: The message buffers/class strings are initialized and live for these UI-thread
        // calls; callback-owned data remains retained until synchronous dispatch finishes.
        unsafe {
            TranslateMessage(&message);
            DispatchMessageW(&message)
        };
    }
    drop(app.dialogs);
    drop(window_events);
    stop.request();
    if let Some(worker) = worker {
        let _ = worker.join();
    }
    Ok(())
}

/// # Safety
/// The HWND must retain its boxed App on this UI thread with exclusive access.
unsafe fn compass_tick(window: HWND) {
    // SAFETY: The dispatcher retains the App through HWND teardown and owns this UI thread.
    let Some(a) = (unsafe { app(window) }) else {
        return;
    };
    let enabled = a.dialog_context.preferences.get().compass_enabled
        && !a.hidden
        && a.preview.is_none()
        && !a.pause_position
        && crate::compass::focused(a.game)
        && crate::compass::gameplay_cursor();
    if !enabled {
        a.compass.hide();
    }
    let Ok(r) = a.shared.try_lock() else {
        return;
    };
    r.navigation.set(enabled);
    let sample = if !r.no_world {
        r.compass
    } else {
        None
    };
    drop(r);
    if let Err(e) = a.compass.update(a.game, sample, enabled) {
        a.compass.hide();
        eprintln!("Compass rendering unavailable: {e}");
    }
}
fn reader(
    config: Config,
    shared: Arc<Mutex<Reading>>,
    stop: Arc<StopSignal>,
    window: usize,
    tracker: Option<starrupture_overlay_reader::recipe::Tracker>,
    setup: Option<Box<dyn starrupture_overlay_reader::recipe::AssetSetup>>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        use crate::stream_client::Update;
        let navigation = shared.lock().unwrap_or_else(|e| e.into_inner()).navigation.clone();
        let publish = |event: Update| {
            let compass_only = matches!(event, Update::Compass(_));
            let mut r = shared.lock().unwrap_or_else(|e| e.into_inner());
            match event {
                Update::Compass(value) => r.compass = value,
                Update::Status(status) => r.connection.update(status),
                Update::Reset(pid) => {
                    r.no_world = false;
                    r.connection.ready(pid);
                    r.process = pid;
                    r.menu = None;
                    r.value = None;
                    r.compass = None;
                }
                Update::Menu(menu, at) => r.menu = Some((menu, at)),
                Update::World(value) => {
                    r.no_world = false;
                    r.value = Some(value);
                }
                Update::NoWorld => {
                    r.no_world = true;
                    r.connection.unavailable();
                    r.menu = None;
                    r.value = None;
                    r.compass = None;
                    eprintln!("No local gameplay world loaded; overlay hidden.");
                }
                Update::Unavailable(reason) => {
                    r.no_world = false;
                    r.connection.unavailable();
                    eprintln!("Live observation unavailable: {reason}");
                    r.menu = None;
                    r.value = None;
                    r.compass = None;
                }
            }
            drop(r);
            if !stop.is_requested() {
                // SAFETY: Only scalar event data/IDs are queued to the retained UI HWND; no pointer
                // to a Rust stack value or borrow crosses the asynchronous message boundary.
                unsafe {
                    PostMessageW(
                        window as HWND,
                        if compass_only {
                            COMPASS_READY
                        } else {
                            READING_READY
                        },
                        0,
                        0,
                    );
                }
            }
        };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::stream_client::run_with_navigation(
                config,
                stop.clone(),
                &publish,
                tracker,
                setup,
                Some(navigation),
            )
        }));
        match result {
            Ok(Ok(stats)) => eprintln!("Embedded reader stopped: {stats:?}"),
            Ok(Err(error)) => {
                eprintln!("Embedded reader unavailable: {error}");
                publish(Update::Unavailable(error));
            }
            Err(_) => publish(Update::Unavailable("Read-only worker failed; no fallback".into())),
        }
        eprintln!("Overlay reader joined; no reads remain in flight.");
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn menu_layout_is_independent_of_simulation_pause_and_unknown_preserves_position() {
        let open = PauseState {
            menu_open: true,
            paused: Some(false),
            ..PauseState::default()
        };
        let closed = PauseState {
            menu_open: false,
            paused: None,
            ..PauseState::default()
        };
        assert!(pause_menu_layout(Some(&open), false));
        assert!(!pause_menu_layout(Some(&closed), true));
        assert!(pause_menu_layout(None, true));
        assert!(!pause_menu_layout(None, false));
    }
}
