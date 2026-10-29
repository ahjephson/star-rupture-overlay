//! Build-bound local asset cache. All preparation runs on the existing reader worker.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use starrupture_observation_model::{
    recipe::{Catalogue, Control},
    shutdown::StopSignal,
};
use starrupture_overlay_reader::recipe::{AssetSetup, Tracker};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
mod child;
pub use child::{self_test as self_test_cancel, wait_fixture};

#[derive(Clone, Default)]
pub(super) struct State {
    pub revision: u64,
    pub ready: Option<PathBuf>,
    pub message: String,
    pub progress: Option<PathBuf>,
    pub cache_hit: bool,
}
pub(super) type Shared = Arc<Mutex<State>>;
pub(super) struct Preparation {
    pub root: PathBuf,
    pub state: Shared,
    pub control: Arc<Control>,
    pub enabled: Arc<std::sync::atomic::AtomicBool>,
    pub pending_directory: Option<PathBuf>,
}
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Source {
    executable: PathBuf,
    files: BTreeMap<String, (u64, u128)>,
}
#[derive(Serialize, Deserialize)]
struct Receipt {
    schema: u32,
    source: Source,
    directory: String,
    files: BTreeMap<String, String>,
}
fn source(executable: &Path) -> Result<(Source, PathBuf)> {
    ensure!(
        executable.file_name().is_some_and(|n| n
            .to_string_lossy()
            .eq_ignore_ascii_case("StarRuptureGameSteam-Win64-Shipping.exe")),
        "Unexpected game executable"
    );
    let game = executable
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .context("Game installation folder unavailable")?;
    let paks = game.join("Content/Paks");
    let mut files = BTreeMap::new();
    for entry in fs::read_dir(&paks).context("Installed game files unavailable")? {
        let entry = entry?;
        if entry
            .path()
            .extension()
            .is_some_and(|e| ["pak", "utoc", "ucas"].iter().any(|s| e.eq_ignore_ascii_case(s)))
        {
            let metadata = entry.metadata()?;
            ensure!(metadata.is_file() && files.len() < 512, "Game archive inventory bound");
            files.insert(
                entry.file_name().to_string_lossy().into_owned(),
                (metadata.len(), metadata.modified()?.duration_since(UNIX_EPOCH)?.as_nanos()),
            );
        }
    }
    ensure!(
        files.keys().any(|p| p.ends_with(".utoc")) && files.keys().any(|p| p.ends_with(".pak")),
        "Installed game archives are missing"
    );
    Ok((
        Source {
            executable: executable.to_path_buf(),
            files,
        },
        paks,
    ))
}
pub(super) fn diagnostic_assets() -> Result<PathBuf> {
    let root = crate::preferences::directory()?.join("recipe-cache");
    let receipt: Receipt = serde_json::from_slice(&read_bounded(&root.join("current.json"), 256 * 1024)
        .context("Prepare the recipe tracker in a supported loaded world before running asset diagnostics")?)?;
    ensure!(
        receipt.schema == 3
            && receipt.directory.starts_with("assets-")
            && starrupture_observation_model::recipe::safe_file(&receipt.directory),
        "Invalid prepared-asset receipt"
    );
    let directory = root.join(receipt.directory);
    let cat = catalogue(&directory)?;
    ensure!(
        inventory(&directory, &cat)? == receipt.files,
        "Prepared diagnostic assets failed integrity checks"
    );
    Ok(directory)
}
fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let file = fs::File::open(path)?;
    ensure!(file.metadata()?.len() <= limit, "Local setup file exceeds size bound");
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= limit, "Local setup file grew beyond size bound");
    Ok(bytes)
}
fn hash_file(path: &Path) -> Result<String> {
    let bytes = read_bounded(path, 4 * 1024 * 1024 + 8)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
fn catalogue(directory: &Path) -> Result<Arc<Catalogue>> {
    let value: Catalogue =
        serde_json::from_slice(&read_bounded(&directory.join("catalogue.json"), 512 * 1024)?)?;
    value.validate().map_err(anyhow::Error::msg)?;
    Ok(Arc::new(value))
}
fn inventory(directory: &Path, catalogue: &Catalogue) -> Result<BTreeMap<String, String>> {
    let mut names: Vec<_> = catalogue.items.values().map(|i| i.image.clone()).collect();
    names.extend([
        catalogue.tick.clone(),
        catalogue.frame.clone(),
        catalogue.background.clone(),
        catalogue.font.clone(),
        "catalogue.json".into(),
    ]);
    names.sort();
    names.dedup();
    let mut total = 0;
    let mut files = BTreeMap::new();
    for name in names {
        let path = directory.join(&name);
        total += fs::metadata(&path)?.len();
        ensure!(total <= 68 * 1024 * 1024, "Prepared recipe cache exceeds bound");
        files.insert(name, hash_file(&path)?);
    }
    Ok(files)
}
impl Preparation {
    fn update(&self, ready: Option<PathBuf>, message: &str, progress: Option<PathBuf>) {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        state.revision = state.revision.saturating_add(1);
        state.ready = ready;
        state.message = message.into();
        state.progress = progress;
    }
    fn tracker(&mut self, directory: PathBuf) -> Result<Tracker> {
        let tracker = Tracker {
            catalogue: catalogue(&directory)?,
            control: self.control.clone(),
        };
        self.pending_directory = Some(directory);
        self.update(None, "Connecting recipe tracker...", None);
        Ok(tracker)
    }
}
impl AssetSetup for Preparation {
    fn enabled(&self) -> bool {
        self.enabled.load(std::sync::atomic::Ordering::Relaxed)
    }
    fn cached(&mut self, executable: &Path, game_hash: &str) -> Result<Option<Tracker>> {
        self.update(None, "Checking recipe tracker cache...", None);
        let (current, _) = source(executable)?;
        let cached: Result<_> = (|| {
            let receipt: Receipt = serde_json::from_slice(&read_bounded(
                &self.root.join("current.json"),
                256 * 1024,
            )?)?;
            ensure!(
                receipt.schema == 3
                    && receipt.source == current
                    && receipt.directory.starts_with("assets-")
                    && starrupture_observation_model::recipe::safe_file(&receipt.directory),
                "Recipe cache belongs to another game installation or build"
            );
            let directory = self.root.join(receipt.directory);
            let cat = catalogue(&directory)?;
            ensure!(cat.game_hash == game_hash, "Recipe cache belongs to another game build");
            ensure!(
                inventory(&directory, &cat)? == receipt.files,
                "Recipe cache integrity mismatch"
            );
            self.tracker(directory)
        })();
        match cached {
            Ok(t) => {
                self.state.lock().unwrap_or_else(|p| p.into_inner()).cache_hit = true;
                eprintln!("Recipe assets reused from validated local cache.");
                Ok(Some(t))
            }
            Err(error) => {
                eprintln!("Recipe cache preparation needed: {error:#}");
                Ok(None)
            }
        }
    }
    fn ready(&mut self) {
        if let Some(directory) = self.pending_directory.take() {
            self.update(Some(directory), "Recipe tracker ready.", None);
        }
    }
    fn preparing(&mut self) {
        self.state.lock().unwrap_or_else(|p| p.into_inner()).cache_hit = false;
        self.update(None, "Preparing recipe tracker for this game build...", None);
    }
    fn complete(
        &mut self,
        executable: &Path,
        game_hash: &str,
        mappings: &[u8],
        stop: &StopSignal,
    ) -> Result<Tracker> {
        ensure!(self.enabled(), "Recipe tracker disabled during preparation");
        let (before, paks) = source(executable)?;
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let name = format!("assets-{}-{nonce}", std::process::id());
        let output = self.root.join(&name);
        fs::create_dir_all(&output)?;
        let mapping_path = output.join("native.usmap");
        fs::write(&mapping_path, mappings)?;
        let progress = output.join("progress.txt");
        let request = output.join("request.json");
        let engine = std::env::current_exe()?;
        fs::write(
            &request,
            serde_json::to_vec(&serde_json::json!({
                "paks":paks, "mappings":mapping_path, "output":output, "progress":progress,
                "error":output.join("setup-error.txt"),
                "gameHash":game_hash,
            }))?,
        )?;
        self.update(None, "Preparing recipe tracker...", Some(progress));
        child::run_executable(
            &engine,
            &request,
            &output.join("setup-error.txt"),
            stop,
            &self.enabled,
        )?;
        ensure!(!stop.is_requested(), "Recipe setup cancelled");
        let (after, _) = source(executable)?;
        ensure!(before == after, "Game files changed during recipe setup");
        let cat = catalogue(&output)?;
        ensure!(cat.game_hash == game_hash, "Prepared recipe cache belongs to another game build");
        let files = inventory(&output, &cat)?;
        let receipt = Receipt {
            schema: 3,
            source: after,
            directory: name,
            files,
        };
        let pending = self.root.join("current.pending.json");
        fs::write(&pending, serde_json::to_vec_pretty(&receipt)?)?;
        fs::rename(&pending, self.root.join("current.json"))?;
        self.tracker(output)
    }
    fn failed(&mut self, message: &str) {
        eprintln!("Recipe asset preparation failed: {message}");
        self.update(None, "Recipe setup could not finish. Check that Steam has finished updating and that your world is loaded, then turn Recipe tracker off and on to retry. No internet connection is needed.", None);
    }
}

pub fn self_test(directory: &Path) -> Result<()> {
    self_test_impl(directory, true)
}
pub fn self_test_disabled(directory: &Path) -> Result<()> {
    self_test_impl(directory, false)
}
fn self_test_impl(directory: &Path, enabled: bool) -> Result<()> {
    use starrupture_observation_model::overlay_stream::Payload;
    ensure!(directory.is_absolute(), "Absolute diagnostic folder required");
    fs::create_dir_all(directory)?;
    let control = Arc::new(Control::default());
    let state = Arc::new(Mutex::new(State::default()));
    let setup = Preparation {
        root: directory.join("recipe-cache"),
        state: state.clone(),
        control: control.clone(),
        enabled: Arc::new(std::sync::atomic::AtomicBool::new(enabled)),
        pending_directory: None,
    };
    let stop = Arc::new(StopSignal::default());
    let started = std::time::Instant::now();
    let mut worlds = 0;
    let deadline_stop = stop.clone();
    let deadline = std::thread::spawn(move || {
        deadline_stop.wait_until(std::time::Instant::now() + std::time::Duration::from_secs(320));
        deadline_stop.request();
    });
    let result = starrupture_overlay_reader::overlay_stream::run_with_assets(
        &directory.join("reader-cache"),
        stop.clone(),
        |message| {
            if matches!(message.payload, Payload::World(_)) {
                worlds += 1;
            }
            if worlds >= 24 || started.elapsed().as_secs() >= 320 {
                stop.request();
            }
            Ok(())
        },
        |_| {},
        None,
        Some(Box::new(setup)),
    );
    stop.request();
    deadline.join().map_err(|_| anyhow::anyhow!("Asset diagnostic deadline thread failed"))?;
    result?;
    let state = state.lock().unwrap_or_else(|p| p.into_inner());
    ensure!(
        worlds >= 24 && state.ready.is_some() == enabled,
        "Automatic setup did not complete: {}",
        state.message
    );
    ensure!(
        !control.request().active && control.view().snapshot.is_none(),
        "Setup changed recipe selection"
    );
    if !enabled {
        ensure!(
            !directory.join("recipe-cache").exists(),
            "Disabled tracker created an asset cache"
        );
    }
    let recipes = if let Some(ready) = state.ready.as_deref() {
        super::render::self_test_at(&directory.join("render"), ready)?;
        Some(catalogue(ready)?.recipes.len())
    } else {
        None
    };
    fs::write(
        directory.join("asset-setup-validation.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "readOnly":true, "gameCodeInvoked":false, "readerStopped":true,
            "worldObservations":worlds, "elapsedMillis":started.elapsed().as_millis(), "ready":state.ready,
            "recipes":recipes,"disabledVerified":!enabled,"selectionChanged":false,"cacheHit":state.cache_hit,
        }))?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cache_rejects_changed_archives_corrupt_images_and_escaping_receipts() -> Result<()> {
        let scratch = std::env::temp_dir().join(format!(
            "starrupture-asset-cache-test-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
        fs::create_dir_all(&scratch)?;
        struct Fixture(PathBuf);
        impl Drop for Fixture {
            fn drop(&mut self) {
                let (Ok(target), Ok(parent)) =
                    (self.0.canonicalize(), std::env::temp_dir().canonicalize())
                else {
                    return;
                };
                if target.starts_with(&parent)
                    && target.file_name().is_some_and(|s| {
                        s.to_string_lossy().starts_with("starrupture-asset-cache-test-")
                    })
                {
                    let _ = fs::remove_dir_all(target);
                }
            }
        }
        let _fixture = Fixture(scratch.clone());
        let game = scratch.join("Game");
        let executable = game.join("Binaries/Win64/StarRuptureGameSteam-Win64-Shipping.exe");
        fs::create_dir_all(executable.parent().context("Executable parent")?)?;
        let paks = game.join("Content/Paks");
        fs::create_dir_all(&paks)?;
        fs::write(paks.join("fixture.pak"), b"pak")?;
        fs::write(paks.join("fixture.utoc"), b"utoc")?;
        let root = scratch.join("cache");
        let directory = root.join("assets-fixture");
        fs::create_dir_all(&directory)?;
        fs::write(directory.join("pixel.bgra"), [1, 0, 0, 0, 1, 0, 0, 0, 128, 64, 32, 255])?;
        fs::write(directory.join("font.otf"), b"OTTO-fixture")?;
        let game_hash = "a".repeat(64);
        let data = serde_json::json!({"gameHash": game_hash,
            "recipes":{"/Game/TestRecipe":{"name":"Fixture","itemId":"/Game/TestItem","requirements":{"/Game/TestItem":10}}},
            "items":{"/Game/TestItem":{"name":"Fixture","image":"pixel.bgra"}},
            "tick":"pixel.bgra","frame":"pixel.bgra","background":"pixel.bgra","font":"font.otf"});
        fs::write(directory.join("catalogue.json"), serde_json::to_vec(&data)?)?;
        let receipt = Receipt {
            schema: 3,
            source: source(&executable)?.0,
            directory: "assets-fixture".into(),
            files: inventory(&directory, catalogue(&directory)?.as_ref())?,
        };
        fs::write(root.join("current.json"), serde_json::to_vec(&receipt)?)?;
        let mut preparation = Preparation {
            root: root.clone(),
            state: Arc::new(Mutex::new(State::default())),
            control: Arc::new(Control::default()),
            enabled: Arc::new(std::sync::atomic::AtomicBool::new(true)),
            pending_directory: None,
        };
        assert!(preparation.cached(&executable, &"b".repeat(64))?.is_none());
        assert!(preparation.cached(&executable, &game_hash)?.is_some());
        assert!(preparation.state.lock().unwrap().ready.is_none());
        preparation.ready();
        assert!(preparation.state.lock().unwrap().ready.is_some());
        fs::write(paks.join("fixture.utoc"), b"changed archive")?;
        assert!(preparation.cached(&executable, &game_hash)?.is_none());
        fs::write(paks.join("fixture.utoc"), b"utoc")?;
        let mut receipt = receipt;
        receipt.source = source(&executable)?.0;
        fs::write(root.join("current.json"), serde_json::to_vec(&receipt)?)?;
        fs::write(directory.join("pixel.bgra"), b"corrupt")?;
        assert!(preparation.cached(&executable, &game_hash)?.is_none());
        receipt.directory = "../assets-fixture".into();
        fs::write(root.join("current.json"), serde_json::to_vec(&receipt)?)?;
        assert!(preparation.cached(&executable, &game_hash)?.is_none());
        Ok(())
    }
}
