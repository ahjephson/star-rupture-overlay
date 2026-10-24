#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
//! Explicitly launched standalone desktop HUD. No DirectX hook or controller code.
#[cfg(windows)]
mod fonts;
#[cfg(windows)]
mod stacking;
#[cfg(windows)]
mod stream_client;
#[cfg(windows)]
mod window;
fn main() {
    #[cfg(windows)]
    if let Err(error) = window::run() {
        eprintln!("Wave overlay: {error}");
        if !std::env::args().any(|a| a.starts_with("--self-test")) {
            let text: Vec<u16> = format!("The wave overlay could not start:\n\n{error}")
                .encode_utf16()
                .chain([0])
                .collect();
            let title: Vec<u16> = "StarRupture Wave Overlay".encode_utf16().chain([0]).collect();
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
        eprintln!("Wave overlay requires Windows");
        std::process::exit(1)
    }
}
