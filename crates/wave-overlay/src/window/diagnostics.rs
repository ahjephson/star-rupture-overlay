//! Native diagnostics using owned hidden or off-screen windows and synthetic HUD data.
use super::render::{BACKGROUND_ALPHA, LayerRenderer, STAGE_TOP_PADDING, draw};
use super::{App, Reading, Result, preview, procedure, wide};
use serde_json::json;
use starrupture_observation_model::shutdown::StopSignal;
use starrupture_observation_model::wave::HudPresenter;
use std::{
    mem::size_of,
    sync::{Arc, Mutex},
};
use windows_sys::Win32::{
    Foundation::*, Graphics::Gdi::*, System::LibraryLoader::GetModuleHandleW,
    UI::WindowsAndMessaging::*,
};

/// Prove event -> posted message -> normal stacking with no UI timer and the
/// reader mutex deliberately held. Uses only off-screen windows we own.
pub(super) fn window_events_self_test() -> Result<()> {
    use std::time::{Duration, Instant};
    use windows_sys::Win32::UI::Accessibility::NotifyWinEvent;
    struct Fixture(HWND);
    impl Drop for Fixture {
        fn drop(&mut self) {
            unsafe {
                DestroyWindow(self.0);
            }
        }
    }
    unsafe {
        let foreground = GetForegroundWindow();
        let module = GetModuleHandleW(std::ptr::null());
        let hud_class = wide("StarRuptureEventHudSelfTest");
        let game_class = wide("StarRuptureEventGameSelfTest");
        let classes: [(&[u16], WNDPROC); 2] =
            [(&hud_class, Some(procedure)), (&game_class, Some(DefWindowProcW))];
        for (class, callback) in classes {
            let wc = WNDCLASSW {
                lpfnWndProc: callback,
                hInstance: module,
                lpszClassName: class.as_ptr(),
                ..Default::default()
            };
            if RegisterClassW(&wc) == 0 {
                return Err(std::io::Error::last_os_error().into());
            }
        }
        let shared = Arc::new(Mutex::new(Reading {
            value: None,
            menu: None,
            process: 0,
            connection: crate::connection::Connection::default(),
        }));
        let mut state = Box::new(App {
            recipe: crate::recipe_tracker::Panel::new(),
            dialogs: crate::dialogs::Dialogs::default(),
            dialog_context: crate::dialogs::Context::diagnostic(std::path::PathBuf::new()),
            closed_sessions: 0,
            launch: crate::connection::Launch::default(),
            tray_status: String::new(),
            shared: shared.clone(),
            stop: Arc::new(StopSignal::default()),
            frame: preview("waiting"),
            presenter: HudPresenter::default(),
            renderer: LayerRenderer::default(),
            preview: None,
            hidden: false,
            unlocked: false,
            user_moved: false,
            pause_position: false,
            last_rect: RECT::default(),
            game: std::ptr::null_mut(),
        });
        let game = Fixture(CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            game_class.as_ptr(),
            wide("Offscreen event game").as_ptr(),
            WS_POPUP,
            -30000,
            -30000,
            1000,
            800,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            module,
            std::ptr::null(),
        ));
        if game.0.is_null() {
            return Err("Cannot create event fixture game".into());
        }
        state.game = game.0;
        let hud = Fixture(CreateWindowExW(
            crate::stacking::EXTENDED_STYLE,
            hud_class.as_ptr(),
            wide("Offscreen event HUD").as_ptr(),
            WS_POPUP,
            -29992,
            -29992,
            220,
            84,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            module,
            (&mut *state as *mut App).cast(),
        ));
        if hud.0.is_null() {
            return Err("Cannot create event fixture HUD".into());
        }
        let flags = SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_NOOWNERZORDER | SWP_SHOWWINDOW;
        SetWindowPos(game.0, HWND_TOP, 0, 0, 0, 0, flags);
        if !crate::stacking::follow_game(hud.0, game.0) {
            return Err("Event fixture initial stacking failed".into());
        }
        let events = crate::window_events::WindowEvents::diagnostic(hud.0)?;
        let reader_lock = shared.lock().map_err(|_| "Fixture reader mutex poisoned")?;
        let mut handled = 0usize;
        let mut timers = 0usize;
        let mut max_micros = 0u128;
        for _ in 0..20 {
            SetWindowPos(game.0, HWND_TOP, 0, 0, 0, 0, flags);
            if GetWindow(game.0, GW_HWNDPREV) == hud.0 {
                return Err("Fixture did not reproduce returning-game order".into());
            }
            let before = handled;
            let start = Instant::now();
            // This is an allowed object notification, not a fabricated system
            // foreground event. It uses the same callback/posted-message path.
            NotifyWinEvent(EVENT_OBJECT_REORDER, game.0, OBJID_WINDOW, 0);
            while handled == before || GetWindow(game.0, GW_HWNDPREV) != hud.0 {
                if start.elapsed() > Duration::from_secs(2) {
                    return Err("Window event did not repair stacking without the timer".into());
                }
                let mut message = MSG::default();
                while PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                    if message.message == crate::window_events::CHANGED {
                        handled += 1;
                    }
                    if message.message == WM_TIMER {
                        timers += 1;
                    }
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
                std::thread::yield_now();
            }
            max_micros = max_micros.max(start.elapsed().as_micros());
        }
        if timers != 0 || GetForegroundWindow() != foreground {
            return Err("Event fixture changed focus or used a timer".into());
        }
        drop(reader_lock);
        drop(events);
        // Unregistered callbacks must not enqueue new work, including callbacks
        // already queued before unregistration (the target has been disabled).
        let before = handled;
        let start = Instant::now();
        NotifyWinEvent(EVENT_OBJECT_REORDER, game.0, OBJID_WINDOW, 0);
        while start.elapsed() < Duration::from_millis(30) {
            let mut message = MSG::default();
            while PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                if message.message == crate::window_events::CHANGED {
                    handled += 1;
                }
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            std::thread::yield_now();
        }
        if handled != before {
            return Err("Notification teardown left queued stacking work".into());
        }
        println!(
            "{}",
            json!({
                "status": "passed",
                "ownedOffscreenWindows": 2,
                "gameRead": false,
                "actualWindowsModified": false,
                "injectedCode": false,
                "notificationRoundTrips": 20,
                "timerMessages": timers,
                "readerMutexHeldDuringRecovery": true,
                "maxRecoveryMicros": max_micros,
                "unregisterStopsDelivery": true,
                "foregroundPreserved": true
            })
        );
        Ok(())
    }
}

