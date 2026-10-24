//! Bundled OFL fonts, registered only inside this process. No system installation.
use windows_sys::Win32::{
    Foundation::HANDLE,
    Graphics::Gdi::{AddFontMemResourceEx, RemoveFontMemResourceEx},
};
pub struct Fonts(Vec<HANDLE>);
impl Fonts {
    pub fn load() -> std::io::Result<Self> {
        let mut fonts = Self(Vec::new());
        for bytes in [
            include_bytes!("../assets/fonts/Oxanium-Regular.ttf").as_slice(),
            include_bytes!("../assets/fonts/Oxanium-SemiBold.ttf").as_slice(),
            include_bytes!("../assets/fonts/Oxanium-Bold.ttf").as_slice(),
            include_bytes!("../assets/fonts/Oxanium-ExtraBold.ttf").as_slice(),
        ] {
            let count = 0;
            let handle = unsafe {
                AddFontMemResourceEx(
                    bytes.as_ptr().cast(),
                    bytes.len() as u32,
                    std::ptr::null(),
                    &count,
                )
            };
            if handle.is_null() || count == 0 {
                return Err(std::io::Error::other(
                    "Bundled overlay font could not be loaded; no silent substitute",
                ));
            }
            fonts.0.push(handle);
        }
        verify_faces()?;
        Ok(fonts)
    }
}
fn verify_faces() -> std::io::Result<()> {
    use windows_sys::Win32::Graphics::Gdi::*;
    unsafe {
        let dc = CreateCompatibleDC(std::ptr::null_mut());
        if dc.is_null() {
            return Err(std::io::Error::other("Cannot validate overlay fonts"));
        }
        for (weight, face) in [
            (400, "Oxanium"),
            (600, "Oxanium SemiBold"),
            (700, "Oxanium"),
            (800, "Oxanium ExtraBold"),
        ] {
            let family: Vec<u16> = face.encode_utf16().chain([0]).collect();
            let font = CreateFontW(
                -24,
                0,
                0,
                0,
                weight,
                0,
                0,
                0,
                DEFAULT_CHARSET as u32,
                OUT_DEFAULT_PRECIS as u32,
                CLIP_DEFAULT_PRECIS as u32,
                ANTIALIASED_QUALITY as u32,
                DEFAULT_PITCH as u32,
                family.as_ptr(),
            );
            let old = SelectObject(dc, font);
            let mut name = [0u16; 128];
            let len = GetTextFaceW(dc, 128, name.as_mut_ptr());
            let mut metrics = TEXTMETRICW::default();
            let ok = GetTextMetricsW(dc, &mut metrics);
            SelectObject(dc, old);
            DeleteObject(font);
            if len <= 0
                || ok == 0
                || !String::from_utf16_lossy(&name[..len as usize]).starts_with("Oxanium")
                || metrics.tmWeight != weight
            {
                DeleteDC(dc);
                return Err(std::io::Error::other(format!(
                    "Overlay font mismatch: requested {face}/{weight}, got {} / {}",
                    String::from_utf16_lossy(&name[..len.max(0) as usize]),
                    metrics.tmWeight
                )));
            }
        }
        DeleteDC(dc);
    }
    Ok(())
}
impl Drop for Fonts {
    fn drop(&mut self) {
        for font in self.0.drain(..) {
            unsafe { RemoveFontMemResourceEx(font) };
        }
    }
}
