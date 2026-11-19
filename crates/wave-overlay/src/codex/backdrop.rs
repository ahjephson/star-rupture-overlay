//! Place a system-drawn backdrop over the game and intercept outside clicks.
#[path = "backdrop/frost.rs"]
mod frost;
use crate::{desktop::wide, recipe_tracker::cursor::GameCursor};
use std::{
    cell::{Cell, RefCell},
    ptr::null_mut,
    rc::Rc,
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::{LibraryLoader::GetModuleHandleW, Registry::*},
    UI::{
        Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW},
        WindowsAndMessaging::*,
    },
};

pub(super) type Cursor = Rc<RefCell<GameCursor>>;

struct Input {
    cursor: Cursor,
    codex: Cell<HWND>,
}

pub(super) struct Backdrop {
    pub hwnd: HWND,
    bounds: [i32; 4],
    input: Box<Input>,
    frost: Option<frost::Frost>,
}

fn client_bounds(game: HWND) -> Option<[i32; 4]> {
    let mut bounds = RECT::default();
    let mut origin = POINT::default();
    // SAFETY: Both queries write to initialized stack storage for the duration of the call.
    unsafe {
        if IsIconic(game) != 0
            || GetClientRect(game, &mut bounds) == 0
            || ClientToScreen(game, &mut origin) == 0
            || bounds.right <= 0
            || bounds.bottom <= 0
        {
            return None;
        }
    }
    Some([origin.x, origin.y, bounds.right, bounds.bottom])
}

impl Backdrop {
    pub fn open(game: HWND, cursor: Cursor) -> Option<Self> {
        let bounds = client_bounds(game)?;
        let [x, y, width, height] = bounds;
        let input = Box::new(Input {
            cursor,
            codex: Cell::new(null_mut()),
        });
        // SAFETY: Only own windows are created or moved. The cursor reference stays boxed until
        // after the backdrop window is destroyed.
        unsafe {
            let name = wide("StarRuptureCodexBackdrop");
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
            for layered in [false, true] {
                if !layered && !effects_allowed() {
                    continue;
                }
                let ex_style = WS_EX_TOOLWINDOW
                    | WS_EX_NOACTIVATE
                    | if layered {
                        WS_EX_LAYERED
                    } else {
                        WS_EX_NOREDIRECTIONBITMAP
                    };
                let hwnd = CreateWindowExW(
                    ex_style,
                    name.as_ptr(),
                    wide("Codex background").as_ptr(),
                    WS_POPUP,
                    x,
                    y,
                    width,
                    height,
                    null_mut(),
                    null_mut(),
                    module,
                    (&*input as *const Input).cast(),
                );
                if hwnd.is_null() {
                    continue;
                }
                let frost = (!layered).then(|| frost::Frost::open(hwnd).ok()).flatten();
                let configured = if layered {
                    SetLayeredWindowAttributes(hwnd, 0, 90, LWA_ALPHA) != 0
                } else {
                    frost.is_some()
                };
                if configured && crate::stacking::follow_game(hwnd, game) {
                    return Some(Self {
                        hwnd,
                        bounds,
                        input,
                        frost,
                    });
                }
                drop(frost);
                DestroyWindow(hwnd);
            }
            None
        }
    }

    pub fn bind_codex(&self, window: HWND) {
        self.input.codex.set(window);
    }

    pub fn resize(&mut self, game: HWND) {
        if let Some(bounds) = client_bounds(game)
            && bounds != self.bounds
        {
            let [x, y, width, height] = bounds;
            // SAFETY: Position only the owned shield without changing focus or stacking.
            if unsafe {
                SetWindowPos(
                    self.hwnd,
                    null_mut(),
                    x,
                    y,
                    width,
                    height,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                )
            } != 0
            {
                self.bounds = bounds;
            }
        }
    }
}

fn effects_allowed() -> bool {
    let mut high_contrast = HIGHCONTRASTW {
        cbSize: size_of::<HIGHCONTRASTW>() as u32,
        ..Default::default()
    };
    let mut transparency = 1u32;
    let mut bytes = size_of::<u32>() as u32;
    // SAFETY: Only query Windows preferences. The strings and output storage live through
    // the calls, and the registry query is constrained to a single DWORD.
    unsafe {
        if GetSystemMetrics(SM_REMOTESESSION) != 0
            || (SystemParametersInfoW(
                SPI_GETHIGHCONTRAST,
                high_contrast.cbSize,
                (&mut high_contrast as *mut HIGHCONTRASTW).cast(),
                0,
            ) != 0
                && high_contrast.dwFlags & HCF_HIGHCONTRASTON != 0)
        {
            return false;
        }
        let status = RegGetValueW(
            HKEY_CURRENT_USER,
            wide(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize").as_ptr(),
            wide("EnableTransparency").as_ptr(),
            RRF_RT_REG_DWORD,
            null_mut(),
            (&mut transparency as *mut u32).cast(),
            &mut bytes,
        );
        status != ERROR_SUCCESS || transparency != 0
    }
}

impl Drop for Backdrop {
    fn drop(&mut self) {
        self.frost = None;
        // SAFETY: The backdrop owns this HWND and retains its input state through teardown.
        unsafe { DestroyWindow(self.hwnd) };
    }
}

unsafe extern "system" fn procedure(hwnd: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    // SAFETY: The callback follows the Win32 message contract. The input state is boxed by Backdrop.
    unsafe {
        match message {
            WM_NCCREATE => {
                let cs = &*(l as *const CREATESTRUCTW);
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
                1
            }
            WM_MOUSEACTIVATE => MA_NOACTIVATE as isize,
            WM_SETCURSOR => {
                if let Some(input) =
                    (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Input).as_ref()
                {
                    SetCursor(input.cursor.borrow().handle());
                    1
                } else {
                    DefWindowProcW(hwnd, message, w, l)
                }
            }
            WM_LBUTTONDOWN | WM_LBUTTONUP => {
                // Color-keyed gaps in the footer receive clicks on the backdrop underneath.
                if let Some(input) =
                    (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Input).as_ref()
                {
                    let codex = input.codex.get();
                    if !codex.is_null() {
                        let mut point = POINT {
                            x: l as i16 as i32,
                            y: (l >> 16) as i16 as i32,
                        };
                        MapWindowPoints(hwnd, codex, &mut point, 1);
                        let position = (usize::from(point.x as u16)
                            | (usize::from(point.y as u16) << 16))
                            as isize;
                        SendMessageW(
                            codex,
                            super::window::FOOTER_CLICK,
                            message as usize,
                            position,
                        );
                    }
                }
                0
            }
            WM_RBUTTONDOWN | WM_RBUTTONUP | WM_MBUTTONDOWN | WM_MBUTTONUP | WM_MOUSEWHEEL
            | WM_MOUSEHWHEEL => 0,
            WM_ERASEBKGND => 1,
            WM_PAINT => {
                let mut paint = PAINTSTRUCT::default();
                let dc = BeginPaint(hwnd, &mut paint);
                FillRect(dc, &paint.rcPaint, GetStockObject(BLACK_BRUSH));
                EndPaint(hwnd, &paint);
                0
            }
            _ => DefWindowProcW(hwnd, message, w, l),
        }
    }
}
