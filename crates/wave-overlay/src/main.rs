//! Explicitly launched standalone desktop HUD. No DirectX hook or controller code.
#[cfg(windows)]
mod fonts;
#[cfg(windows)]
mod stream_client;
#[cfg(windows)]
mod window;
fn main() {
    #[cfg(windows)]
    if let Err(error) = window::run() {
        eprintln!("Wave overlay: {error}");
        std::process::exit(1)
    }
    #[cfg(not(windows))]
    {
        eprintln!("Wave overlay requires Windows");
        std::process::exit(1)
    }
}
