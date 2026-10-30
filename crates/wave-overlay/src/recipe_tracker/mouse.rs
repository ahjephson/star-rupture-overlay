//! Foreground-scoped, pass-through mouse observation. Callback only posts compact events.
use super::*;
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
pub(super) const MESSAGE: u32 = WM_APP + 0x93;
static TARGET: AtomicIsize = AtomicIsize::new(0);
static PROCESS: AtomicU32 = AtomicU32::new(0);
unsafe extern "system" fn callback(code: i32, message: usize, data: isize) -> isize {
    if code >= 0 && message == WM_LBUTTONDOWN as usize {
        // Windows owns this pointer for the synchronous duration of the hook callback.
        let event = unsafe { &*(data as *const MSLLHOOKSTRUCT) };
        let target = TARGET.load(Ordering::Relaxed);
        let mut process = 0;
        unsafe {
            GetWindowThreadProcessId(GetForegroundWindow(), &mut process);
        }
        if target != 0
            && process != 0
            && process == PROCESS.load(Ordering::Relaxed)
            && event.flags & LLMHF_INJECTED == 0
        {
            let point = (event.pt.x as u32 as u64) | ((event.pt.y as u32 as u64) << 32);
            unsafe {
                PostMessageW(target as HWND, MESSAGE, event.time as usize, point as isize);
            }
        }
    }
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, message, data) }
}
pub(super) fn cursor_visible() -> bool {
    let mut cursor = CURSORINFO {
        cbSize: size_of::<CURSORINFO>() as u32,
        ..Default::default()
    };
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
        unsafe {
            GetWindowThreadProcessId(game, &mut process);
        }
        if process == 0 {
            return;
        }
        self.game = game;
        PROCESS.store(process, Ordering::Relaxed);
        TARGET.store(owner as isize, Ordering::Relaxed);
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
        if self.hook.is_null() || unsafe { GetForegroundWindow() } != game || !cursor_visible() {
            self.previous = None;
            return false;
        }
        // Close buttons and every non-game surface are excluded even while game focus remains.
        let under = unsafe {
            WindowFromPoint(POINT {
                x: point[0],
                y: point[1],
            })
        };
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
                unsafe { GetDoubleClickTime() },
                unsafe { GetSystemMetrics(SM_CXDOUBLECLK) },
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
