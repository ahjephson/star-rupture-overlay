//! Local caches are hints, never trusted observations. Bound allocation before parsing.
use anyhow::{Result, ensure};
use serde::de::DeserializeOwned;
use std::{fs::File, io::Read, path::Path};

pub(crate) fn read_json<T: DeserializeOwned>(path: &Path, limit: usize) -> Result<T> {
    decode(File::open(path)?, limit)
}

fn decode<T: DeserializeOwned>(reader: impl Read, limit: usize) -> Result<T> {
    let mut bytes = Vec::new();
    reader.take(limit as u64 + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= limit, "Local cache exceeds read bound");
    Ok(serde_json::from_slice(&bytes)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    #[test]
    fn oversized_and_invalid_caches_fail_before_becoming_observations() {
        assert_eq!(decode::<Value>(&b"{}"[..], 2).unwrap(), serde_json::json!({}));
        assert!(decode::<Value>(&b"{} "[..], 2).is_err());
        assert!(decode::<Value>(&b"not json"[..], 100).is_err());
    }
}
