//! Launch feedback lives on the UI thread and uses the existing tray icon.
use super::*;

const MIN_SPLASH_TIME: Duration = Duration::from_millis(800);
const STARTUP_LIMIT: Duration = Duration::from_secs(3);

#[derive(Default)]
pub(super) struct Startup {
    began: Option<Instant>,
    splash: Option<Splash>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Readiness {
    Checking,
    Connecting,
    Waiting,
    Attached,
    FeaturesDisabled,
    Unsupported,
}

impl Readiness {
    fn from_connection(connection: &Connection) -> Self {
        use crate::connection::GameStatus;
        match connection.status {
            GameStatus::Unknown => Self::Checking,
            GameStatus::Waiting => Self::Waiting,
            GameStatus::FeaturesDisabled {
                ..
            } => Self::FeaturesDisabled,
            GameStatus::Running {
                supported: false,
                ..
            } => Self::Unsupported,
            GameStatus::Running {
                ..
            } if connection.connected => Self::Attached,
            GameStatus::Running {
                ..
            } => Self::Connecting,
        }
    }

    fn terminal(self) -> bool {
        !matches!(self, Self::Checking | Self::Connecting)
    }

    fn startup_complete(self, elapsed: Duration, splash_visible: bool) -> bool {
        elapsed >= STARTUP_LIMIT
            || (self.terminal() && (!splash_visible || elapsed >= MIN_SPLASH_TIME))
    }

    fn splash_status(self) -> &'static str {
        match self {
            Self::Checking => "Starting overlay…",
            Self::Connecting => "Connecting to StarRupture…",
            Self::Attached => "Connected to StarRupture",
            Self::Waiting => "Waiting for StarRupture",
            Self::FeaturesDisabled => "Overlay ready",
            Self::Unsupported => "Unsupported game build",
        }
    }

    fn notification(self, starting_game: bool) -> (&'static str, &'static str, u32) {
        let status = match self {
            Self::Checking => "Checking StarRupture status.",
            Self::Connecting => "StarRupture detected. Connecting in the background.",
            Self::Waiting if starting_game => "Waiting for StarRupture to start.",
            Self::Waiting => "Waiting for StarRupture.",
            Self::Attached => return ("", "Connected to StarRupture", NIIF_NONE),
            Self::FeaturesDisabled => "HUD features can be enabled in Settings.",
            Self::Unsupported => {
                return (
                    "StarRupture Overlay needs attention",
                    "This StarRupture build is unsupported. Open About for compatibility details.",
                    NIIF_WARNING,
                );
            }
        };
        ("StarRupture Overlay ready", status, NIIF_USER)
    }
}

impl Startup {
    pub fn start(&mut self) {
        self.splash = Splash::open();
        self.began = Some(Instant::now());
    }

    pub fn update(
        &mut self,
        owner: HWND,
        connection: &Connection,
        preferences: Preferences,
        starting_game: bool,
    ) {
        let Some(began) = self.began else {
            return;
        };
        let elapsed = began.elapsed();
        let readiness = Readiness::from_connection(connection);
        if readiness.startup_complete(elapsed, self.splash.is_some()) {
            self.began = None;
            self.splash = None;
            if preferences.startup_notification {
                let (title, message, flags) = readiness.notification(starting_game);
                notify(owner, title, message, flags);
            }
            return;
        }
        if let Some(splash) = &self.splash {
            splash.update(readiness, elapsed);
        }
    }
}

fn notify(owner: HWND, title: &str, message: &str, flags: u32) {
    let mut icon = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: owner,
        uID: 1,
        uFlags: NIF_INFO,
        dwInfoFlags: flags | NIIF_NOSOUND | NIIF_RESPECT_QUIET_TIME,
        ..Default::default()
    };
    for (dest, source) in icon.szInfoTitle.iter_mut().zip(title.encode_utf16()) {
        *dest = source;
    }
    for (dest, source) in icon.szInfo.iter_mut().zip(message.encode_utf16()) {
        *dest = source;
    }
    // SAFETY: The initialized message belongs to the owner's existing tray icon.
    unsafe { Shell_NotifyIconW(NIM_MODIFY, &icon) };
}

const WIDTH: i32 = 420;
const HEIGHT: i32 = 260;
const BACKGROUND: [u8; 3] = [15, 16, 19];

struct Art {
    dpi: Cell<i32>,
    icon: Cell<HICON>,
    text: Cell<Readiness>,
    step: Cell<u32>,
    animate: bool,
}

impl Art {
    fn px(&self, n: i32) -> i32 {
        (n * self.dpi.get() + 48) / 96
    }
}

