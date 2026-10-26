//! HUD geometry, alpha composition and GDI drawing on the UI thread.
use super::{rgb, wide};
use starrupture_observation_model::wave::HudFrame;
use std::mem::size_of;
use windows_sys::Win32::{
    Foundation::{HWND, POINT, RECT, SIZE},
    Graphics::Gdi::*,
    UI::WindowsAndMessaging::{GetClientRect, ULW_ALPHA, UpdateLayeredWindow},
};

pub(super) const STAGE_TOP_PADDING: i32 = 4;
const CONTENT_OFFSET_Y: i32 = 2;
pub(super) const BACKGROUND_ALPHA: u8 = 230;
const PANEL_EDGE: [u8; 3] = [220, 222, 218];
// Game-style panel: raised left tab, stepped top, one clipped corner.
pub(super) fn panel_points(width: i32, height: i32, inset: i32) -> [POINT; 7] {
    let cut = (height / 8).max(5);
    let tab = width * 26 / 100;
    let right = width - 1 - inset;
    let bottom = height - 1 - inset;
    [
        POINT {
            x: inset,
            y: inset,
        },
        POINT {
            x: tab,
            y: inset,
        },
        POINT {
            x: tab + cut,
            y: inset + cut,
        },
        POINT {
            x: right,
            y: inset + cut,
        },
        POINT {
            x: right,
            y: bottom - cut,
        },
        POINT {
            x: right - cut,
            y: bottom,
        },
        POINT {
            x: inset,
            y: bottom,
        },
    ]
}
fn panel_contains(x: usize, y: usize, width: usize, height: usize) -> bool {
    let cut = (height / 8).max(5);
    let tab = width * 26 / 100;
    x < width
        && y < height
        && (x <= tab || y >= (x - tab).min(cut))
        && x + y <= width.saturating_sub(1) + height.saturating_sub(1) - cut
}
fn layer_alpha(pixels: &mut [u8], width: usize, height: usize) {
    for (i, pixel) in pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let x = i % width;
        let y = i / width;
        if !panel_contains(x, y, width, height) {
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
pub(super) unsafe fn render_layer(window: HWND, screen: HDC, frame: &HudFrame) -> bool {
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
    let bitmap =
        unsafe { CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0) };
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
    let source = POINT {
        x: 0,
        y: 0,
    };
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
pub(super) unsafe fn draw(dc: HDC, rect: RECT, frame: &HudFrame) {
    let width = rect.right;
    let panel_height = rect.bottom;
    let content_y = STAGE_TOP_PADDING + CONTENT_OFFSET_Y;
    // Extra headroom must not enlarge fonts or spread the existing content.
    let height = (panel_height - STAGE_TOP_PADDING).max(1);
    let background = unsafe { CreateSolidBrush(rgb([24, 30, 32])) };
    unsafe {
        FillRect(dc, &rect, background);
        DeleteObject(background)
    };
    let edge = unsafe { CreatePen(PS_SOLID, 1, rgb(PANEL_EDGE)) };
    let old_pen = unsafe { SelectObject(dc, edge) };
    let old_brush = unsafe { SelectObject(dc, GetStockObject(NULL_BRUSH)) };
    let c = (height / 8).max(5);
    let points = panel_points(width, panel_height, 1);
    unsafe {
        Polygon(dc, points.as_ptr(), points.len() as i32);
        MoveToEx(dc, c + 3, height / 2 - 2 + content_y, std::ptr::null_mut());
        LineTo(dc, width - c - 3, height / 2 - 2 + content_y);
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
        top: height / 9 + content_y,
        right: width - c,
        bottom: height / 2 + content_y,
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
        top: height / 2 + content_y,
        right: width / 2 + width / 10,
        bottom: height - c + content_y,
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
        top: height / 2 - 2 + content_y,
        right: width - c - 3,
        bottom: height - c + content_y,
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn game_panel_mask_keeps_square_corners_step_and_translucent_background() {
        let mut pixels = [32u8, 30, 24, 0].repeat(20 * 20);
        let text = (10 * 20 + 10) * 4;
        pixels[text..text + 4].copy_from_slice(&[248, 247, 245, 0]);
        layer_alpha(&mut pixels, 20, 20);
        // Upper-left and lower-left corners remain square; the top step
        // and lower-right cut are transparent outside the frame.
        assert_eq!(&pixels[..4], &[29, 27, 22, 230]);
        assert_eq!(&pixels[19 * 4..20 * 4], &[0, 0, 0, 0]);
        assert_eq!(&pixels[399 * 4..400 * 4], &[0, 0, 0, 0]);
        assert!(panel_contains(0, 19, 20, 20));
        assert!(panel_contains(19, 5, 20, 20));
        assert!(!panel_contains(19, 4, 20, 20));
        assert_eq!(&pixels[text..text + 4], &[248, 247, 245, 255]);
        let background = (10 * 20 + 8) * 4;
        assert_eq!(&pixels[background..background + 4], &[29, 27, 22, 230]);
        assert!(pixels.as_chunks::<4>().0.iter().all(|p| p[..3].iter().all(|c| *c <= p[3])));
    }
}
