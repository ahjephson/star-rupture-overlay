use super::{Catalogue, NativeContext, archive::Archive, normalize};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use starrupture_observation_model::shutdown::StopSignal;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Cursor, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Source {
    paks: PathBuf,
    files: BTreeMap<String, (u64, u128)>,
}
impl Source {
    fn read(paks: &Path) -> Result<Self> {
        let paks = paks.canonicalize()?;
        let mut files = BTreeMap::new();
        for e in fs::read_dir(&paks)? {
            let e = e?;
            let p = e.path();
            if !p.extension().is_some_and(|s| ["utoc", "ucas", "pak"].iter().any(|x| s == *x)) {
                continue;
            }
            let m = e.metadata()?;
            ensure!(m.is_file() && files.len() < 512, "Archive fingerprint bound");
            let name = e
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("Archive filename encoding"))?;
            files.insert(name, (m.len(), m.modified()?.duration_since(UNIX_EPOCH)?.as_nanos()));
        }
        ensure!(!files.is_empty(), "No game archives");
        Ok(Self {
            paks,
            files,
        })
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Image {
    hash: String,
    width: u32,
    height: u32,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    version: u32,
    key: String,
    source: Source,
    generation: String,
    catalogue_hash: String,
    icons: BTreeMap<String, Image>,
}
#[derive(Default, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Metrics {
    pub cache_hit: bool,
    pub archive_index_passes: u32,
    pub decoded_packages: usize,
    pub icon_count: usize,
}
/// Prepared metadata owns no decoded icons. Callers retain only images currently displayed.
pub struct Prepared {
    pub catalogue: Arc<Catalogue>,
    root: PathBuf,
    icons: BTreeMap<String, Image>,
    search: Vec<(String, String)>,
    pub metrics: Metrics,
}
pub struct Icon {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}
impl Prepared {
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<&str>> {
        ensure!(query.len() <= 256 && limit <= 256, "Search bound");
        let query = query.to_lowercase();
        Ok(self
            .search
            .iter()
            .filter(|(_, name)| name.contains(&query))
            .take(limit)
            .map(|(id, _)| id.as_str())
            .collect())
    }
    pub fn icon(&self, id: &str) -> Result<Icon> {
        let image = self.icons.get(id).context("Unknown catalogue icon")?;
        let bytes = super::super::bounded(
            &self.root.join(format!("{}.bgra", digest(id.as_bytes()))),
            4 * 1024 * 1024 + 8,
        )?;
        ensure!(digest(&bytes) == image.hash, "Catalogue icon integrity");
        let (w, h) = shape(&bytes)?;
        ensure!(w == image.width && h == image.height, "Catalogue icon manifest dimensions");
        Ok(Icon {
            width: w,
            height: h,
            pixels: bytes[8..].to_vec(),
        })
    }
    pub fn retained_icon_bytes(&self) -> usize {
        0
    }
}
fn shape(bytes: &[u8]) -> Result<(u32, u32)> {
    ensure!(bytes.len() >= 8, "Icon header");
    let w = u32::from_le_bytes(bytes[..4].try_into()?);
    let h = u32::from_le_bytes(bytes[4..8].try_into()?);
    ensure!(
        (1..=1024).contains(&w)
            && (1..=1024).contains(&h)
            && bytes.len() == 8 + w as usize * h as usize * 4,
        "Icon dimensions/payload"
    );
    Ok((w, h))
}
fn key(context: &NativeContext) -> Result<String> {
    context.validate().map_err(anyhow::Error::msg)?;
    let mut c = context.clone();
    c.buildings.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(digest(&serde_json::to_vec(&c)?))
}
fn required_icons(c: &Catalogue) -> BTreeSet<String> {
    c.items
        .values()
        .map(|v| v.icon.clone())
        .chain(c.buildings.values().map(|v| v.icon.clone()))
        .chain(c.recipes.values().map(|v| v.icon.clone()))
        .collect()
}
fn icon_ids(c: &Catalogue) -> BTreeSet<String> {
    let mut icons = required_icons(c);
    icons.extend(c.lock_icon.clone());
    icons.extend(c.corporations.values().filter_map(|v| v.icon.clone()));
    icons.extend(c.weapon_purchases.values().filter_map(|v| v.icon.clone()));
    icons.extend(
        c.weapon_purchases.values().flat_map(|v| v.mods.values().filter_map(|m| m.icon.clone())),
    );
    icons
}
fn discard_missing_optional_icons(c: &mut Catalogue, kept: &BTreeMap<String, Image>) {
    let discard = |icon: &mut Option<String>| {
        if icon.as_ref().is_some_and(|id| !kept.contains_key(id)) {
            *icon = None;
        }
    };
    discard(&mut c.lock_icon);
    for corporation in c.corporations.values_mut() {
        discard(&mut corporation.icon);
    }
    for weapon in c.weapon_purchases.values_mut() {
        discard(&mut weapon.icon);
        for upgrade in weapon.mods.values_mut() {
            discard(&mut upgrade.icon);
        }
    }
}
fn safe_generation(s: &str) -> bool {
    s.starts_with("catalogue-")
        && s.len() <= 100
        && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}
