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
    unsafe { LoadCursorW(std::ptr::null_mut(), IDC_ARROW) }
}
impl GameCursor {
    fn clear(&mut self) {
        if !self.owned.is_null() {
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
        if game.is_null() || process == 0 || unsafe { GetForegroundWindow() } != game {
            return;
        }
        let mut info = CURSORINFO {
            cbSize: size_of::<CURSORINFO>() as u32,
            ..Default::default()
        };
        if unsafe { GetCursorInfo(&mut info) } == 0
            || info.flags & CURSOR_SHOWING == 0
            || info.hCursor.is_null()
        {
            return;
        }
        let under = unsafe { WindowFromPoint(info.ptScreenPos) };
        let root = unsafe { GetAncestor(under, GA_ROOT) };
        let mut owner = 0;
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
            unsafe {
                DestroyCursor(copied);
            }
        }
    }
}
pub(super) fn self_test() -> Result<GameCursor> {
    let mut cursor = GameCursor::default();
    let source = unsafe { LoadCursorW(std::ptr::null_mut(), IDC_HAND) };
    let copied = GameCursor::copy(source)?;
    ensure!(copied != source, "Cursor copy unexpectedly shares game ownership");
    cursor.replace(source, copied);
    ensure!(cursor.handle() == copied && cursor.source == source, "Cursor identity not retained");
    fn hotspot(handle: HCURSOR) -> Result<(u32, u32, i32)> {
        let mut info = ICONINFO::default();
        ensure!(unsafe { GetIconInfo(handle, &mut info) } != 0, "Cursor shape unavailable");
        let result = (info.xHotspot, info.yHotspot, info.fIcon);
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
