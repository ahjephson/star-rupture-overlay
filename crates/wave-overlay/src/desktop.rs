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
pub fn shortcut_owned(path: &Path, config: Option<&Path>) -> Result<bool> {
    if !path.try_exists()? {
        return Ok(false);
    }
    let _com = Com::initialize()?;
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        let persist: IPersistFile = link.cast()?;
        persist.Load(PCWSTR(wide(path).as_ptr()), STGM_READ)?;
        let mut target = vec![0; 32768];
        let mut args = vec![0; 32768];
        link.GetPath(&mut target, std::ptr::null_mut(), 0)?;
        link.GetArguments(&mut args)?;
        let target = &target[..target.iter().position(|u| *u == 0).unwrap_or(target.len())];
        let args = &args[..args.iter().position(|u| *u == 0).unwrap_or(args.len())];
        Ok(OsString::from_wide(target) == std::env::current_exe()?.as_os_str()
            && OsString::from_wide(args) == arguments(config, true))
    }
}
pub fn toggle_shortcut(config: Option<&Path>) -> Result<bool> {
    let path = shortcut_path()?;
    toggle_shortcut_at(&path, config)
}
fn toggle_shortcut_at(path: &Path, config: Option<&Path>) -> Result<bool> {
    if path.try_exists()? {
        ensure!(
            shortcut_owned(path, config)?,
            "A different shortcut already uses this name. Rename it before creating this shortcut."
        );
        std::fs::remove_file(path)?;
        return Ok(false);
    }
    let _com = Com::initialize()?;
    let executable = std::env::current_exe()?;
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    ensure!(!temporary.try_exists()?, "A temporary shortcut already exists");
    let result = (|| -> Result<()> {
        unsafe {
            let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
            link.SetPath(PCWSTR(wide(&executable).as_ptr()))?;
            link.SetArguments(PCWSTR(wide(arguments(config, true)).as_ptr()))?;
            link.SetWorkingDirectory(PCWSTR(
                wide(executable.parent().context("Application directory unavailable")?).as_ptr(),
            ))?;
            link.SetIconLocation(PCWSTR(wide(&executable).as_ptr()), 0)?;
            link.SetDescription(PCWSTR(
                wide("Start StarRupture with StarRupture Overlay").as_ptr(),
            ))?;
            let persist: IPersistFile = link.cast()?;
            persist.Save(PCWSTR(wide(&temporary).as_ptr()), true)?;
            ensure!(
                windows_sys::Win32::Storage::FileSystem::MoveFileExW(
                    wide(&temporary).as_ptr(),
                    wide(path).as_ptr(),
                    0
                ) != 0,
                "Could not create shortcut: {}",
                std::io::Error::last_os_error()
            );
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temporary);
    }
    result?;
    Ok(true)
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
        assert!(shortcut_owned(&path, Some(&config)).unwrap());
        assert!(!shortcut_owned(&path, None).unwrap());
        assert!(!toggle_shortcut_at(&path, Some(&config)).unwrap());
        assert!(!path.exists());
        std::fs::write(&path, b"Unrelated file").unwrap();
        assert!(toggle_shortcut_at(&path, Some(&config)).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"Unrelated file");
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
