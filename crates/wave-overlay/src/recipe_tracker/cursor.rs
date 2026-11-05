//! Owned copy of the cursor observed over the foreground game. Never changes game resources.
use super::*;
#[derive(Default)]
pub(super) struct GameCursor {
    game: HWND,
    process: u32,
    source: HCURSOR,
    owned: HCURSOR,
}
impl Drop for GameCursor {
    fn drop(&mut self) {
        self.clear();
    }
}
fn arrow() -> HCURSOR {
    // SAFETY: The module/resource identifiers or terminated path are valid for the synchronous
    // lookup; shared image handles are borrowed, while explicitly extracted copies retain their
    // documented owners.
    unsafe { LoadCursorW(std::ptr::null_mut(), IDC_ARROW) }
}
impl GameCursor {
    fn clear(&mut self) {
        if !self.owned.is_null() {
            // SAFETY: Only the cursor duplicate created by CopyImage is destroyed here; shared
            // game/system cursor handles are never freed.
            unsafe {
                if GetCursor() == self.owned {
                    SetCursor(arrow());
                }
                DestroyCursor(self.owned);
            }
        }
        self.owned = std::ptr::null_mut();
        self.source = std::ptr::null_mut();
    }
    fn copy(source: HCURSOR) -> Result<HCURSOR> {
        ensure!(!source.is_null(), "Game cursor is unavailable");
        // Zero dimensions retain the original size/hotspot; no shared/delete flags.
        // SAFETY: The source cursor is borrowed; CopyImage creates an independently owned
        // duplicate, so cleanup destroys only that duplicate.
        let copied = unsafe { CopyImage(source, IMAGE_CURSOR, 0, 0, 0) };
        ensure!(!copied.is_null(), "Could not copy the visible game cursor");
        Ok(copied)
    }
    fn replace(&mut self, source: HCURSOR, copied: HCURSOR) {
        self.clear();
        self.source = source;
        self.owned = copied;
    }
    pub(super) fn handle(&self) -> HCURSOR {
        if self.owned.is_null() {
            arrow()
        } else {
            self.owned
        }
    }
    pub(super) fn observe(&mut self, game: HWND, process: u32) {
        if self.game != game || self.process != process {
            self.clear();
            self.game = game;
            self.process = process;
        }
        // SAFETY: These Win32 queries use scalar values or borrowed handles, validate native handle
        // identity themselves, and neither dereference Rust memory nor transfer ownership.
        if game.is_null() || process == 0 || unsafe { GetForegroundWindow() } != game {
            return;
        }
        let mut info = CURSORINFO {
            cbSize: size_of::<CURSORINFO>() as u32,
            ..Default::default()
        };
        // SAFETY: Win32 validates the borrowed handle where required; every output points to an
        // initialized local struct or bounded writable slice that stays live for the synchronous
        // query.
        if unsafe { GetCursorInfo(&mut info) } == 0
            || info.flags & CURSOR_SHOWING == 0
            || info.hCursor.is_null()
        {
            return;
        }
        // SAFETY: These Win32 queries use scalar values or borrowed handles, validate native handle
        // identity themselves, and neither dereference Rust memory nor transfer ownership.
        let under = unsafe { WindowFromPoint(info.ptScreenPos) };
        // SAFETY: These Win32 queries use scalar values or borrowed handles, validate native handle
        // identity themselves, and neither dereference Rust memory nor transfer ownership.
        let root = unsafe { GetAncestor(under, GA_ROOT) };
        let mut owner = 0;
        // SAFETY: Win32 validates the borrowed handle where required; every output points to an
        // initialized local struct or bounded writable slice that stays live for the synchronous
        // query.
        unsafe {
            GetWindowThreadProcessId(root, &mut owner);
        }
        // Never learn our close-window cursor, a different app's cursor or a hidden cursor.
        if root != game
            || owner != process
            || info.hCursor == self.source
            || info.hCursor == self.owned
        {
            return;
        }
        let Ok(copied) = Self::copy(info.hCursor) else {
            return;
        };
        let mut again = CURSORINFO {
            cbSize: size_of::<CURSORINFO>() as u32,
            ..Default::default()
        };
        // SAFETY: Win32 validates the borrowed handle where required; every output points to an
        // initialized local struct or bounded writable slice that stays live for the synchronous
        // query.
        let stable = unsafe {
            GetCursorInfo(&mut again) != 0
                && again.flags & CURSOR_SHOWING != 0
                && again.hCursor == info.hCursor
                && GetForegroundWindow() == game
                && GetAncestor(WindowFromPoint(again.ptScreenPos), GA_ROOT) == game
        };
        if stable {
            self.replace(info.hCursor, copied);
        } else {
            // SAFETY: Only the cursor duplicate created by CopyImage is destroyed here; shared
            // game/system cursor handles are never freed.
            unsafe {
                DestroyCursor(copied);
            }
        }
    }
}
pub(super) fn self_test() -> Result<GameCursor> {
    let mut cursor = GameCursor::default();
    // SAFETY: The module/resource identifiers or terminated path are valid for the synchronous
    // lookup; shared image handles are borrowed, while explicitly extracted copies retain their
    // documented owners.
    let source = unsafe { LoadCursorW(std::ptr::null_mut(), IDC_HAND) };
    let copied = GameCursor::copy(source)?;
    ensure!(copied != source, "Cursor copy unexpectedly shares game ownership");
    cursor.replace(source, copied);
    ensure!(cursor.handle() == copied && cursor.source == source, "Cursor identity not retained");
    fn hotspot(handle: HCURSOR) -> Result<(u32, u32, i32)> {
        let mut info = ICONINFO::default();
        // SAFETY: The cursor/icon handle is borrowed and the ICONINFO output is initialized;
        // returned bitmap copies are owned by this code and deleted without destroying the original
        // shared cursor/icon.
        ensure!(unsafe { GetIconInfo(handle, &mut info) } != 0, "Cursor shape unavailable");
        let result = (info.xHotspot, info.yHotspot, info.fIcon);
        // SAFETY: The drawing DC is live on this UI thread; local text/geometry buffers match their
        // lengths. Owned GDI objects are restored/deselected before deletion; stock/system objects
        // remain borrowed.
        unsafe {
            if !info.hbmColor.is_null() {
                DeleteObject(info.hbmColor);
            }
            if !info.hbmMask.is_null() {
                DeleteObject(info.hbmMask);
            }
        }
        Ok(result)
    }
    ensure!(hotspot(source)? == hotspot(copied)?, "Cursor hotspot/type changed during copying");
    ensure!(
        GameCursor::copy(std::ptr::null_mut()).is_err() && cursor.handle() == copied,
        "Failed cursor read lost the last valid appearance"
    );
    cursor.clear();
    ensure!(cursor.owned.is_null() && cursor.handle() == arrow(), "Cursor cleanup/fallback failed");
    ensure!(hotspot(source)?.2 == 0, "Cleanup damaged the original shared cursor");
    cursor.replace(source, GameCursor::copy(source)?);
    Ok(cursor)
}
