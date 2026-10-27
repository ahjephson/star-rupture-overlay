//! Keep our ordinary top-level HUD immediately above the game, below windows
//! that cover the game. No foreign ownership, activation, injection or game changes.
use std::ptr::null_mut;
use windows_sys::Win32::{Foundation::*, Graphics::Gdi::IntersectRect, UI::WindowsAndMessaging::*};

pub(super) const EXTENDED_STYLE: u32 =
    WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT | WS_EX_LAYERED;
const POSITION_FLAGS: u32 = SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOMOVE | SWP_NOSIZE;

/// Changes only our HWND. Topmost games are followed only while foreground;
/// switching away hides and demotes the HUD without activating another window.
pub(super) fn follow_game(overlay: HWND, game: HWND) -> bool {
    follow_game_with_foreground(overlay, game, || unsafe { GetForegroundWindow() })
}

// A foreground provider lets the owned-window diagnostic exercise focus transitions
// without stealing the user's actual foreground window.
fn follow_game_with_foreground(overlay: HWND, game: HWND, foreground: impl Fn() -> HWND) -> bool {
    unsafe {
        if IsWindow(overlay) == 0 || overlay == game {
            return false;
        }
        if !target_available(game) {
            hide_and_demote(overlay);
            return false;
        }
        let game_topmost = topmost(game);
        if game_topmost && foreground() != game {
            hide_and_demote(overlay);
            return false;
        }
        if !game_topmost
            && topmost(overlay)
            && SetWindowPos(overlay, HWND_NOTOPMOST, 0, 0, 0, 0, POSITION_FLAGS) == 0
        {
            hide_and_demote(overlay);
            return false;
        }
        let previous = GetWindow(game, GW_HWNDPREV);
        let result = if previous == overlay && topmost(overlay) == game_topmost {
            if IsWindowVisible(overlay) == 0 {
                SetWindowPos(
                    overlay,
                    null_mut(),
                    0,
                    0,
                    0,
                    0,
                    POSITION_FLAGS | SWP_NOZORDER | SWP_SHOWWINDOW,
                ) != 0
            } else {
                true
            }
        } else {
            // Stay immediately above the game, below any existing covering window.
            // At the band boundary use TOP rather than a topmost predecessor, which
            // would implicitly promote an ordinary HUD. Only fullscreen needs TOPMOST.
            let after = if previous.is_null()
                || previous == overlay
                || (!game_topmost && topmost(previous))
            {
                if game_topmost {
                    HWND_TOPMOST
                } else {
                    HWND_TOP
                }
            } else {
                previous
            };
            SetWindowPos(overlay, after, 0, 0, 0, 0, POSITION_FLAGS | SWP_SHOWWINDOW) != 0
        };
        let valid = result
            && target_available(game)
            && topmost(game) == game_topmost
            && (!game_topmost || foreground() == game)
            && GetWindow(game, GW_HWNDPREV) == overlay
            && topmost(overlay) == game_topmost;
        if !valid {
            hide_and_demote(overlay);
        }
        valid
    }
}

