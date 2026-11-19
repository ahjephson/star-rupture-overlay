//! Foreground-scoped, pass-through mouse observation. Callback only posts compact events.
use super::*;
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
pub(super) const MESSAGE: u32 = WM_APP + 0x93;
static TARGET: AtomicIsize = AtomicIsize::new(0);
static PROCESS: AtomicU32 = AtomicU32::new(0);
/// # Safety
/// Windows must invoke this WH_MOUSE_LL callback with a live MSLLHOOKSTRUCT for actionable
/// codes; the pointer must not escape the callback.
unsafe extern "system" fn callback(code: i32, message: usize, data: isize) -> isize {
    if code >= 0 && message == WM_LBUTTONDOWN as usize {
        // Windows owns this pointer for the synchronous duration of the hook callback.
        // SAFETY: For an actionable WH_MOUSE_LL callback, Windows supplies a live MSLLHOOKSTRUCT;
        // it is read only here and no pointer is posted to the UI.
        let event = unsafe { &*(data as *const MSLLHOOKSTRUCT) };
        let target = TARGET.load(Ordering::Relaxed);
        let mut process = 0;
        // SAFETY: Win32 validates the borrowed handle where required; every output points to an
        // initialized local struct or bounded writable slice that stays live for the synchronous
        // query.
        unsafe {
            GetWindowThreadProcessId(GetForegroundWindow(), &mut process);
        }
        if target != 0
            && process != 0
            && process == PROCESS.load(Ordering::Relaxed)
            && event.flags & LLMHF_INJECTED == 0
        {
            let point = (event.pt.x as u32 as u64) | ((event.pt.y as u32 as u64) << 32);
            // SAFETY: Only scalar event data/IDs are queued to the retained UI HWND; no pointer to
            // a Rust stack value or borrow crosses the asynchronous message boundary.
            unsafe {
                PostMessageW(target as HWND, MESSAGE, event.time as usize, point as isize);
            }
        }
    }
    // SAFETY: The hook handle is borrowed from its owner; callback arguments are forwarded
    // unchanged, and teardown disables stored targets before releasing the hook.
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, message, data) }
}
pub(super) fn cursor_visible() -> bool {
    let mut cursor = CURSORINFO {
        cbSize: size_of::<CURSORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: Win32 validates the borrowed handle where required; every output points to an
    // initialized local struct or bounded writable slice that stays live for the synchronous query.
    unsafe { GetCursorInfo(&mut cursor) != 0 && cursor.flags & CURSOR_SHOWING != 0 }
}
#[derive(Clone, Copy)]
struct Click {
    point: [i32; 2],
    time: u32,
}
#[derive(Default)]
pub(super) struct Input {
    hook: HHOOK,
    game: HWND,
    previous: Option<Click>,
    failed: bool,
}
impl Drop for Input {
    fn drop(&mut self) {
        self.disable();
    }
}
impl Input {
    fn disable(&mut self) {
        TARGET.store(0, Ordering::Relaxed);
        PROCESS.store(0, Ordering::Relaxed);
        if !self.hook.is_null() {
            // SAFETY: The hook handle is borrowed from its owner; callback arguments are forwarded
            // unchanged, and teardown disables stored targets before releasing the hook.
            unsafe {
                UnhookWindowsHookEx(self.hook);
            }
            self.hook = std::ptr::null_mut();
        }
        self.previous = None;
        self.game = std::ptr::null_mut();
    }
    pub(super) fn configure(&mut self, owner: HWND, game: HWND, enabled: bool) {
        if !enabled {
            self.disable();
            self.failed = false;
            return;
        }
        if !self.hook.is_null() && self.game != game {
            self.disable();
            self.failed = false;
        }
        if !self.hook.is_null() || self.failed {
            return;
        }
        let mut process = 0;
        // SAFETY: Win32 validates the borrowed handle where required; every output points to an
        // initialized local struct or bounded writable slice that stays live for the synchronous
        // query.
        unsafe {
            GetWindowThreadProcessId(game, &mut process);
        }
        if process == 0 {
            return;
        }
        self.game = game;
        PROCESS.store(process, Ordering::Relaxed);
        TARGET.store(owner as isize, Ordering::Relaxed);
        // SAFETY: The callback has the required Win32 ABI and static code lifetime; registration
        // stays on the owning UI thread and teardown clears delivery state before unhooking.
        self.hook = unsafe {
            SetWindowsHookExW(WH_MOUSE_LL, Some(callback), GetModuleHandleW(std::ptr::null()), 0)
        };
        if self.hook.is_null() {
            self.disable();
            self.failed = true;
            eprintln!("Recipe mouse shortcut unavailable: {}", std::io::Error::last_os_error());
        }
    }
    pub(super) fn accept(&mut self, game: HWND, time: u32, point: [i32; 2]) -> bool {
        // SAFETY: These Win32 queries use scalar values or borrowed handles, validate native handle
        // identity themselves, and neither dereference Rust memory nor transfer ownership.
        if self.hook.is_null() || unsafe { GetForegroundWindow() } != game || !cursor_visible() {
            self.previous = None;
            return false;
        }
        // Close buttons and every non-game surface are excluded even while game focus remains.
        // SAFETY: These Win32 queries use scalar values or borrowed handles, validate native handle
        // identity themselves, and neither dereference Rust memory nor transfer ownership.
        let under = unsafe {
            WindowFromPoint(POINT {
                x: point[0],
                y: point[1],
            })
        };
        // SAFETY: These Win32 queries use scalar values or borrowed handles, validate native handle
        // identity themselves, and neither dereference Rust memory nor transfer ownership.
        if under.is_null() || unsafe { GetAncestor(under, GA_ROOT) } != game {
            self.previous = None;
            return false;
        }
        let next = Click {
            point,
            time,
        };
        let matched = self.previous.is_some_and(|first| {
            pair(
                first,
                next,
                // SAFETY: These Win32 queries use scalar values or borrowed handles, validate
                // native handle identity themselves, and neither dereference Rust memory nor
                // transfer ownership.
                unsafe { GetDoubleClickTime() },
                // SAFETY: These Win32 queries use scalar values or borrowed handles, validate
                // native handle identity themselves, and neither dereference Rust memory nor
                // transfer ownership.
                unsafe { GetSystemMetrics(SM_CXDOUBLECLK) },
                // SAFETY: These Win32 queries use scalar values or borrowed handles, validate
                // native handle identity themselves, and neither dereference Rust memory nor
                // transfer ownership.
                unsafe { GetSystemMetrics(SM_CYDOUBLECLK) },
            )
        });
        self.previous = if matched {
            None
        } else {
            Some(next)
        };
        matched
    }
}
fn pair(first: Click, next: Click, timeout: u32, w: i32, h: i32) -> bool {
    next.time.wrapping_sub(first.time) <= timeout
        && (i64::from(next.point[0]) - i64::from(first.point[0])).abs() <= i64::from(w / 2)
        && (i64::from(next.point[1]) - i64::from(first.point[1])).abs() <= i64::from(h / 2)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn double_click_uses_windows_time_distance_and_wraps_safely() {
        let first = Click {
            point: [10, 10],
            time: u32::MAX - 20,
        };
        assert!(pair(
            first,
            Click {
                point: [12, 12],
                time: 10
            },
            500,
            4,
            4
        ));
        assert!(!pair(
            first,
            Click {
                point: [13, 12],
                time: 10
            },
            500,
            4,
            4
        ));
        assert!(!pair(
            first,
            Click {
                point: [12, 12],
                time: 600
            },
            500,
            4,
            4
        ));
    }
}
