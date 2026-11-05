//! Bounded choice files, separate from live counters, replaced atomically.
use super::identity::World;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use starrupture_observation_model::recipe::{Control, Snapshot};
use std::{
    io::Write,
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    version: u8,
    recipes: Vec<String>,
}
struct Active {
    key: String,
    ids: Vec<String>,
    pending: bool,
    retry_at: std::time::Instant,
}
pub(crate) struct Remembered {
    root: PathBuf,
    active: Option<Active>,
}
impl Remembered {
    pub fn new(cache: &Path) -> Self {
        Self {
            root: cache.join("pinned-recipes"),
            active: None,
        }
    }
    pub fn bound(&self) -> bool {
        self.active.is_some()
    }
    pub fn detach(&mut self) {
        self.active = None;
    }
    pub fn begin(&mut self, world: World) -> Result<()> {
        ensure!(key(&world.key), "Invalid world persistence key");
        let path = self.root.join(format!("{}.json", world.key));
        let ids = match super::storage::load(&path) {
            Ok(ids) => ids,
            Err(e) if !path.exists() => {
                let _ = e;
                Vec::new()
            }
            Err(e) => {
                eprintln!("Remembered recipes unavailable: {e:#}");
                Vec::new()
            }
        };
        self.active = Some(Active {
            key: world.key,
            ids,
            pending: true,
            retry_at: std::time::Instant::now(),
        });
        Ok(())
    }
    pub fn desired(
        &mut self,
        control: &Control,
    ) -> Option<(starrupture_observation_model::recipe::Request, Vec<String>)> {
        let active = self.active.as_mut()?;
        if !active.pending || std::time::Instant::now() < active.retry_at {
            return None;
        }
        let Some(request) = control.restoration_request() else {
            active.pending = false;
            active.retry_at = std::time::Instant::now();
            return None;
        };
        active.retry_at = std::time::Instant::now() + std::time::Duration::from_secs(2);
        Some((request, active.ids.clone()))
    }
    pub fn restored(
        &mut self,
        control: &Control,
        request: starrupture_observation_model::recipe::Request,
        snapshots: Vec<Snapshot>,
    ) {
        if control.restore(request, snapshots)
            && let Some(active) = &mut self.active
        {
            active.pending = false;
            active.retry_at = std::time::Instant::now();
        }
    }
    pub fn flush(&mut self, control: &Control) -> Result<()> {
        if let Some(active) = &mut self.active {
            active.retry_at = std::time::Instant::now();
            if active.pending && control.restoration_request().is_none() {
                active.pending = false;
                active.retry_at = std::time::Instant::now();
            }
        }
        self.checkpoint(control)
    }
    pub fn checkpoint(&mut self, control: &Control) -> Result<()> {
        let Some(active) = &mut self.active else {
            return Ok(());
        };
        if active.pending
            || control.request().cursor.is_some()
            || std::time::Instant::now() < active.retry_at
        {
            return Ok(());
        }
        let ids: Vec<_> =
            control.views().into_iter().filter_map(|v| v.snapshot.map(|s| s.recipe_id)).collect();
        if ids == active.ids {
            return Ok(());
        }
        validate(&ids)?;
        if let Err(error) = save(&self.root.join(format!("{}.json", active.key)), &ids) {
            active.retry_at = std::time::Instant::now() + std::time::Duration::from_secs(2);
            return Err(error);
        }
        active.ids = ids;
        Ok(())
    }
}
fn key(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}
fn validate(ids: &[String]) -> Result<()> {
    let mut seen = std::collections::BTreeSet::new();
    ensure!(
        ids.len() <= 3
            && ids.iter().all(|id| id.starts_with("/Game/")
                && id.len() <= 512
                && !id.chars().any(char::is_control)
                && seen.insert(id)),
        "Invalid remembered recipe choices"
    );
    Ok(())
}
fn load(path: &Path) -> Result<Vec<String>> {
    let record: Record = crate::cache::read_json(path, 4096)?;
    ensure!(record.version == 1, "Unsupported remembered recipes format");
    validate(&record.recipes)?;
    Ok(record.recipes)
}
fn save(path: &Path, ids: &[String]) -> Result<()> {
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };
    validate(ids)?;
    let parent = path.parent().context("Recipe persistence directory")?;
    std::fs::create_dir_all(parent)?;
    let bytes = serde_json::to_vec(&Record {
        version: 1,
        recipes: ids.to_vec(),
    })?;
    ensure!(bytes.len() <= 4096, "Remembered recipe file bound");
    let suffix = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let temporary = path.with_extension(format!("{}-{suffix}.tmp", std::process::id()));
    let result = (|| -> Result<()> {
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        let from: Vec<_> = temporary.as_os_str().encode_wide().chain([0]).collect();
        let to: Vec<_> = path.as_os_str().encode_wide().chain([0]).collect();
        // SAFETY: Both terminated paths point only to this app's private choice file and owned
        // temporary; buffers live through synchronous replacement. No save-game path is written.
        ensure!(
            // SAFETY: The terminated buffers reference only the owned app-private files.
            unsafe {
                MoveFileExW(
                    from.as_ptr(),
                    to.as_ptr(),
                    MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
                )
            } != 0,
            "Could not replace remembered recipe choices: {}",
            std::io::Error::last_os_error()
        );
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../local/persistence-tests")
                .join(format!(
                    "{}-{}",
                    std::process::id(),
                    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
                ));
            std::fs::create_dir_all(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn snapshot(id: &str, remaining: u32) -> Snapshot {
        Snapshot {
            recipe_id: id.into(),
            ingredients: vec![starrupture_observation_model::recipe::Ingredient {
                item_id: "item".into(),
                submitted: 100 - remaining,
                required: 100,
                remaining,
                bag_count: None,
            }],
            observed_at_ms: 2000,
            generation: 0,
            process: 1,
        }
    }
    #[test]
    fn remembered_choices_restore_fresh_snapshots_and_persist_clear() {
        let root = Temp::new();
        let key = "a".repeat(64);
        let path = root.0.join("pinned-recipes").join(format!("{key}.json"));
        save(&path, &["/Game/A".into(), "/Game/Done".into()]).unwrap();
        let control = Control::default();
        let mut remembered = Remembered::new(&root.0);
        remembered
            .begin(World {
                key: key.clone(),
                label: "Example".into(),
            })
            .unwrap();
        let (request, ids) = remembered.desired(&control).unwrap();
        assert_eq!(ids.len(), 2);
        remembered.restored(
            &control,
            request,
            vec![snapshot("/Game/A", 25), snapshot("/Game/Done", 0)],
        );
        remembered.flush(&control).unwrap();
        assert_eq!(load(&path).unwrap(), vec!["/Game/A"]);
        assert_eq!(control.view().snapshot.unwrap().ingredients[0].remaining, 25);
        control.clear();
        remembered.flush(&control).unwrap();
        assert!(load(&path).unwrap().is_empty());
        control.reset_session();
        remembered.detach();
        remembered
            .begin(World {
                key: "b".repeat(64),
                label: "Other".into(),
            })
            .unwrap();
        assert!(remembered.desired(&control).unwrap().1.is_empty());
        let raw = std::fs::read_to_string(path).unwrap();
        assert!(!raw.contains("remaining"));
        assert!(!raw.contains("submitted"));
    }
    #[test]
    fn manual_selection_cancels_pending_restoration_and_write_failure_preserves_file() {
        let root = Temp::new();
        let key = "a".repeat(64);
        let path = root.0.join("pinned-recipes").join(format!("{key}.json"));
        save(&path, &["/Game/Old".into()]).unwrap();
        let control = Control::default();
        let mut remembered = Remembered::new(&root.0);
        remembered
            .begin(World {
                key,
                label: "Example".into(),
            })
            .unwrap();
        control.clear();
        assert!(remembered.desired(&control).is_none());
        remembered.flush(&control).unwrap();
        assert!(load(&path).unwrap().is_empty());
        let before = std::fs::read(&path).unwrap();
        let blocker = root.0.join("blocker");
        std::fs::write(&blocker, b"x").unwrap();
        assert!(save(&blocker.join("child.json"), &["/Game/A".into()]).is_err());
        assert_eq!(std::fs::read(path).unwrap(), before);
    }
    #[test]
    fn choice_files_are_bounded_and_separate_worlds() {
        let root = Temp::new();
        let a = root.0.join("a.json");
        let b = root.0.join("b.json");
        save(&a, &["/Game/A".into()]).unwrap();
        save(&b, &["/Game/B".into()]).unwrap();
        assert_eq!(load(&a).unwrap(), vec!["/Game/A"]);
        assert_eq!(load(&b).unwrap(), vec!["/Game/B"]);
        save(&a, &[]).unwrap();
        assert!(load(&a).unwrap().is_empty());
        assert_eq!(load(&b).unwrap(), vec!["/Game/B"]);
        std::fs::write(&a, vec![b' '; 4097]).unwrap();
        assert!(load(&a).is_err());
        assert!(validate(&["/Game/A".into(), "/Game/A".into()]).is_err());
        assert!(validate(&["bad".into()]).is_err());
    }
    #[test]
    fn save_folder_ambiguity_and_labels_cannot_cross_worlds() {
        use super::super::identity::{component, folder_key, resolve_folder};
        let root = Temp::new();
        let steam = root.0.join("Steam");
        let a = steam.join("userdata/1/1631270/remote/Saved/SaveGames/Example");
        std::fs::create_dir_all(&a).unwrap();
        let b = steam.join("userdata/1/1631270/remote/Saved/SaveGames/Other");
        std::fs::create_dir_all(&b).unwrap();
        assert_ne!(folder_key(&a).unwrap(), folder_key(&b).unwrap());
        assert_eq!(
            resolve_folder(std::slice::from_ref(&steam), "Example").unwrap(),
            a.canonicalize().unwrap()
        );
        let duplicate = steam.join("userdata/2/1631270/remote/Saved/SaveGames/Example");
        std::fs::create_dir_all(duplicate).unwrap();
        assert!(resolve_folder(&[steam], "Example").is_err());
        for bad in ["", "..", "A/B", "A\\B", "A:"] {
            assert!(!component(bad));
        }
    }
}
