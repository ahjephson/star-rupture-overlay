use anyhow::{Result, bail};
use std::io::Write;
use strum::{AsRefStr, EnumString, VariantArray};

#[derive(Debug, Clone, Copy, PartialEq, EnumString, AsRefStr, VariantArray)]
pub enum CompressionMethod {
    Zlib,
    Zstd,
    LZ4,
    Oodle,
}
impl CompressionMethod {
    pub fn from_str_ignore_case(value: &str) -> Option<Self> {
        CompressionMethod::VARIANTS.iter().copied().find(|v| v.as_ref().eq_ignore_ascii_case(value))
    }
}

pub fn compress<S: Write>(compression: CompressionMethod, _input: &[u8], _output: S) -> Result<()> {
    bail!("Archive compression {compression:?} is unsupported by this build")
}
pub fn decompress(compression: CompressionMethod, _input: &[u8], _output: &mut [u8]) -> Result<()> {
    bail!("Archive compression {compression:?} is unsupported by this build")
}