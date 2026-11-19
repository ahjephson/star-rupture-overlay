use super::window::{CYAN, DIM, Drawing, Font, PANEL, outline, rect, wide};
use std::ptr::null_mut;
use windows_sys::Win32::{
    Foundation::*, Graphics::Gdi::*, System::LibraryLoader::GetModuleHandleW,
    UI::WindowsAndMessaging::*,
};

pub(super) struct Banner(HWND);

impl Banner {
    pub fn open(game: HWND) -> Option<Self> {
        let name = wide("StarRuptureOverlayCodexStatus");
        let mut bounds = RECT::default();
        // SAFETY: The class name and rectangle live through synchronous registration and creation.
        unsafe {
            if GetWindowRect(game, &mut bounds) == 0 {
                return None;
            }
            let module = GetModuleHandleW(std::ptr::null());
            let class = WNDCLASSW {
                lpfnWndProc: Some(procedure),
                lpszClassName: name.as_ptr(),
                hInstance: module,
                ..Default::default()
            };
            if RegisterClassW(&class) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
                return None;
            }
            let width = 300;
            let hwnd = CreateWindowExW(
                WS_EX_NOACTIVATE | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                name.as_ptr(),
                wide("Preparing Codex").as_ptr(),
                WS_POPUP,
                bounds.left + (bounds.right - bounds.left - width) / 2,
                bounds.top + 80,
                width,
                60,
                null_mut(),
                null_mut(),
                module,
                std::ptr::null(),
            );
            if hwnd.is_null() {
                return None;
            }
            let path = outline(0, 0, width, 60);
            let region = CreatePolygonRgn(path.as_ptr(), path.len() as i32, WINDING);
            if !region.is_null() && SetWindowRgn(hwnd, region, 0) == 0 {
                DeleteObject(region);
            }
            ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            Some(Self(hwnd))
        }
    }
}

impl Drop for Banner {
    fn drop(&mut self) {
        // SAFETY: This owns an independent HWND with no retained pointer or child controls.
        unsafe { DestroyWindow(self.0) };
    }
}

unsafe extern "system" fn procedure(hwnd: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match message {
        WM_NCHITTEST => HTTRANSPARENT as isize,
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            // SAFETY: Native paint state stays live until EndPaint and is confined to this HWND.
            unsafe {
                let mut paint = PAINTSTRUCT::default();
                let dc = BeginPaint(hwnd, &mut paint);
                let drawing = Drawing::unscaled(dc);
                drawing.polygon(&outline(0, 0, 299, 59), PANEL, DIM);
                drawing.label(
                    &Font::new(17, 600),
                    "Preparing Codex…",
                    rect(14, 23, 281, 46),
                    CYAN,
                    DT_SINGLELINE | DT_CENTER,
                );
                EndPaint(hwnd, &paint);
            }
            0
        }
        // SAFETY: Win32 owns default handling and no Rust state is retained in this window.
        _ => unsafe { DefWindowProcW(hwnd, message, w, l) },
    }
}