struct Splash {
    window: HWND,
    art: Box<Art>,
}
impl Splash {
    fn open() -> Option<Self> {
        let class = wide("StarRuptureOverlayStartup");
        let mut monitor = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let mut animate = 0i32;
        // SAFETY: These queries write initialized local values and register only our own callback.
        unsafe {
            if GetMonitorInfoW(
                MonitorFromWindow(GetForegroundWindow(), MONITOR_DEFAULTTOPRIMARY),
                &mut monitor,
            ) == 0
            {
                return None;
            }
            SystemParametersInfoW(
                SPI_GETCLIENTAREAANIMATION,
                0,
                (&mut animate as *mut i32).cast(),
                0,
            );
            let module = GetModuleHandleW(std::ptr::null());
            let wc = WNDCLASSW {
                lpfnWndProc: Some(splash_proc),
                hInstance: module,
                lpszClassName: class.as_ptr(),
                ..Default::default()
            };
            if RegisterClassW(&wc) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
                return None;
            }
            let art = Box::new(Art {
                dpi: Cell::new(96),
                icon: Cell::new(std::ptr::null_mut()),
                text: Cell::new(Readiness::Checking),
                step: Cell::new(0),
                animate: animate != 0,
            });
            let work = monitor.rcWork;
            let x = (work.left + work.right) / 2;
            let y = (work.top + work.bottom) / 2;
            let window = CreateWindowExW(
                crate::stacking::EXTENDED_STYLE,
                class.as_ptr(),
                wide("Starting StarRupture Overlay").as_ptr(),
                WS_POPUP,
                x,
                y,
                1,
                1,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                module,
                (&*art as *const Art).cast(),
            );
            if window.is_null() {
                return None;
            }
            let splash = Self {
                window,
                art,
            };
            splash.art.dpi.set(GetDpiForWindow(window).max(96) as i32);
            let width = splash.art.px(WIDTH);
            let height = splash.art.px(HEIGHT);
            let icon_size = splash.art.px(120);
            splash.art.icon.set(crate::icons::load(icon_size, icon_size).ok()?);
            if SetLayeredWindowAttributes(window, 0, 255, LWA_ALPHA) == 0
                || SetWindowPos(
                    window,
                    HWND_TOPMOST,
                    x - width / 2,
                    y - height / 2,
                    width,
                    height,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW,
                ) == 0
            {
                return None;
            }
            // Paint before starting the minimum display interval. The window stays non-activating.
            UpdateWindow(window);
            Some(splash)
        }
    }

    fn update(&self, readiness: Readiness, elapsed: Duration) {
        let changed = self.art.text.replace(readiness) != readiness;
        let step = if self.art.animate {
            (elapsed.as_millis() / 100 % 12) as u32
        } else {
            0
        };
        if self.art.step.replace(step) != step || changed {
            // SAFETY: Repaint is queued for the retained app-owned splash window.
            unsafe { InvalidateRect(self.window, std::ptr::null(), 0) };
        }
    }
}
impl Drop for Splash {
    fn drop(&mut self) {
        // SAFETY: The owned window is destroyed before its callback state is freed.
        unsafe { DestroyWindow(self.window) };
    }
}

unsafe extern "system" fn splash_proc(window: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    // SAFETY: The creation data and retained Art belong to this owned window. Paint DCs,
    // bitmap and other GDI objects are used synchronously and then released.
    unsafe {
        match message {
            WM_NCCREATE => {
                let create = &*(l as *const CREATESTRUCTW);
                SetWindowLongPtrW(window, GWLP_USERDATA, create.lpCreateParams as isize);
                1
            }
            WM_MOUSEACTIVATE => MA_NOACTIVATE as isize,
            WM_ERASEBKGND => 1,
            WM_PAINT => {
                let mut paint = PAINTSTRUCT::default();
                let dc = BeginPaint(window, &mut paint);
                let art = (GetWindowLongPtrW(window, GWLP_USERDATA) as *const Art).as_ref();
                if let Some(art) = art {
                    let width = art.px(WIDTH);
                    let height = art.px(HEIGHT);
                    let buffer = CreateCompatibleDC(dc);
                    let bitmap = CreateCompatibleBitmap(dc, width, height);
                    if !buffer.is_null() && !bitmap.is_null() {
                        let previous = SelectObject(buffer, bitmap);
                        paint_splash(buffer, art);
                        BitBlt(dc, 0, 0, width, height, buffer, 0, 0, SRCCOPY);
                        SelectObject(buffer, previous);
                    } else {
                        paint_splash(dc, art);
                    }
                    DeleteObject(bitmap);
                    DeleteDC(buffer);
                }
                EndPaint(window, &paint);
                0
            }
            _ => DefWindowProcW(window, message, w, l),
        }
    }
}

