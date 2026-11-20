//! Bounded reflection reached from current owned controller, camera and game state.
use super::*;
use jmap::EClassCastFlags as K;
use jmap_dumper::objects::{FNameData, UField, ZEnumProperty, ZObjectProperty, ZProperty};
use std::collections::BTreeSet;
async fn native(ctx: &Ctx, mut address: u64, wanted: &str) -> Result<Ptr<UStruct>> {
    let mut seen = BTreeSet::new();
    for _ in 0..32 {
        ensure!(seen.insert(address), "Cyclic navigation class ancestry");
        let object = Ptr::<UObject>::new(address, ctx.clone())?;
        recipe::allocation(ctx, address).await?;
        if crate::path(ctx, address).await? == wanted {
            return Ok(object.cast());
        }
        address = object
            .cast::<UStruct>()
            .super_struct()
            .read()
            .await?
            .context("Navigation native class missing")?
            .address();
    }
    anyhow::bail!("Navigation class ancestry bound")
}
async fn property(owner: &Ptr<UStruct>, name: &str) -> Result<Ptr<ZProperty>> {
    let size = owner.properties_size().read().await?;
    ensure!((1..=262144).contains(&size), "Navigation owner size");
    let mut properties = owner.properties(false);
    for _ in 0..512 {
        let Some(property) = properties.next().await else {
            break;
        };
        let property = property?;
        if property.zfield().name_private().read().await? == name {
            let offset = property.offset_internal().read().await?;
            let bytes = property.element_size().read().await?;
            ensure!(
                offset >= 0
                    && bytes > 0
                    && offset.checked_add(bytes).is_some_and(|v| v <= size)
                    && property.array_dim().read().await? == 1,
                "Navigation targeted property bounds"
            );
            return Ok(property);
        }
    }
    anyhow::bail!("Navigation property missing: {name}")
}
async fn pointer(ctx: &Ctx, object: u64, class: &Ptr<UStruct>, name: &str) -> Result<u64> {
    let property = property(class, name).await?;
    ensure!(property.element_size().read().await? == 8, "Navigation pointer field width");
    ctx.read(object + property.offset_internal().read().await? as u64).await
}
async fn structure(ctx: &Ctx, property: Ptr<ZProperty>, wanted: &str) -> Result<Ptr<UStruct>> {
    let class = property.cast::<ZStructProperty>().struct_().read().await?;
    ensure!(crate::path(ctx, class.address()).await? == wanted, "Navigation nested type changed");
    Ok(class.cast())
}
pub(super) struct MapUnlock {
    state: u64,
    owner_field: u64,
    owner_class: u64,
    flags_offset: u64,
    mask: u8,
}

impl MapUnlock {
    async fn bind(ctx: &Ctx, state: u64, game_class: &Ptr<UStruct>) -> Result<Self> {
        let owner = property(game_class, "CorporationsOwner").await?;
        ensure!(
            owner.zfield().cast_flags().await?.contains(K::CASTCLASS_FObjectProperty)
                && owner.element_size().read().await? == 8,
            "Map unlock owner reference type"
        );
        let class = owner.cast::<ZObjectProperty>().property_class().read().await?;
        let class = native(ctx, class.address(), "/Script/Chimera.CrCorporationsOwner").await?;
        let flags = property(&class, "UnlockedFeaturesFlags").await?;
        ensure!(
            flags.zfield().cast_flags().await?.contains(K::CASTCLASS_FEnumProperty)
                && flags.element_size().read().await? == 1,
            "Map unlock feature flags type"
        );
        let enumeration = flags
            .cast::<ZEnumProperty>()
            .enum_()
            .read()
            .await?
            .context("Map unlock feature enumeration missing")?;
        let allocation = recipe::allocation(ctx, enumeration.address()).await?;
        ensure!(
            crate::path(ctx, enumeration.address()).await?
                == "/Script/Chimera.ECrCorporationUnlockedFeatures",
            "Map unlock enumeration identity"
        );
        // Match the bounded enum layouts supported by the reflected name reader.
        let layout = ctx.get_struct("UEnum");
        let offset = |name| {
            layout.members.iter().find(|member| member.name == name).map(|member| member.offset)
        };
        let names_size = offset("CppForm")
            .zip(offset("Names"))
            .and_then(|(end, start)| end.checked_sub(start))
            .context("Map unlock enum layout")?;
        let count = if names_size >= 24 {
            enumeration.names().cast::<FNameData>().num_values().read().await?
        } else {
            i32::try_from(enumeration.names().len().await?)?
        };
        ensure!((1..=64).contains(&count), "Map unlock enumeration size");
        let names = enumeration.read_names().await?;
        ensure!(names.len() == count as usize, "Map unlock enumeration changed");
        let mut map_values = names.iter().filter_map(|(name, value)| {
            (name.rsplit("::").next() == Some("Map")).then_some(*value)
        });
        let mask = u8::try_from(map_values.next().context("Map unlock bit missing")?)?;
        ensure!(
            mask.is_power_of_two()
                && map_values.next().is_none()
                && recipe::allocation(ctx, enumeration.address()).await? == allocation,
            "Map unlock flag mask"
        );
        Ok(Self {
            state,
            owner_field: state + owner.offset_internal().read().await? as u64,
            owner_class: class.address(),
            flags_offset: flags.offset_internal().read().await? as u64,
            mask,
        })
    }

