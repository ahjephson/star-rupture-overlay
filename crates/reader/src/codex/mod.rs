//! Bounded native preparation inputs. No game functions are invoked.
use anyhow::{Context, Result, ensure};
use jmap::EClassCastFlags as K;
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
use starrupture_observation_model::{
    codex::{BuildingRoot, Defaults, NativeContext},
    shutdown::StopSignal,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::Arc,
};
async fn stamp(ctx: &Ctx, at: u64) -> Result<Value> {
    let obj = Ptr::<UObject>::new(at, ctx.clone())?;
    let index = obj.internal_index().read().await?;
    ensure!(
        obj.object_flags().read().await?.bits() & (0x8000 | 0x10000) == 0,
        "Codex object is being destroyed"
    );
    let registry = Ptr::<FUObjectArray>::new(ctx.uobjectarray, ctx.clone())?;
    let count = registry.num_elements().await?;
    ensure!((1..=2_000_000).contains(&count), "Codex registry bound");
    ensure!(
        index >= 0
            && index < count
            && registry.read_item_ptr(index as usize).await?.is_some_and(|p| p.address() == at),
        "Codex allocation identity"
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
        ensure!(seen.len() < 32 && seen.insert(c.address()), "Codex field ancestry");
        let mut properties = c.properties(false);
        let mut n = 0;
        while let Some(p) = properties.next().await {
            n += 1;
            ensure!(n <= 4096, "Codex property bound");
            let p = p?;
            if p.zfield().name_private().read().await? == name {
                let off = p.offset_internal().read().await?;
                let size = p.element_size().read().await?;
                ensure!(
                    off >= 0
                        && size > 0
                        && off.checked_add(size).is_some_and(|end| end <= 262144)
                        && p.array_dim().read().await? == 1,
                    "Codex field bounds"
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
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnerHint {
    version: u32,
    game_hash: String,
    state_class: String,
    owner_size: u32,
    offset: u32,
}
async fn component(
    ctx: &Ctx,
    state: u64,
    quest_class: u64,
    hash: &str,
    directory: &Path,
) -> Result<(u64, bool)> {
    let owner = crate::recipe::allocation(ctx, state).await?;
    let class = crate::path(ctx, owner.0.class).await?;
    let size = Ptr::<UStruct>::new(owner.0.class, ctx.clone())?.properties_size().read().await?;
    ensure!((40..=16384).contains(&size), "Owned GameState size bound");
    let check = async |offset: u32| -> Result<u64> {
        ensure!(
            offset >= 40 && offset.is_multiple_of(8) && offset + 8 <= size as u32,
            "Owned component offset bound"
        );
        let at = ctx.read::<u64>(state + u64::from(offset)).await?;
        let proof = crate::recipe::allocation(ctx, at).await?;
        ensure!(
            proof.0.class == quest_class && ctx.read::<u64>(at + 32).await? == state,
            "Shared quest component ownership"
        );
        ensure!(
            ctx.read::<u64>(state + u64::from(offset)).await? == at
                && crate::recipe::allocation(ctx, at).await? == proof,
            "Shared quest binding changed"
        );
        Ok(at)
    };
    let file = directory.join("codex-owner.json");
    if let Ok(h) = crate::cache::read_json::<OwnerHint>(&file, 4096)
        && h.version == 1
        && h.game_hash == hash
        && h.state_class == class
        && h.owner_size == size as u32
        && let Ok(at) = check(h.offset).await
    {
        return Ok((at, false));
    }
    // Recovery is confined to this already validated owner, never the global registry.
    let mut bytes = vec![0; size as usize];
    ctx.read_buf(state, &mut bytes).await?;
    let mut matches = Vec::new();
    for offset in (40..size as u32 - 7).step_by(8) {
        let at = u64::from_le_bytes(bytes[offset as usize..offset as usize + 8].try_into()?);
        if !(0x10000..0x800000000000).contains(&at) || !at.is_multiple_of(8) {
            continue;
        }
        if let Ok(valid) = check(offset).await {
            matches.push((offset, valid));
        }
    }
    ensure!(matches.len() == 1, "Unique direct shared quest component reference unavailable");
    ensure!(
        crate::recipe::allocation(ctx, state).await? == owner,
        "GameState changed during owned lookup"
    );
    let (offset, at) = matches[0];
    let h = OwnerHint {
        version: 1,
        game_hash: hash.into(),
        state_class: class,
        owner_size: size as u32,
        offset,
    };
    let pending = file.with_extension(format!("{}.tmp", std::process::id()));
    std::fs::write(&pending, serde_json::to_vec(&h)?)?;
    std::fs::rename(pending, file)?;
    Ok((at, true))
}
async fn capture(
    ctx: &Ctx,
    base: &crate::RuntimeCache,
    lookup: &crate::native_lookup::Lookup,
    directory: &Path,
) -> Result<(NativeContext, bool)> {
    let objects = lookup.resolve_codex(ctx, base)?;
    let mut classes = BTreeMap::new();
    for (_, at) in objects {
        let p = crate::path(ctx, at).await?;
        if crate::native_lookup::CODEX_CLASSES.contains(&p.as_str()) {
            classes.insert(p, at);
        }
    }
    let mut get = |name: &str| {
        classes.remove(&format!("/Script/Chimera.{name}")).context("Codex native class missing")
    };
    let bd = get("CrBuildingData")?;
    let recipe = get("CrItemRecipeData")?;
    let trait_class = get("CrBuildingCraftingTrait")?;
    let settings_class = get("CrBaseCoreSettings")?;
    let quest_class = get("SharedQuestItemsListComponent")?;
    let bdo = default_object(ctx, bd).await?;
    let rdo = default_object(ctx, recipe).await?;
    let tdo = default_object(ctx, trait_class).await?;
    ensure!(
        scalar(ctx, rdo, recipe, "NeededResources").await?["value"] == json!([])
            && scalar(ctx, rdo, recipe, "UnlockRequirements").await?["value"] == json!([])
            && scalar(ctx, rdo, recipe, "UnlockType").await?["value"] == 0,
        "Unsupported recipe native defaults"
    );
    let cp = field(ctx, trait_class, "CraftingParameters").await?;
    ensure!(
        cp.zfield().cast_flags().await?.contains(K::CASTCLASS_FStructProperty),
        "Crafting default structure"
    );
    let st = cp.cast::<ZStructProperty>().struct_().read().await?;
    let at = tdo + cp.offset_internal().read().await? as u64;
    let defaults = Defaults {
        add_platform: scalar(ctx, bdo, bd, "AddBasePlatformCost").await?["value"]
            .as_bool()
            .context("Platform flag")?,
        add_pillar: scalar(ctx, bdo, bd, "AddPillarCost").await?["value"]
            .as_bool()
            .context("Pillar flag")?,
        recipe_seconds: scalar(ctx, rdo, recipe, "BuildTime").await?["value"]
            .as_f64()
            .context("Recipe duration")? as f32,
        crafting_loop_seconds: scalar(ctx, at, st.address(), "CraftingLoopDuration").await?["value"]
            .as_f64()
            .context("Crafting loop")? as f32,
        overwrite_recipe_seconds:
            scalar(ctx, at, st.address(), "OverwriteRecipeBuildTime").await?["value"]
                .as_f64()
                .context("Recipe override")? as f32,
        use_crafting_processor:
            scalar(ctx, at, st.address(), "bUseCraftingProcessor").await?["value"]
                .as_bool()
                .context("Crafting processor")?,
    };
    let controller = crate::recipe::allocation(ctx, base.controller.address).await?;
    ensure!(controller.0 == base.controller, "Codex controller changed");
    let player = object_field(ctx, controller.0.address, "Player").await?;
    ensure!(
        object_field(ctx, player, "PlayerController").await? == controller.0.address,
        "Codex local player changed"
    );
    let level = ctx.read::<u64>(controller.0.address + 32).await?;
    let world = object_field(ctx, level, "OwningWorld").await?;
    let world_proof = crate::recipe::allocation(ctx, world).await?;
    let mode = object_field(ctx, world, "AuthorityGameMode").await?;
    let mode_class = crate::recipe::allocation(ctx, mode).await?.0.class;
    let mode_cdo = default_object(ctx, mode_class).await?;
    let state_class = object_field(ctx, mode_cdo, "GameStateClass").await?;
    let state_cdo = default_object(ctx, state_class).await?;
    let state = object_field(ctx, world, "GameState").await?;
    ensure!(
        crate::recipe::allocation(ctx, state).await?.0.class == state_class,
        "Codex selected GameState differs from current world"
    );
    let keeper_class = object_field(ctx, state_cdo, "TechnologyKeeperClass").await?;
    let keeper_cdo = default_object(ctx, keeper_class).await?;
    let root = object_array(ctx, keeper_cdo, "AllBuildingList").await?;
    let category_property = field(ctx, bd, "UIType").await?;
    ensure!(category_property.element_size().read().await? == 1, "Building category width");
    let category_offset = category_property.offset_internal().read().await? as u64;
    let mut buildings = Vec::new();
    for o in root["objects"].as_array().context("Building root array")? {
        let addr = o["address"].as_u64().context("Building root address")?;
        let class = o["class"].as_u64().context("Building class")?;
        let category = if class == bd {
            ctx.read::<u8>(addr + category_offset).await?
        } else {
            u8::try_from(
                scalar(ctx, addr, class, "UIType").await?["value"]
                    .as_u64()
                    .context("Building category")?,
            )?
        };
        ensure!(stamp(ctx, addr).await? == *o, "Building root changed");
        buildings.push(BuildingRoot {
            id: o["path"]
                .as_str()
                .context("Building identity")?
                .split('.')
                .next()
                .context("Building package")?
                .into(),
            category,
        });
    }
    let settings = default_object(ctx, settings_class).await?;
    let core = soft_asset_path(ctx, settings, "BaseCoreData").await?;
    let (component, recovered) =
        component(ctx, state, quest_class, &base.identity.executable_sha256, directory).await?;
    let component_proof = crate::recipe::allocation(ctx, component).await?;
    let offers = object_array(ctx, component, "AvailableWeaponsForUnlock").await?;
    let weapon_classes = offers["objects"]
        .as_array()
        .context("Registered weapon classes")?
        .iter()
        .map(|v| v["path"].as_str().map(str::to_owned).context("Registered weapon identity"))
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        crate::recipe::allocation(ctx, world).await? == world_proof
            && crate::recipe::allocation(ctx, controller.0.address).await? == controller
            && object_field(ctx, world, "AuthorityGameMode").await? == mode
            && object_field(ctx, world, "GameState").await? == state
            && crate::recipe::allocation(ctx, component).await? == component_proof
            && ctx.read::<u64>(component + 32).await? == state,
        "Codex world changed during preparation"
    );
    let context = NativeContext {
        version: 1,
        game_hash: base.identity.executable_sha256.clone(),
        mode_class: crate::path(ctx, mode_class).await?,
        state_class: crate::path(ctx, state_class).await?,
        keeper_class: crate::path(ctx, keeper_class).await?,
        buildings,
        weapon_classes,
        core_asset: core["value"]["package"].as_str().context("Core asset")?.into(),
        defaults,
    };
    context.validate().map_err(anyhow::Error::msg)?;
    Ok((context, recovered))
}
/// Finite worker-side preparation. This creates no feature worker or image buffers until requested.
/// The caller owns the prepared catalogue and must drop it when its feature is disabled.
pub fn prepare<T>(
    directory: &Path,
    stop: Arc<StopSignal>,
    cached: impl FnOnce(&Path, &NativeContext) -> Result<Option<T>>,
    build: impl FnOnce(&Path, NativeContext, &[u8], &StopSignal) -> Result<T>,
) -> Result<T> {
    ensure!(!crate::platform::is_elevated()?, "Codex preparation must run unelevated");
    std::fs::create_dir_all(directory)?;
    let budget = crate::platform::ReadBudget::new(stop.clone());
    let process = crate::platform::find_game(budget.clone())?;
    budget.renew(120000);
    smol::block_on(async {
        let base = crate::overlay_stream::load_base(&process, directory).await?;
        let ctx = jmap_dumper::connect_manual(process.clone(), base.config.clone(), None).await?;
        let scans = crate::recipe::registry_scans();
        let (context, recovered) =
            capture(&ctx, &base, &crate::native_lookup::Lookup::new(directory), directory).await?;
        process.ensure_alive()?;
        eprintln!(
            "Codex native context: {} registry recovery scans; {} owned-reference recovery; {} rooted buildings.",
            crate::recipe::registry_scans() - scans,
            u8::from(recovered),
            context.buildings.len()
        );
        let paks = Path::new(&process.identity.executable)
            .parent()
            .and_then(Path::parent)
            .and_then(Path::parent)
            .context("Game executable layout")?
            .join("Content/Paks");
        if let Some(value) = cached(&paks, &context)? {
            return Ok(value);
        }
        let schemas = crate::schema::Store::new(directory).get(&ctx, &context.game_hash)?;
        let (fresh, _) =
            capture(&ctx, &base, &crate::native_lookup::Lookup::new(directory), directory).await?;
        ensure!(fresh == context, "Codex native scope changed before extraction");
        process.ensure_alive()?;
        let value = build(&paks, context.clone(), &schemas, &stop)?;
        process.ensure_alive()?;
        let (fresh, _) =
            capture(&ctx, &base, &crate::native_lookup::Lookup::new(directory), directory).await?;
        ensure!(fresh == context, "Codex native scope changed during extraction");
        Ok(value)
    })
}
