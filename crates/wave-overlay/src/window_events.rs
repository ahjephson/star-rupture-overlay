//! Queued desktop window notifications. Callbacks run in this app's UI thread,
//! never inside the game. They only coalesce/post work, never touch App or memory.
use std::{
    marker::PhantomData,
    ptr::null_mut,
    rc::Rc,
    sync::atomic::{AtomicBool, AtomicPtr, Ordering},
};
use windows_sys::Win32::{
    Foundation::HWND,
    UI::{Accessibility::*, WindowsAndMessaging::*},
};

pub(super) const CHANGED: u32 = WM_APP + 3;
static TARGET: AtomicPtr<std::ffi::c_void> = AtomicPtr::new(null_mut());
static PENDING: AtomicBool = AtomicBool::new(false);
const RANGES: [(u32, u32); 3] = [
    (EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND),
    (EVENT_SYSTEM_MINIMIZESTART, EVENT_SYSTEM_MINIMIZEEND),
    (EVENT_OBJECT_DESTROY, EVENT_OBJECT_REORDER),
];

fn relevant(event: u32, object: i32, child: i32) -> bool {
    object == OBJID_WINDOW
        && child == 0
        && RANGES.iter().any(|(first, last)| (*first..=*last).contains(&event))
}

unsafe extern "system" fn notify(
    _: HWINEVENTHOOK,
    event: u32,
    source: HWND,
    object: i32,
    child: i32,
    _: u32,
    _: u32,
) {
    let target = TARGET.load(Ordering::Relaxed);
    if target.is_null() || source == target || !relevant(event, object, child) {
        return;
    }
    if !PENDING.swap(true, Ordering::Relaxed) && unsafe { PostMessageW(target, CHANGED, 0, 0) } == 0
    {
        PENDING.store(false, Ordering::Relaxed);
    }
}

/// Thread-affine RAII: register and unhook on the owning UI thread. Out-of-context
/// callbacks are queued, no DLL is supplied or mapped into another process.
pub(super) struct WindowEvents {
    hooks: Vec<HWINEVENTHOOK>,
    window: HWND,
    _ui_thread: PhantomData<Rc<()>>,
}
impl WindowEvents {
    pub(super) fn install(window: HWND) -> std::io::Result<Self> {
        Self::register(window, true)
    }
    // Synthetic diagnostic events originate in this process, unlike real desktop events.
    pub(super) fn diagnostic(window: HWND) -> std::io::Result<Self> {
        Self::register(window, false)
    }
    fn register(window: HWND, skip_own: bool) -> std::io::Result<Self> {
        if window.is_null()
            || TARGET
                .compare_exchange(null_mut(), window, Ordering::Relaxed, Ordering::Relaxed)
                .is_err()
        {
            return Err(std::io::Error::other(
                "Window notification target already registered or invalid",
            ));
        }
        PENDING.store(false, Ordering::Relaxed);
        let mut owner = Self {
            hooks: Vec::new(),
            window,
            _ui_thread: PhantomData,
        };
        for (first, last) in RANGES {
            let flags = WINEVENT_OUTOFCONTEXT
                | if skip_own {
                    WINEVENT_SKIPOWNPROCESS
                } else {
                    0
                };
            let hook =
                unsafe { SetWinEventHook(first, last, null_mut(), Some(notify), 0, 0, flags) };
            if hook.is_null() {
                return Err(std::io::Error::other(format!(
                    "Could not subscribe to window events {first:#x}..{last:#x}: {}",
                    std::io::Error::last_os_error()
                )));
            }
            owner.hooks.push(hook);
        }
        Ok(owner)
    }
}
impl Drop for WindowEvents {
    fn drop(&mut self) {
        // Disable delivery before unhooking. A queued stale notification is harmless.
        let _ =
            TARGET.compare_exchange(self.window, null_mut(), Ordering::Relaxed, Ordering::Relaxed);
        for hook in self.hooks.drain(..) {
            unsafe {
                UnhookWinEvent(hook);
            }
        }
        PENDING.store(false, Ordering::Relaxed);
    }
}
pub(super) fn acknowledge() {
    PENDING.store(false, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn notifications_ignore_accessibility_children_and_unrelated_events() {
        assert!(relevant(EVENT_SYSTEM_FOREGROUND, OBJID_WINDOW, 0));
        assert!(relevant(EVENT_OBJECT_REORDER, OBJID_WINDOW, 0));
        assert!(!relevant(EVENT_OBJECT_REORDER, OBJID_CLIENT, 0));
        assert!(!relevant(EVENT_OBJECT_REORDER, OBJID_WINDOW, 1));
        assert!(!relevant(EVENT_OBJECT_NAMECHANGE, OBJID_WINDOW, 0));
    }
}
