use serde_json::{Value, json};
use starrupture_wave_core::{config::Config, saves::now_ms};
use starrupture_observation_model::wave::{HudFrame, HudPresenter, style};
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
    System::{LibraryLoader::GetModuleHandleW, Threading::GetCurrentProcessId},
    UI::{HiDpi::*, Input::KeyboardAndMouse::ReleaseCapture, Shell::*, WindowsAndMessaging::*},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const TRAY: u32 = WM_APP + 1;
const READING_READY: u32 = WM_APP + 2;
fn pause_menu_layout(v: &Value, previous: bool) -> bool {
    if v["status"] != "ok" {
        return previous;
    }
    // Menu visibility controls position only, never simulation/timer pause truth.
    v["data"]["pause"]["pauseMenuOpen"]
        .as_bool()
        .or_else(|| v["data"]["pause"]["paused"].as_bool())
        .unwrap_or(previous)
}
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}
fn rgb(c: [u8; 3]) -> u32 {
    u32::from(c[0]) | (u32::from(c[1]) << 8) | (u32::from(c[2]) << 16)
}
struct Reading {
    value: Value,
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
    let (stage, colour) = style(phase);
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
            None => pause_menu_layout(&r.value, a.pause_position),
        };
        if paused != a.pause_position {
            a.pause_position = paused;
            eprintln!(
                "Wave timer layout: {}",
                if paused { "paused" } else { "gameplay" }
            );
        }
        (
            a.presenter.update(&r.value, now_ms().max(0) as u64, 5000),
            r.process,
        )
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
    let mut origin = POINT { x: 0, y: 0 };
    unsafe { ClientToScreen(a.game, &mut origin) };
    let w = rect.right.max(1);
    let h = rect.bottom.max(1);
    let width = ((w as f64 * 0.087).round() as i32).max(180);
    let height = ((h as f64 * 0.059).round() as i32).max(60);
    if !a.user_moved {
        let mut current = RECT::default();
        unsafe { GetWindowRect(window, &mut current) };
        let gap = (w as f64 * 0.003).round() as i32;
        let x = origin.x + gap;
        // Player screenshot: pause border ends at80px on1440px client height.
        // Match the left inset below that border; play uses the same top inset.
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
                    HWND_TOPMOST,
                    x,
                    y,
                    width,
                    height,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW,
                )
            };
        }
    } else {
        if unsafe { IsWindowVisible(window) } == 0 {
            unsafe {
                SetWindowPos(
                    window,
                    HWND_TOPMOST,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
                )
            };
        }
    }
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
const BACKGROUND_ALPHA: u8 = 230;
fn layer_alpha(pixels: &mut [u8], width: usize, height: usize) {
    let cut = (height / 8).max(5);
    for (i, pixel) in pixels.chunks_exact_mut(4).enumerate() {
        let x = i % width;
        let y = i / width;
        let dx = x.min(width - 1 - x);
        let dy = y.min(height - 1 - y);
        if dx + dy < cut {
            pixel.fill(0);
            continue;
        }
        let alpha = if pixel[..3] == [32, 30, 24] {
            BACKGROUND_ALPHA
        } else {
            255
        };
        for channel in &mut pixel[..3] {
            *channel = ((u16::from(*channel) * u16::from(alpha) + 127) / 255) as u8;
        }
        pixel[3] = alpha;
    }
}
unsafe fn render_layer(window: HWND, screen: HDC, frame: &HudFrame) -> bool {
    let mut rect = RECT::default();
    unsafe { GetClientRect(window, &mut rect) };
    let width = rect.right.max(1);
    let height = rect.bottom.max(1);
    let dc = unsafe { CreateCompatibleDC(screen) };
    if dc.is_null() {
        return false;
    }
    let mut info = BITMAPINFO::default();
    info.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
    info.bmiHeader.biWidth = width;
    info.bmiHeader.biHeight = -height;
    info.bmiHeader.biPlanes = 1;
    info.bmiHeader.biBitCount = 32;
    info.bmiHeader.biCompression = BI_RGB;
    let mut bits = std::ptr::null_mut();
    let bitmap = unsafe {
        CreateDIBSection(
            dc,
            &info,
            DIB_RGB_COLORS,
            &mut bits,
            std::ptr::null_mut(),
            0,
        )
    };
    if bitmap.is_null() || bits.is_null() {
        unsafe { DeleteDC(dc) };
        return false;
    }
    let old = unsafe { SelectObject(dc, bitmap) };
    unsafe {
        draw(dc, rect, frame);
        GdiFlush();
    }
    let pixels =
        unsafe { std::slice::from_raw_parts_mut(bits as *mut u8, (width * height * 4) as usize) };
    layer_alpha(pixels, width as usize, height as usize);
    let source = POINT { x: 0, y: 0 };
    let size = SIZE {
        cx: width,
        cy: height,
    };
    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: 255,
        AlphaFormat: AC_SRC_ALPHA as u8,
    };
    let ok = unsafe {
        UpdateLayeredWindow(
            window,
            std::ptr::null_mut(),
            std::ptr::null(),
            &size,
            dc,
            &source,
            0,
            &blend,
            ULW_ALPHA,
        )
    } != 0;
    unsafe {
        SelectObject(dc, old);
        DeleteObject(bitmap);
        DeleteDC(dc);
    }
    ok
}
unsafe fn draw(dc: HDC, rect: RECT, frame: &HudFrame) {
    let width = rect.right;
    let height = rect.bottom;
    let background = unsafe { CreateSolidBrush(rgb([24, 30, 32])) };
    unsafe {
        FillRect(dc, &rect, background);
        DeleteObject(background)
    };
    let edge = unsafe { CreatePen(PS_SOLID, 1, rgb(frame.colour)) };
    let old_pen = unsafe { SelectObject(dc, edge) };
    let old_brush = unsafe { SelectObject(dc, GetStockObject(NULL_BRUSH)) };
    let c = (height / 8).max(5);
    let points = [
        POINT { x: c, y: 1 },
        POINT {
            x: width - c - 1,
            y: 1,
        },
        POINT { x: width - 2, y: c },
        POINT {
            x: width - 2,
            y: height - c - 1,
        },
        POINT {
            x: width - c - 1,
            y: height - 2,
        },
        POINT {
            x: c,
            y: height - 2,
        },
        POINT {
            x: 1,
            y: height - c - 1,
        },
        POINT { x: 1, y: c },
    ];
    unsafe {
        Polygon(dc, points.as_ptr(), points.len() as i32);
        SelectObject(dc, old_pen);
        SelectObject(dc, old_brush);
        DeleteObject(edge);
        SetBkMode(dc, TRANSPARENT as i32)
    };
    let font_name = wide("Oxanium");
    let small = (height as f64 * 0.30).round() as i32;
    let large = (height as f64 * 0.48).round() as i32;
    let font = unsafe {
        CreateFontW(
            -small,
            0,
            0,
            0,
            700,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            OUT_DEFAULT_PRECIS as u32,
            CLIP_DEFAULT_PRECIS as u32,
            CLEARTYPE_QUALITY as u32,
            DEFAULT_PITCH as u32,
            font_name.as_ptr(),
        )
    };
    let old = unsafe { SelectObject(dc, font) };
    unsafe { SetTextColor(dc, rgb(frame.colour)) };
    let stage = wide(frame.stage);
    let mut title = RECT {
        left: c + 3,
        top: height / 9,
        right: width - c,
        bottom: height / 2,
    };
    unsafe {
        DrawTextW(
            dc,
            stage.as_ptr(),
            (stage.len() - 1) as i32,
            &mut title,
            DT_LEFT | DT_SINGLELINE | DT_NOPREFIX,
        )
    };
    unsafe {
        SelectObject(dc, old);
        DeleteObject(font)
    };
    let font = unsafe {
        CreateFontW(
            -(height as f64 * 0.22).round() as i32,
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            OUT_DEFAULT_PRECIS as u32,
            CLIP_DEFAULT_PRECIS as u32,
            CLEARTYPE_QUALITY as u32,
            DEFAULT_PITCH as u32,
            font_name.as_ptr(),
        )
    };
    let old = unsafe { SelectObject(dc, font) };
    unsafe { SetTextColor(dc, rgb([245, 247, 248])) };
    let label = wide(frame.next_label);
    let mut line = RECT {
        left: c + 3,
        top: height / 2,
        right: width / 2 + width / 10,
        bottom: height - c,
    };
    unsafe {
        DrawTextW(
            dc,
            label.as_ptr(),
            (label.len() - 1) as i32,
            &mut line,
            DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
        );
        SelectObject(dc, old);
        DeleteObject(font)
    };
    let mut digit_height = large;
    let timer_width = width - c - 3 - width / 2;
    let timer_height = height - c - (height / 2 - 2);
    let text = wide(&frame.countdown);
    let digits = loop {
        let font = unsafe {
            CreateFontW(
                -digit_height,
                0,
                0,
                0,
                800,
                0,
                0,
                0,
                DEFAULT_CHARSET as u32,
                OUT_DEFAULT_PRECIS as u32,
                CLIP_DEFAULT_PRECIS as u32,
                CLEARTYPE_QUALITY as u32,
                DEFAULT_PITCH as u32,
                wide("Oxanium ExtraBold").as_ptr(),
            )
        };
        let old = unsafe { SelectObject(dc, font) };
        let mut extent = SIZE::default();
        unsafe {
            GetTextExtentPoint32W(dc, text.as_ptr(), (text.len() - 1) as i32, &mut extent);
            SelectObject(dc, old);
        }
        if digit_height <= 8 || extent.cx <= timer_width && extent.cy <= timer_height {
            break font;
        }
        unsafe {
            DeleteObject(font);
        }
        digit_height -= 1;
    };
    let old = unsafe { SelectObject(dc, digits) };
    let mut timer = RECT {
        left: width / 2,
        top: height / 2 - 2,
        right: width - c - 3,
        bottom: height - c,
    };
    unsafe {
        DrawTextW(
            dc,
            text.as_ptr(),
            (text.len() - 1) as i32,
            &mut timer,
            DT_RIGHT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
        );
        SelectObject(dc, old);
        DeleteObject(digits);
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
            wide(if a.hidden { "Show" } else { "Hide" }).as_ptr(),
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
            let c = (height / 8).max(5);
            let points = [
                POINT { x: c, y: 0 },
                POINT { x: width - c, y: 0 },
                POINT { x: width, y: c },
                POINT {
                    x: width,
                    y: height - c,
                },
                POINT {
                    x: width - c,
                    y: height,
                },
                POINT { x: c, y: height },
                POINT {
                    x: 0,
                    y: height - c,
                },
                POINT { x: 0, y: c },
            ];
            let region = unsafe { CreatePolygonRgn(points.as_ptr(), 8, WINDING) };
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
            if let Some(a) = unsafe { app(window) } {
                if a.unlocked {
                    a.user_moved = true;
                    unsafe {
                        ReleaseCapture();
                        SendMessageW(window, WM_NCLBUTTONDOWN, HTCAPTION as usize, 0)
                    };
                }
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
            let mut icon = NOTIFYICONDATAW {
                cbSize: size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: window,
                uID: 1,
                ..Default::default()
            };
            unsafe {
                Shell_NotifyIconW(NIM_DELETE, &mut icon);
                KillTimer(window, 1);
                PostQuitMessage(0)
            };
            0
        }
        _ => unsafe { DefWindowProcW(window, message, w, l) },
    }
}
pub fn run() -> Result<()> {
    let _fonts = crate::fonts::Fonts::load()?;
    let mut args = std::env::args().skip(1);
    let mut path: Option<PathBuf> = None;
    let mut preview_phase = None;
    if let Some(first) = args.next() {
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
        match arg.as_str(){"--config"=>path=Some(args.next().ok_or("Missing configuration path")?.into()),"--preview-stage"=>preview_phase=Some(args.next().ok_or("Missing preview phase")?),_=>return Err("Usage: starrupture-wave-overlay --config FILE [--preview-stage waiting|PreWave|Moving|Fadeout|Growback|unavailable]".into())}
    }
    if let Some(phase) = &preview_phase {
        if ![
            "waiting",
            "PreWave",
            "Moving",
            "Fadeout",
            "Growback",
            "unavailable",
        ]
        .contains(&phase.as_str())
        {
            return Err("Unknown preview stage".into());
        }
    }
    let config = Config::load(path.as_deref())?;
    if preview_phase.is_none() && !config.live.as_ref().is_some_and(|v| v.enabled) {
        return Err("Overlay needs an explicitly enabled pinned live helper".into());
    }
    let stop = Arc::new(AtomicBool::new(false));
    let shared = Arc::new(Mutex::new(Reading {
        value: json!({}),
        menu: None,
        process: 0,
    }));
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    let class = wide("StarRuptureWaveTimer");
    let module = unsafe { GetModuleHandleW(std::ptr::null()) };
    let wc = WNDCLASSW {
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
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT | WS_EX_LAYERED,
            class.as_ptr(),
            wide("StarRupture Wave Timer").as_ptr(),
            WS_POPUP,
            100,
            100,
            220,
            80,
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
    let mut icon = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: window,
        uID: 1,
        uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
        uCallbackMessage: TRAY,
        hIcon: unsafe { LoadIconW(std::ptr::null_mut(), IDI_APPLICATION) },
        ..Default::default()
    };
    let tip = wide("StarRupture Wave Timer â€” right-click for controls");
    icon.szTip[..tip.len()].copy_from_slice(&tip);
    if unsafe { Shell_NotifyIconW(NIM_ADD, &mut icon) } == 0 {
        unsafe { DestroyWindow(window) };
        return Err("Could not create overlay exit/control tray icon".into());
    }
    let worker = if app.preview.is_none() {
        Some(reader(
            config,
            app.shared.clone(),
            stop.clone(),
            window as usize,
        ))
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
    stop.store(true, Ordering::Relaxed);
    if let Some(worker) = worker {
        let _ = worker.join();
    }
    Ok(())
}

/// Exercise our own hidden HWND only: no game reads, focus change or tray icon.
fn window_self_test() -> Result<()> {
    unsafe {
        let foreground = GetForegroundWindow();
        let module = GetModuleHandleW(std::ptr::null());
        let class = wide("StarRuptureWaveTimerSelfTest");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(procedure),
            hInstance: module,
            lpszClassName: class.as_ptr(),
            ..Default::default()
        };
        if RegisterClassW(&wc) == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let stop = Arc::new(AtomicBool::new(false));
        let mut state = Box::new(App {
            shared: Arc::new(Mutex::new(Reading {
                value: json!({}),
                menu: None,
                process: 0,
            })),
            stop: stop.clone(),
            frame: preview("Moving"),
            presenter: HudPresenter::default(),
            preview: Some("Moving".into()),
            hidden: true,
            unlocked: false,
            user_moved: false,
            pause_position: false,
            last_rect: RECT::default(),
            game: std::ptr::null_mut(),
        });
        let flags =
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT | WS_EX_LAYERED;
        let window = CreateWindowExW(
            flags,
            class.as_ptr(),
            wide("Wave timer self-test").as_ptr(),
            WS_POPUP,
            0,
            0,
            220,
            80,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            module,
            (&mut *state as *mut App).cast(),
        );
        if window.is_null() {
            return Err(std::io::Error::last_os_error().into());
        }
        let screen = GetDC(std::ptr::null_mut());
        let alpha_update = render_layer(window, screen, &state.frame);
        ReleaseDC(std::ptr::null_mut(), screen);
        let locked = SendMessageW(window, WM_NCHITTEST, 0, 0) == HTTRANSPARENT as isize;
        state.unlocked = true;
        let movable = SendMessageW(window, WM_NCHITTEST, 0, 0) == HTCLIENT as isize;
        let no_activate = SendMessageW(window, WM_MOUSEACTIVATE, 0, 0) == MA_NOACTIVATE as isize;
        let styles = GetWindowLongPtrW(window, GWL_EXSTYLE) as u32 & flags == flags;
        let region = CreateRectRgn(0, 0, 0, 0);
        let shaped = GetWindowRgn(window, region) == COMPLEXREGION;
        DeleteObject(region);
        let invisible = IsWindowVisible(window) == 0;
        DestroyWindow(window);
        let destroyed = IsWindow(window) == 0 && stop.load(Ordering::Relaxed);
        let focus_preserved = foreground == GetForegroundWindow();
        if !(alpha_update
            && locked
            && movable
            && no_activate
            && styles
            && shaped
            && invisible
            && destroyed
            && focus_preserved)
        {
            return Err("Native overlay window lifecycle/style verification failed".into());
        }
        println!(
            "{}",
            json!({"status":"ok","mode":"hidden_native_window_self_test","gameRead":false,"visibleOverlayLaunched":false,"perPixelLayerUpdate":alpha_update,"backgroundAlpha":BACKGROUND_ALPHA,"textAndBorderAlpha":255,"clickThroughHitTest":locked,"movableHitTest":movable,"noActivate":no_activate,"styles":styles,"chamferedRegion":shaped,"hidden":invisible,"destroyed":destroyed,"foregroundPreserved":focus_preserved})
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn background_translucency_does_not_fade_text_and_corners_are_clear() {
        let mut pixels = [32u8, 30, 24, 0].repeat(20 * 20);
        let text = (10 * 20 + 10) * 4;
        pixels[text..text + 4].copy_from_slice(&[248, 247, 245, 0]);
        layer_alpha(&mut pixels, 20, 20);
        assert_eq!(&pixels[..4], &[0, 0, 0, 0]);
        assert_eq!(&pixels[text..text + 4], &[248, 247, 245, 255]);
        let background = (10 * 20 + 8) * 4;
        assert_eq!(&pixels[background..background + 4], &[29, 27, 22, 230]);
        assert!(
            pixels
                .chunks_exact(4)
                .all(|p| p[..3].iter().all(|c| *c <= p[3]))
        );
    }
    #[test]
    fn menu_layout_is_independent_of_simulation_pause_and_unknown_preserves_position() {
        assert!(pause_menu_layout(
            &json!({"status":"ok","data":{"pause":{"pauseMenuOpen":true,"paused":false}}}),
            false
        ));
        assert!(!pause_menu_layout(
            &json!({"status":"ok","data":{"pause":{"pauseMenuOpen":false,"paused":null}}}),
            true
        ));
        assert!(pause_menu_layout(&json!({"status":"unavailable"}), true));
        assert!(pause_menu_layout(
            &json!({"status":"ok","data":{"pause":{"paused":true}}}),
            false
        ));
    }
}

fn self_test(dir: &std::path::Path) -> Result<()> {
    use std::io::Write;
    std::fs::create_dir_all(dir)?;
    let width = 220;
    let height = 80;
    unsafe {
        let screen = GetDC(std::ptr::null_mut());
        let dc = CreateCompatibleDC(screen);
        if dc.is_null() {
            return Err("No offscreen GDI device".into());
        }
        let mut info = BITMAPINFO::default();
        info.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = width;
        info.bmiHeader.biHeight = -height;
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        info.bmiHeader.biCompression = BI_RGB;
        let mut bits = std::ptr::null_mut();
        let bitmap = CreateDIBSection(
            dc,
            &info,
            DIB_RGB_COLORS,
            &mut bits,
            std::ptr::null_mut(),
            0,
        );
        if bitmap.is_null() || bits.is_null() {
            DeleteDC(dc);
            ReleaseDC(std::ptr::null_mut(), screen);
            return Err("No offscreen bitmap".into());
        }
        let old = SelectObject(dc, bitmap);
        let mut checks = Vec::new();
        for phase in [
            "waiting",
            "PreWave",
            "Moving",
            "Fadeout",
            "Growback",
            "unavailable",
        ] {
            let frame = preview(phase);
            draw(
                dc,
                RECT {
                    left: 0,
                    top: 0,
                    right: width,
                    bottom: height,
                },
                &frame,
            );
            GdiFlush();
            let pixels =
                std::slice::from_raw_parts(bits as *const u8, (width * height * 4) as usize);
            let coloured = pixels
                .chunks_exact(4)
                .filter(|b| {
                    b[0] == frame.colour[2] && b[1] == frame.colour[1] && b[2] == frame.colour[0]
                })
                .count();
            let white = pixels
                .chunks_exact(4)
                .filter(|b| b[0] > 230 && b[1] > 230 && b[2] > 230)
                .count();
            if coloured < 100 || white < 30 {
                return Err("Rendered panel lacks expected stage border/white timer".into());
            }
            let mut header = Vec::new();
            header.extend(b"BM");
            header.extend((54 + pixels.len() as u32).to_le_bytes());
            header.extend([0u8; 4]);
            header.extend(54u32.to_le_bytes());
            header.extend(40u32.to_le_bytes());
            header.extend(width.to_le_bytes());
            header.extend((-height).to_le_bytes());
            header.extend(1u16.to_le_bytes());
            header.extend(32u16.to_le_bytes());
            header.extend([0u8; 24]);
            let mut file = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(dir.join(format!("{phase}.bmp")))?;
            file.write_all(&header)?;
            file.write_all(pixels)?;
            checks.push(json!({"phase":phase,"displayLabel":frame.stage,"colour":frame.colour,"stagePixels":coloured,"whitePixels":white}));
        }
        SelectObject(dc, old);
        DeleteObject(bitmap);
        DeleteDC(dc);
        ReleaseDC(std::ptr::null_mut(), screen);
        println!(
            "{}",
            json!({"status":"ok","mode":"offscreen_GDI_self_test","gameRead":false,"overlayLaunched":false,"width":width,"height":height,"checks":checks})
        );
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
                    r.value = starrupture_wave_core::live::unavailable(
                        "SESSION_CHANGED",
                        "Waiting for fresh world observation",
                    );
                }
                Update::Menu(menu, at) => r.menu = Some((menu, at)),
                Update::World(value) => r.value = value,
                Update::Unavailable(reason) => {
                    r.menu = None;
                    r.value =
                        starrupture_wave_core::live::unavailable("OVERLAY_READER_UNAVAILABLE", reason);
                }
            }
            drop(r);
            if !stop.load(Ordering::Relaxed) {
                unsafe {
                    PostMessageW(window as HWND, READING_READY, 0, 0);
                }
            }
        };
        if let Err(error) = crate::stream_client::run(config, stop.clone(), &publish) {
            eprintln!("Persistent overlay reader unavailable: {error}");
            publish(Update::Unavailable(error));
        }
        eprintln!("Wave timer reader stopped; owned helper reaped, no reads remain in flight.");
    })
}
