#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
//! Windows desktop HUD for StarRupture wave stages and countdowns.
#[cfg(windows)]
mod connection;
#[cfg(windows)]
mod desktop;
#[cfg(windows)]
mod dialogs;
#[cfg(windows)]
mod fonts;
#[cfg(windows)]
mod icons;
#[cfg(windows)]
mod preferences;
#[cfg(windows)]
mod stacking;
#[cfg(windows)]
mod status;
#[cfg(windows)]
mod stream_client;
#[cfg(windows)]
mod window;
#[cfg(windows)]
mod window_events;
fn main() {
    #[cfg(windows)]
    if let Err(error) = window::run() {
        eprintln!("StarRupture Overlay: {error}");
        if !std::env::args().any(|a| a.starts_with("--self-test")) {
            let text: Vec<u16> = format!("StarRupture Overlay could not start:\n\n{error}")
                .encode_utf16()
                .chain([0])
                .collect();
            let title: Vec<u16> = "StarRupture Overlay".encode_utf16().chain([0]).collect();
            unsafe {
                windows_sys::Win32::UI::WindowsAndMessaging::MessageBoxW(
                    std::ptr::null_mut(),
                    text.as_ptr(),
                    title.as_ptr(),
                    windows_sys::Win32::UI::WindowsAndMessaging::MB_OK
                        | windows_sys::Win32::UI::WindowsAndMessaging::MB_ICONERROR,
                );
            }
        }
        std::process::exit(1)
    }
    #[cfg(not(windows))]
    {
        eprintln!("StarRupture Overlay requires Windows");
        std::process::exit(1)
    }
}
