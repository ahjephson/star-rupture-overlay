//! Native checks and captures using owned windows and isolated preferences.
use super::*;

/// Exercise owned dialogs and isolated preference files without shell actions or game access.
pub fn self_test(directory: &Path) -> Result<()> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::IsWindowEnabled;
    fs::create_dir_all(directory)?;
    let foreground = unsafe { GetForegroundWindow() };
    let context = Context::diagnostic(directory.join("preferences.json"));
    let mut dialogs = Dialogs::default();
    dialogs.open(SETTINGS, std::ptr::null_mut(), context.clone())?;
    let window = dialogs.windows[0].0.window.get();
    dialogs.open(SETTINGS, std::ptr::null_mut(), context.clone())?;
    ensure!(dialogs.windows.len() == 1, "Settings opened more than one window");
    ensure!(
        unsafe { IsWindowEnabled(GetDlgItem(window, IDOK)) } == 0,
        "Save should start disabled"
    );
    ensure!(
        unsafe { SendDlgItemMessageW(window, 111, CB_GETCURSEL, 0, 0) } == 0,
        "Default exit preference changed"
    );
    for (state, expected_label, expected_action) in [
        (desktop::ShortcutState::OtherCopy, "Shortcut points to another copy", "Update shortcut"),
        (desktop::ShortcutState::Current, "Desktop shortcut created", "Remove shortcut"),
        (desktop::ShortcutState::Other, "A different shortcut uses this name", "Create shortcut"),
        (desktop::ShortcutState::Missing, "No shortcut created", "Create shortcut"),
    ] {
        dialogs.windows[0].0.shortcut.set(state);
        shortcut_labels(&dialogs.windows[0].0);
        for (control, expected) in [(112, expected_label), (113, expected_action)] {
            let mut buffer = [0u16; 128];
            let length = unsafe {
                GetDlgItemTextW(window, control, buffer.as_mut_ptr(), buffer.len() as i32)
            };
            ensure!(
                String::from_utf16_lossy(&buffer[..length as usize]) == expected,
                "Shortcut state label/action mismatch"
            );
        }
        if state == desktop::ShortcutState::OtherCopy {
            capture(window, &directory.join("settings-shortcut-update.bmp"))?;
        }
    }
    capture(window, &directory.join("settings.bmp"))?;
    unsafe {
        SendDlgItemMessageW(window, 155, TCM_SETCURSEL, 1, 0);
    }
    settings_tab(&dialogs.windows[0].0);
    capture(window, &directory.join("settings-recipe.bmp"))?;
    ensure!(
        unsafe { GetWindowLongPtrW(GetDlgItem(window, 150), GWL_STYLE) } as u32 & WS_VISIBLE != 0,
        "Recipe tab controls hidden"
    );
    ensure!(
        unsafe { GetWindowLongPtrW(GetDlgItem(window, 110), GWL_STYLE) } as u32 & WS_VISIBLE == 0,
        "General tab overlaps recipe controls"
    );
    unsafe {
        SendDlgItemMessageW(window, 155, TCM_SETCURSEL, 0, 0);
    }
    settings_tab(&dialogs.windows[0].0);
    ensure!(
        shortcut_text(&dialogs.windows[0].0) == "Alt+X",
        "Default recipe shortcut is not Alt+X"
    );
    ensure!(
        unsafe { SendDlgItemMessageW(window, 150, BM_GETCHECK, 0, 0) } == BST_CHECKED as isize,
        "Recipe tracker default changed"
    );
    ensure!(
        unsafe { SendDlgItemMessageW(window, 157, BM_GETCHECK, 0, 0) } == BST_CHECKED as isize,
        "Double-click pinning default changed"
    );
    let mut selected_label = [0u16; 128];
    let selected_length = unsafe {
        GetDlgItemTextW(window, 111, selected_label.as_mut_ptr(), selected_label.len() as i32)
    };
    ensure!(
        String::from_utf16_lossy(&selected_label[..selected_length as usize])
            == "Keep running in the tray",
        "Selected exit preference is not visible to Windows controls"
    );
    ensure!(
        unsafe { SendDlgItemMessageW(window, 156, CB_GETCURSEL, 0, 0) } == 0,
        "Default countdown format should show hours when needed"
    );
    // Diagnostic windows remain hidden when reused too.
    unsafe {
        SendDlgItemMessageW(window, 111, CB_SETCURSEL, 1, 0);
        SendDlgItemMessageW(window, 156, CB_SETCURSEL, 1, 0);
        SendDlgItemMessageW(window, 150, BM_SETCHECK, BST_UNCHECKED as usize, 0);
        SendMessageW(window, WM_COMMAND, 111 | ((CBN_SELCHANGE as usize) << 16), 0);
    }
    ensure!(
        unsafe { IsWindowEnabled(GetDlgItem(window, IDOK)) } != 0,
        "Edited settings did not enable Save"
    );
    unsafe {
        SendMessageW(window, WM_COMMAND, IDCANCEL as usize, 0);
    }
    ensure!(context.preferences.get() == Preferences::default(), "Cancel changed preferences");
    dialogs.open(SETTINGS, std::ptr::null_mut(), context.clone())?;
    let window = dialogs.windows[0].0.window.get();
    unsafe {
        SendDlgItemMessageW(window, 111, CB_SETCURSEL, 2, 0);
        SendDlgItemMessageW(window, 156, CB_SETCURSEL, 1, 0);
        text(window, 151, "Ctrl+Alt+J");
        SendDlgItemMessageW(window, 157, BM_SETCHECK, BST_UNCHECKED as usize, 0);
        SendDlgItemMessageW(window, 152, CB_SETCURSEL, 2, 0);
        SendDlgItemMessageW(window, 153, CB_SETCURSEL, 2, 0);
        SendMessageW(window, WM_COMMAND, IDOK as usize, 0);
    }
    ensure!(
        preferences::load(&context.preferences_path)?.game_exit == GameExit::Exit,
        "Save did not persist exit preference"
    );
    ensure!(
        context.preferences.get().game_exit == GameExit::Exit,
        "Save did not update running preferences"
    );
    ensure!(
        preferences::load(&context.preferences_path)?.countdown_format
            == starrupture_observation_model::wave::CountdownFormat::Minutes,
        "Save did not persist countdown format"
    );
    ensure!(
        context.preferences.get().countdown_format
            == starrupture_observation_model::wave::CountdownFormat::Minutes,
        "Save did not update countdown format"
    );
    let recipe = preferences::load(&context.preferences_path)?.recipe_tracker;
    ensure!(
        recipe.key == 0x4a
            && recipe.modifiers == 3
            && recipe.position == 2
            && recipe.scale == 125
            && !recipe.double_click_pin,
        "Recipe preferences did not round trip through native controls"
    );
    let blocked = directory.join("blocked");
    fs::create_dir_all(&blocked)?;
    let mut failure = context.clone();
    failure.preferences_path = blocked;
    dialogs.open(SETTINGS, std::ptr::null_mut(), failure)?;
    let window = dialogs.windows[0].0.window.get();
    unsafe {
        SendDlgItemMessageW(window, 111, CB_SETCURSEL, 1, 0);
        SendDlgItemMessageW(window, 156, CB_SETCURSEL, 1, 0);
        SendMessageW(window, WM_COMMAND, IDOK as usize, 0);
    }
    ensure!(unsafe { IsWindow(window) } != 0, "Save failure closed Settings");
    let mut value = [0u16; 512];
    ensure!(
        unsafe { GetDlgItemTextW(window, 114, value.as_mut_ptr(), value.len() as i32) } > 0,
        "Save failure did not show an error"
    );
    ensure!(
        context.preferences.get().game_exit == GameExit::Exit,
        "Failed save changed preferences"
    );
    unsafe {
        SendMessageW(window, WM_COMMAND, IDCANCEL as usize, 0);
    }
    context.connection.borrow_mut().update(crate::connection::GameStatus::Running {pid:123,creation_ticks:456,executable:r"D:\SteamLibrary\steamapps\common\StarRupture\StarRupture\Binaries\Win64\StarRuptureGameSteam-Win64-Shipping.exe".into(),supported:true});
    context.connection.borrow_mut().ready(123);
    dialogs.open(ABOUT, std::ptr::null_mut(), context.clone())?;
    let about =
        dialogs.windows.iter().find(|dialog| dialog.0.kind == ABOUT).unwrap().0.window.get();
    capture(about, &directory.join("about.bmp"))?;
    let mut icon_rect = RECT::default();
    unsafe {
        GetWindowRect(GetDlgItem(about, 143), &mut icon_rect);
    }
    let dpi = unsafe { windows_sys::Win32::UI::HiDpi::GetDpiForWindow(about) }.max(96);
    ensure!(
        icon_rect.right - icon_rect.left == (57 * dpi as i32 + 48) / 96,
        "About artwork is not sized for the dialog DPI"
    );
    crate::status::self_test(about)?;
    context.connection.borrow_mut().update(crate::connection::GameStatus::Running {
        pid: 123,
        creation_ticks: 456,
        executable: r"D:\SteamLibrary\StarRuptureGameSteam-Win64-Shipping.exe".into(),
        supported: false,
    });
    dialogs.refresh_about();
    capture(about, &directory.join("about-unsupported.bmp"))?;
    context.connection.borrow_mut().update(crate::connection::GameStatus::Waiting);
    dialogs.refresh_about();
    capture(about, &directory.join("about-waiting.bmp"))?;
    let length = unsafe { GetDlgItemTextW(about, 121, value.as_mut_ptr(), value.len() as i32) };
    ensure!(
        String::from_utf16_lossy(&value[..length as usize]) == "Waiting for StarRupture",
        "About did not follow connection changes"
    );
    dialogs.open(NOTICES, std::ptr::null_mut(), context.clone())?;
    dialogs.open(EXIT_PROMPT, std::ptr::null_mut(), context)?;
    let prompt =
        dialogs.windows.iter().find(|dialog| dialog.0.kind == EXIT_PROMPT).unwrap().0.window.get();
    capture(prompt, &directory.join("exit-prompt.bmp"))?;
    let handles: Vec<_> = dialogs.windows.iter().map(|dialog| dialog.0.window.get()).collect();
    drop(dialogs);
    ensure!(
        handles.iter().all(|window| unsafe { IsWindow(*window) } == 0),
        "Dialog resources survived close"
    );
    ensure!(
        unsafe { GetForegroundWindow() } == foreground,
        "Hidden diagnostics changed foreground focus"
    );
    println!(
        "Native dialog checks passed: defaults, Cancel, Save, persistence failure, connection refresh and cleanup. Startup, shortcuts, clipboard and game launch unchanged."
    );
    Ok(())
}

