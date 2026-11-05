//! Small executable-scoped object-index hints. Addresses and observations are never persisted.
//! Normal lookup reads only recorded slots; bounded recovery repairs missing/stale hints.
use crate::{ObjectStamp, RuntimeCache, recipe};
use anyhow::{Context, Result, ensure};
use jmap_dumper::{
    mem::{Ctx, Ptr},
    objects::{FChunkedFixedUObjectArray, UClass, UObject, UStruct},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Instant, SystemTime, UNIX_EPOCH},
};
#[derive(Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
struct Record {
    version: u8,
    game_hash: String,
    definitions: BTreeMap<String, u32>,
    managers: Vec<u32>,
}
#[derive(Default)]
struct State {
    record: Option<Record>,
    attempts: Option<((u32, u64, i32, i32), u8)>,
}
pub(crate) struct Lookup {
    root: PathBuf,
    state: Mutex<State>,
}
struct Instance {
    controller: (ObjectStamp, i32),
    player: (ObjectStamp, i32),
    viewport: (ObjectStamp, i32),
    game: (ObjectStamp, i32),
}
impl Instance {
    async fn read(ctx: &Ctx, base: &RuntimeCache) -> Result<Self> {
        let controller = recipe::allocation(ctx, base.controller.address).await?;
        ensure!(controller.0 == base.controller, "Lookup controller changed");
        let player = recipe::allocation(
            ctx,
            ctx.read::<u64>(base.controller.address + base.fields.player).await?,
        )
        .await?;
        ensure!(
            ctx.read::<u64>(player.0.address + base.fields.player_controller).await?
                == base.controller.address,
            "Lookup local player ownership"
        );
        ensure!(
            ancestor(ctx, player.0.class, "/Script/Engine.LocalPlayer").await?,
            "Lookup player class"
        );
        let viewport =
            recipe::allocation(ctx, ctx.read::<u64>(player.0.address + 120).await?).await?;
        ensure!(
            ancestor(ctx, viewport.0.class, "/Script/Engine.GameViewportClient").await?,
            "Lookup viewport class"
        );
        let game =
            recipe::allocation(ctx, ctx.read::<u64>(viewport.0.address + 128).await?).await?;
        ensure!(
            ancestor(ctx, game.0.class, "/Script/Engine.GameInstance").await?,
            "Lookup game instance class"
        );
        Ok(Self {
            controller,
            player,
            viewport,
            game,
        })
    }
    async fn check(&self, ctx: &Ctx, base: &RuntimeCache) -> Result<()> {
        ensure!(
            recipe::allocation(ctx, self.controller.0.address).await? == self.controller
                && recipe::allocation(ctx, self.player.0.address).await? == self.player
                && recipe::allocation(ctx, self.viewport.0.address).await? == self.viewport
                && recipe::allocation(ctx, self.game.0.address).await? == self.game
                && ctx.read::<u64>(base.controller.address + base.fields.player).await?
                    == self.player.0.address
                && ctx.read::<u64>(self.player.0.address + base.fields.player_controller).await?
                    == base.controller.address
                && ctx.read::<u64>(self.player.0.address + 120).await? == self.viewport.0.address
                && ctx.read::<u64>(self.viewport.0.address + 128).await? == self.game.0.address,
            "Lookup owner changed"
        );
        Ok(())
    }
}
async fn ancestor(ctx: &Ctx, mut class: u64, wanted: &str) -> Result<bool> {
    let mut seen = BTreeSet::new();
    for _ in 0..32 {
        ensure!(seen.insert(class), "Lookup class ancestry cycle");
        if crate::path(ctx, class).await? == wanted {
            return Ok(true);
        }
        let Some(parent) = Ptr::<UStruct>::new(class, ctx.clone())?.super_struct().read().await?
        else {
            return Ok(false);
        };
        class = parent.address();
    }
    anyhow::bail!("Lookup class ancestry bound")
}
impl Record {
    fn validate(&self, hash: &str) -> Result<()> {
        let wanted = recipe::definition_paths();
        ensure!(
            self.version == 1
                && self.game_hash == hash
                && self.definitions.len() == wanted.len()
                && self.definitions.keys().eq(wanted.iter())
                && !self.managers.is_empty()
                && self.managers.len() <= 16,
            "Invalid native index cache shape/build"
        );
        let mut seen = BTreeSet::new();
        ensure!(
            self.definitions
                .values()
                .chain(self.managers.iter())
                .all(|i| *i < 2_000_000 && seen.insert(*i)),
            "Invalid/duplicate native index hint"
        );
        Ok(())
    }
}
async fn indexed(ctx: &Ctx, index: u32) -> Result<Option<u64>> {
    let registry = Ptr::<FChunkedFixedUObjectArray>::new(
        ctx.uobjectarray + ctx.struct_member("FUObjectArray", "ObjObjects") as u64,
        ctx.clone(),
    )?;
    let count = registry.num_elements().read().await?;
    ensure!((1..=2_000_000).contains(&count), "Native lookup registry bound");
    if index >= count as u32 {
        return Ok(None);
    }
    let item = registry.read_item_ptr(index as usize).await?;
    let address = ctx.read::<u64>(item.address()).await?;
    if !(0x10000..0x800000000000).contains(&address) || !address.is_multiple_of(8) {
        return Ok(None);
    }
    let object = Ptr::<UObject>::new(address, ctx.clone())?;
    if object.object_flags().read().await?.bits() & (0x10 | 0x20 | 0x8000 | 0x10000) != 0 {
        return Ok(None);
    }
    let allocation = recipe::allocation(ctx, address).await?;
    if allocation.0.index != index as i32 {
        return Ok(None);
    }
    Ok(Some(address))
}
// Default objects are metadata templates, not live instances. Validate their exact
// UClass-owned identity separately; ordinary allocation guards remain unchanged.
async fn default_proof(
    ctx: &Ctx,
    address: u64,
    class: u64,
    wanted: &str,
) -> Result<(ObjectStamp, i32)> {
    let object = Ptr::<UObject>::new(address, ctx.clone())?;
    let flags = object.object_flags().read().await?.bits();
    ensure!(flags & 0x10 != 0 && flags & (0x8000 | 0x10000) == 0, "Native default object flags");
    ensure!(
        object.class_private().read().await?.address() == class
            && crate::path(ctx, address).await? == wanted,
        "Native default object identity"
    );
    let index = object.internal_index().read().await?;
    let registry = Ptr::<FChunkedFixedUObjectArray>::new(
        ctx.uobjectarray + ctx.struct_member("FUObjectArray", "ObjObjects") as u64,
        ctx.clone(),
    )?;
    let count = registry.num_elements().read().await?;
    ensure!(
        index >= 0
            && index < count
            && count <= 2_000_000
            && ctx.struct_member("FUObjectItem", "SerialNumber") == 16,
        "Native default registry bound"
    );
    let slot = registry.read_item_ptr(index as usize).await?;
    ensure!(
        ctx.read::<u64>(slot.address()).await? == address,
        "Native default registry allocation changed"
    );
    let serial = ctx.read::<i32>(slot.address() + 16).await?;
    ensure!(serial >= 0, "Native default serial");
    Ok((
        ObjectStamp {
            address,
            index,
            class,
            name: object.name_private().read().await?,
        },
        serial,
    ))
}
async fn targets(
    ctx: &Ctx,
    record: &Record,
    instance: &Instance,
) -> Result<Option<Vec<(usize, u64)>>> {
    let mut result = Vec::with_capacity(record.definitions.len() + 3);
    let mut proofs = Vec::new();
    let mut defaults = Vec::new();
    let mut definitions = BTreeMap::new();
    for (path, index) in &record.definitions {
        let Some(address) = indexed(ctx, *index).await? else {
            return Ok(None);
        };
        if crate::path(ctx, address).await? != *path {
            return Ok(None);
        }
        proofs.push(recipe::allocation(ctx, address).await?);
        definitions.insert(path.as_str(), address);
        result.push((*index as usize, address));
    }
    for (owner, wanted) in [
        ("/Script/UMG.Widget", "/Script/UMG.Default__Widget"),
        (
            "/Script/ChimeraUI.CrUW_RecipeTableRecipe",
            "/Script/ChimeraUI.Default__CrUW_RecipeTableRecipe",
        ),
    ] {
        let class = definitions[owner];
        let object = Ptr::<UClass>::new(class, ctx.clone())?
            .class_default_object()
            .read()
            .await?
            .context("Native default object unavailable")?;
        ensure!(
            crate::path(ctx, object.address()).await? == wanted,
            "Native default object identity"
        );
        let proof = default_proof(ctx, object.address(), class, wanted).await?;
        ensure!(proof.0.class == class, "Native default object class changed");
        result.push((proof.0.index as usize, object.address()));
        defaults.push((class, wanted, proof));
    }
    let mut managers = 0;
    for index in &record.managers {
        let Some(address) = indexed(ctx, *index).await? else {
            continue;
        };
        let proof = recipe::allocation(ctx, address).await?;
        if proof.0.name.starts_with("CrUIManagerSubsystem_")
            && ctx.read::<u64>(address + 32).await? == instance.game.0.address
            && ancestor(ctx, proof.0.class, "/Script/CommonGame.GameUIManagerSubsystem").await?
        {
            managers += 1;
            result.push((*index as usize, address));
            proofs.push(proof);
        }
    }
    if managers != 1 {
        return Ok(None);
    }
    for proof in proofs {
        ensure!(
            recipe::allocation(ctx, proof.0.address).await? == proof,
            "Native lookup allocation changed"
        )
    }
    for (class, wanted, proof) in defaults {
        ensure!(
            default_proof(ctx, proof.0.address, class, wanted).await? == proof,
            "Native default object changed during lookup"
        );
        ensure!(
            Ptr::<UClass>::new(class, ctx.clone())?
                .class_default_object()
                .read()
                .await?
                .is_some_and(|o| o.address() == proof.0.address),
            "Native class default pointer changed"
        );
    }
    Ok(Some(result))
}
impl Lookup {
    pub fn new(directory: &Path) -> Self {
        Self {
            root: directory.join("native-lookups"),
            state: Mutex::new(State::default()),
        }
    }
    /// Runs on a dedicated native worker. This lock serializes only index-cache loading/recovery,
    /// never frame publication. Native read futures execute synchronously on this same thread.
    pub fn resolve(&self, ctx: &Ctx, base: &RuntimeCache) -> Result<Vec<(usize, u64)>> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        smol::block_on(async {
            ensure!(
                starrupture_observation_model::overlay_stream::valid_executable_hash(
                    &base.identity.executable_sha256
                ),
                "Native lookup build hash"
            );
            let instance = Instance::read(ctx, base).await?;
            let hash = &base.identity.executable_sha256;
            let file = self.root.join(format!("{hash}.json"));
            if state.record.as_ref().is_none_or(|r| r.game_hash != *hash) {
                state.record = crate::cache::read_json::<Record>(&file, 65536)
                    .ok()
                    .filter(|r| r.validate(hash).is_ok());
            }
            if let Some(record) = &state.record
                && let Some(result) = targets(ctx, record, &instance).await?
            {
                instance.check(ctx, base).await?;
                eprintln!(
                    "Recipe metadata indexed lookup: {} objects; no recovery scan.",
                    result.len()
                );
                return Ok(result);
            }
            let key = (
                base.identity.pid,
                base.identity.creation_ticks,
                instance.game.0.index,
                instance.game.1,
            );
            let attempts = match state.attempts {
                Some((k, n)) if k == key => n,
                _ => 0,
            };
            ensure!(attempts < 3, "Native index recovery limit reached for this game instance");
            state.attempts = Some((key, attempts + 1));
            eprintln!(
                "Native index recovery {}/3: cache missing or stale; rebuilding bounded hints.",
                attempts + 1
            );
            let began = Instant::now();
            let wanted = recipe::definition_paths();
            let names: BTreeSet<_> = wanted.iter().filter_map(|p| p.rsplit('.').next()).collect();
            let mut definitions = BTreeMap::new();
            let mut managers = Vec::new();
            for (index, address) in recipe::registered_objects(ctx).await? {
                let object = Ptr::<UObject>::new(address, ctx.clone())?;
                if object.internal_index().read().await.ok() != Some(index as i32) {
                    continue;
                }
                let name = object.name_private().read().await?;
                if name.starts_with("CrUIManagerSubsystem_")
                    && ctx.read::<u64>(address + 32).await? == instance.game.0.address
                {
                    ensure!(managers.len() < 16, "Native UI manager candidate bound");
                    managers.push(index as u32);
                }
                if names.contains(name.as_str()) {
                    let path = crate::path(ctx, address).await?;
                    if wanted.contains(&path) {
                        ensure!(
                            definitions.insert(path, index as u32).is_none(),
                            "Ambiguous native definition path"
                        )
                    }
                }
            }
            let record = Record {
                version: 1,
                game_hash: hash.clone(),
                definitions,
                managers,
            };
            record.validate(hash)?;
            let result = targets(ctx, &record, &instance)
                .await?
                .context("Recovered native indices did not validate")?;
            instance.check(ctx, base).await?;
            if let Err(e) = save(&file, &record) {
                eprintln!("Native index hints could not be saved: {e:#}")
            }
            state.record = Some(record);
            eprintln!(
                "Native index recovery completed in {} ms; future lookups use {} recorded objects.",
                began.elapsed().as_millis(),
                result.len()
            );
            Ok(result)
        })
    }
}
fn save(path: &Path, record: &Record) -> Result<()> {
    record.validate(&record.game_hash)?;
    let bytes = serde_json::to_vec(record)?;
    ensure!(bytes.len() <= 65536, "Native index cache size bound");
    let parent = path.parent().context("Native index cache parent")?;
    std::fs::create_dir_all(parent)?;
    let suffix = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let temporary = path.with_extension(format!("{}-{suffix}.tmp", std::process::id()));
    let result = (|| -> Result<()> {
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, path)?;
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
    fn record() -> Record {
        Record {
            version: 1,
            game_hash: "a".repeat(64),
            definitions: recipe::definition_paths()
                .into_iter()
                .enumerate()
                .map(|(i, p)| (p, i as u32))
                .collect(),
            managers: vec![1000],
        }
    }
    #[test]
    fn stale_build_missing_paths_and_duplicate_indices_are_rejected() {
        let good = record();
        assert!(good.validate(&"a".repeat(64)).is_ok());
        assert!(good.validate(&"b".repeat(64)).is_err());
        let mut bad = good.clone();
        bad.definitions.pop_first();
        assert!(bad.validate(&good.game_hash).is_err());
        let mut bad = good.clone();
        bad.managers = vec![*bad.definitions.values().next().unwrap()];
        assert!(bad.validate(&good.game_hash).is_err());
        let mut bad = good.clone();
        bad.managers = vec![2_000_000];
        assert!(bad.validate(&good.game_hash).is_err());
        let mut bad = good.clone();
        bad.managers = vec![];
        assert!(bad.validate(&good.game_hash).is_err());
    }
    #[test]
    fn index_cache_replacement_is_bounded_and_never_persists_addresses() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../local/native-lookup-tests")
            .join(format!(
                "{}-{}",
                std::process::id(),
                SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
            ));
        struct Temp(PathBuf);
        impl Drop for Temp {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _temp = Temp(root.clone());
        let path = root.join("cache.json");
        let mut first = record();
        save(&path, &first).unwrap();
        first.managers = vec![1001];
        save(&path, &first).unwrap();
        let read: Record = crate::cache::read_json(&path, 65536).unwrap();
        assert_eq!(read.managers, vec![1001]);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("address"));
        assert!(!text.contains("submitted"));
        std::fs::write(&path, vec![b' '; 65537]).unwrap();
        assert!(crate::cache::read_json::<Record>(&path, 65536).is_err());
    }
}
