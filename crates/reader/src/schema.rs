//! One build-bound native schema service shared by asset consumers. Idle buffers are not retained.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, Weak},
    time::{SystemTime, UNIX_EPOCH},
};
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    version: u32,
    game_hash: String,
    file: String,
    sha256: String,
}
pub(crate) struct Store {
    root: PathBuf,
    state: Mutex<Option<(String, Weak<[u8]>)>>,
}
impl Store {
    pub fn new(directory: &Path) -> Self {
        Self {
            root: directory.join("native-schemas"),
            state: Mutex::new(None),
        }
    }
    pub fn get(&self, ctx: &jmap_dumper::mem::Ctx, hash: &str) -> Result<Arc<[u8]>> {
        self.get_or_generate(hash, || smol::block_on(crate::recipe::asset_mappings(ctx)))
    }
    fn get_or_generate(
        &self,
        hash: &str,
        generate: impl FnOnce() -> Result<Vec<u8>>,
    ) -> Result<Arc<[u8]>> {
        ensure!(
            starrupture_observation_model::overlay_stream::valid_executable_hash(hash),
            "Schema build hash"
        );
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((key, weak)) = &*state
            && key == hash
            && let Some(bytes) = weak.upgrade()
        {
            return Ok(bytes);
        }
        let file = self.root.join(format!("{hash}.json"));
        let cached = (|| -> Result<Vec<u8>> {
            let r = crate::cache::read_json::<Receipt>(&file, 4096)?;
            ensure!(
                r.version == 1
                    && r.game_hash == hash
                    && r.file.starts_with("schema-")
                    && r.file.len() < 128
                    && r.file.bytes().all(|b| b.is_ascii_alphanumeric() || b"-.".contains(&b))
                    && !r.file.contains(".."),
                "Schema receipt shape"
            );
            let p = self.root.join(r.file);
            ensure!(std::fs::metadata(&p)?.len() <= 16 * 1024 * 1024, "Schema size bound");
            let mut b = Vec::new();
            std::fs::File::open(p)?.take(16 * 1024 * 1024 + 1).read_to_end(&mut b)?;
            ensure!(b.len() <= 16 * 1024 * 1024, "Schema grew beyond read bound");
            ensure!(
                hex::encode(Sha256::digest(&b)) == r.sha256 && b.get(3) == Some(&0),
                "Schema integrity"
            );
            Ok(b)
        })();
        let bytes = match cached {
            Ok(b) => {
                eprintln!("Native schema cache hit; no type discovery.");
                b
            }
            Err(_) => {
                eprintln!("Native schema cache missing/stale; one bounded type discovery.");
                let b = generate()?;
                ensure!(
                    b.len() <= 16 * 1024 * 1024 && b.get(3) == Some(&0),
                    "Generated schema bound"
                );
                std::fs::create_dir_all(&self.root)?;
                let generation = format!(
                    "schema-{}-{}.usmap",
                    std::process::id(),
                    SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
                );
                let data = self.root.join(&generation);
                let mut f = std::fs::OpenOptions::new().create_new(true).write(true).open(&data)?;
                f.write_all(&b)?;
                f.sync_all()?;
                drop(f);
                let r = Receipt {
                    version: 1,
                    game_hash: hash.into(),
                    file: generation,
                    sha256: hex::encode(Sha256::digest(&b)),
                };
                let temp = file.with_extension(format!("{}.tmp", std::process::id()));
                let publish = (|| -> Result<()> {
                    let mut f =
                        std::fs::OpenOptions::new().create_new(true).write(true).open(&temp)?;
                    f.write_all(&serde_json::to_vec(&r)?)?;
                    f.sync_all()?;
                    drop(f);
                    std::fs::rename(&temp, &file)?;
                    Ok(())
                })();
                if publish.is_err() {
                    let _ = std::fs::remove_file(temp);
                    let _ = std::fs::remove_file(data);
                }
                publish.context("Publishing native schema")?;
                b
            }
        };
        let bytes: Arc<[u8]> = bytes.into();
        *state = Some((hash.into(), Arc::downgrade(&bytes)));
        Ok(bytes)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Result<Self> {
            let p = std::env::temp_dir().join(format!(
                "starrupture-schema-test-{}-{}",
                std::process::id(),
                SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
            ));
            std::fs::create_dir(&p)?;
            Ok(Self(p))
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn schema() -> Result<Vec<u8>> {
        let mut b = Vec::new();
        usmap::Usmap {
            enums: vec![],
            structs: vec![],
            cext: None,
            ppth: None,
            eatr: None,
            envp: None,
        }
        .write(&mut b)?;
        Ok(b)
    }
    #[test]
    fn shared_requests_generate_once_and_idle_buffers_are_released() -> Result<()> {
        let directory = Directory::new()?;
        let store = Store::new(&directory.0);
        let calls = AtomicUsize::new(0);
        let hash = "a".repeat(64);
        let generated = || {
            calls.fetch_add(1, Ordering::SeqCst);
            schema()
        };
        let (a, b) = std::thread::scope(|s| {
            let a = s.spawn(|| store.get_or_generate(&hash, generated));
            let b = s.spawn(|| store.get_or_generate(&hash, generated));
            (a.join().unwrap(), b.join().unwrap())
        });
        let a = a?;
        let b = b?;
        assert!(Arc::ptr_eq(&a, &b));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let weak = Arc::downgrade(&a);
        drop(a);
        drop(b);
        assert!(weak.upgrade().is_none());
        let fresh = Store::new(&directory.0)
            .get_or_generate(&hash, || anyhow::bail!("warm lookup attempted discovery"))?;
        assert!(!fresh.is_empty());
        Ok(())
    }
    #[test]
    fn corrupted_or_changed_build_schema_requires_bounded_regeneration() -> Result<()> {
        let directory = Directory::new()?;
        let hash = "b".repeat(64);
        let store = Store::new(&directory.0);
        drop(store.get_or_generate(&hash, schema)?);
        let receipt: Receipt =
            crate::cache::read_json(&store.root.join(format!("{hash}.json")), 4096)?;
        std::fs::write(store.root.join(receipt.file), b"corrupt")?;
        let count = AtomicUsize::new(0);
        let repaired = store.get_or_generate(&hash, || {
            count.fetch_add(1, Ordering::SeqCst);
            schema()
        })?;
        assert_eq!(count.load(Ordering::SeqCst), 1);
        drop(repaired);
        assert!(
            store
                .get_or_generate(&"c".repeat(64), || anyhow::bail!(
                    "new build requires fresh schema"
                ))
                .is_err()
        );
        assert!(store.get_or_generate("invalid", schema).is_err());
        Ok(())
    }
}