    pub(super) async fn read(&self, ctx: &Ctx, state: u64) -> Result<bool> {
        ensure!(self.state == state, "Map unlock game state changed");
        let owner: u64 = ctx.read(self.owner_field).await?;
        let allocation = recipe::allocation(ctx, owner).await?;
        if allocation.0.class != self.owner_class {
            ensure!(
                native(ctx, allocation.0.class, "/Script/Chimera.CrCorporationsOwner")
                    .await?
                    .address()
                    == self.owner_class,
                "Map unlock corporation owner type changed"
            );
        }
        let address = owner + self.flags_offset;
        let flags: u8 = ctx.read(address).await?;
        ensure!(
            ctx.read::<u8>(address).await? == flags
                && ctx.read::<u64>(self.owner_field).await? == owner
                && recipe::allocation(ctx, owner).await? == allocation,
            "Map unlock observation changed"
        );
        Ok(flags & self.mask != 0)
    }
}

pub(super) async fn objects(
    ctx: &Ctx,
    base: &RuntimeCache,
    world: u64,
) -> Result<(Vec<(usize, u64)>, Option<MapUnlock>)> {
    let controller = recipe::allocation(ctx, base.controller.address).await?;
    ensure!(controller.0 == base.controller, "Navigation lookup controller changed");
    let pc = native(ctx, controller.0.class, "/Script/Engine.PlayerController").await?;
    let parent = native(ctx, controller.0.class, "/Script/Engine.Controller").await?;
    let camera = pointer(ctx, base.controller.address, &pc, "PlayerCameraManager").await?;
    let camera_stamp = recipe::allocation(ctx, camera).await?;
    let camera_class =
        native(ctx, camera_stamp.0.class, "/Script/Engine.PlayerCameraManager").await?;
    let state = pointer(ctx, base.controller.address, &parent, "PlayerState").await?;
    let state_stamp = recipe::allocation(ctx, state).await?;
    let player_state = native(ctx, state_stamp.0.class, "/Script/Engine.PlayerState").await?;
    let world_stamp = recipe::allocation(ctx, world).await?;
    let world_class = native(ctx, world_stamp.0.class, "/Script/Engine.World").await?;
    let game = pointer(ctx, world, &world_class, "GameState").await?;
    let game_stamp = recipe::allocation(ctx, game).await?;
    let game_class = native(ctx, game_stamp.0.class, "/Script/Chimera.CrGameStateBase").await?;
    let cache = structure(
        ctx,
        property(&camera_class, "CameraCachePrivate").await?,
        "/Script/Engine.CameraCacheEntry",
    )
    .await?;
    let pov =
        structure(ctx, property(&cache, "POV").await?, "/Script/Engine.MinimalViewInfo").await?;
    let markers = property(&game_class, "PlayerPersonalMarkers")
        .await?
        .cast::<ZArrayProperty>()
        .inner()
        .read()
        .await?;
    let marker = structure(ctx, markers, "/Script/Chimera.CrPlayerPersonalMarkerData").await?;
    let mut function = player_state.children().read().await?;
    let mut seen = BTreeSet::new();
    let mut getter = None;
    while let Some(item) = function {
        ensure!(seen.len() < 512 && seen.insert(item.address()), "Navigation function chain bound");
        let object = item.cast::<UObject>();
        if object.name_private().read().await? == "GetPlayerName" {
            ensure!(
                getter.is_none()
                    && crate::path(ctx, item.address()).await?
                        == "/Script/Engine.PlayerState:GetPlayerName",
                "Navigation getter identity"
            );
            getter = Some(item.address());
        }
        function = item.cast::<UField>().next().read().await?;
    }
    let addresses = [
        pc.address(),
        parent.address(),
        camera_class.address(),
        player_state.address(),
        game_class.address(),
        cache.address(),
        pov.address(),
        marker.address(),
        getter.context("Navigation owner getter missing")?,
    ];
    let mut result = Vec::with_capacity(addresses.len());
    for address in addresses {
        let allocation = recipe::allocation(ctx, address).await?;
        result.push((allocation.0.index as usize, address));
    }
    ensure!(
        recipe::allocation(ctx, base.controller.address).await? == controller
            && recipe::allocation(ctx, camera).await? == camera_stamp
            && recipe::allocation(ctx, state).await? == state_stamp
            && recipe::allocation(ctx, world).await? == world_stamp
            && recipe::allocation(ctx, game).await? == game_stamp
            && pointer(ctx, base.controller.address, &pc, "PlayerCameraManager").await? == camera
            && pointer(ctx, base.controller.address, &parent, "PlayerState").await? == state
            && pointer(ctx, world, &world_class, "GameState").await? == game,
        "Navigation lookup ownership changed"
    );
    Ok((result, MapUnlock::bind(ctx, game, &game_class).await.ok()))
}