/// Exercise our own hidden HWND only: no game reads, focus change or tray icon.
pub(super) fn window_self_test() -> Result<()> {
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
        let stop = Arc::new(StopSignal::default());
        let mut state = Box::new(App {
            recipe: crate::recipe_tracker::Panel::new(),
            dialogs: crate::dialogs::Dialogs::default(),
            dialog_context: crate::dialogs::Context::diagnostic(std::path::PathBuf::new()),
            closed_sessions: 0,
            launch: crate::connection::Launch::default(),
            tray_status: String::new(),
            shared: Arc::new(Mutex::new(Reading {
                value: None,
                menu: None,
                process: 0,
                connection: crate::connection::Connection::default(),
            })),
            stop: stop.clone(),
            frame: preview("Moving"),
            presenter: HudPresenter::default(),
            renderer: LayerRenderer::default(),
            preview: Some("Moving".into()),
            hidden: true,
            unlocked: false,
            user_moved: false,
            pause_position: false,
            last_rect: RECT::default(),
            game: std::ptr::null_mut(),
        });
        let flags = crate::stacking::EXTENDED_STYLE;
        let window = CreateWindowExW(
            flags,
            class.as_ptr(),
            wide("Overlay self-test").as_ptr(),
            WS_POPUP,
            0,
            0,
            220,
            80 + STAGE_TOP_PADDING,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            module,
            (&mut *state as *mut App).cast(),
        );
        if window.is_null() {
            return Err(std::io::Error::last_os_error().into());
        }
        let app_icons = crate::icons::Icons::load()?;
        app_icons.set_window(window);
        let assigned_icons = SendMessageW(window, WM_GETICON, ICON_BIG as usize, 0)
            == app_icons.large as isize
            && SendMessageW(window, WM_GETICON, ICON_SMALL as usize, 0) == app_icons.small as isize;
        let screen = GetDC(std::ptr::null_mut());
        let alpha_update = state.renderer.render(window, screen, &state.frame);
        ReleaseDC(std::ptr::null_mut(), screen);
        let locked = SendMessageW(window, WM_NCHITTEST, 0, 0) == HTTRANSPARENT as isize;
        state.unlocked = true;
        let movable = SendMessageW(window, WM_NCHITTEST, 0, 0) == HTCLIENT as isize;
        let no_activate = SendMessageW(window, WM_MOUSEACTIVATE, 0, 0) == MA_NOACTIVATE as isize;
        let styles = GetWindowLongPtrW(window, GWL_EXSTYLE) as u32 & flags == flags
            && GetWindowLongPtrW(window, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST == 0;
        let region = CreateRectRgn(0, 0, 0, 0);
        let shaped = GetWindowRgn(window, region) == COMPLEXREGION;
        DeleteObject(region);
        let invisible = IsWindowVisible(window) == 0;
        DestroyWindow(window);
        let destroyed = IsWindow(window) == 0 && stop.is_requested();
        let focus_preserved = foreground == GetForegroundWindow();
        if !(assigned_icons
            && alpha_update
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
            json!({
                "status": "ok",
                "mode": "hidden_native_window_self_test",
                "gameRead": false,
                "visibleOverlayLaunched": false,
                "assignedWindowIcons": assigned_icons,
                "perPixelLayerUpdate": alpha_update,
                "backgroundAlpha": BACKGROUND_ALPHA,
                "textAndBorderAlpha": 255,
                "clickThroughHitTest": locked,
                "movableHitTest": movable,
                "noActivate": no_activate,
                "styles": styles,
                "chamferedRegion": shaped,
                "hidden": invisible,
                "destroyed": destroyed,
                "foregroundPreserved": focus_preserved
            })
        );
        Ok(())
    }
}

