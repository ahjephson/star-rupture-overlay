//! Read executable sections once through the existing guarded process handle.
use crate::{Mem, platform::Process};
use anyhow::{Context, Result, ensure};
use object::{Object, ObjectSection};
use patternsleuth::{
    Memory,
    image::{Image, pe::PEImage},
};
fn header_size(header: &[u8], image_size: usize) -> Result<usize> {
    ensure!(header.get(..2) == Some(b"MZ"), "Invalid DOS header");
    let nt =
        u32::from_le_bytes(header.get(60..64).context("Missing PE offset")?.try_into()?) as usize;
    ensure!(header.get(nt..nt + 4) == Some(b"PE\0\0"), "Invalid PE header");
    let optional = nt.checked_add(24).context("PE offset overflow")?;
    ensure!(header.get(optional..optional + 2) == Some(&[0x0b, 0x02]), "Expected PE64");
    let size = u32::from_le_bytes(
        header.get(optional + 60..optional + 64).context("Missing header size")?.try_into()?,
    ) as usize;
    ensure!(
        (optional + 64..=1024 * 1024).contains(&size) && size <= image_size,
        "PE header size bound"
    );
    Ok(size)
}
pub(crate) async fn read(process: &Process) -> Result<Image<'static>> {
    let (base, size) = process.main_module()?;
    ensure!((4096..=512 * 1024 * 1024).contains(&size), "Game image size bound");
    let mut header = vec![0; 4096];
    process.read_buf(base, &mut header).await?;
    let headers = header_size(&header, size)?;
    header.resize(headers, 0);
    process.read_buf(base, &mut header).await?;
    let object = object::File::parse(header.as_slice())?;
    ensure!(object.sections().count() <= 128, "PE section count bound");
    let preferred = object.relative_address_base();
    let mut sections = Vec::new();
    let mut total = 0usize;
    for section in object.sections() {
        let offset = section.address().checked_sub(preferred).context("PE section address")?;
        let length = usize::try_from(section.size())?;
        let end = offset.checked_add(length as u64).context("PE section overflow")?;
        ensure!(end <= size as u64, "PE section outside image");
        total = total.checked_add(length).context("PE image allocation overflow")?;
        ensure!(total <= size, "PE section allocation bound");
        let mut data = vec![0; length];
        for (index, chunk) in data.chunks_mut(1024 * 1024).enumerate() {
            process.read_buf(base + offset + (index * 1024 * 1024) as u64, chunk).await?;
        }
        sections.push((section, data));
    }
    process.ensure_alive()?;
    let memory = Memory::new_external_data(sections)?;
    PEImage::read_inner_memory::<String>(base, None, false, memory, object)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_truncated_and_oversized_headers() {
        for n in 0..256 {
            assert!(header_size(&vec![0; n], 4096).is_err());
        }
        let mut h = vec![0; 4096];
        h[..2].copy_from_slice(b"MZ");
        h[60..64].copy_from_slice(&128u32.to_le_bytes());
        h[128..132].copy_from_slice(b"PE\0\0");
        h[152..154].copy_from_slice(&[0x0b, 0x02]);
        h[212..216].copy_from_slice(&1024u32.to_le_bytes());
        assert_eq!(header_size(&h, 4096).unwrap(), 1024);
        h[212..216].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(header_size(&h, 4096).is_err());
        h[60..64].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(header_size(&h, 4096).is_err());
    }
}
