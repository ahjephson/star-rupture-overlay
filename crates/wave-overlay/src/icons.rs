//! Shared executable-resource icons live for the process lifetime. Windows owns
//! LR_SHARED handles; callers must not destroy them. GetIconInfo diagnostic bitmap
//! copies are different owned objects and are always deleted.
use anyhow::{Result, ensure};
use windows_sys::Win32::{
    Foundation::HWND,
    Graphics::Gdi::{BITMAP, DeleteObject, GetObjectW},
    System::LibraryLoader::GetModuleHandleW,
    UI::{Shell::ExtractIconExW, WindowsAndMessaging::*},
};

fn load(width: i32, height: i32) -> Result<HICON> {
    // Integer resource 1 is the sole group icon, also selected by Explorer.
    let module = unsafe { GetModuleHandleW(std::ptr::null()) };
    let icon = unsafe {
        LoadImageW(module, std::ptr::without_provenance(1), IMAGE_ICON, width, height, LR_SHARED)
    };
    ensure!(
        !icon.is_null(),
        "Embedded app icon could not load: {}",
        std::io::Error::last_os_error()
    );
    Ok(icon)
}

pub struct Icons {
    pub large: HICON,
    pub small: HICON,
}
impl Icons {
    pub fn load() -> Result<Self> {
        Ok(Self {
            large: load(unsafe { GetSystemMetrics(SM_CXICON) }, unsafe {
                GetSystemMetrics(SM_CYICON)
            })?,
            small: load(unsafe { GetSystemMetrics(SM_CXSMICON) }, unsafe {
                GetSystemMetrics(SM_CYSMICON)
            })?,
        })
    }
    pub fn set_window(&self, window: HWND) {
        unsafe {
            SendMessageW(window, WM_SETICON, ICON_BIG as usize, self.large as isize);
            SendMessageW(window, WM_SETICON, ICON_SMALL as usize, self.small as isize);
        }
    }
}

pub fn set_about(window: HWND) -> Result<()> {
    let dpi = unsafe { windows_sys::Win32::UI::HiDpi::GetDpiForWindow(window) }.max(96);
    let size = (57 * dpi as i32 + 48) / 96;
    let icon = load(size, size)?;
    unsafe {
        let control = GetDlgItem(window, 143);
        SendMessageW(control, STM_SETICON, icon as usize, 0);
        SetWindowPos(
            control,
            std::ptr::null_mut(),
            0,
            0,
            size,
            size,
            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
    Ok(())
}

/// Native resource-load checks only: no window, tray, focus or game access.
pub fn self_test() -> Result<()> {
    let path: Vec<u16> =
        std::env::current_exe()?.as_os_str().to_string_lossy().encode_utf16().chain([0]).collect();
    let groups =
        unsafe { ExtractIconExW(path.as_ptr(), -1, std::ptr::null_mut(), std::ptr::null_mut(), 0) };
    ensure!(groups == 1, "Expected one executable icon group, got {groups}");
    let mut sizes = Vec::new();
    for size in [16, 20, 24, 32, 40, 48, 64, 128, 256] {
        let icon = load(size, size)?;
        let mut info = ICONINFO::default();
        ensure!(unsafe { GetIconInfo(icon, &mut info) } != 0, "Icon bitmap unavailable");
        let mut bitmap = BITMAP::default();
        let bytes = unsafe {
            GetObjectW(
                info.hbmColor,
                std::mem::size_of::<BITMAP>() as i32,
                (&raw mut bitmap).cast(),
            )
        };
        unsafe {
            if !info.hbmColor.is_null() {
                DeleteObject(info.hbmColor);
            }
            if !info.hbmMask.is_null() {
                DeleteObject(info.hbmMask);
            }
        }
        ensure!(
            bytes != 0 && bitmap.bmWidth == size && bitmap.bmHeight == size,
            "Embedded icon has unexpected dimensions at {size}"
        );
        sizes.push(size);
    }
    let _icons = Icons::load()?;
    println!(
        "{}",
        serde_json::json!({
            "status": "passed",
            "executableIconGroups": groups,
            "sizes": sizes,
            "systemTraySizeLoads": true,
            "gameRead": false,
            "windowCreated": false
        })
    );
    Ok(())
}
