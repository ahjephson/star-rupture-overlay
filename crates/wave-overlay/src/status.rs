//! Shared status artwork for native controls and the informational tray-menu row.
use crate::{connection::Connection, desktop::wide};
use std::mem::size_of;
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    UI::{
        Accessibility::{MSAA_MENU_SIG, MSAAMENUINFO},
        Controls::*,
        HiDpi::*,
        WindowsAndMessaging::*,
    },
};

pub fn colour(rgb: [u8; 3]) -> u32 {
    u32::from(rgb[0]) | u32::from(rgb[1]) << 8 | u32::from(rgb[2]) << 16
}
fn dot_diameter(dc: HDC, dpi: u32) -> i32 {
    let mut metrics = TEXTMETRICW::default();
    if unsafe { GetTextMetricsW(dc, &mut metrics) } != 0 {
        metrics.tmHeight
    } else {
        (14 * dpi as i32 + 48) / 96
    }
}
pub fn draw_dot(dc: HDC, rect: RECT, rgb: [u8; 3], dpi: u32, background: u32) {
    let diameter = dot_diameter(dc, dpi).min(rect.right - rect.left).min(rect.bottom - rect.top);
    let x = rect.left + (rect.right - rect.left - diameter) / 2;
    let y = rect.top + (rect.bottom - rect.top - diameter) / 2;
    if diameter <= 0 {
        return;
    }
    let pixels = circle_pixels(diameter, rgb, background);
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: diameter,
            biHeight: -diameter,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..Default::default()
        },
        ..Default::default()
    };
    unsafe {
        SetDIBitsToDevice(
            dc,
            x,
            y,
            diameter as u32,
            diameter as u32,
            0,
            0,
            0,
            diameter as u32,
            pixels.as_ptr().cast(),
            &info,
            DIB_RGB_COLORS,
        );
    }
}
fn circle_pixels(diameter: i32, foreground: [u8; 3], background: u32) -> Vec<u32> {
    let background = [background as u8, (background >> 8) as u8, (background >> 16) as u8];
    let radius = diameter as f64 / 2.0;
    let mut pixels = Vec::with_capacity((diameter * diameter) as usize);
    for y in 0..diameter {
        for x in 0..diameter {
            // Subpixel coverage smooths the edge while retaining a filled, circular centre.
            let mut covered = 0u32;
            for sy in 0..4 {
                for sx in 0..4 {
                    let dx = x as f64 + (sx as f64 + 0.5) / 4.0 - radius;
                    let dy = y as f64 + (sy as f64 + 0.5) / 4.0 - radius;
                    covered += u32::from(dx * dx + dy * dy <= radius * radius);
                }
            }
            let rgb: [u32; 3] = std::array::from_fn(|channel| {
                (u32::from(foreground[channel]) * covered
                    + u32::from(background[channel]) * (16 - covered)
                    + 8)
                    / 16
            });
            pixels.push(rgb[0] << 16 | rgb[1] << 8 | rgb[2]);
        }
    }
    pixels
}
#[repr(C)]
pub struct MenuHeader {
    // Windows accessibility reads this prefix from owner-drawn menu item data.
    accessibility: MSAAMENUINFO,
    text: Vec<u16>,
    rgb: [u8; 3],
    dpi: u32,
    font: HFONT,
    owned_font: bool,
    width: u32,
    height: u32,
}
impl MenuHeader {
    pub fn new(window: HWND, connection: &Connection, starting: bool) -> Self {
        let dpi = unsafe { GetDpiForWindow(window) }.max(96);
        let mut metrics = NONCLIENTMETRICSW {
            cbSize: size_of::<NONCLIENTMETRICSW>() as u32,
            ..Default::default()
        };
        let font = if unsafe {
            SystemParametersInfoForDpi(
                SPI_GETNONCLIENTMETRICS,
                metrics.cbSize,
                (&mut metrics as *mut NONCLIENTMETRICSW).cast(),
                0,
                dpi,
            )
        } != 0
        {
            unsafe { CreateFontIndirectW(&metrics.lfMenuFont) }
        } else {
            std::ptr::null_mut()
        };
        let owned_font = !font.is_null();
        let font = if owned_font {
            font
        } else {
            unsafe { GetStockObject(DEFAULT_GUI_FONT) }
        };
        let text = wide(if starting {
            "Starting StarRupture…"
        } else {
            connection.label()
        });
        let dc = unsafe { GetDC(window) };
        let mut extent = SIZE::default();
        if !dc.is_null() {
            unsafe {
                let old = SelectObject(dc, font);
                GetTextExtentPoint32W(dc, text.as_ptr(), (text.len() - 1) as i32, &mut extent);
                SelectObject(dc, old);
                ReleaseDC(window, dc);
            }
        }
        Self {
            accessibility: MSAAMENUINFO {
                dwMSAASignature: MSAA_MENU_SIG as u32,
                cchWText: (text.len() - 1) as u32,
                pszWText: text.as_ptr().cast_mut(),
            },
            text,
            rgb: if starting {
                [0xa7, 0x66, 0x04]
            } else {
                connection.indicator_colour()
            },
            dpi,
            font,
            owned_font,
            width: (extent.cx + extent.cy + 36 * dpi as i32 / 96) as u32,
            height: (extent.cy + 16 * dpi as i32 / 96) as u32,
        }
    }
    pub fn append(&self, menu: HMENU) {
        unsafe {
            AppendMenuW(menu, MF_OWNERDRAW | MF_GRAYED, 0, (self as *const Self).cast());
        }
    }
    fn draw(&self, item: &DRAWITEMSTRUCT) {
        let dc = item.hDC;
        let inset = (12 * self.dpi as i32 + 48) / 96;
        let mut rect = item.rcItem;
        unsafe {
            FillRect(dc, &rect, GetSysColorBrush(COLOR_MENU));
            let saved = SaveDC(dc);
            if saved == 0 {
                return;
            }
            SelectObject(dc, self.font);
            let diameter = dot_diameter(dc, self.dpi);
            rect.left += inset;
            let dot = RECT {
                right: rect.left + diameter,
                ..rect
            };
            draw_dot(dc, dot, self.rgb, self.dpi, GetSysColor(COLOR_MENU));
            rect.left += diameter + inset;
            SetBkMode(dc, TRANSPARENT as i32);
            SetTextColor(dc, GetSysColor(COLOR_GRAYTEXT));
            DrawTextW(
                dc,
                self.text.as_ptr(),
                (self.text.len() - 1) as i32,
                &mut rect,
                DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
            );
            RestoreDC(dc, saved);
        }
    }
}
impl Drop for MenuHeader {
    fn drop(&mut self) {
        if self.owned_font {
            unsafe {
                DeleteObject(self.font);
            }
        }
    }
}
/// Called only by the owning HUD window for its own owner-drawn menu row.
///
/// # Safety
/// `l` must point to the native structure for this message. Its item data must
/// reference a live `MenuHeader` until the popup menu has been destroyed.
pub unsafe fn menu_message(message: u32, l: isize) -> Option<isize> {
    match message {
        WM_MEASUREITEM if l != 0 => {
            let item = unsafe { &mut *(l as *mut MEASUREITEMSTRUCT) };
            if item.CtlType != ODT_MENU || item.itemID != 0 || item.itemData == 0 {
                return None;
            }
            let header = unsafe { &*(item.itemData as *const MenuHeader) };
            item.itemWidth = header.width;
            item.itemHeight = header.height;
            Some(1)
        }
        WM_DRAWITEM if l != 0 => {
            let item = unsafe { &*(l as *const DRAWITEMSTRUCT) };
            if item.CtlType != ODT_MENU || item.itemID != 0 || item.itemData == 0 {
                return None;
            }
            unsafe { &*(item.itemData as *const MenuHeader) }.draw(item);
            Some(1)
        }
        _ => None,
    }
}

