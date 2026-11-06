//! Native checks and captures using owned windows and isolated preferences.
use super::*;

/// Exercise owned dialogs and isolated preference files without shell actions or game access.
pub fn self_test(directory: &Path) -> Result<()> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::IsWindowEnabled;
    fs::create_dir_all(directory)?;
    // SAFETY: These Win32 queries use scalar values or borrowed handles, validate native handle
    // identity themselves, and neither dereference Rust memory nor transfer ownership.
    let foreground = unsafe { GetForegroundWindow() };
    let context = Context::diagnostic(directory.join("preferences.json"));
    let mut dialogs = Dialogs::default();
    dialogs.open(SETTINGS, std::ptr::null_mut(), context.clone())?;
    let window = dialogs.windows[0].0.window.get();
    dialogs.open(SETTINGS, std::ptr::null_mut(), context.clone())?;
    ensure!(dialogs.windows.len() == 1, "Settings opened more than one window");
    ensure!(
        // SAFETY: The UI/fixture owns the dialog and its resource-template child handles; only
        // scalar enabled/theme/invalidation state is changed or queried, with no Rust pointer
        // retained.
        unsafe { IsWindowEnabled(GetDlgItem(window, IDOK)) } == 0,
        "Save should start disabled"
    );
    ensure!(
        // SAFETY: These synchronous messages target owned UI controls/windows; scalar parameters or
        // initialized text/message buffers remain live until the call returns, with no retained
        // Rust pointers.
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
            // SAFETY: These synchronous messages target owned UI controls/windows; scalar
            // parameters or initialized text/message buffers remain live until the call returns,
            // with no retained Rust pointers.
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
    ensure!(selected(&dialogs.windows[0].0).compass_enabled, "Compass should default on");
    // SAFETY: The diagnostic owns the dialog and checkbox and sends only scalar control messages.
    unsafe {
        SendDlgItemMessageW(window, 174, BM_SETCHECK, BST_UNCHECKED as usize, 0);
        SendMessageW(window, WM_COMMAND, 174, 0);
    }
    ensure!(
        !selected(&dialogs.windows[0].0).compass_enabled,
        "Compass checkbox did not update selection"
    );
    // SAFETY: IsWindowEnabled queries only the diagnostic's owned Save control.
    ensure!(
        // SAFETY: This query uses only the diagnostic-owned Save control.
        unsafe { IsWindowEnabled(GetDlgItem(window, IDOK)) } != 0,
        "Compass edit did not enable Save"
    );
    // SAFETY: Restore the owned checkbox using synchronous scalar messages.
    unsafe {
        SendDlgItemMessageW(window, 174, BM_SETCHECK, BST_CHECKED as usize, 0);
        SendMessageW(window, WM_COMMAND, 174, 0);
    }
    check_feature_tabs(&dialogs.windows[0].0, directory)?;
    capture(window, &directory.join("settings.bmp"))?;
    let mut general = RECT::default();
    // SAFETY: Win32 validates the borrowed handle where required; every output points to an
    // initialized local struct or bounded writable slice that stays live for the synchronous query.
    unsafe {
        GetClientRect(window, &mut general);
    }
    // SAFETY: These synchronous messages target owned UI controls/windows; scalar parameters or
    // initialized text/message buffers remain live until the call returns, with no retained Rust
    // pointers.
    unsafe {
        SendDlgItemMessageW(window, 155, TCM_SETCURSEL, 3, 0);
    }
    settings_tab(&dialogs.windows[0].0);
    capture(window, &directory.join("settings-recipe.bmp"))?;
    let before = selected(&dialogs.windows[0].0).recipe_tracker;
    // SAFETY: These synchronous messages target owned UI controls/windows; scalar parameters or
    // initialized text/message buffers remain live until the call returns, with no retained Rust
    // pointers.
    unsafe {
        SendDlgItemMessageW(window, 150, BM_SETCHECK, BST_UNCHECKED as usize, 0);
        SendMessageW(window, WM_COMMAND, 150, 0);
    }
    for id in RECIPE_FIELDS {
        ensure!(
            // SAFETY: The UI/fixture owns the dialog and its resource-template child handles; only
            // scalar enabled/theme/invalidation state is changed or queried, with no Rust pointer
            // retained.
            unsafe { IsWindowEnabled(GetDlgItem(window, id)) } == 0,
            "Disabled recipe control remained active: {id}"
        );
    }
    ensure!(
        // SAFETY: The UI/fixture owns the dialog and its resource-template child handles; only
        // scalar enabled/theme/invalidation state is changed or queried, with no Rust pointer
        // retained.
        unsafe { IsWindowEnabled(GetDlgItem(window, 150)) } != 0,
        "Recipe enable switch disabled itself"
    );
    ensure!(
        // SAFETY: The UI/fixture owns the dialog and its resource-template child handles; only
        // scalar enabled/theme/invalidation state is changed or queried, with no Rust pointer
        // retained.
        // SAFETY: This query uses only the diagnostic-owned Save control.
        unsafe { IsWindowEnabled(GetDlgItem(window, IDOK)) } != 0,
        "Disabling recipes did not enable Save"
    );
    let mut expected = before;
    expected.enabled = false;
    ensure!(
        selected(&dialogs.windows[0].0).recipe_tracker == expected,
        "Disabling recipes changed saved options"
    );
    capture(window, &directory.join("settings-recipe-disabled.bmp"))?;
    // SAFETY: These synchronous messages target owned UI controls/windows; scalar parameters or
    // initialized text/message buffers remain live until the call returns, with no retained Rust
    // pointers.
    unsafe {
        SendDlgItemMessageW(window, 150, BM_SETCHECK, BST_CHECKED as usize, 0);
        SendMessageW(window, WM_COMMAND, 150, 0);
    }
    for id in RECIPE_FIELDS {
        ensure!(
            // SAFETY: The UI/fixture owns the dialog and its resource-template child handles; only
            // scalar enabled/theme/invalidation state is changed or queried, with no Rust pointer
            // retained.
            unsafe { IsWindowEnabled(GetDlgItem(window, id)) } != 0,
            "Re-enabled recipe control stayed disabled: {id}"
        );
    }
    ensure!(
        selected(&dialogs.windows[0].0).recipe_tracker == before,
        "Re-enabling recipes lost options"
    );
    ensure!(
        // SAFETY: The UI/fixture owns the dialog and its resource-template child handles; only
        // scalar enabled/theme/invalidation state is changed or queried, with no Rust pointer
        // retained.
        unsafe { IsWindowEnabled(GetDlgItem(window, IDOK)) } == 0,
        "Restoring enable state left Save dirty"
    );
    settings_message(&dialogs.windows[0].0, "Choose a shortcut including Alt or Ctrl.");
    let mut message_size = RECT::default();
    // SAFETY: Win32 validates the borrowed handle where required; every output points to an
    // initialized local struct or bounded writable slice that stays live for the synchronous query.
    unsafe {
        GetClientRect(window, &mut message_size);
    }
    capture(window, &directory.join("settings-recipe-message.bmp"))?;
    settings_message(&dialogs.windows[0].0, "");
    let mut restored_size = RECT::default();
    // SAFETY: Win32 validates the borrowed handle where required; every output points to an
    // initialized local struct or bounded writable slice that stays live for the synchronous query.
    unsafe {
        GetClientRect(window, &mut restored_size);
    }
    ensure!(message_size.bottom > restored_size.bottom, "Messages did not receive dedicated space");
    let mut compact = RECT::default();
    // SAFETY: Win32 validates the borrowed handle where required; every output points to an
    // initialized local struct or bounded writable slice that stays live for the synchronous query.
    unsafe {
        GetClientRect(window, &mut compact);
    }
    ensure!(compact.right == general.right, "Tab changes altered the settings width");
    *context.recipe_status.borrow_mut() =
        "Preparing recipe icons. You can keep playing while setup finishes.".into();
    dialogs.refresh_recipe_status();
    let mut with_status = RECT::default();
    // SAFETY: Win32 validates the borrowed handle where required; every output points to an
    // initialized local struct or bounded writable slice that stays live for the synchronous query.
    unsafe {
        GetClientRect(window, &mut with_status);
    }
    let mut maximum_growth = RECT {
        bottom: 40,
        ..Default::default()
    };
    // SAFETY: The dialog converts the initialized local rectangle in place.
    unsafe {
        MapDialogRect(window, &mut maximum_growth);
    }
    ensure!(
        with_status.bottom > compact.bottom
            && with_status.bottom <= compact.bottom + maximum_growth.bottom,
        "Recipe setup status layout is not bounded"
    );
    capture(window, &directory.join("settings-recipe-status.bmp"))?;
    context.recipe_status.borrow_mut().clear();
    dialogs.refresh_recipe_status();
    // SAFETY: Win32 validates the borrowed handle where required; every output points to an
    // initialized local struct or bounded writable slice that stays live for the synchronous query.
    unsafe {
        GetClientRect(window, &mut with_status);
    }
    ensure!(with_status.bottom == compact.bottom, "Empty setup status left blank space");
    ensure!(
        // SAFETY: These Win32 queries use scalar values or borrowed handles, validate native handle
        // identity themselves, and neither dereference Rust memory nor transfer ownership.
        unsafe { GetWindowLongPtrW(GetDlgItem(window, 150), GWL_STYLE) } as u32 & WS_VISIBLE != 0,
        "Recipe tab controls hidden"
    );
    ensure!(
        // SAFETY: These Win32 queries use scalar values or borrowed handles, validate native handle
        // identity themselves, and neither dereference Rust memory nor transfer ownership.
        unsafe { GetWindowLongPtrW(GetDlgItem(window, 110), GWL_STYLE) } as u32 & WS_VISIBLE == 0,
        "General tab overlaps recipe controls"
    );
    // SAFETY: These synchronous messages target owned UI controls/windows; scalar parameters or
    // initialized text/message buffers remain live until the call returns, with no retained Rust
    // pointers.
    unsafe {
        SendDlgItemMessageW(window, 155, TCM_SETCURSEL, 0, 0);
    }
    settings_tab(&dialogs.windows[0].0);
    ensure!(
        shortcut_text(&dialogs.windows[0].0) == "Alt+X",
        "Default recipe shortcut is not Alt+X"
    );
    ensure!(
        // SAFETY: These synchronous messages target owned UI controls/windows; scalar parameters or
        // initialized text/message buffers remain live until the call returns, with no retained
        // Rust pointers.
        unsafe { SendDlgItemMessageW(window, 150, BM_GETCHECK, 0, 0) } == BST_CHECKED as isize,
        "Recipe tracker default changed"
    );
    ensure!(
        // SAFETY: These synchronous messages target owned UI controls/windows; scalar parameters or
        // initialized text/message buffers remain live until the call returns, with no retained
        // Rust pointers.
        unsafe { SendDlgItemMessageW(window, 157, BM_GETCHECK, 0, 0) } == BST_CHECKED as isize,
        "Double-click pinning default changed"
    );
    // Exercise the native control using only this diagnostic thread's keyboard state
    // and messages to our owned child HWND; no desktop input is injected.
    {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
        struct Keyboard([u8; 256]);
        impl Drop for Keyboard {
            fn drop(&mut self) {
                // SAFETY: The keyboard-state array has exactly 256 initialized bytes; these calls
                // affect only this diagnostic thread, and the guard restores the saved state before
                // leaving the fixture.
                unsafe {
                    SetKeyboardState(self.0.as_ptr());
                }
            }
        }
        let mut saved = Keyboard([0; 256]);
        ensure!(
            // SAFETY: The keyboard-state array has exactly 256 initialized bytes; these calls
            // affect only this diagnostic thread, and the guard restores the saved state before
            // leaving the fixture.
            unsafe { GetKeyboardState(saved.0.as_mut_ptr()) } != 0,
            "Keyboard state unavailable"
        );
        for (key, modifiers) in [(0x4a, 3), (0x78, 6), (0x31, 1)] {
            let mut keys = [0u8; 256];
            for (flag, virtual_key) in [(1, VK_MENU), (2, VK_CONTROL), (4, VK_SHIFT)] {
                if modifiers & flag != 0 {
                    keys[virtual_key as usize] = 0x80;
                }
            }
            // SAFETY: The keyboard-state array has exactly 256 initialized bytes; these calls
            // affect only this diagnostic thread, and the guard restores the saved state before
            // leaving the fixture.
            ensure!(unsafe { SetKeyboardState(keys.as_ptr()) } != 0, "Diagnostic keyboard state");
            // SAFETY: These synchronous messages target owned UI controls/windows; scalar
            // parameters or initialized text/message buffers remain live until the call returns,
            // with no retained Rust pointers.
            unsafe {
                SendDlgItemMessageW(window, 151, HKM_SETHOTKEY, 0, 0);
                SendDlgItemMessageW(window, 151, WM_KEYDOWN, key as usize, 1);
                SendDlgItemMessageW(window, 151, WM_KEYUP, key as usize, 1);
            }
            ensure!(
                preferences::parse_shortcut(&shortcut_text(&dialogs.windows[0].0))?
                    == (key, modifiers),
                "Native keypress capture lost a modifier or key"
            );
        }
        drop(saved);
        set_shortcut(
            window,
            preferences::RecipeSettings {
                key: 0x7b,
                modifiers: 2,
                ..Default::default()
            },
        );
        ensure!(save_settings(&dialogs.windows[0].0).is_err(), "Reserved F12 shortcut was saved");
        set_shortcut(
            window,
            preferences::RecipeSettings {
                key: 0x41,
                modifiers: 0,
                ..Default::default()
            },
        );
        ensure!(
            save_settings(&dialogs.windows[0].0).is_err(),
            "Shortcut without Alt/Ctrl was saved"
        );
        for key in [0x61, 0x6b] {
            set_shortcut(
                window,
                preferences::RecipeSettings {
                    key,
                    modifiers: 2,
                    ..Default::default()
                },
            );
            ensure!(
                captured_shortcut(&dialogs.windows[0].0).key == key,
                "Captured virtual key was remapped"
            );
            ensure!(
                save_settings(&dialogs.windows[0].0).is_err(),
                "Unsupported numpad key was saved as a letter"
            );
            ensure!(
                selected(&dialogs.windows[0].0).recipe_tracker.key == 0x58,
                "Invalid capture replaced the previous key"
            );
        }
        ensure!(
            context.preferences.get() == Preferences::default(),
            "Invalid capture changed preferences"
        );
        // SAFETY: These synchronous messages target owned UI controls/windows; scalar parameters or
        // initialized text/message buffers remain live until the call returns, with no retained
        // Rust pointers.
        unsafe {
            SendDlgItemMessageW(window, 150, BM_SETCHECK, BST_UNCHECKED as usize, 0);
            SendMessageW(window, WM_COMMAND, 150, 0);
        }
        save_settings(&dialogs.windows[0].0)?;
        let disabled = preferences::load(&context.preferences_path)?.recipe_tracker;
        ensure!(
            !disabled.enabled && disabled.key == 0x58 && disabled.modifiers == 1,
            "Disabling with an invalid capture did not preserve the valid shortcut"
        );
        context.preferences.set(Preferences::default());
        // SAFETY: These synchronous messages target owned UI controls/windows; scalar parameters or
        // initialized text/message buffers remain live until the call returns, with no retained
        // Rust pointers.
        unsafe {
            SendDlgItemMessageW(window, 150, BM_SETCHECK, BST_CHECKED as usize, 0);
            SendMessageW(window, WM_COMMAND, 150, 0);
        }
        set_shortcut(window, preferences::RecipeSettings::default());
        dirty(&dialogs.windows[0].0);
        ensure!(
            // SAFETY: The UI/fixture owns the dialog and its resource-template child handles; only
            // scalar enabled/theme/invalidation state is changed or queried, with no Rust pointer
            // retained.
            unsafe { IsWindowEnabled(GetDlgItem(window, IDOK)) } == 0,
            "Restoring the shortcut did not clear dirty state"
        );
    }
    let mut selected_label = [0u16; 128];
    // SAFETY: These synchronous messages target owned UI controls/windows; scalar parameters or
    // initialized text/message buffers remain live until the call returns, with no retained Rust
    // pointers.
    let selected_length = unsafe {
        GetDlgItemTextW(window, 111, selected_label.as_mut_ptr(), selected_label.len() as i32)
    };
    ensure!(
        String::from_utf16_lossy(&selected_label[..selected_length as usize])
            == "Keep running in the tray",
        "Selected exit preference is not visible to Windows controls"
    );
    ensure!(
        // SAFETY: These synchronous messages target owned UI controls/windows; scalar parameters or
        // initialized text/message buffers remain live until the call returns, with no retained
        // Rust pointers.
        unsafe { SendDlgItemMessageW(window, 156, CB_GETCURSEL, 0, 0) } == 0,
        "Default countdown format should show hours when needed"
    );
    // Diagnostic windows remain hidden when reused too.
    // SAFETY: These synchronous messages target owned UI controls/windows; scalar parameters or
    // initialized text/message buffers remain live until the call returns, with no retained Rust
    // pointers.
    unsafe {
        SendDlgItemMessageW(window, 111, CB_SETCURSEL, 1, 0);
        SendDlgItemMessageW(window, 156, CB_SETCURSEL, 1, 0);
        SendDlgItemMessageW(window, 150, BM_SETCHECK, BST_UNCHECKED as usize, 0);
        SendMessageW(window, WM_COMMAND, 111 | ((CBN_SELCHANGE as usize) << 16), 0);
    }
    ensure!(
        // SAFETY: The UI/fixture owns the dialog and its resource-template child handles; only
        // scalar enabled/theme/invalidation state is changed or queried, with no Rust pointer
        // retained.
        // SAFETY: This query uses only the diagnostic-owned Save control.
        unsafe { IsWindowEnabled(GetDlgItem(window, IDOK)) } != 0,
        "Edited settings did not enable Save"
    );
    // SAFETY: These synchronous messages target owned UI controls/windows; scalar parameters or
    // initialized text/message buffers remain live until the call returns, with no retained Rust
    // pointers.
    unsafe {
        SendMessageW(window, WM_COMMAND, IDCANCEL as usize, 0);
    }
    ensure!(context.preferences.get() == Preferences::default(), "Cancel changed preferences");
    dialogs.open(SETTINGS, std::ptr::null_mut(), context.clone())?;
    let window = dialogs.windows[0].0.window.get();
    // SAFETY: These synchronous messages target owned UI controls/windows; scalar parameters or
    // initialized text/message buffers remain live until the call returns, with no retained Rust
    // pointers.
    unsafe {
        SendDlgItemMessageW(window, 111, CB_SETCURSEL, 2, 0);
        SendDlgItemMessageW(window, 156, CB_SETCURSEL, 1, 0);
        set_shortcut(
            window,
            preferences::RecipeSettings {
                key: 0x4a,
                modifiers: 3,
                ..Default::default()
            },
        );
        SendDlgItemMessageW(window, 157, BM_SETCHECK, BST_UNCHECKED as usize, 0);
        SendDlgItemMessageW(window, 152, CB_SETCURSEL, 2, 0);
        SendDlgItemMessageW(window, 153, CB_SETCURSEL, 2, 0);
        text(window, 183, "50");
        text(window, 185, "65");
        SendMessageW(window, WM_COMMAND, IDOK as usize, 0);
    }
    ensure!(
        preferences::load(&context.preferences_path)?.compass.arrival_metres == 50
            && context.preferences.get().compass.departure_metres == 65,
        "Arrival distances did not persist through native controls"
    );
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
    let mut disabled = context.preferences.get();
    disabled.recipe_tracker.enabled = false;
    context.preferences.set(disabled);
    dialogs.open(SETTINGS, std::ptr::null_mut(), context.clone())?;
    let disabled_window = dialogs.windows[0].0.window.get();
    for id in RECIPE_FIELDS {
        ensure!(
            // SAFETY: The UI/fixture owns the dialog and its resource-template child handles; only
            // scalar enabled/theme/invalidation state is changed or queried, with no Rust pointer
            // retained.
            unsafe { IsWindowEnabled(GetDlgItem(disabled_window, id)) } == 0,
            "Initially disabled tracker field is active: {id}"
        );
    }
    // SAFETY: These synchronous messages target owned UI controls/windows; scalar parameters or
    // initialized text/message buffers remain live until the call returns, with no retained Rust
    // pointers.
    unsafe {
        SendMessageW(disabled_window, WM_COMMAND, IDCANCEL as usize, 0);
    }
    ensure!(!context.preferences.get().recipe_tracker.enabled, "Cancel enabled the tracker");
    let blocked = directory.join("blocked");
    fs::create_dir_all(&blocked)?;
    let mut failure = context.clone();
    failure.preferences_path = blocked;
    dialogs.open(SETTINGS, std::ptr::null_mut(), failure)?;
    let window = dialogs.windows[0].0.window.get();
    // SAFETY: These synchronous messages target owned UI controls/windows; scalar parameters or
    // initialized text/message buffers remain live until the call returns, with no retained Rust
    // pointers.
    unsafe {
        SendDlgItemMessageW(window, 111, CB_SETCURSEL, 1, 0);
        SendDlgItemMessageW(window, 156, CB_SETCURSEL, 1, 0);
        SendMessageW(window, WM_COMMAND, IDOK as usize, 0);
    }
    // SAFETY: These Win32 queries use scalar values or borrowed handles, validate native handle
    // identity themselves, and neither dereference Rust memory nor transfer ownership.
    ensure!(unsafe { IsWindow(window) } != 0, "Save failure closed Settings");
    let mut value = [0u16; 512];
    ensure!(
        // SAFETY: These synchronous messages target owned UI controls/windows; scalar parameters or
        // initialized text/message buffers remain live until the call returns, with no retained
        // Rust pointers.
        unsafe { GetDlgItemTextW(window, 114, value.as_mut_ptr(), value.len() as i32) } > 0,
        "Save failure did not show an error"
    );
    ensure!(
        context.preferences.get().game_exit == GameExit::Exit,
        "Failed save changed preferences"
    );
    // SAFETY: These synchronous messages target owned UI controls/windows; scalar parameters or
    // initialized text/message buffers remain live until the call returns, with no retained Rust
    // pointers.
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
    // SAFETY: Win32 validates the borrowed handle where required; every output points to an
    // initialized local struct or bounded writable slice that stays live for the synchronous query.
    unsafe {
        GetWindowRect(GetDlgItem(about, 143), &mut icon_rect);
    }
    // SAFETY: These Win32 queries use scalar values or borrowed handles, validate native handle
    // identity themselves, and neither dereference Rust memory nor transfer ownership.
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
    // SAFETY: These synchronous messages target owned UI controls/windows; scalar parameters or
    // initialized text/message buffers remain live until the call returns, with no retained Rust
    // pointers.
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
        // SAFETY: These Win32 queries use scalar values or borrowed handles, validate native handle
        // identity themselves, and neither dereference Rust memory nor transfer ownership.
        handles.iter().all(|window| unsafe { IsWindow(*window) } == 0),
        "Dialog resources survived close"
    );
    ensure!(
        // SAFETY: These Win32 queries use scalar values or borrowed handles, validate native handle
        // identity themselves, and neither dereference Rust memory nor transfer ownership.
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
            // SAFETY: The drawing DC is live on this UI thread; local text/geometry buffers match
            // their lengths. Owned GDI objects are restored/deselected before deletion;
            // stock/system objects remain borrowed.
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
    // SAFETY: Win32 validates the borrowed handle where required; every output points to an
    // initialized local struct or bounded writable slice that stays live for the synchronous query.
    ensure!(unsafe { GetWindowRect(window, &mut rect) } != 0, "Could not measure dialog");
    struct OffscreenWindow(HWND, RECT);
    impl Drop for OffscreenWindow {
        fn drop(&mut self) {
            // SAFETY: The UI thread uses owned overlay/menu HWNDs and borrowed comparison handles;
            // native position/style values are scalar, and referenced menu/state objects remain
            // alive through synchronous dispatch.
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
    // SAFETY: The UI thread uses owned overlay/menu HWNDs and borrowed comparison handles; native
    // position/style values are scalar, and referenced menu/state objects remain alive through
    // synchronous dispatch.
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
    // SAFETY: The HWND is borrowed (or null for the screen); each acquired DC is released with the
    // same target and is not deleted as an owned compatible DC.
    let screen = unsafe { GetDC(std::ptr::null_mut()) };
    // SAFETY: Win32 accepts the borrowed source DC or null; the returned compatible DC is owned
    // here and is released with DeleteDC after selected objects are restored.
    let dc = unsafe { CreateCompatibleDC(screen) };
    // SAFETY: The HWND is borrowed (or null for the screen); each acquired DC is released with the
    // same target and is not deleted as an owned compatible DC.
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
        // SAFETY: BITMAPINFO has the required initialized header and bounded dimensions; the
        // writable bits out-pointer is local. The returned bitmap is owned and remains alive until
        // it is deselected and deleted.
        unsafe { CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0) };
    ensure!(!surface.bitmap.is_null() && !bits.is_null(), "No dialog capture bitmap");
    // SAFETY: The drawing DC is live on this UI thread; local text/geometry buffers match their
    // lengths. Owned GDI objects are restored/deselected before deletion; stock/system objects
    // remain borrowed.
    surface.old = unsafe { SelectObject(dc, surface.bitmap) };
    // SAFETY: These synchronous messages target owned UI controls/windows; scalar parameters or
    // initialized text/message buffers remain live until the call returns, with no retained Rust
    // pointers.
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
        // SAFETY: The owned DIB exposes initialized width * height * 4 bytes; its bitmap remains
        // selected and alive through this UI-thread-only slice access. GdiFlush finishes native
        // writes first.
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

fn check_feature_tabs(state: &State, directory: &Path) -> Result<()> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::IsWindowEnabled;
    let window = state.window.get();
    // SAFETY: Only the diagnostic-owned tab and checkbox controls are synchronously queried/changed.
    unsafe {
        ensure!(
            SendDlgItemMessageW(window, 155, TCM_GETITEMCOUNT, 0, 0) == 4,
            "Expected four settings tabs"
        );
        for (tab, toggle, fields, name) in [
            (1, 180, &[170, 156][..], "wave"),
            (2, 174, &[175, 181, 182, 183, 184, 185, 186][..], "compass"),
        ] {
            SendDlgItemMessageW(window, 155, TCM_SETCURSEL, tab, 0);
            settings_tab(state);
            capture(window, &directory.join(format!("settings-{name}.bmp")))?;
            SendDlgItemMessageW(window, toggle, BM_SETCHECK, BST_UNCHECKED as usize, 0);
            SendMessageW(window, WM_COMMAND, toggle as usize, 0);
            for id in fields {
                ensure!(
                    IsWindowEnabled(GetDlgItem(window, *id)) == 0,
                    "Disabled feature control remained enabled: {id}"
                );
            }
            ensure!(
                IsWindowEnabled(GetDlgItem(window, toggle)) != 0,
                "Feature toggle disabled itself"
            );
            SendDlgItemMessageW(window, toggle, BM_SETCHECK, BST_CHECKED as usize, 0);
            SendMessageW(window, WM_COMMAND, toggle as usize, 0);
            for id in fields {
                ensure!(
                    IsWindowEnabled(GetDlgItem(window, *id)) != 0,
                    "Enabled feature control stayed disabled: {id}"
                );
            }
        }
    }
    ensure!(
        selected(state).compass == preferences::CompassSettings::default(),
        "Arrival defaults changed"
    );
    text(window, 183, "50");
    text(window, 185, "40");
    ensure!(save_settings(state).is_err(), "Reversed arrival distances were saved");
    ensure!(!state.context.preferences_path.exists(), "Invalid arrival changed preferences");
    text(window, 183, "25");
    text(window, 185, "35");
    // SAFETY: The diagnostic owns these native controls; all parameters are scalars.
    unsafe {
        SendDlgItemMessageW(window, 181, BM_SETCHECK, BST_UNCHECKED as usize, 0);
        SendMessageW(window, WM_COMMAND, 181, 0);
        ensure!(
            IsWindowEnabled(GetDlgItem(window, 183)) == 0,
            "Arrival toggle did not disable distances"
        );
        SendDlgItemMessageW(window, 181, BM_SETCHECK, BST_CHECKED as usize, 0);
        SendMessageW(window, WM_COMMAND, 181, 0);
        SendDlgItemMessageW(window, 155, TCM_SETCURSEL, 0, 0);
    }
    settings_tab(state);
    dirty(state);
    Ok(())
}
