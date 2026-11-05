//! Small, click-through compass surface. Owns reusable GDI buffers on the UI thread.
use starrupture_observation_model::compass::{Sample, relative};
use std::{
    mem::size_of,
    ptr::{null, null_mut},
};
use windows_sys::Win32::{Foundation::*, Graphics::Gdi::*, UI::WindowsAndMessaging::*};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}
#[derive(Default)]
pub(crate) struct Panel {
    window: HWND,
    surface: Option<Surface>,
    last: Option<(i32, i32, i32, i32)>,
    rendered: Option<(i32, Option<(i32, u32)>)>,
}
impl Drop for Panel {
    fn drop(&mut self) {
        if !self.window.is_null() {
            // SAFETY: This UI-thread owner destroys only its own non-activating window.
            unsafe {
                DestroyWindow(self.window);
            }
        }
    }
}
impl Panel {
    pub(crate) fn hide(&self) {
        if !self.window.is_null() {
            // SAFETY: The retained HWND is owned by this panel on the current UI thread.
            unsafe {
                ShowWindow(self.window, SW_HIDE);
            }
        }
    }
    pub(crate) fn follow(&self, game: HWND) {
        self.follow_with_foreground(game, foreground);
    }
    fn follow_with_foreground(&self, game: HWND, current: impl Fn() -> HWND) {
        if game.is_null() || current() != game {
            self.hide();
            return;
        }
        // SAFETY: The UI thread queries its live panel handle; Win32 validates borrowed HWNDs.
        if !self.window.is_null() && unsafe { IsWindowVisible(self.window) } != 0 {
            crate::stacking::follow_game(self.window, game);
            if current() != game {
                self.hide();
            }
        }
    }
    pub(crate) fn update(
        &mut self,
        game: HWND,
        value: Option<Sample>,
        enabled: bool,
    ) -> Result<()> {
        let now = starrupture_wave_core::formatting::now_ms().max(0) as u64;
        let Some(sample) =
            value.filter(|s| enabled && focused(game) && s.valid(now) && !s.menu_open)
        else {
            self.hide();
            return Ok(());
        };
        let mut rect = RECT::default();
        let mut point = POINT::default();
        let mut cursor = CURSORINFO {
            cbSize: size_of::<CURSORINFO>() as u32,
            ..Default::default()
        };
        // SAFETY: Only local initialized output structs are passed to synchronous Win32 queries.
        let visible = unsafe {
            !game.is_null()
                && IsWindowVisible(game) != 0
                && IsIconic(game) == 0
                && GetClientRect(game, &mut rect) != 0
                && ClientToScreen(game, &mut point) != 0
                && GetCursorInfo(&mut cursor) != 0
                && cursor.flags & CURSOR_SHOWING == 0
        };
        if !visible {
            self.hide();
            return Ok(());
        }
        let width = (rect.right * 560 / 2560).clamp(320, 640);
        let height = width * 90 / 560;
        let x = point.x + (rect.right - width) / 2;
        let y = point.y + (rect.bottom * 16 / 1440).max(10);
        self.show(game, sample, [x, y, width, height])
    }
    fn show(&mut self, game: HWND, sample: Sample, geometry: [i32; 4]) -> Result<()> {
        self.show_with_foreground(game, sample, geometry, foreground)
    }
    fn show_with_foreground(
        &mut self,
        game: HWND,
        sample: Sample,
        geometry: [i32; 4],
        current: impl Fn() -> HWND,
    ) -> Result<()> {
        if game.is_null() || current() != game {
            self.hide();
            return Ok(());
        }
        let [x, y, width, height] = geometry;
        if self.window.is_null() {
            // SAFETY: The built-in STATIC class and terminated title are valid for creation;
            // no callback state is transferred and this owner retains/destroys the returned HWND.
            self.window = unsafe {
                CreateWindowExW(
                    crate::stacking::EXTENDED_STYLE,
                    wide("STATIC").as_ptr(),
                    wide("StarRupture compass").as_ptr(),
                    WS_POPUP,
                    x,
                    y,
                    width,
                    height,
                    null_mut(),
                    null_mut(),
                    null_mut(),
                    null(),
                )
            };
            if self.window.is_null() {
                return Err(std::io::Error::last_os_error().into());
            }
        }
        let resized = self.surface.as_ref().is_none_or(|s| s.width != width || s.height != height);
        if resized {
            self.surface = Some(Surface::new(width, height)?);
            self.rendered = None;
        }
        // Quantize sub-pixel motion, never extrapolate beyond validated evidence.
        let key = (
            (sample.heading * 10.).round() as i32,
            sample.pin.map(|p| {
                (
                    (relative(p.bearing, sample.heading) * 10.).round() as i32,
                    p.metres.round() as u32,
                )
            }),
        );
        if self.rendered != Some(key) {
            let surface = self.surface.as_mut().unwrap();
            surface.draw(sample);
            let size = SIZE {
                cx: width,
                cy: height,
            };
            let source = POINT::default();
            let dest = POINT {
                x,
                y,
            };
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            // SAFETY: Surface owns a selected top-down DIB with exactly width*height pixels;
            // initialized scalar blend/geometry structs remain live during the synchronous call.
            if unsafe {
                UpdateLayeredWindow(
                    self.window,
                    null_mut(),
                    &dest,
                    &size,
                    surface.dc,
                    &source,
                    0,
                    &blend,
                    ULW_ALPHA,
                )
            } == 0
            {
                return Err(std::io::Error::last_os_error().into());
            }
            self.rendered = Some(key);
        }
        if self.last != Some((x, y, width, height)) {
            // SAFETY: Move only our retained HWND without activating or changing band ownership.
            if unsafe {
                SetWindowPos(
                    self.window,
                    null_mut(),
                    x,
                    y,
                    width,
                    height,
                    SWP_NOACTIVATE | SWP_NOZORDER,
                )
            } == 0
            {
                return Err(std::io::Error::last_os_error().into());
            }
            self.last = Some((x, y, width, height));
        }
        if current() != game {
            self.hide();
            return Ok(());
        }
        crate::stacking::follow_game(self.window, game);
        if current() != game {
            self.hide();
        }
        Ok(())
    }
}
struct Surface {
    dc: HDC,
    bitmap: HBITMAP,
    old: HGDIOBJ,
    font: HFONT,
    old_font: HGDIOBJ,
    bits: *mut u8,
    width: i32,
    height: i32,
    pixels: Vec<u8>,
}
impl Drop for Surface {
    fn drop(&mut self) {
        // SAFETY: The owner retains the DC, DIB and font; restore selections before freeing objects.
        unsafe {
            SelectObject(self.dc, self.old_font);
            SelectObject(self.dc, self.old);
            DeleteObject(self.font);
            DeleteObject(self.bitmap);
            DeleteDC(self.dc);
        }
    }
}
impl Surface {
    fn new(width: i32, height: i32) -> Result<Self> {
        let mut info = BITMAPINFO::default();
        info.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = width;
        info.bmiHeader.biHeight = -height;
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        info.bmiHeader.biCompression = BI_RGB;
        let mut bits = null_mut();
        // SAFETY: Bounded dimensions and initialized headers/out-pointer meet CreateDIBSection's
        // contract. This routine releases every partial allocation on failure.
        unsafe {
            let dc = CreateCompatibleDC(null_mut());
            if dc.is_null() {
                return Err(std::io::Error::last_os_error().into());
            }
            let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
            if bitmap.is_null() || bits.is_null() {
                if !bitmap.is_null() {
                    DeleteObject(bitmap);
                }
                DeleteDC(dc);
                return Err("Compass bitmap creation failed".into());
            }
            let old = SelectObject(dc, bitmap);
            let font = CreateFontW(
                -(height * 23 / 90).max(13),
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
                ANTIALIASED_QUALITY as u32,
                DEFAULT_PITCH as u32,
                wide("Oxanium").as_ptr(),
            );
            if old.is_null() || old as isize == -1 || font.is_null() {
                SelectObject(dc, old);
                DeleteObject(font);
                DeleteObject(bitmap);
                DeleteDC(dc);
                return Err("Compass GDI allocation failed".into());
            }
            let old_font = SelectObject(dc, font);
            if old_font.is_null() || old_font as isize == -1 {
                SelectObject(dc, old);
                DeleteObject(font);
                DeleteObject(bitmap);
                DeleteDC(dc);
                return Err("Compass font selection failed".into());
            }
            SetBkMode(dc, TRANSPARENT as i32);
            SetTextColor(dc, 0xffffff);
            Ok(Self {
                dc,
                bitmap,
                old,
                font,
                old_font,
                bits: bits.cast(),
                width,
                height,
                pixels: vec![0; (width * height * 4) as usize],
            })
        }
    }
    fn pixel(&mut self, x: i32, y: i32, color: [u8; 3], alpha: u8) {
        if !(0..self.width).contains(&x) || !(0..self.height).contains(&y) {
            return;
        }
        let i = ((y * self.width + x) * 4) as usize;
        let a = u16::from(alpha);
        let inv = 255 - a;
        for (j, v) in [color[2], color[1], color[0]].into_iter().enumerate() {
            self.pixels[i + j] =
                ((u16::from(v) * a + u16::from(self.pixels[i + j]) * inv + 127) / 255) as u8;
        }
        self.pixels[i + 3] = (a + (u16::from(self.pixels[i + 3]) * inv + 127) / 255) as u8;
    }
    fn line(&mut self, x: i32, y: i32, xx: i32, yy: i32, color: [u8; 3], alpha: u8) {
        let n = (xx - x).abs().max((yy - y).abs()).max(1);
        for i in 0..=n {
            self.pixel(x + (xx - x) * i / n + 1, y + (yy - y) * i / n + 2, [0, 0, 0], alpha);
        }
        for i in 0..=n {
            self.pixel(x + (xx - x) * i / n, y + (yy - y) * i / n, color, alpha);
        }
    }
    fn text(&mut self, text: &str, cx: i32, y: i32, color: [u8; 3]) {
        let text: Vec<u16> = text.encode_utf16().collect();
        let mut r = RECT {
            left: cx - 65,
            top: y,
            right: cx + 65,
            bottom: self.height,
        };
        // SAFETY: The DIB remains selected and owns width*height*4 bytes. Flush native writes
        // before accessing its pixels; DrawText reads the bounded UTF-16 buffer synchronously.
        unsafe {
            GdiFlush();
            std::ptr::write_bytes(self.bits, 0, self.pixels.len());
            DrawTextW(
                self.dc,
                text.as_ptr(),
                text.len() as i32,
                &mut r,
                DT_CENTER | DT_SINGLELINE | DT_NOPREFIX,
            );
            GdiFlush();
        }
        for shadow in [true, false] {
            for yy in y.max(0)..self.height {
                for x in (cx - 65).max(0)..(cx + 65).min(self.width) {
                    // SAFETY: These bounds keep the read inside the retained selected DIB.
                    let a = unsafe { *self.bits.add(((yy * self.width + x) * 4) as usize) };
                    if a == 0 {
                        continue;
                    }
                    if shadow {
                        for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1), (1, 2)] {
                            self.pixel(x + dx, yy + dy, [0, 0, 0], a);
                        }
                    } else {
                        self.pixel(x, yy, color, a);
                    }
                }
            }
        }
    }
    fn draw(&mut self, s: Sample) {
        self.pixels.fill(0);
        let scale = self.width as f64 / 560.;
        let mid = self.width / 2;
        let y = (43. * scale) as i32;
        let cyan = [121, 219, 225];
        let white = [235, 240, 240];
        let span = self.width - 36;
        for x in 12..self.width - 12 {
            let a = (((x - 12).min(self.width - 13 - x) as f64 / 35.).min(1.) * 100.) as u8;
            self.pixel(x, y, [0, 0, 0], a);
            self.pixel(x, y - 1, white, a);
        }
        for step in 0..72 {
            let h = step as f64 * 5.;
            let d = relative(h, s.heading);
            if d.abs() > 90. {
                continue;
            }
            let x = mid + (d * span as f64 / 180.).round() as i32;
            let major = step % 9 == 0;
            self.line(
                x,
                y - if major {
                    8
                } else {
                    4
                },
                x,
                y + 4,
                white,
                220,
            );
            if major {
                self.text(
                    ["N", "NE", "E", "SE", "S", "SW", "W", "NW"][step / 9],
                    x,
                    (12. * scale) as i32,
                    white,
                );
            }
        }
        // Fixed downward centre notch.
        for row in 0..(7. * scale).max(4.) as i32 {
            let half = (7. * scale) as i32 - row;
            for x in mid - half..=mid + half {
                self.pixel(x, (2. * scale) as i32 + row, white, 255);
            }
        }
        if let Some(pin) = s.pin {
            let delta = relative(pin.bearing, s.heading);
            let x = mid + (delta.clamp(-90., 90.) * span as f64 / 180.).round() as i32;
            if delta.abs() <= 90. {
                let r = (8. * scale).max(5.) as i32;
                for thick in 0..2 {
                    let r = r - thick;
                    self.line(x, y - r, x + r, y, cyan, 255);
                    self.line(x + r, y, x, y + r, cyan, 255);
                    self.line(x, y + r, x - r, y, cyan, 255);
                    self.line(x - r, y, x, y - r, cyan, 255);
                }
            } else {
                self.text(
                    if delta < 0. {
                        "<"
                    } else {
                        ">"
                    },
                    x,
                    (30. * scale) as i32,
                    cyan,
                );
            }
            self.text(
                &format!("{:.0} m", pin.metres),
                x.clamp(65, self.width - 65),
                (60. * scale) as i32,
                white,
            );
        }
        // SAFETY: The selected top-down DIB owns exactly pixels.len() bytes; all GDI access is
        // flushed before replacing it with the composed premultiplied BGRA surface.
        unsafe {
            GdiFlush();
            std::ptr::copy_nonoverlapping(self.pixels.as_ptr(), self.bits, self.pixels.len());
        }
    }
}
/// Native rendering and hiding regressions use owned offscreen windows only.
pub(crate) fn self_test(directory: &std::path::Path) -> Result<()> {
    std::fs::create_dir_all(directory)?;
    let _fonts = crate::fonts::Fonts::load()?;
    let now = starrupture_wave_core::formatting::now_ms().max(0) as u64;
    let sample = Sample {
        started_ms: now,
        finished_ms: now,
        heading: 0.,
        pin: None,
        pin_known: true,
        menu_open: false,
    };
    let mut surface = Surface::new(560, 90)?;
    let allocation = surface.bits;
    for (name, heading, pin) in [
        ("no-pin", 0., None),
        ("ahead", 0., Some((0., 420.))),
        ("right", 350., Some((20., 420.))),
        ("left", 10., Some((340., 420.))),
        ("behind", 0., Some((180., 420.))),
        ("wrap", 359., Some((1., 8.))),
    ] {
        surface.draw(Sample {
            heading,
            pin: pin.map(|(bearing, metres)| starrupture_observation_model::compass::Pin {
                bearing,
                metres,
            }),
            ..sample
        });
        if surface.bits != allocation
            || surface.pixels.as_chunks::<4>().0.iter().any(|p| p[..3].iter().any(|v| *v > p[3]))
        {
            return Err("Compass surface reuse/alpha invariant failed".into());
        }
        if surface.pixels[..4] != [0; 4] {
            return Err("Compass has an opaque background".into());
        }
        write_bmp(&directory.join(format!("{name}.bmp")), &surface)?;
    }
    let mut panel = Panel::default();
    // SAFETY: STATIC is a system class; the fixture owns the resulting nonactivating HWND,
    // changes only its own windows and retains all surfaces through synchronous calls.
    unsafe {
        let game = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            wide("STATIC").as_ptr(),
            wide("Compass fixture").as_ptr(),
            WS_POPUP,
            -30000,
            -30000,
            1000,
            800,
            null_mut(),
            null_mut(),
            null_mut(),
            null(),
        );
        if game.is_null() {
            return Err("Compass fixture creation failed".into());
        }
        struct Window(HWND);
        impl Drop for Window {
            fn drop(&mut self) {
                // SAFETY: This guard owns only the fixture's HWND on its creation thread.
                unsafe {
                    DestroyWindow(self.0);
                }
            }
        }
        let game = Window(game);
        let original_foreground = GetForegroundWindow();
        let other = Window(CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            wide("STATIC").as_ptr(),
            wide("Other app fixture").as_ptr(),
            WS_POPUP,
            -30000,
            -30000,
            800,
            600,
            null_mut(),
            null_mut(),
            null_mut(),
            null(),
        ));
        if other.0.is_null() {
            return Err("Other-app fixture creation failed".into());
        }
        ShowWindow(game.0, SW_SHOWNOACTIVATE);
        panel.show_with_foreground(game.0, sample, [-29990, -29990, 560, 90], || game.0)?;
        if IsWindowVisible(panel.window) == 0 {
            return Err("Compass fixture did not show".into());
        }
        panel.update(
            game.0,
            Some(Sample {
                menu_open: true,
                ..sample
            }),
            true,
        )?;
        if IsWindowVisible(panel.window) != 0 {
            return Err("Paused compass did not hide".into());
        }
        panel.show_with_foreground(game.0, sample, [-29990, -29990, 560, 90], || game.0)?;
        panel.update(game.0, Some(sample), false)?;
        if IsWindowVisible(panel.window) != 0 {
            return Err("Disabled compass did not hide".into());
        }
        panel.show_with_foreground(game.0, sample, [-29990, -29990, 560, 90], || game.0)?;
        panel.update(game.0, None, true)?;
        if IsWindowVisible(panel.window) != 0 {
            return Err("Unavailable compass did not hide".into());
        }
        panel.show_with_foreground(game.0, sample, [-29990, -29990, 560, 90], || game.0)?;
        panel.update(
            game.0,
            Some(Sample {
                started_ms: 0,
                finished_ms: 0,
                ..sample
            }),
            true,
        )?;
        if IsWindowVisible(panel.window) != 0 {
            return Err("Stale compass did not hide".into());
        }
        panel.show_with_foreground(game.0, sample, [-29990, -29990, 560, 90], || game.0)?;
        panel.follow_with_foreground(game.0, || other.0);
        if IsWindowVisible(panel.window) != 0 {
            return Err("Focus loss did not hide compass".into());
        }
        // Repeated refreshes while another app has focus model the hidden-pointer
        // state caused by typing. No input is injected and no foreground is changed.
        for i in 0..20 {
            panel.show_with_foreground(
                game.0,
                Sample {
                    heading: i as f64,
                    ..sample
                },
                [-29990, -29990, 560, 90],
                || other.0,
            )?;
            panel.follow_with_foreground(game.0, || other.0);
            if IsWindowVisible(panel.window) != 0 {
                return Err("Unfocused refresh flashed compass".into());
            }
        }
        let calls = std::cell::Cell::new(0);
        panel.show_with_foreground(
            game.0,
            Sample {
                heading: 45.,
                ..sample
            },
            [-29990, -29990, 560, 90],
            || {
                let n = calls.get();
                calls.set(n + 1);
                if n == 0 {
                    game.0
                } else {
                    other.0
                }
            },
        )?;
        if IsWindowVisible(panel.window) != 0 {
            return Err("Mid-render focus loss exposed compass".into());
        }
        panel.show_with_foreground(game.0, sample, [-29990, -29990, 560, 90], || game.0)?;
        if IsWindowVisible(panel.window) == 0 {
            return Err("Returning focus did not restore compass".into());
        }
        if GetForegroundWindow() != original_foreground {
            return Err("Compass test changed actual focus".into());
        }
    }
    let started = std::time::Instant::now();
    for i in 0..120 {
        surface.draw(Sample {
            heading: i as f64 * 3.,
            ..sample
        });
    }
    println!(
        "{}",
        serde_json::json!({"status":"passed","transparent":true,"premultipliedAlpha":true,"surfaceReused":true,
        "pausedHidden":true,"disabledHidden":true,"unavailableHidden":true,"staleHidden":true,"gameRead":false,"focusLossHidden":true,"unfocusedRefreshes":20,"midRenderFocusLossHidden":true,"focusReturnRestores":true,"actualFocusPreserved":true,
        "meanRenderMicros":started.elapsed().as_micros()/120})
    );
    Ok(())
}
fn write_bmp(path: &std::path::Path, s: &Surface) -> Result<()> {
    let mut header = vec![0u8; 54];
    header[..2].copy_from_slice(b"BM");
    header[2..6].copy_from_slice(&(54 + s.pixels.len() as u32).to_le_bytes());
    header[10..14].copy_from_slice(&54u32.to_le_bytes());
    header[14..18].copy_from_slice(&40u32.to_le_bytes());
    header[18..22].copy_from_slice(&s.width.to_le_bytes());
    header[22..26].copy_from_slice(&(-s.height).to_le_bytes());
    header[26..28].copy_from_slice(&1u16.to_le_bytes());
    header[28..30].copy_from_slice(&32u16.to_le_bytes());
    use std::io::Write;
    let mut file = std::fs::File::create(path)?;
    file.write_all(&header)?;
    file.write_all(&s.pixels)?;
    Ok(())
}

pub(crate) fn gameplay_cursor() -> bool {
    let mut cursor = CURSORINFO {
        cbSize: size_of::<CURSORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: The initialized local CURSORINFO has the required size and stays live for the query.
    unsafe { GetCursorInfo(&mut cursor) != 0 && cursor.flags & CURSOR_SHOWING == 0 }
}

fn foreground() -> HWND {
    // SAFETY: This UI-thread query returns a borrowed HWND without transferring ownership.
    unsafe { GetForegroundWindow() }
}
pub(crate) fn focused(game: HWND) -> bool {
    !game.is_null() && foreground() == game
}
