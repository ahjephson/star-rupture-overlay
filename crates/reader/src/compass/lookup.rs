//! Bounded reflection reached from current owned controller, camera and game state.
use super::*;
use jmap_dumper::objects::{UField, ZProperty};
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
pub(super) async fn objects(
    ctx: &Ctx,
    base: &RuntimeCache,
    world: u64,
) -> Result<Vec<(usize, u64)>> {
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
    Ok(result)
}