/// # Safety
/// The supplied DC is live on the splash's UI thread.
unsafe fn paint_splash(dc: HDC, art: &Art) {
    let px = |value| art.px(value);
    let rect = |x, y, r, b| RECT {
        left: px(x),
        top: px(y),
        right: px(r),
        bottom: px(b),
    };
    let status = art.text.get().splash_status();
    // SAFETY: Brushes, pens and fonts stay live while borrowed by the DC. Selected objects
    // are restored before deletion, and text buffers stay live through each DrawTextW call.
    unsafe {
        let brush = CreateSolidBrush(rgb(BACKGROUND));
        FillRect(dc, &rect(0, 0, WIDTH, HEIGHT), brush);
        DeleteObject(brush);
        let edge = CreateSolidBrush(rgb([63, 68, 73]));
        FrameRect(dc, &rect(0, 0, WIDTH, HEIGHT), edge);
        DeleteObject(edge);
        let icon_size = px(120);
        DrawIconEx(
            dc,
            px(150),
            px(32),
            art.icon.get(),
            icon_size,
            icon_size,
            0,
            std::ptr::null_mut(),
            DI_NORMAL,
        );
        if art.animate && !art.text.get().terminal() {
            let angle = f64::from(art.step.get()) * std::f64::consts::TAU / 12.0;
            let point = |offset: f64| {
                (
                    px(210 + (72.0 * (angle + offset).cos()).round() as i32),
                    px(92 + (72.0 * (angle + offset).sin()).round() as i32),
                )
            };
            let (x1, y1) = point(0.0);
            let (x2, y2) = point(-0.5);
            let colour = if art.step.get() < 6 {
                [215, 145, 53]
            } else {
                [87, 186, 198]
            };
            let pen = CreatePen(PS_SOLID, px(1).max(1), rgb(colour));
            if !pen.is_null() {
                let previous = SelectObject(dc, pen);
                Arc(dc, px(138), px(20), px(282), px(164), x1, y1, x2, y2);
                SelectObject(dc, previous);
                DeleteObject(pen);
            }
        }
        SetBkMode(dc, TRANSPARENT as i32);
        for (value, mut bounds, size, weight, colour) in [
            ("StarRupture Overlay", rect(16, 170, 404, 205), 24, 600, [244, 245, 247]),
            (status, rect(16, 209, 404, 236), 15, 400, [183, 196, 204]),
        ] {
            let family = wide("Segoe UI");
            let font = CreateFontW(
                -px(size),
                0,
                0,
                0,
                weight,
                0,
                0,
                0,
                DEFAULT_CHARSET as u32,
                0,
                0,
                ANTIALIASED_QUALITY as u32,
                0,
                family.as_ptr(),
            );
            if font.is_null() {
                continue;
            }
            let previous = SelectObject(dc, font);
            let value = wide(value);
            SetTextColor(dc, rgb(colour));
            DrawTextW(
                dc,
                value.as_ptr(),
                -1,
                &mut bounds,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
            );
            SelectObject(dc, previous);
            DeleteObject(font);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fast_launch_keeps_a_rendered_splash_readable_without_delaying_a_missing_splash() {
        let fast = Duration::from_millis(20);
        for ready in [Readiness::Waiting, Readiness::Attached] {
            assert!(!ready.startup_complete(fast, true));
            assert!(ready.startup_complete(MIN_SPLASH_TIME, true));
            assert!(ready.startup_complete(fast, false));
        }
        assert!(!Readiness::Connecting.startup_complete(MIN_SPLASH_TIME, true));
        assert!(Readiness::Connecting.startup_complete(STARTUP_LIMIT, true));
    }

    #[test]
    fn connection_status_controls_the_one_time_startup_outcome() {
        use crate::connection::GameStatus;
        let mut connection = Connection::default();
        assert!(!Readiness::from_connection(&connection).terminal());
        connection.update(GameStatus::Waiting);
        assert_eq!(Readiness::from_connection(&connection), Readiness::Waiting);
        connection.update(GameStatus::Running {
            pid: 42,
            creation_ticks: 123,
            executable: "StarRupture.exe".into(),
            supported: false,
        });
        assert_eq!(Readiness::from_connection(&connection), Readiness::Unsupported);
        connection.update(GameStatus::Running {
            pid: 42,
            creation_ticks: 123,
            executable: "StarRupture.exe".into(),
            supported: true,
        });
        assert_eq!(Readiness::from_connection(&connection), Readiness::Connecting);
        connection.ready(42);
        let attached = Readiness::from_connection(&connection);
        assert_eq!(attached, Readiness::Attached);
        assert_eq!(attached.notification(false), ("", "Connected to StarRupture", NIIF_NONE));
        assert_eq!(
            Readiness::Waiting.notification(false),
            ("StarRupture Overlay ready", "Waiting for StarRupture.", NIIF_USER)
        );
        assert_eq!(
            Readiness::Unsupported.notification(false),
            (
                "StarRupture Overlay needs attention",
                "This StarRupture build is unsupported. Open About for compatibility details.",
                NIIF_WARNING
            )
        );
    }
}