fn capture(window: HWND, path: &Path) -> Result<()> {
    use std::io::Write;
    use windows_sys::Win32::Graphics::Gdi::*;
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
    let mut rect = RECT::default();
    ensure!(unsafe { GetWindowRect(window, &mut rect) } != 0, "Could not measure dialog");
    struct OffscreenWindow(HWND, RECT);
    impl Drop for OffscreenWindow {
        fn drop(&mut self) {
            unsafe {
                ShowWindow(self.0, SW_HIDE);
                SetWindowPos(
                    self.0,
                    std::ptr::null_mut(),
                    self.1.left,
                    self.1.top,
                    0,
                    0,
                    SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOSIZE,
                );
            }
        }
    }
    let _visible = OffscreenWindow(window, rect);
    unsafe {
        SetWindowPos(
            window,
            std::ptr::null_mut(),
            -30000,
            -30000,
            0,
            0,
            SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOSIZE,
        );
        ShowWindow(window, SW_SHOWNOACTIVATE);
        RedrawWindow(
            window,
            std::ptr::null(),
            std::ptr::null_mut(),
            RDW_UPDATENOW | RDW_ALLCHILDREN | RDW_INVALIDATE,
        );
    }
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    let screen = unsafe { GetDC(std::ptr::null_mut()) };
    let dc = unsafe { CreateCompatibleDC(screen) };
    unsafe {
        ReleaseDC(std::ptr::null_mut(), screen);
    }
    ensure!(!dc.is_null(), "No dialog capture device");
    let mut surface = Surface {
        dc,
        bitmap: std::ptr::null_mut(),
        old: std::ptr::null_mut(),
    };
    let mut info = BITMAPINFO::default();
    info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    info.bmiHeader.biWidth = width;
    info.bmiHeader.biHeight = -height;
    info.bmiHeader.biPlanes = 1;
    info.bmiHeader.biBitCount = 32;
    let mut bits = std::ptr::null_mut();
    surface.bitmap =
        unsafe { CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0) };
    ensure!(!surface.bitmap.is_null() && !bits.is_null(), "No dialog capture bitmap");
    surface.old = unsafe { SelectObject(dc, surface.bitmap) };
    unsafe {
        SendMessageW(
            window,
            WM_PRINT,
            dc as usize,
            (PRF_CLIENT | PRF_NONCLIENT | PRF_CHILDREN | PRF_ERASEBKGND) as isize,
        );
        GdiFlush();
    }
    let pixels =
        unsafe { std::slice::from_raw_parts(bits.cast::<u8>(), (width * height * 4) as usize) };
    let mut header = Vec::new();
    header.extend(b"BM");
    header.extend((54 + pixels.len() as u32).to_le_bytes());
    header.extend([0u8; 4]);
    header.extend(54u32.to_le_bytes());
    header.extend(40u32.to_le_bytes());
    header.extend(width.to_le_bytes());
    header.extend((-height).to_le_bytes());
    header.extend(1u16.to_le_bytes());
    header.extend(32u16.to_le_bytes());
    header.extend([0u8; 24]);
    let mut file = fs::OpenOptions::new().create_new(true).write(true).open(path)?;
    file.write_all(&header)?;
    file.write_all(pixels)?;
    Ok(())
}