/// A per-feature store. It performs no work until lookup or preparation is requested.
pub struct Store {
    root: PathBuf,
    preparing: std::sync::Mutex<()>,
}
impl Store {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            preparing: std::sync::Mutex::new(()),
        }
    }
    pub fn lookup(&self, paks: &Path, context: &NativeContext) -> Result<Option<Prepared>> {
        let key = key(context)?;
        let file = self.root.join(format!("{key}.json"));
        if !file.exists() {
            return Ok(None);
        }
        let admitted = (|| -> Result<Prepared> {
            let receipt: Receipt =
                serde_json::from_slice(&super::super::bounded(&file, 2 * 1024 * 1024)?)?;
            ensure!(
                receipt.version == 1
                    && receipt.key == key
                    && safe_generation(&receipt.generation)
                    && receipt.source == Source::read(paks)?,
                "Stale catalogue receipt"
            );
            let root = self.root.join(&receipt.generation);
            let bytes = super::super::bounded(&root.join("catalogue.json"), 16 * 1024 * 1024)?;
            ensure!(digest(&bytes) == receipt.catalogue_hash, "Catalogue metadata integrity");
            let mut catalogue: Catalogue = serde_json::from_slice(&bytes)?;
            catalogue.normalize_currency_labels();
            catalogue.validate()?;
            ensure!(
                self::key(&catalogue.context)? == key
                    && receipt.icons.keys().cloned().collect::<BTreeSet<_>>()
                        == icon_ids(&catalogue),
                "Catalogue context/icon scope"
            );
            for image in receipt.icons.values() {
                ensure!(
                    image.hash.len() == 64
                        && image.hash.bytes().all(|b| b.is_ascii_hexdigit())
                        && (1..=1024).contains(&image.width)
                        && (1..=1024).contains(&image.height),
                    "Icon receipt shape"
                );
            }
            let search =
                catalogue.items.iter().map(|(id, v)| (id.clone(), v.name.to_lowercase())).collect();
            let count = receipt.icons.len();
            Ok(Prepared {
                catalogue: Arc::new(catalogue),
                root,
                icons: receipt.icons,
                search,
                metrics: Metrics {
                    cache_hit: true,
                    icon_count: count,
                    ..Metrics::default()
                },
            })
        })();
        match admitted {
            Ok(p) => Ok(Some(p)),
            Err(_) => Ok(None),
        }
    }
    pub fn prepare(
        &self,
        paks: &Path,
        context: NativeContext,
        mappings: &[u8],
        stop: &StopSignal,
    ) -> Result<Prepared> {
        let _gate = self.preparing.lock().unwrap_or_else(|e| e.into_inner());
        ensure!(!stop.is_requested(), "Catalogue preparation cancelled");
        if let Some(p) = self.lookup(paks, &context)? {
            return Ok(p);
        }
        let source = Source::read(paks)?;
        fs::create_dir_all(&self.root)?;
        let root = self.root.canonicalize()?;
        ensure!(
            !root.starts_with(
                source.paks.parent().and_then(Path::parent).context("Game directory")?
            ),
            "Catalogue cache must be outside game archives"
        );
        ensure!(
            mappings.len() <= 16 * 1024 * 1024 && mappings.get(3) == Some(&0),
            "Native schema bounds/compression"
        );
        let schemas = usmap::Usmap::read(&mut Cursor::new(mappings))?;
        super::super::archives::preflight(&source.paks)?;

        ensure!(!stop.is_requested(), "Catalogue preparation cancelled");
        let store = retoc::iostore::open(&source.paks, Arc::new(retoc::Config::default()))?;
        let log = retoc::logging::Log::no_log();
        let mut archive = Archive::new(store.as_ref(), schemas, &log, stop)?;
        let mut catalogue = normalize::build(&mut archive, context)?;
        catalogue.normalize_currency_labels();
        let key = key(&catalogue.context)?;
        let name = format!(
            "catalogue-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        );
        let generation = root.join(&name);
        fs::create_dir(&generation)?;
        let mut pending = Pending(Some(generation.clone()));
        let mut icons = BTreeMap::new();
        let ids = icon_ids(&catalogue);
        ensure!(ids.len() <= 1024, "Catalogue icon count bound");
        let mut icon_bytes = 0usize;
        let required = required_icons(&catalogue);
        for id in ids {
            let bytes = match archive.icon(&id) {
                Ok(bytes) => bytes,
                Err(error) if !required.contains(&id) && !stop.is_requested() => {
                    eprintln!("Codex optional icon {id}: {error:#}");
                    continue;
                }
                Err(error) => return Err(error.context(format!("Preparing icon {id}"))),
            };
            let (width, height) = shape(&bytes)?;
            icon_bytes = icon_bytes.checked_add(bytes.len()).context("Icon byte overflow")?;
            ensure!(icon_bytes <= 256 * 1024 * 1024, "Catalogue icon disk budget");
            fs::write(generation.join(format!("{}.bgra", digest(id.as_bytes()))), &bytes)?;
            icons.insert(
                id,
                Image {
                    hash: digest(&bytes),
                    width,
                    height,
                },
            );
        }
        discard_missing_optional_icons(&mut catalogue, &icons);
        let packages = archive.packages;
        drop(archive);
        drop(store);
        ensure!(
            !stop.is_requested() && Source::read(paks)? == source,
            "Cancelled or changed game archives during catalogue setup"
        );
        let bytes = serde_json::to_vec(&catalogue)?;
        ensure!(bytes.len() <= 16 * 1024 * 1024, "Catalogue serialized byte bound");
        fs::write(generation.join("catalogue.json"), &bytes)?;
        let receipt = Receipt {
            version: 1,
            key: key.clone(),
            source,
            generation: name,
            catalogue_hash: digest(&bytes),
            icons,
        };
        let temp = root.join(format!("{key}-{}.pending", std::process::id()));
        let mut receipt_pending = Pending(Some(temp.clone()));
        let mut file = fs::OpenOptions::new().create_new(true).write(true).open(&temp)?;
        file.write_all(&serde_json::to_vec(&receipt)?)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, root.join(format!("{key}.json")))?;
        receipt_pending.0 = None;
        pending.0 = None;
        let mut prepared = self
            .lookup(paks, &catalogue.context)?
            .context("Prepared catalogue did not validate")?;
        prepared.metrics.cache_hit = false;
        prepared.metrics.archive_index_passes = 1;
        prepared.metrics.decoded_packages = packages;
        Ok(prepared)
    }
}
struct Pending(Option<PathBuf>);
impl Drop for Pending {
    fn drop(&mut self) {
        if let Some(p) = &self.0 {
            if p.is_dir() {
                let _ = fs::remove_dir_all(p);
            } else {
                let _ = fs::remove_file(p);
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_corrupt_pixel_shapes_and_path_like_generation_names() {
        assert!(shape(&[0; 8]).is_err());
        let mut p = vec![1, 0, 0, 0, 1, 0, 0, 0];
        assert!(shape(&p).is_err());
        p.extend([0; 4]);
        assert_eq!(shape(&p).unwrap(), (1, 1));
        for s in ["../catalogue-a", "catalogue-a/child", "C:/catalogue-a"] {
            assert!(!safe_generation(s))
        }
        assert!(safe_generation("catalogue-123-456"));
    }
}

#[cfg(test)]
mod cache_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_FIXTURE: AtomicUsize = AtomicUsize::new(0);
    fn catalogue() -> Result<Catalogue> {
        Ok(serde_json::from_value(serde_json::json!({
            "version":8,"lock_icon":null,"corporations":{},
            "context":{"version":1,"gameHash":"a".repeat(64),"modeClass":"/Game/Chimera/Mode.Mode_C","stateClass":"/Game/Chimera/State.State_C","keeperClass":"/Game/Chimera/Keeper.Keeper_C","buildings":[{"id":"/Game/Chimera/Forge","category":1}],"weaponClasses":["/ChimeraWeapon/Pistol.Pistol_C"],"weaponModAssets":[],"coreAsset":"/Game/Chimera/Core","defaults":{"addPlatform":false,"addPillar":false,"recipeSeconds":10.0,"craftingLoopSeconds":0.0,"overwriteRecipeSeconds":-1.0,"useCraftingProcessor":true}},
            "items":{
                "/Game/Chimera/Ore":{"name":"Calcium Ore","description":null,"icon":"/Game/Chimera/T_Ore"},
                "/Game/Chimera/Items/I_ArmoryToken":{"name":"Armory Token","description":null,"icon":"/Game/Chimera/T_Ore"}
            },
            "recipes":{"/Game/Chimera/CR_Ore":{"name":"Ore","output":{"item":"/Game/Chimera/Ore","quantity":2},"inputs":[{"item":"/Game/Chimera/Ore","quantity":1}],"producers":["/Game/Chimera/Forge"],"base_seconds":10.0,"icon":"/Game/Chimera/T_Ore","unlock":{"kind":"Building"}}},
            "buildings":{"/Game/Chimera/Forge":{"name":"Forge","icon":"/Game/Chimera/T_Ore","category":1,"materials":[],"add_platform":false,"add_pillar":false,"unlocks":[{"kind":"Corporation","id":"/Game/Chimera/Corp","name":"Corporation","level":1,"collection":null}],"core_progression":false,"crafting":null}},
            "weapon_purchases":{},"war_bond_currency":null,"equipment_material_names":{},"core_tiers":[{"source_index":0,"materials":[],"cooling":100}],"excluded":{}
        }))?)
    }
    struct Fixture {
        directory: PathBuf,
        store: Store,
        paks: PathBuf,
        context: NativeContext,
        receipt: PathBuf,
        image: PathBuf,
        metadata: PathBuf,
    }
    impl Fixture {
        fn new() -> Result<Self> {
            let directory = std::env::temp_dir().join(format!(
                "starrupture-codex-test-{}-{}-{}",
                std::process::id(),
                SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos(),
                NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed),
            ));
            fs::create_dir(&directory)?;
            let paks = directory.join("game/Content/Paks");
            fs::create_dir_all(&paks)?;
            fs::write(
                paks.join("synthetic.utoc"),
                b"fake archive; warm lookup must never decode it",
            )?;
            let root = directory.join("cache");
            let generation = root.join("catalogue-1-1");
            fs::create_dir_all(&generation)?;
            let catalogue = catalogue()?;
            catalogue.validate()?;
            let bytes = serde_json::to_vec(&catalogue)?;
            let metadata = generation.join("catalogue.json");
            fs::write(&metadata, &bytes)?;
            let icon = "/Game/Chimera/T_Ore";
            let pixels = vec![1, 0, 0, 0, 1, 0, 0, 0, 10, 20, 30, 255];
            let image = generation.join(format!("{}.bgra", digest(icon.as_bytes())));
            fs::write(&image, &pixels)?;
            let key = key(&catalogue.context)?;
            let r = Receipt {
                version: 1,
                key: key.clone(),
                source: Source::read(&paks)?,
                generation: "catalogue-1-1".into(),
                catalogue_hash: digest(&bytes),
                icons: BTreeMap::from([(
                    icon.into(),
                    Image {
                        hash: digest(&pixels),
                        width: 1,
                        height: 1,
                    },
                )]),
            };
            let receipt = root.join(format!("{key}.json"));
            fs::write(&receipt, serde_json::to_vec(&r)?)?;
            Ok(Self {
                directory,
                store: Store::new(root),
                paks,
                context: catalogue.context,
                receipt,
                image,
                metadata,
            })
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.directory);
        }
    }
    #[test]
    fn warm_lookup_and_search_never_decode_archives_or_load_icons() -> Result<()> {
        let f = Fixture::new()?;
        fs::remove_file(&f.image)?;
        let p = f.store.lookup(&f.paks, &f.context)?.context("warm fixture")?;
        assert_eq!(p.metrics.archive_index_passes, 0);
        assert_eq!(p.metrics.decoded_packages, 0);
        assert_eq!(p.retained_icon_bytes(), 0);
        assert_eq!(p.search("CALCIUM", 10)?, vec!["/Game/Chimera/Ore"]);
        assert!(p.search("ore", 257).is_err());
        assert!(p.search(&"x".repeat(257), 1).is_err());
        assert!(p.icon("/Game/Chimera/T_Ore").is_err());
        Ok(())
    }
    #[test]
    fn cached_native_currency_adopts_the_in_game_name() -> Result<()> {
        let f = Fixture::new()?;
        let prepared = f.store.lookup(&f.paks, &f.context)?.context("warm fixture")?;
        assert_eq!(prepared.catalogue.items[super::super::model::WAR_BONDS].name, "War Bonds");
        assert_eq!(prepared.search("war bonds", 10)?, vec![super::super::model::WAR_BONDS]);
        assert!(prepared.search("armory token", 10)?.is_empty());
        assert_eq!(prepared.metrics.archive_index_passes, 0);
        Ok(())
    }

