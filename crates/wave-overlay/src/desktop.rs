//! User-scoped shell integration. These operations run only on explicit actions.
use anyhow::{Context, Result, bail, ensure};
use std::{
    ffi::{OsStr, OsString},
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
};
use windows::{
    Win32::{
        System::Com::{
            CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
            CoCreateInstance, CoInitializeEx, CoUninitialize, IPersistFile, STGM_READ,
        },
        UI::Shell::{IShellLinkW, ShellLink},
    },
    core::{Interface, PCWSTR},
};
use windows_sys::Win32::{
    Foundation::*,
    System::Registry::*,
    UI::{
        Shell::{FOLDERID_Desktop, SHGetKnownFolderPath, ShellExecuteW},
        WindowsAndMessaging::SW_SHOWNORMAL,
    },
};

pub fn wide(value: impl AsRef<OsStr>) -> Vec<u16> {
    value.as_ref().encode_wide().chain([0]).collect()
}
fn checked(code: u32) -> Result<()> {
    if code == 0 {
        Ok(())
    } else {
        Err(std::io::Error::from_raw_os_error(code as i32).into())
    }
}
struct Key(HKEY);
impl Drop for Key {
    fn drop(&mut self) {
        unsafe {
            RegCloseKey(self.0);
        }
    }
}
const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE: &str = "StarRuptureOverlay";
pub fn startup_value() -> Result<Option<Vec<u16>>> {
    let mut size = 0;
    let code = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            wide(RUN).as_ptr(),
            wide(VALUE).as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut size,
        )
    };
    if code == ERROR_FILE_NOT_FOUND {
        return Ok(None);
    }
    checked(code)?;
    ensure!(size <= 65536 && size % 2 == 0, "Invalid startup registration");
    let mut value = vec![0u16; size as usize / 2];
    checked(unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            wide(RUN).as_ptr(),
            wide(VALUE).as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            value.as_mut_ptr().cast(),
            &mut size,
        )
    })?;
    value.truncate(size as usize / 2);
    Ok(Some(value))
}
pub fn set_startup(value: Option<&[u16]>) -> Result<()> {
    let mut key = std::ptr::null_mut();
    checked(unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            wide(RUN).as_ptr(),
            0,
            std::ptr::null(),
            0,
            KEY_SET_VALUE,
            std::ptr::null(),
            &mut key,
            std::ptr::null_mut(),
        )
    })?;
    let key = Key(key);
    match value {
        Some(value) => checked(unsafe {
            RegSetValueExW(
                key.0,
                wide(VALUE).as_ptr(),
                0,
                REG_SZ,
                value.as_ptr().cast(),
                std::mem::size_of_val(value) as u32,
            )
        }),
        None => {
            let code = unsafe { RegDeleteValueW(key.0, wide(VALUE).as_ptr()) };
            if code == ERROR_FILE_NOT_FOUND {
                Ok(())
            } else {
                checked(code)
            }
        }
    }
}

/// Windows command-line quoting preserves embedded quotes and trailing slashes.
fn quote(value: &OsStr) -> Vec<u16> {
    let mut output = vec![b'"' as u16];
    let mut slashes = 0;
    for unit in value.encode_wide() {
        if unit == b'\\' as u16 {
            slashes += 1;
            continue;
        }
        if unit == b'"' as u16 {
            output.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2 + 1));
        } else {
            output.extend(std::iter::repeat_n(b'\\' as u16, slashes));
        }
        slashes = 0;
        output.push(unit);
    }
    output.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2));
    output.push(b'"' as u16);
    output
}
pub fn arguments(config: Option<&Path>, launch: bool) -> OsString {
    let mut output = Vec::new();
    if launch {
        output.extend("--launch-game".encode_utf16());
    }
    if let Some(path) = config {
        if !output.is_empty() {
            output.push(b' ' as u16);
        }
        output.extend("--config ".encode_utf16());
        output.extend(quote(path.as_os_str()));
    }
    OsString::from_wide(&output)
}
pub fn startup_command(config: Option<&Path>) -> Result<Vec<u16>> {
    let mut output = quote(std::env::current_exe()?.as_os_str());
    let args = arguments(config, false);
    if !args.is_empty() {
        output.push(b' ' as u16);
        output.extend(args.encode_wide());
    }
    ensure!(
        output.len() <= 260,
        "Startup command exceeds Windows' 260-character limit. Move the app or configuration to a shorter path."
    );
    output.push(0);
    Ok(output)
}

