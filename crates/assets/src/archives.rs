//! Structural admission for the uncompressed UE5.6 archive format we can read offline.
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs, path::Path};
fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}
fn little(bytes: &[u8]) -> u64 {
    bytes.iter().enumerate().fold(0, |n, (i, b)| n | (u64::from(*b) << (i * 8)))
}
fn big(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0, |n, b| n * 256 + u64::from(*b))
}
fn validate(bytes: &[u8], data_len: u64) -> Result<Value> {
    ensure!((144..=32 * 1024 * 1024).contains(&bytes.len()), "Archive TOC size bound");
    ensure!(&bytes[..16] == b"-==--==--==--==-", "Archive TOC magic");
    ensure!(bytes[16] == 8 && u32_at(bytes, 20) == 144, "Unsupported archive TOC version/header");
    ensure!(
        u32_at(bytes, 32) == 12 && u32_at(bytes, 36) == 0 && bytes[80] & !8 == 0,
        "Only uncompressed, unsigned, unencrypted archives are supported"
    );
    ensure!(u32_at(bytes, 52) == 1, "Unsupported archive partition count");
    let block_size = u32_at(bytes, 44) as u64;
    ensure!(block_size == 65536 && data_len > 0, "Archive block/data size");
    let entries = u32_at(bytes, 24) as usize;
    let blocks = u32_at(bytes, 28) as usize;
    let seeds = u32_at(bytes, 84) as usize;
    let overflow = u32_at(bytes, 96) as usize;
    let directory = u32_at(bytes, 48) as usize;
    ensure!(
        entries > 0
            && entries <= bytes.len() / 46
            && blocks > 0
            && blocks <= bytes.len() / 12
            && seeds <= entries
            && overflow <= entries
            && directory <= 8 * 1024 * 1024,
        "Archive table count bound"
    );
    let block_start = 144 + entries * 22 + (seeds + overflow) * 4;
    let block_end = block_start + blocks * 12;
    ensure!(
        block_end + directory + entries * 24 == bytes.len(),
        "Archive table lengths/truncation"
    );
    let mut ids = BTreeSet::new();
    for id in bytes[144..144 + entries * 12].as_chunks::<12>().0 {
        ensure!(ids.insert(id), "Duplicate archive chunk identity");
    }
    for chunk in bytes[144 + entries * 12..144 + entries * 22].as_chunks::<10>().0 {
        let offset = big(&chunk[..5]);
        let length = big(&chunk[5..]);
        ensure!(
            length > 0 && offset + length <= blocks as u64 * block_size,
            "Archive virtual chunk bounds"
        );
    }
    for block in bytes[block_start..block_end].as_chunks::<12>().0 {
        let offset = little(&block[..5]);
        let stored = little(&block[5..8]);
        let unpacked = little(&block[8..11]);
        ensure!(
            block[11] == 0 && stored == unpacked && stored > 0 && stored <= block_size,
            "Unsupported compression or invalid archive block"
        );
        ensure!(offset + stored <= data_len, "Archive block escapes data file");
    }
    let meta_start = block_end + directory;
    for meta in bytes[meta_start..].as_chunks::<24>().0 {
        ensure!(meta[20] & !2 == 0, "Unsupported archive chunk flags");
    }
    Ok(
        json!({"tocVersion":8,"chunks":entries,"blocks":blocks,"compression":"none","tocSha256":format!("{:x}",Sha256::digest(bytes))}),
    )
}
pub(super) fn preflight(paks: &Path) -> Result<Vec<Value>> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(paks)? {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == "utoc") {
            ensure!(paths.len() < 32, "Archive container count bound");
            paths.push(path);
        }
    }
    paths.sort();
    ensure!(!paths.is_empty() && paks.join("global.utoc").is_file(), "Missing global game archive");
    paths
        .into_iter()
        .map(|path| {
            let bytes = super::bounded(&path, 32 * 1024 * 1024)?;
            let data = path.with_extension("ucas");
            let meta = fs::metadata(&data)?;
            ensure!(meta.is_file(), "Archive data must be a file");
            let mut result = validate(&bytes, meta.len())
                .with_context(|| format!("Checking {}", path.display()))?;
            result["file"] = json!(path.file_name().context("Archive filename")?.to_string_lossy());
            Ok(result)
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Vec<u8> {
        let mut b = vec![0; 202];
        b[..16].copy_from_slice(b"-==--==--==--==-");
        b[16] = 8;
        for (offset, value) in
            [(20, 144u32), (24, 1), (28, 1), (32, 12), (40, 32), (44, 65536), (52, 1)]
        {
            b[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        b[165] = 16; // big-endian virtual chunk length
        b[171] = 16; // stored block size
        b[174] = 16; // uncompressed block size
        b
    }
    #[test]
    fn compatible_content_changes_are_accepted_but_unsupported_or_malformed_archives_are_not() {
        let b = fixture();
        validate(&b, 16).unwrap();
        let mut changed = b.clone();
        changed[144] = 99;
        validate(&changed, 16).unwrap();
        assert!(validate(&b, 15).is_err());
        for length in 0..b.len() {
            assert!(validate(&b[..length], 16).is_err(), "length {length}");
        }
        for (offset, value) in [
            (16, 9),
            (20, 0),
            (24, 255),
            (28, 255),
            (32, 13),
            (36, 1),
            (52, 2),
            (80, 1),
            (80, 2),
            (80, 4),
            (177, 1),
            (171, 15),
            (198, 1),
        ] {
            let mut bad = b.clone();
            bad[offset] = value;
            assert!(validate(&bad, 16).is_err(), "offset {offset}");
        }
    }
}