    #[test]
    fn changed_archives_context_or_metadata_invalidate_the_cache() -> Result<()> {
        let f = Fixture::new()?;
        assert!(f.store.lookup(&f.paks, &f.context)?.is_some());
        let mut other = f.context.clone();
        other.game_hash = "b".repeat(64);
        assert!(f.store.lookup(&f.paks, &other)?.is_none());
        other = f.context.clone();
        other.defaults.recipe_seconds = 20.0;
        assert!(f.store.lookup(&f.paks, &other)?.is_none());
        fs::write(&f.metadata, b"corrupt")?;
        assert!(f.store.lookup(&f.paks, &f.context)?.is_none());
        let f = Fixture::new()?;
        fs::write(f.paks.join("synthetic.utoc"), b"changed")?;
        assert!(f.store.lookup(&f.paks, &f.context)?.is_none());
        Ok(())
    }
    #[test]
    fn icon_integrity_is_checked_on_demand_and_cancellation_preserves_the_good_generation()
    -> Result<()> {
        let f = Fixture::new()?;
        let p = f.store.lookup(&f.paks, &f.context)?.context("cache")?;
        assert_eq!(p.icon("/Game/Chimera/T_Ore")?.pixels, vec![10, 20, 30, 255]);
        fs::write(&f.image, [0; 12])?;
        assert!(p.icon("/Game/Chimera/T_Ore").is_err());
        assert!(f.store.lookup(&f.paks, &f.context)?.is_some());
        let receipt = fs::read(&f.receipt)?;
        let stop = StopSignal::default();
        stop.request();
        assert!(f.store.prepare(&f.paks, f.context.clone(), &[], &stop).is_err());
        assert_eq!(fs::read(&f.receipt)?, receipt);
        Ok(())
    }
    #[test]
    fn malformed_relations_and_template_ids_are_rejected() -> Result<()> {
        let mut c = catalogue()?;
        c.recipes.values_mut().next().unwrap().inputs[0].quantity = 0;
        assert!(c.validate().is_err());
        let mut c = catalogue()?;
        c.recipes.values_mut().next().unwrap().producers = vec!["/Game/Chimera/Missing".into()];
        assert!(c.validate().is_err());
        let mut c = catalogue()?;
        c.context.buildings[0].id = "../elsewhere".into();
        assert!(c.validate().is_err());
        Ok(())
    }
}
