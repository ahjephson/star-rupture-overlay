//! Owns the native window, GDI rendering and tray controls on the UI thread.
//!
//! Win32 handles and the boxed App remain alive through the message loop. The
//! reader only publishes locked data and posts a notification; it never touches
//! GDI resources. Shutdown cancels and joins it before the shared state drops.
mod diagnostics;
mod render;

use diagnostics::{self_test, window_events_self_test, window_self_test};
use render::{STAGE_TOP_PADDING, panel_points, render_layer};
use starrupture_observation_model::observation::{Observation, PauseState};
use starrupture_observation_model::wave::{HudFrame, HudPresenter, style};
use starrupture_wave_core::{config::Config, formatting::now_ms};
use std::{
    ffi::c_void,
    mem::size_of,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
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
    value: Option<Box<Observation>>,
    menu: Option<(bool, u64)>,
    process: u32,
}
struct App {
    shared: Arc<Mutex<Reading>>,
    stop: Arc<AtomicBool>,
    frame: HudFrame,
    presenter: HudPresenter,
    preview: Option<String>,
    hidden: bool,
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
    let p = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) } as *mut App;
    if p.is_null() {
        None
    } else {
        Some(unsafe { &mut *p })
    }
}
unsafe extern "system" fn find_game(window: HWND, param: isize) -> i32 {
    let tuple = unsafe { &mut *(param as *mut (u32, HWND)) };
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(window, &mut pid) };
    if pid == tuple.0 && unsafe { IsWindowVisible(window) } != 0 {
        let mut name = [0u16; 64];
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
        "FadeoutEarly" => ("INCINERATING", [0xff, 0x71, 0x69]),
        "FadeoutCooling" => ("COOLING", [0xff, 0xaa, 0x66]),
        _ => style(phase),
    };
    HudFrame {
        stage,
        colour,
        next_label: if phase == "waiting" || phase == "PreWave" {
            "Rupture in"
        } else {
            "Next stage in"
        },
        countdown: if stage == "UNAVAILABLE" {
            "--:--".into()
        } else {
            "12:34".into()
        },
    }
}
unsafe fn tick(window: HWND) {
    let Some(a) = (unsafe { app(window) }) else {
        return;
    };
    let (frame, pid) = if let Some(phase) = &a.preview {
        (preview(phase), 0)
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
                "Wave timer layout: {}",
                if paused {
                    "paused"
                } else {
                    "gameplay"
                }
            );
        }
        (a.presenter.update(r.value.as_deref(), now_ms().max(0) as u64, 5000), r.process)
    };
    if a.frame != frame {
        a.frame = frame;
        unsafe { InvalidateRect(window, std::ptr::null(), 0) };
    }
    if a.preview.is_some() {
        if !a.hidden {
            unsafe { ShowWindow(window, SW_SHOWNOACTIVATE) };
        }
        return;
    }
    let mut found = (pid, std::ptr::null_mut());
    if pid != 0 {
        unsafe { EnumWindows(Some(find_game), (&mut found as *mut (u32, HWND)) as isize) };
    }
    a.game = found.1;
    let visible = !a.hidden && !a.game.is_null() && unsafe { IsIconic(a.game) } == 0;
    if !visible {
        unsafe { ShowWindow(window, SW_HIDE) };
        return;
    }
    let mut rect = RECT::default();
    if unsafe { GetClientRect(a.game, &mut rect) } == 0 {
        unsafe { ShowWindow(window, SW_HIDE) };
        return;
    }
    let mut origin = POINT {
        x: 0,
        y: 0,
    };
    unsafe { ClientToScreen(a.game, &mut origin) };
    let w = rect.right.max(1);
    let h = rect.bottom.max(1);
    let width = ((w as f64 * 0.087).round() as i32).max(180);
    let height = ((h as f64 * 0.059).round() as i32).max(60) + STAGE_TOP_PADDING;
    if !a.user_moved {
        let mut current = RECT::default();
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
            || unsafe { IsWindowVisible(window) } == 0
        {
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
        unsafe { InvalidateRect(window, std::ptr::null(), 0) };
    }
}
unsafe fn paint(window: HWND) {
    let Some(a) = (unsafe { app(window) }) else {
        return;
    };
    let mut ps = PAINTSTRUCT::default();
    let dc = unsafe { BeginPaint(window, &mut ps) };
    unsafe {
        if !render_layer(window, dc, &a.frame) {
            eprintln!(
                "Wave timer: layered frame update failed: {}",
                std::io::Error::last_os_error()
            );
        }
        EndPaint(window, &ps)
    };
}
unsafe fn menu(window: HWND) {
    let m = unsafe { CreatePopupMenu() };
    let Some(a) = (unsafe { app(window) }) else {
        return;
    };
    unsafe {
        AppendMenuW(
            m,
            MF_STRING,
            1,
            wide(if a.hidden {
                "Show"
            } else {
                "Hide"
            })
            .as_ptr(),
        );
        AppendMenuW(
            m,
            MF_STRING,
            2,
            wide(if a.unlocked {
                "Lock position"
            } else {
                "Unlock to move"
            })
            .as_ptr(),
        );
        AppendMenuW(m, MF_STRING, 3, wide("Reset position").as_ptr());
        AppendMenuW(m, MF_SEPARATOR, 0, std::ptr::null());
        AppendMenuW(m, MF_STRING, 4, wide("Exit").as_ptr())
    };
    let mut cursor = POINT::default();
    unsafe {
        GetCursorPos(&mut cursor);
        SetForegroundWindow(window)
    };
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
    unsafe { DestroyMenu(m) };
    match choice {
        1 => a.hidden = !a.hidden,
        2 => {
            a.unlocked = !a.unlocked;
            let style = unsafe { GetWindowLongPtrW(window, GWL_EXSTYLE) };
            unsafe {
                SetWindowLongPtrW(
                    window,
                    GWL_EXSTYLE,
                    if a.unlocked {
                        style & !(WS_EX_TRANSPARENT as isize)
                    } else {
                        style | WS_EX_TRANSPARENT as isize
                    },
                )
            };
        }
        3 => a.user_moved = false,
        4 => {
            unsafe { DestroyWindow(window) };
        }
        _ => {}
    }
}
unsafe extern "system" fn procedure(window: HWND, message: u32, w: usize, l: isize) -> isize {
    match message {
        WM_NCCREATE => {
            let create = unsafe { &*(l as *const CREATESTRUCTW) };
            unsafe { SetWindowLongPtrW(window, GWLP_USERDATA, create.lpCreateParams as isize) };
            1
        }
        crate::window_events::CHANGED => {
            crate::window_events::acknowledge();
            // Focus recovery must not wait for the reader mutex, timing/geometry
            // work, or the next low-priority WM_TIMER. Reuse the checked game HWND.
            if let Some(a) = unsafe { app(window) }
                && !a.hidden
                && a.preview.is_none()
            {
                crate::stacking::follow_game(window, a.game);
            }
            0
        }
        WM_TIMER | READING_READY => {
            unsafe { tick(window) };
            0
        }
        WM_PAINT => {
            unsafe { paint(window) };
            0
        }
        WM_SIZE => {
            let width = (l as u32 & 0xffff) as i32;
            let height = ((l as u32 >> 16) & 0xffff) as i32;
            let points = panel_points(width, height, 0);
            let region = unsafe { CreatePolygonRgn(points.as_ptr(), points.len() as i32, WINDING) };
            if !region.is_null() && unsafe { SetWindowRgn(window, region, 1) } == 0 {
                unsafe { DeleteObject(region) };
            }
            0
        }
        WM_ERASEBKGND => 1,
        WM_MOUSEACTIVATE => MA_NOACTIVATE as isize,
        WM_NCHITTEST => {
            if unsafe { app(window) }.is_some_and(|a| !a.unlocked) {
                HTTRANSPARENT as isize
            } else {
                HTCLIENT as isize
            }
        }
        WM_LBUTTONDOWN => {
            if let Some(a) = unsafe { app(window) }
                && a.unlocked
            {
                a.user_moved = true;
                unsafe {
                    ReleaseCapture();
                    SendMessageW(window, WM_NCLBUTTONDOWN, HTCAPTION as usize, 0)
                };
            }
            0
        }
        TRAY => {
            if l as u32 == WM_RBUTTONUP {
                unsafe { menu(window) }
            };
            0
        }
        WM_DESTROY => {
            if let Some(a) = unsafe { app(window) } {
                a.stop.store(true, Ordering::Relaxed)
            };
            let icon = NOTIFYICONDATAW {
                cbSize: size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: window,
                uID: 1,
                ..Default::default()
            };
            unsafe {
                Shell_NotifyIconW(NIM_DELETE, &icon);
                KillTimer(window, 1);
                PostQuitMessage(0)
            };
            0
        }
        _ => unsafe { DefWindowProcW(window, message, w, l) },
    }
}
// One normal HUD instance per interactive Windows session; tests bypass this.
struct Instance(HANDLE);
impl Instance {
    fn acquire() -> Result<Self> {
        let handle = unsafe {
            CreateMutexW(std::ptr::null(), 0, wide("Local\\StarRuptureWaveOverlay").as_ptr())
        };
        if handle.is_null() {
            return Err(std::io::Error::last_os_error().into());
        }
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            unsafe { CloseHandle(handle) };
            return Ok(Self(std::ptr::null_mut()));
        }
        Ok(Self(handle))
    }
}
impl Drop for Instance {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CloseHandle(self.0) };
        }
    }
}
pub fn run() -> Result<()> {
    let _fonts = crate::fonts::Fonts::load()?;
    let mut args = std::env::args().skip(1);
    let mut path: Option<PathBuf> = None;
    let mut preview_phase = None;
    if let Some(first) = args.next() {
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
            "--config" => path = Some(args.next().ok_or("Missing configuration path")?.into()),
            "--preview-stage" => preview_phase = Some(args.next().ok_or("Missing preview phase")?),
            _ => return Err("Usage: starrupture-wave-overlay --config FILE [--preview-stage waiting|PreWave|Moving|FadeoutEarly|FadeoutCooling|Fadeout|Growback|unavailable]".into()),
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
    let mut config = Config::load(path.as_deref())?;
    if preview_phase.is_none() && config.live.as_ref().is_some_and(|v| !v.enabled) {
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
        return Ok(());
    }

    let stop = Arc::new(AtomicBool::new(false));
    let shared = Arc::new(Mutex::new(Reading {
        value: None,
        menu: None,
        process: 0,
    }));
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    let class = wide("StarRuptureWaveTimer");
    let module = unsafe { GetModuleHandleW(std::ptr::null()) };
    let app_icons = crate::icons::Icons::load()?;
    let wc = WNDCLASSW {
        hIcon: app_icons.large,
        lpfnWndProc: Some(procedure),
        hInstance: module,
        lpszClassName: class.as_ptr(),
        hCursor: unsafe { LoadCursorW(std::ptr::null_mut(), IDC_ARROW) },
        ..Default::default()
    };
    if unsafe { RegisterClassW(&wc) } == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let mut app = Box::new(App {
        shared,
        stop: stop.clone(),
        frame: preview("unavailable"),
        presenter: HudPresenter::default(),
        preview: preview_phase,
        hidden: false,
        unlocked: false,
        user_moved: false,
        pause_position: false,
        last_rect: RECT::default(),
        game: std::ptr::null_mut(),
    });
    let window = unsafe {
        CreateWindowExW(
            crate::stacking::EXTENDED_STYLE,
            class.as_ptr(),
            wide("StarRupture Wave Timer").as_ptr(),
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
        stop.store(true, Ordering::Relaxed);
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
    let tip = wide("StarRupture Wave Timer â€” right-click for controls");
    icon.szTip[..tip.len()].copy_from_slice(&tip);
    if unsafe { Shell_NotifyIconW(NIM_ADD, &icon) } == 0 {
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
        Some(reader(config, app.shared.clone(), stop.clone(), window as usize))
    } else {
        None
    };
    unsafe {
        SetTimer(window, 1, 100, None);
        tick(window)
    };
    eprintln!(
        "Wave timer running (PID{}); right-click its tray icon to hide, move or exit. Preview values are illustrative.",
        unsafe { GetCurrentProcessId() }
    );
    let mut message = MSG::default();
    loop {
        let state = unsafe { GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) };
        if state <= 0 {
            break;
        }
        unsafe {
            TranslateMessage(&message);
            DispatchMessageW(&message)
        };
    }
    drop(window_events);
    stop.store(true, Ordering::Relaxed);
    if let Some(worker) = worker {
        let _ = worker.join();
    }
    Ok(())
}

fn reader(
    config: Config,
    shared: Arc<Mutex<Reading>>,
    stop: Arc<AtomicBool>,
    window: usize,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        use crate::stream_client::Update;
        let publish = |event: Update| {
            let mut r = shared.lock().unwrap_or_else(|e| e.into_inner());
            match event {
                Update::Reset(pid) => {
                    r.process = pid;
                    r.menu = None;
                    r.value = None;
                }
                Update::Menu(menu, at) => r.menu = Some((menu, at)),
                Update::World(value) => r.value = Some(value),
                Update::Unavailable(reason) => {
                    eprintln!("Live observation unavailable: {reason}");
                    r.menu = None;
                    r.value = None;
                }
            }
            drop(r);
            if !stop.load(Ordering::Relaxed) {
                unsafe {
                    PostMessageW(window as HWND, READING_READY, 0, 0);
                }
            }
        };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::stream_client::run(config, stop.clone(), &publish)
        }));
        match result {
            Ok(Ok(stats)) => eprintln!("Embedded reader stopped: {stats:?}"),
            Ok(Err(error)) => {
                eprintln!("Embedded reader unavailable: {error}");
                publish(Update::Unavailable(error));
            }
            Err(_) => publish(Update::Unavailable("Read-only worker failed; no fallback".into())),
        }
        eprintln!("Wave timer reader joined; no reads remain in flight.");
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