/// Verifies native menu ownership, sizing and coloured pixels without opening a popup.
pub fn self_test(window: HWND) -> anyhow::Result<()> {
    use anyhow::ensure;
    struct Popup(HMENU);
    impl Drop for Popup {
        fn drop(&mut self) {
            unsafe {
                DestroyMenu(self.0);
            }
        }
    }
    struct Surface {
        dc: HDC,
        bitmap: HBITMAP,
        old: HGDIOBJ,
    }
    impl Drop for Surface {
        fn drop(&mut self) {
            unsafe {
                if !self.old.is_null() {
                    SelectObject(self.dc, self.old);
                }
                if !self.bitmap.is_null() {
                    DeleteObject(self.bitmap);
                }
                DeleteDC(self.dc);
            }
        }
    }
    let mut connected = Connection::default();
    connected.update(crate::connection::GameStatus::Running {
        pid: 123,
        creation_ticks: 456,
        executable: "Game.exe".into(),
        supported: true,
    });
    connected.ready(123);
    let mut waiting = Connection::default();
    waiting.update(crate::connection::GameStatus::Waiting);
    let mut unsupported = Connection::default();
    unsupported.update(crate::connection::GameStatus::Running {
        pid: 123,
        creation_ticks: 456,
        executable: "Game.exe".into(),
        supported: false,
    });
    for (connection, starting, expected) in [
        (&connected, false, [0x16, 0x83, 0x4a]),
        (&waiting, false, [0x85, 0x8e, 0x9a]),
        (&unsupported, false, [0xa7, 0x66, 0x04]),
        (&waiting, true, [0xa7, 0x66, 0x04]),
    ] {
        let header = Box::new(MenuHeader::new(window, connection, starting));
        let menu = Popup(unsafe { CreatePopupMenu() });
        ensure!(!menu.0.is_null(), "No menu fixture");
        header.append(menu.0);
        let mut info = MENUITEMINFOW {
            cbSize: size_of::<MENUITEMINFOW>() as u32,
            fMask: MIIM_DATA | MIIM_FTYPE,
            ..Default::default()
        };
        ensure!(
            unsafe { GetMenuItemInfoW(menu.0, 0, 1, &mut info) } != 0,
            "Menu status row missing"
        );
        ensure!(info.fType & MFT_OWNERDRAW != 0, "Status row lost custom drawing");
        let accessible = unsafe { &*(info.dwItemData as *const MSAAMENUINFO) };
        ensure!(
            accessible.dwMSAASignature == MSAA_MENU_SIG as u32,
            "Menu status accessibility metadata missing"
        );
        ensure!(
            unsafe {
                std::slice::from_raw_parts(accessible.pszWText, accessible.cchWText as usize)
            } == &header.text[..header.text.len() - 1],
            "Accessible menu status does not match the visible label"
        );
        let mut measure = MEASUREITEMSTRUCT {
            CtlType: ODT_MENU,
            itemData: info.dwItemData,
            ..Default::default()
        };
        ensure!(
            unsafe {
                menu_message(WM_MEASUREITEM, (&mut measure as *mut MEASUREITEMSTRUCT) as isize)
            } == Some(1),
            "Menu status measurement failed"
        );
        ensure!(measure.itemWidth > 32 && measure.itemHeight > 8, "Menu status row is too small");
        let source = unsafe { GetDC(window) };
        let dc = unsafe { CreateCompatibleDC(source) };
        let bitmap = unsafe {
            CreateCompatibleBitmap(source, measure.itemWidth as i32, measure.itemHeight as i32)
        };
        unsafe {
            ReleaseDC(window, source);
        }
        let mut surface = Surface {
            dc,
            bitmap,
            old: std::ptr::null_mut(),
        };
        ensure!(!dc.is_null() && !bitmap.is_null(), "No menu status drawing surface");
        surface.old = unsafe { SelectObject(dc, bitmap) };
        let item = DRAWITEMSTRUCT {
            CtlType: ODT_MENU,
            itemData: info.dwItemData,
            hDC: dc,
            rcItem: RECT {
                left: 0,
                top: 0,
                right: measure.itemWidth as i32,
                bottom: measure.itemHeight as i32,
            },
            ..Default::default()
        };
        ensure!(
            unsafe { menu_message(WM_DRAWITEM, (&item as *const DRAWITEMSTRUCT) as isize) }
                == Some(1),
            "Menu status drawing failed"
        );
        let diameter = measure.itemHeight as i32 - 16 * header.dpi as i32 / 96;
        let x = (12 * header.dpi as i32 + 48) / 96 + diameter / 2;
        ensure!(
            unsafe { GetPixel(dc, x, measure.itemHeight as i32 / 2) } == colour(expected),
            "Menu status dot has the wrong colour"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn circle_has_symmetric_smooth_edges_and_a_solid_centre_at_multiple_dpis() {
        for diameter in [8, 12, 15, 16, 24] {
            let pixels = circle_pixels(diameter, [0x16, 0x83, 0x4a], 0xffffff);
            let side = diameter as usize;
            assert_eq!(pixels[side / 2 * side + side / 2], 0x16834a);
            assert_eq!(pixels[0], 0xffffff);
            assert!(pixels.iter().any(|pixel| *pixel != 0xffffff && *pixel != 0x16834a));
            for y in 0..side {
                for x in 0..side {
                    assert_eq!(pixels[y * side + x], pixels[y * side + side - 1 - x]);
                    assert_eq!(pixels[y * side + x], pixels[(side - 1 - y) * side + x]);
                }
            }
        }
    }
}
