//! Finite read-only codex research. Not called by the overlay.
use anyhow::{Context, Result, ensure};
use jmap::EClassCastFlags as K;
pub use jmap_dumper::mem::Mem;
use jmap_dumper::{
    containers::{FName, FScriptMap},
    mem::{Ctx, Ptr},
    objects::{
        FChunkedFixedUObjectArray, FUObjectArray, UClass, UObject, UStruct, ZBoolProperty,
        ZProperty, ZStructProperty,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::Arc,
};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Identity {
    pid: u32,
    creation_ticks: u64,
    executable: String,
    executable_sha256: String,
}
pub mod overlay_stream {
    pub use starrupture_overlay_reader::overlay_stream::GameStatus;
}
#[path = "../src/discovery_memory.rs"]
mod discovery_memory;
#[allow(
    dead_code,
    reason = "Reuse the unmodified query/read-only OS boundary in a finite research tool."
)]
#[path = "../src/platform.rs"]
mod platform;

async fn stamp(ctx: &Ctx, at: u64) -> Result<Value> {
    let obj = Ptr::<UObject>::new(at, ctx.clone())?;
    let index = obj.internal_index().read().await?;
    let registry = Ptr::<FUObjectArray>::new(ctx.uobjectarray, ctx.clone())?;
    ensure!(
        index >= 0
            && index < registry.num_elements().await?
            && registry.read_item_ptr(index as usize).await?.is_some_and(|p| p.address() == at),
        "Research allocation identity"
    );
    let items = Ptr::<FChunkedFixedUObjectArray>::new(
        ctx.uobjectarray + ctx.struct_member("FUObjectArray", "ObjObjects") as u64,
        ctx.clone(),
    )?;
    let slot = items.read_item_ptr(index as usize).await?;
    let serial: i32 =
        ctx.read(slot.address() + ctx.struct_member("FUObjectItem", "SerialNumber") as u64).await?;
    Ok(
        json!({"address":at,"index":index,"serial":serial,"class":obj.class_private().read().await?.address(),"flags":obj.object_flags().read().await?.bits(),"path":obj.path().await?}),
    )
}
async fn field(ctx: &Ctx, class: u64, name: &str) -> Result<Ptr<ZProperty>> {
    let mut current = Some(Ptr::<UStruct>::new(class, ctx.clone())?);
    let mut seen = BTreeSet::new();
    while let Some(c) = current {
        ensure!(seen.len() < 32 && seen.insert(c.address()), "Research field ancestry");
        let mut properties = c.properties(false);
        let mut n = 0;
        while let Some(p) = properties.next().await {
            n += 1;
            ensure!(n <= 4096, "Research property bound");
            let p = p?;
            if p.zfield().name_private().read().await? == name {
                let off = p.offset_internal().read().await?;
                let size = p.element_size().read().await?;
                ensure!(
                    off >= 0
                        && size > 0
                        && off.checked_add(size).is_some_and(|end| end <= 262144)
                        && p.array_dim().read().await? == 1,
                    "Research field bounds"
                );
                return Ok(p);
            }
        }
        current = c.super_struct().read().await?;
    }
    anyhow::bail!("Missing reflected field {name}")
}
async fn object_field(ctx: &Ctx, owner: u64, name: &str) -> Result<u64> {
    let before = stamp(ctx, owner).await?;
    let class = before["class"].as_u64().context("Object class")?;
    let p = field(ctx, class, name).await?;
    ensure!(
        p.zfield().cast_flags().await?.contains(K::CASTCLASS_FObjectProperty)
            && p.element_size().read().await? == 8,
        "Expected reflected object reference"
    );
    let at = owner + p.offset_internal().read().await? as u64;
    let value: u64 = ctx.read(at).await?;
    stamp(ctx, value).await?;
    ensure!(
        ctx.read::<u64>(at).await? == value && stamp(ctx, owner).await? == before,
        "Object reference changed"
    );
    Ok(value)
}
async fn default_object(ctx: &Ctx, class: u64) -> Result<u64> {
    let class_before = stamp(ctx, class).await?;
    let c = Ptr::<UClass>::new(class, ctx.clone())?;
    let obj = c.class_default_object().read().await?.context("Native CDO absent")?;
    let s = stamp(ctx, obj.address()).await?;
    ensure!(
        s["class"].as_u64() == Some(class) && s["flags"].as_u64().is_some_and(|v| v & 0x10 != 0),
        "Class default ownership"
    );
    ensure!(
        c.class_default_object().read().await?.is_some_and(|v| v.address() == obj.address())
            && stamp(ctx, class).await? == class_before,
        "Class default binding changed"
    );
    Ok(obj.address())
}
async fn scalar(ctx: &Ctx, owner: u64, class: u64, name: &str) -> Result<Value> {
    let p = field(ctx, class, name).await?;
    let off = p.offset_internal().read().await? as u64;
    let size = p.element_size().read().await? as usize;
    ensure!(size <= 256, "Default property byte bound");
    let flags = p.zfield().cast_flags().await?;
    let mut a = vec![0; size];
    let mut b = vec![0; size];
    ctx.read_buf(owner + off, &mut a).await?;
    ctx.read_buf(owner + off, &mut b).await?;
    ensure!(a == b, "Native default changed");
    let value = if flags.contains(K::CASTCLASS_FBoolProperty) {
        let mask = p.cast::<ZBoolProperty>().field_mask().read().await?;
        ensure!(size == 1 && mask != 0, "Native boolean shape");
        json!(a[0] & mask != 0)
    } else if flags.contains(K::CASTCLASS_FArrayProperty) {
        ensure!(size == 16, "Native default array shape");
        let count = i32::from_le_bytes(a[8..12].try_into()?);
        let capacity = i32::from_le_bytes(a[12..16].try_into()?);
        ensure!(count == 0 && capacity >= 0, "Nonempty native default array unsupported");
        json!([])
    } else if flags.contains(K::CASTCLASS_FMapProperty) {
        let map = Ptr::<FScriptMap>::new(owner + off, ctx.clone())?;
        ensure!(
            map.pairs().elements().get_max_index().await? == 0,
            "Nonempty native default map unsupported"
        );
        json!([])
    } else if flags.contains(K::CASTCLASS_FFloatProperty) {
        ensure!(size == 4, "Float size");
        let n = f32::from_le_bytes(a.as_slice().try_into()?);
        ensure!(n.is_finite(), "Finite native default");
        json!(n)
    } else if size == 1 {
        json!(a[0])
    } else {
        anyhow::bail!("Unsupported scalar kind for {name}")
    };
    Ok(json!({"value":value,"offset":off,"size":size,"kind":format!("{flags:?}"),"raw":a}))
}
async fn object_array(ctx: &Ctx, owner: u64, name: &str) -> Result<Value> {
    let before = stamp(ctx, owner).await?;
    let class = before["class"].as_u64().context("Array owner class")?;
    let p = field(ctx, class, name).await?;
    ensure!(
        p.zfield().cast_flags().await?.contains(K::CASTCLASS_FArrayProperty)
            && p.element_size().read().await? == 16,
        "Array field shape"
    );
    let at = owner + p.offset_internal().read().await? as u64;
    let mut h = [0; 16];
    ctx.read_buf(at, &mut h).await?;
    let ptr = u64::from_le_bytes(h[..8].try_into()?);
    let n = i32::from_le_bytes(h[8..12].try_into()?);
    let cap = i32::from_le_bytes(h[12..].try_into()?);
    ensure!(
        (0..=1024).contains(&n) && cap >= n && cap <= 8192 && (n == 0 || ptr >= 0x10000),
        "Array header bounds"
    );
    let mut raw = vec![0; n as usize * 8];
    if n > 0 {
        ctx.read_buf(ptr, &mut raw).await?;
    }
    let mut rows = Vec::new();
    for v in raw.as_chunks::<8>().0 {
        rows.push(stamp(ctx, u64::from_le_bytes(*v)).await?);
    }
    let mut second = [0; 16];
    ctx.read_buf(at, &mut second).await?;
    ensure!(h == second, "Array header changed");
    let mut other = vec![0; raw.len()];
    if n > 0 {
        ctx.read_buf(ptr, &mut other).await?;
    }
    ensure!(raw == other && stamp(ctx, owner).await? == before, "Array content/owner changed");
    Ok(json!({"owner":before,"offset":p.offset_internal().read().await?,"count":n,"objects":rows}))
}
async fn soft_asset_path(ctx: &Ctx, owner: u64, name: &str) -> Result<Value> {
    let before = stamp(ctx, owner).await?;
    let class = before["class"].as_u64().context("Soft reference owner class")?;
    let p = field(ctx, class, name).await?;
    ensure!(
        p.zfield().cast_flags().await?.contains(K::CASTCLASS_FSoftObjectProperty),
        "Expected soft object property"
    );
    let size = usize::try_from(p.element_size().read().await?)?;
    ensure!((16..=128).contains(&size), "Soft reference field size");
    let offset = u64::try_from(p.offset_internal().read().await?)?;
    let width =
        ctx.struct_member("FName", "Number").max(ctx.struct_member("FName", "ComparisonIndex")) + 4;
    ensure!(width >= 8 && 2 * width <= size, "Native name width");
    let mut raw = vec![0; size];
    ctx.read_buf(owner + offset, &mut raw).await?;
    let mut candidates = Vec::new();
    for at in (0..=size - 2 * width).step_by(4) {
        let first = Ptr::<FName>::new(owner + offset + at as u64, ctx.clone())?.read().await;
        let Ok(package) = first else {
            continue;
        };
        if !package.starts_with("/Game/") || package.contains('.') {
            continue;
        }
        let asset =
            Ptr::<FName>::new(owner + offset + (at + width) as u64, ctx.clone())?.read().await?;
        ensure!(
            !asset.is_empty() && asset != "None" && !asset.contains('/'),
            "Soft reference asset name"
        );
        candidates.push(json!({"package":package,"asset":asset,"namePairOffset":at}));
    }
    ensure!(candidates.len() == 1, "Soft reference name pair must be unique");
    let mut second = vec![0; size];
    ctx.read_buf(owner + offset, &mut second).await?;
    ensure!(raw == second && stamp(ctx, owner).await? == before, "Soft reference changed");
    Ok(
        json!({"owner":before,"field":name,"offset":offset,"size":size,"value":candidates.remove(0),"scope":"finite reflected-field name-pair discovery; not a recurring app lookup"}),
    )
}
async fn run(out: &Path, cache: &Path, discover_components: bool) -> Result<()> {
    ensure!(!platform::is_elevated()?, "Research must run unelevated");
    std::fs::create_dir_all(out)?;
    let out = out.canonicalize()?;
    let local = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../local").canonicalize()?;
    ensure!(out.starts_with(local), "Private research directory required");
    let stop = Arc::new(starrupture_observation_model::shutdown::StopSignal::default());
    let budget = platform::ReadBudget::new(stop);
    let process = platform::find_game(budget.clone())?;
    let record: Value = serde_json::from_slice(&std::fs::read(cache.join("session.json"))?)?;
    ensure!(
        serde_json::from_value::<Identity>(record["identity"].clone())? == process.identity,
        "Current validated overlay session required"
    );
    let config: jmap_dumper::Config = serde_json::from_value(record["config"].clone())?;
    ensure!(process.main_module()?.0 == config.image_base, "Current image base changed");
    let ctx = jmap_dumper::connect_manual(
        discovery_memory::DiscoveryMemory::new(process.clone()),
        config.clone(),
        None,
    )
    .await?;
    let registry = Ptr::<FUObjectArray>::new(ctx.uobjectarray, ctx.clone())?;
    let wanted = BTreeMap::from([
        ("CrBuildingData", "/Script/Chimera.CrBuildingData"),
        ("CrBaseCoreSettings", "/Script/Chimera.CrBaseCoreSettings"),
        ("SharedQuestItemsListComponent", "/Script/Chimera.SharedQuestItemsListComponent"),
        ("CrItemRecipeData", "/Script/Chimera.CrItemRecipeData"),
        ("CrBuildingCraftingTrait", "/Script/Chimera.CrBuildingCraftingTrait"),
    ]);
    let hints_file = out.join("class-hints.json");
    let mut classes = BTreeMap::new();
    let mut scanned = false;
    let component_hints_file = out.join("component-hints.json");
    let mut component_indices: Vec<usize> = std::fs::read(&component_hints_file)
        .ok()
        .and_then(|v| serde_json::from_slice(&v).ok())
        .unwrap_or_default();
    ensure!(component_indices.len() <= 32, "Component hint count");
    if let Ok(bytes) = std::fs::read(&hints_file)
        && let Ok(hints) = serde_json::from_slice::<BTreeMap<String, usize>>(&bytes)
    {
        for (name, index) in hints {
            if let Some(expected) = wanted.get(name.as_str())
                && let Ok(Some(obj)) = registry.read_item_ptr(index).await
                && obj.path().await.ok().as_deref() == Some(*expected)
            {
                classes.insert(name, obj.address());
            }
        }
    }
    if classes.len() != wanted.len() || discover_components {
        let known_quest_class = classes.get("SharedQuestItemsListComponent").copied();
        classes.clear();
        component_indices.clear();
        scanned = true;
        let count = registry.num_elements().await?;
        ensure!((1..=2_000_000).contains(&count), "Research registry bound");
        for i in 0..count as usize {
            let Some(obj) = registry.read_item_ptr(i).await? else {
                continue;
            };
            if obj.internal_index().read().await.ok() != Some(i as i32) {
                continue;
            }
            let Ok(name) = obj.name_private().read().await else {
                continue;
            };
            let quest_class =
                known_quest_class.or_else(|| classes.get("SharedQuestItemsListComponent").copied());
            if let Some(quest_class) = quest_class
                && obj.class_private().read().await?.address() == quest_class
                && obj.object_flags().read().await?.bits() & 0x10 == 0
            {
                ensure!(component_indices.len() < 32, "Shared quest component bound");
                component_indices.push(i);
            }
            if let Some(expected) = wanted.get(name.as_str())
                && obj.path().await? == *expected
            {
                ensure!(classes.insert(name, obj.address()).is_none(), "Ambiguous native class");
            }
        }
        ensure!(classes.len() == wanted.len(), "Required native classes missing");
        let mut hints = BTreeMap::new();
        for (name, at) in &classes {
            hints.insert(name.clone(), stamp(&ctx, *at).await?["index"].as_u64().context("Index")?);
        }
        std::fs::write(&hints_file, serde_json::to_vec(&hints)?)?;
        std::fs::write(&component_hints_file, serde_json::to_vec(&component_indices)?)?;
    }
    budget.renew(60000);
    let mut defaults = serde_json::Map::new();
    for (name, fields) in [
        ("CrBuildingData", vec!["AddBasePlatformCost", "AddPillarCost", "UIType"]),
        (
            "CrItemRecipeData",
            vec!["BuildTime", "NeededResources", "UnlockRequirements", "UnlockType"],
        ),
    ] {
        let c = classes[name];
        let d = default_object(&ctx, c).await?;
        let before = stamp(&ctx, d).await?;
        let mut values = serde_json::Map::new();
        for f in fields {
            values.insert(f.into(), scalar(&ctx, d, c, f).await?);
        }
        ensure!(stamp(&ctx, d).await? == before, "Native CDO changed");
        defaults.insert(name.into(), json!({"defaultObject":before,"fields":values}));
    }
    let c = classes["CrBuildingCraftingTrait"];
    let d = default_object(&ctx, c).await?;
    let p = field(&ctx, c, "CraftingParameters").await?;
    ensure!(
        p.zfield().cast_flags().await?.contains(K::CASTCLASS_FStructProperty),
        "Crafting defaults structure"
    );
    let st = p.cast::<ZStructProperty>().struct_().read().await?;
    let at = d + p.offset_internal().read().await? as u64;
    let mut values = serde_json::Map::new();
    for f in ["CraftingLoopDuration", "OverwriteRecipeBuildTime", "bUseCraftingProcessor"] {
        values.insert(f.into(), scalar(&ctx, at, st.address(), f).await?);
    }
    defaults.insert("CrBuildingCraftingTrait".into(),json!({"defaultObject":stamp(&ctx,d).await?,"fields":values,"scope":"native CraftingParameters defaults; authored trait and parent overrides remain separate"}));
    let controller = record["controller"]["address"].as_u64().context("Controller")?;
    let controller_stamp = stamp(&ctx, controller).await?;
    ensure!(
        controller_stamp["class"] == record["controller"]["class"],
        "Cached controller changed"
    );
    let player = object_field(&ctx, controller, "Player").await?;
    ensure!(
        object_field(&ctx, player, "PlayerController").await? == controller,
        "Current local player ownership"
    );
    let level = Ptr::<UObject>::new(controller, ctx.clone())?
        .outer_private()
        .read()
        .await?
        .context("Controller level")?
        .address();
    let world = object_field(&ctx, level, "OwningWorld").await?;
    let world_stamp = stamp(&ctx, world).await?;
    let mode = object_field(&ctx, world, "AuthorityGameMode").await?;
    let mode_class = stamp(&ctx, mode).await?["class"].as_u64().context("Mode class")?;
    let mode_cdo = default_object(&ctx, mode_class).await?;
    let state_class = object_field(&ctx, mode_cdo, "GameStateClass").await?;
    let state_cdo = default_object(&ctx, state_class).await?;
    let actual_state = object_field(&ctx, world, "GameState").await?;
    ensure!(
        stamp(&ctx, actual_state).await?["class"].as_u64() == Some(state_class),
        "Selected/actual game state mismatch"
    );
    let keeper_class = object_field(&ctx, state_cdo, "TechnologyKeeperClass").await?;
    let keeper_cdo = default_object(&ctx, keeper_class).await?;
    let live_ctx = jmap_dumper::connect_manual(process.clone(), config.clone(), None).await?;
    let settings_class = classes["CrBaseCoreSettings"];
    let settings = default_object(&ctx, settings_class).await?;
    let core_binding = soft_asset_path(&live_ctx, settings, "BaseCoreData").await?;
    let mut weapon_offers = Vec::new();
    let player_state = object_field(&ctx, controller, "PlayerState").await?;
    for index in &component_indices {
        let Some(obj) = registry.read_item_ptr(*index).await? else {
            continue;
        };
        if obj.class_private().read().await?.address() != classes["SharedQuestItemsListComponent"] {
            continue;
        }
        let obj = Ptr::<UObject>::new(obj.address(), live_ctx.clone())?;
        let before = stamp(&live_ctx, obj.address()).await?;
        let Some(owner) = obj.outer_private().read().await? else {
            continue;
        };
        if ![controller, actual_state, player_state].contains(&owner.address()) {
            continue;
        }
        let owner_before = stamp(&live_ctx, owner.address()).await?;
        let offers = object_array(&live_ctx, obj.address(), "AvailableWeaponsForUnlock").await?;
        ensure!(
            stamp(&live_ctx, obj.address()).await? == before
                && offers["owner"] == before
                && stamp(&live_ctx, owner.address()).await? == owner_before
                && obj
                    .outer_private()
                    .read()
                    .await?
                    .is_some_and(|v| v.address() == owner.address()),
            "Weapon offer owner changed"
        );
        weapon_offers.push(json!({"component":before,"owner":owner_before,"weapons":offers,"scope":"observed current-world registered purchase candidates; not a universal or owned-weapon list"}));
    }
    let root = object_array(&ctx, keeper_cdo, "AllBuildingList").await?;
    let mut categories = Vec::new();
    for obj in root["objects"].as_array().context("Root entries")? {
        let address = obj["address"].as_u64().context("Root object")?;
        let class = obj["class"].as_u64().context("Root class")?;
        let category = scalar(&ctx, address, class, "UIType").await?;
        ensure!(stamp(&ctx, address).await? == *obj, "Root building identity changed");
        categories.push(json!({"object":obj,"UIType":category}));
    }
    ensure!(
        stamp(&live_ctx, world).await? == world_stamp
            && stamp(&live_ctx, controller).await? == controller_stamp
            && object_field(&live_ctx, world, "AuthorityGameMode").await? == mode
            && object_field(&live_ctx, world, "GameState").await? == actual_state,
        "World changed"
    );
    process.ensure_alive()?;
    let result = json!({"identity":process.identity,"config":config,"readOnly":true,"gameCodeInvoked":false,"registryScans":if scanned {1}else{0},"nativeDefaults":defaults,"baseCoreBinding":core_binding,"weaponOfferEvidence":weapon_offers,"selectedMode":{"class":stamp(&ctx,mode_class).await?,"cdo":stamp(&ctx,mode_cdo).await?,"stateClass":stamp(&ctx,state_class).await?,"stateCdo":stamp(&ctx,state_cdo).await?,"keeperClass":stamp(&ctx,keeper_class).await?,"keeperCdo":stamp(&ctx,keeper_cdo).await?,"authoredRoot":root,"categories":categories}});
    std::fs::write(out.join("native-evidence.json"), serde_json::to_vec_pretty(&result)?)?;
    println!(
        "{}",
        json!({"status":"passed","registryScans":if scanned{1}else{0},"authoredBuildings":result["selectedMode"]["authoredRoot"]["count"],"output":out.join("native-evidence.json")})
    );
    Ok(())
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().collect();
    ensure!(
        args.len() == 3 || args.len() == 4,
        "Private output directory and current overlay cache required"
    );
    let discover_components = args.get(3).is_some_and(|arg| arg == "--discover-quest-components");
    ensure!(args.len() == 3 || discover_components, "Unknown research option");
    smol::block_on(run(Path::new(&args[1]), Path::new(&args[2]), discover_components))
}