unsafe fn topmost(window: HWND) -> bool {
    unsafe { GetWindowLongPtrW(window, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST != 0 }
}

unsafe fn hide_and_demote(overlay: HWND) {
    unsafe {
        ShowWindow(overlay, SW_HIDE);
        if topmost(overlay) {
            SetWindowPos(overlay, HWND_NOTOPMOST, 0, 0, 0, 0, POSITION_FLAGS);
        }
    }
}

unsafe fn target_available(game: HWND) -> bool {
    unsafe {
        !game.is_null() && IsWindow(game) != 0 && IsWindowVisible(game) != 0 && IsIconic(game) == 0
    }
}

/// Uses owned, off-screen test windows only: never moves/focuses an actual app,
/// accesses the game or launches a second HUD. Exercises real Win32 ordering.
pub(super) fn self_test() -> Result<(), Box<dyn std::error::Error>> {
    use serde_json::json;
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain([0]).collect()
    }
    struct OwnedWindow(HWND);
    impl OwnedWindow {
        fn close(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    DestroyWindow(self.0);
                }
                self.0 = null_mut();
            }
        }
    }
    impl Drop for OwnedWindow {
        fn drop(&mut self) {
            self.close();
        }
    }
    unsafe extern "system" fn procedure(window: HWND, message: u32, w: usize, l: isize) -> isize {
        if message == WM_MOUSEACTIVATE {
            MA_NOACTIVATE as isize
        } else {
            unsafe { DefWindowProcW(window, message, w, l) }
        }
    }
    fn require(value: bool, label: &str) -> Result<(), Box<dyn std::error::Error>> {
        if value {
            Ok(())
        } else {
            Err(format!("Stacking check failed: {label}").into())
        }
    }
    unsafe fn above(first: HWND, second: HWND) -> bool {
        let mut current = unsafe { GetWindow(second, GW_HWNDPREV) };
        // Bounded because windows can disappear/reorder during enumeration.
        for _ in 0..4096 {
            if current.is_null() {
                return false;
            }
            if current == first {
                return true;
            }
            current = unsafe { GetWindow(current, GW_HWNDPREV) };
        }
        false
    }
    unsafe {
        let foreground = GetForegroundWindow();
        let module = GetModuleHandleW(null_mut());
        let class = wide("StarRuptureStackingSelfTest");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(procedure),
            hInstance: module,
            lpszClassName: class.as_ptr(),
            ..Default::default()
        };
        require(RegisterClassW(&wc) != 0, "register fixture class")?;
        let create =
            |x, y, width, height, style| -> Result<OwnedWindow, Box<dyn std::error::Error>> {
                let window = CreateWindowExW(
                    style,
                    class.as_ptr(),
                    wide("Offscreen stacking fixture").as_ptr(),
                    WS_POPUP,
                    x,
                    y,
                    width,
                    height,
                    null_mut(),
                    null_mut(),
                    module,
                    null_mut(),
                );
                require(!window.is_null(), "create fixture")?;
                Ok(OwnedWindow(window))
            };
        let normal = WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE;
        let mut game = create(-30000, -30000, 1000, 800, normal)?;
        let hud = create(-29992, -29992, 220, 84, EXTENDED_STYLE)?;
        let browser = create(-29000, -30000, 1000, 800, normal)?;
        let cover = create(-30000, -30000, 1000, 800, normal)?;
        let raise = |window, after| {
            SetWindowPos(window, after, 0, 0, 0, 0, POSITION_FLAGS | SWP_SHOWWINDOW) != 0
        };
        require(raise(game.0, HWND_TOP), "show fixture game")?;
        require(
            !follow_game(game.0, game.0) && IsWindowVisible(game.0) != 0,
            "invalid same-window request leaves target unchanged",
        )?;
        require(follow_game(hud.0, game.0), "HUD above game")?;
        require(raise(browser.0, HWND_TOP), "raise other-monitor browser")?;
        require(
            follow_game(hud.0, game.0) && above(browser.0, hud.0) && above(hud.0, game.0),
            "other-monitor ordering",
        )?;
        let mut hr = RECT::default();
        let mut br = RECT::default();
        let mut intersection = RECT::default();
        GetWindowRect(hud.0, &mut hr);
        GetWindowRect(browser.0, &mut br);
        require(
            IntersectRect(&mut intersection, &hr, &br) == 0,
            "browser does not cover HUD region",
        )?;
        require(raise(cover.0, HWND_TOP), "raise overlapping window")?;
        for _ in 0..100 {
            require(
                follow_game(hud.0, game.0) && above(cover.0, hud.0) && above(hud.0, game.0),
                "cover remains above HUD after repeated sync",
            )?;
        }
        GetWindowRect(cover.0, &mut br);
        require(
            IntersectRect(&mut intersection, &hr, &br) != 0,
            "same-monitor cover overlaps HUD",
        )?;
        SetWindowPos(
            cover.0,
            null_mut(),
            hr.left + 100,
            hr.top,
            100,
            84,
            SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOOWNERZORDER,
        );
        require(follow_game(hud.0, game.0) && above(cover.0, hud.0), "partial cover")?;
        require(
            raise(game.0, HWND_TOP) && follow_game(hud.0, game.0) && above(hud.0, cover.0),
            "return game above covering app",
        )?;
        SetWindowPos(
            hud.0,
            null_mut(),
            -29800,
            -29700,
            230,
            90,
            SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOOWNERZORDER,
        );
        GetWindowRect(hud.0, &mut hr);
        require(follow_game(hud.0, game.0), "follow manually moved HUD")?;
        GetWindowRect(hud.0, &mut br);
        require(
            hr.left == br.left
                && hr.top == br.top
                && hr.right == br.right
                && hr.bottom == br.bottom,
            "stacking preserves manual position and size",
        )?;
        ShowWindow(game.0, SW_HIDE);
        require(!follow_game(hud.0, game.0) && IsWindowVisible(hud.0) == 0, "hidden target")?;
        require(raise(game.0, HWND_TOP) && follow_game(hud.0, game.0), "restore hidden target")?;
        ShowWindow(game.0, SW_SHOWMINNOACTIVE);
        require(
            IsIconic(game.0) != 0 && !follow_game(hud.0, game.0) && IsWindowVisible(hud.0) == 0,
            "minimized target",
        )?;
        ShowWindow(game.0, SW_SHOWNOACTIVATE);
        require(follow_game(hud.0, game.0), "restore minimized target")?;
        require(raise(game.0, HWND_TOPMOST), "topmost fixture game")?;
        require(
            !follow_game_with_foreground(hud.0, game.0, || browser.0)
                && IsWindowVisible(hud.0) == 0
                && !topmost(hud.0),
            "unfocused topmost target hides and demotes HUD",
        )?;
        require(
            follow_game_with_foreground(hud.0, game.0, || game.0)
                && topmost(hud.0)
                && above(hud.0, game.0),
            "foreground topmost game promotes HUD",
        )?;
        require(raise(cover.0, HWND_TOPMOST), "topmost covering fixture")?;
        for _ in 0..100 {
            require(
                follow_game_with_foreground(hud.0, game.0, || game.0)
                    && above(cover.0, hud.0)
                    && above(hud.0, game.0),
                "topmost cover stays above HUD",
            )?;
        }
        require(
            !follow_game_with_foreground(hud.0, game.0, || browser.0)
                && IsWindowVisible(hud.0) == 0
                && !topmost(hud.0),
            "Alt-Tab hides and demotes promoted HUD",
        )?;
        require(
            follow_game_with_foreground(hud.0, game.0, || game.0) && topmost(hud.0),
            "fullscreen focus return restores HUD",
        )?;
        let focus_reads = std::cell::Cell::new(0);
        require(
            !follow_game_with_foreground(hud.0, game.0, || {
                let n = focus_reads.get();
                focus_reads.set(n + 1);
                if n == 0 {
                    game.0
                } else {
                    browser.0
                }
            }) && IsWindowVisible(hud.0) == 0
                && !topmost(hud.0),
            "focus changed during synchronization hides and demotes HUD",
        )?;
        require(
            follow_game_with_foreground(hud.0, game.0, || game.0),
            "restore before fullscreen minimize",
        )?;
        ShowWindow(game.0, SW_SHOWMINNOACTIVE);
        require(
            !follow_game_with_foreground(hud.0, game.0, || game.0)
                && IsWindowVisible(hud.0) == 0
                && !topmost(hud.0),
            "minimized fullscreen target demotes HUD",
        )?;
        ShowWindow(game.0, SW_SHOWNOACTIVATE);
        require(
            raise(game.0, HWND_NOTOPMOST)
                && follow_game(hud.0, game.0)
                && !topmost(hud.0)
                && above(cover.0, hud.0),
            "normal band recovery below topmost cover",
        )?;
        require(
            raise(hud.0, HWND_TOPMOST)
                && follow_game(hud.0, game.0)
                && GetWindowLongPtrW(hud.0, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST == 0,
            "accidental HUD promotion recovered",
        )?;
        game.close();
        require(!follow_game(hud.0, game.0) && IsWindowVisible(hud.0) == 0, "destroyed target")?;
        require(foreground == GetForegroundWindow(), "foreground focus preserved")?;
        println!(
            "{}",
            json!({
                "status": "passed",
                "gameRead": false,
                "actualWindowsModified": false,
                "visibleHudLaunched": false,
                "offscreenOwnedFixtures": 4,
                "otherMonitor": true,
                "overlappingWindow": true,
                "partialCover": true,
                "returnToGame": true,
                "manualPositionPreserved": true,
                "repeatedSyncs": 100,
                "hiddenMinimizedDestroyed": true,
                "topmostForegroundFollow": true,
                "topmostCoverPreserved": true,
                "topmostFocusLossDemotes": true,
                "focusRaceDemotes": true,
                "normalBandRecovery": true,
                "accidentalPromotionRecovered": true,
                "foregroundPreserved": true
            })
        );
        Ok(())
    }
}