struct Com;
impl Com {
    fn initialize() -> Result<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE).ok()?;
        }
        Ok(Self)
    }
}
impl Drop for Com {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

pub fn shortcut_path() -> Result<PathBuf> {
    let mut raw = std::ptr::null_mut();
    let result =
        unsafe { SHGetKnownFolderPath(&FOLDERID_Desktop, 0, std::ptr::null_mut(), &mut raw) };
    ensure!(result >= 0, "Could not find the user's Desktop ({result:#x})");
    let mut size = 0;
    unsafe {
        while *raw.add(size) != 0 {
            size += 1;
        }
        let path = PathBuf::from(OsString::from_wide(std::slice::from_raw_parts(raw, size)));
        windows::Win32::System::Com::CoTaskMemFree(Some(raw.cast()));
        Ok(path.join("Play StarRupture with Overlay.lnk"))
    }
}
const SHORTCUT_DESCRIPTION: &str = "Start StarRupture with StarRupture Overlay";
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShortcutState {
    #[default]
    Missing,
    Current,
    OtherCopy,
    Other,
}
impl ShortcutState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Missing => "No shortcut created",
            Self::Current => "Desktop shortcut created",
            Self::OtherCopy => "Shortcut points to another copy",
            Self::Other => "A different shortcut uses this name",
        }
    }
    pub fn action(self) -> &'static str {
        match self {
            Self::Current => "Remove shortcut",
            Self::OtherCopy => "Update shortcut",
            _ => "Create shortcut",
        }
    }
}
fn shortcut_identity(
    target: &OsStr,
    args: &OsStr,
    description: &OsStr,
    executable: &Path,
    config: Option<&Path>,
) -> ShortcutState {
    if args != arguments(config, true) {
        return ShortcutState::Other;
    }
    if target.to_string_lossy().eq_ignore_ascii_case(&executable.to_string_lossy()) {
        return ShortcutState::Current;
    }
    let known_name = Path::new(target).file_name().is_some_and(|name| {
        ["starrupture-overlay.exe", "starrupture-wave-overlay.exe"]
            .iter()
            .any(|known| name.to_string_lossy().eq_ignore_ascii_case(known))
    });
    if known_name && description == OsStr::new(SHORTCUT_DESCRIPTION) {
        ShortcutState::OtherCopy
    } else {
        ShortcutState::Other
    }
}
pub fn shortcut_state(path: &Path, config: Option<&Path>) -> Result<ShortcutState> {
    if !path.try_exists()? {
        return Ok(ShortcutState::Missing);
    }
    let _com = Com::initialize()?;
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        let persist: IPersistFile = link.cast()?;
        persist.Load(PCWSTR(wide(path).as_ptr()), STGM_READ)?;
        let mut target = vec![0; 32768];
        let mut args = vec![0; 32768];
        let mut description = vec![0; 32768];
        link.GetPath(&mut target, std::ptr::null_mut(), 0)?;
        link.GetArguments(&mut args)?;
        link.GetDescription(&mut description)?;
        let string = |buffer: &[u16]| {
            OsString::from_wide(
                &buffer[..buffer.iter().position(|u| *u == 0).unwrap_or(buffer.len())],
            )
        };
        Ok(shortcut_identity(
            &string(&target),
            &string(&args),
            &string(&description),
            &std::env::current_exe()?,
            config,
        ))
    }
}
pub fn toggle_shortcut(config: Option<&Path>) -> Result<bool> {
    toggle_shortcut_at(&shortcut_path()?, config)
}
fn toggle_shortcut_at(path: &Path, config: Option<&Path>) -> Result<bool> {
    let state = shortcut_state(path, config)?;
    if state == ShortcutState::Current {
        std::fs::remove_file(path)?;
        return Ok(false);
    }
    ensure!(
        state != ShortcutState::Other,
        "A different shortcut already uses this name. Rename it before creating this shortcut."
    );
    let previous = if state == ShortcutState::OtherCopy {
        Some(std::fs::read(path)?)
    } else {
        None
    };
    let executable = std::env::current_exe()?;
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    ensure!(!temporary.try_exists()?, "A temporary shortcut already exists");
    let result = (|| -> Result<()> {
        save_shortcut(&temporary, &executable, config)?;
        let flags = if let Some(previous) = previous {
            ensure!(
                std::fs::read(path)? == previous,
                "Desktop shortcut changed; reopen Settings and try again."
            );
            windows_sys::Win32::Storage::FileSystem::MOVEFILE_REPLACE_EXISTING
        } else {
            0
        };
        ensure!(
            unsafe {
                windows_sys::Win32::Storage::FileSystem::MoveFileExW(
                    wide(&temporary).as_ptr(),
                    wide(path).as_ptr(),
                    flags,
                )
            } != 0,
            "Could not update shortcut: {}",
            std::io::Error::last_os_error()
        );
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temporary);
    }
    result?;
    Ok(true)
}
fn save_shortcut(path: &Path, executable: &Path, config: Option<&Path>) -> Result<()> {
    let _com = Com::initialize()?;
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        link.SetPath(PCWSTR(wide(executable).as_ptr()))?;
        link.SetArguments(PCWSTR(wide(arguments(config, true)).as_ptr()))?;
        link.SetWorkingDirectory(PCWSTR(
            wide(executable.parent().context("Application directory unavailable")?).as_ptr(),
        ))?;
        link.SetIconLocation(PCWSTR(wide(executable).as_ptr()), 0)?;
        link.SetDescription(PCWSTR(wide(SHORTCUT_DESCRIPTION).as_ptr()))?;
        let persist: IPersistFile = link.cast()?;
        persist.Save(PCWSTR(wide(path).as_ptr()), true)?;
    }
    Ok(())
}
pub fn open(owner: HWND, target: &str) -> Result<()> {
    let _com = Com::initialize()?;
    let result = unsafe {
        ShellExecuteW(
            owner,
            wide("open").as_ptr(),
            wide(target).as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    } as isize;
    if result <= 32 {
        bail!("Windows could not open the requested item (error {result})");
    }
    Ok(())
}
pub fn launch_game(owner: HWND) -> Result<()> {
    open(owner, "steam://rungameid/1631270").context(
        "Could not launch StarRupture through Steam. Check that Steam and the game are installed.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shell_link_round_trip_and_unrelated_file_protection() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../local/shortcut-tests")
            .join(std::process::id().to_string());
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("Play StarRupture with Overlay.lnk");
        let config = root.join("Config with spaces.json");
        assert!(toggle_shortcut_at(&path, Some(&config)).unwrap());
        assert_eq!(shortcut_state(&path, Some(&config)).unwrap(), ShortcutState::Current);
        assert_eq!(shortcut_state(&path, None).unwrap(), ShortcutState::Other);
        assert!(!toggle_shortcut_at(&path, Some(&config)).unwrap());
        assert!(!path.exists());
        std::fs::write(&path, b"Unrelated file").unwrap();
        assert!(toggle_shortcut_at(&path, Some(&config)).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"Unrelated file");
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
    #[test]
    fn older_and_missing_version_targets_can_be_updated_then_removed() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../local/shortcut-tests")
            .join(format!("upgrade-{}", std::process::id()));
        let old = root.join("old version");
        std::fs::create_dir_all(&old).unwrap();
        let path = root.join("Play StarRupture with Overlay.lnk");
        let config = root.join("Settings with spaces.json");
        for name in
            ["starrupture-wave-overlay.exe", "starrupture-overlay.exe", "STARRUPTURE-OVERLAY.EXE"]
        {
            let target = old.join(name);
            assert!(!target.exists());
            save_shortcut(&path, &target, Some(&config)).unwrap();
            assert_eq!(shortcut_state(&path, Some(&config)).unwrap(), ShortcutState::OtherCopy);
            let before = std::fs::read(&path).unwrap();
            // A failed replacement leaves the previous shortcut intact.
            let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
            std::fs::write(&temporary, b"already in use").unwrap();
            assert!(toggle_shortcut_at(&path, Some(&config)).is_err());
            assert_eq!(std::fs::read(&path).unwrap(), before);
            assert_eq!(std::fs::read(&temporary).unwrap(), b"already in use");
            std::fs::remove_file(temporary).unwrap();
            // First click updates; it must not remove the link as a toggle would.
            assert!(toggle_shortcut_at(&path, Some(&config)).unwrap());
            assert!(path.exists());
            assert_eq!(shortcut_state(&path, Some(&config)).unwrap(), ShortcutState::Current);
            assert!(!toggle_shortcut_at(&path, Some(&config)).unwrap());
            assert_eq!(shortcut_state(&path, Some(&config)).unwrap(), ShortcutState::Missing);
        }
        std::fs::remove_dir(old).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
    #[test]
    fn unrelated_links_and_other_configuration_profiles_are_preserved() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../local/shortcut-tests")
            .join(format!("foreign-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("Play StarRupture with Overlay.lnk");
        let target = root.join("starrupture-overlay.exe");
        let config = root.join("different.json");
        save_shortcut(&path, &target, Some(&config)).unwrap();
        let before = std::fs::read(&path).unwrap();
        assert_eq!(shortcut_state(&path, None).unwrap(), ShortcutState::Other);
        assert!(toggle_shortcut_at(&path, None).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        save_shortcut(&path, &target, None).unwrap();
        let _com = Com::initialize().unwrap();
        unsafe {
            let link: IShellLinkW =
                CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).unwrap();
            let persist: IPersistFile = link.cast().unwrap();
            persist.Load(PCWSTR(wide(&path).as_ptr()), STGM_READ).unwrap();
            link.SetDescription(PCWSTR(wide("Unrelated shortcut").as_ptr())).unwrap();
            persist.Save(PCWSTR(wide(&path).as_ptr()), true).unwrap();
        }
        let before = std::fs::read(&path).unwrap();
        assert_eq!(shortcut_state(&path, None).unwrap(), ShortcutState::Other);
        assert!(toggle_shortcut_at(&path, None).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
    #[test]
    fn startup_never_launches_the_game_and_quotes_configuration_paths() {
        let path = Path::new(r"D:\My config\overlay.json");
        assert_eq!(
            arguments(Some(path), false),
            OsString::from(r#"--config "D:\My config\overlay.json""#)
        );
        assert_eq!(arguments(None, true), OsString::from("--launch-game"));
        assert!(
            arguments(Some(path), true).to_string_lossy().starts_with("--launch-game --config ")
        );
        assert_eq!(
            OsString::from_wide(&quote(OsStr::new("ends\\"))),
            OsString::from("\"ends\\\\\"")
        );
        assert_eq!(OsString::from_wide(&quote(OsStr::new("a\"b"))), OsString::from("\"a\\\"b\""));
    }
}