pub(super) fn self_test(dir: &std::path::Path) -> Result<()> {
    use std::io::Write;
    std::fs::create_dir_all(dir)?;
    let width = 220;
    let height = 80 + STAGE_TOP_PADDING;
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
        let bitmap =
            CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
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
            "FadeoutEarly",
            "FadeoutCooling",
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
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|b| {
                    b[0] == frame.colour[2] && b[1] == frame.colour[1] && b[2] == frame.colour[0]
                })
                .count();
            let white = pixels
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|b| b[0] > 230 && b[1] > 230 && b[2] > 230)
                .count();
            if coloured < 100 || white < 30 {
                return Err("Rendered panel lacks expected stage title/white timer".into());
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
            checks.push(json!({
                "phase": phase,
                "displayLabel": frame.stage,
                "colour": frame.colour,
                "stagePixels": coloured,
                "whitePixels": white,
            }));
        }
        SelectObject(dc, old);
        DeleteObject(bitmap);
        DeleteDC(dc);
        ReleaseDC(std::ptr::null_mut(), screen);
        println!(
            "{}",
            json!({
                "status": "ok",
                "mode": "offscreen_GDI_self_test",
                "gameRead": false,
                "overlayLaunched": false,
                "width": width,
                "height": height,
                "checks": checks
            })
        );
    }
    Ok(())
}
