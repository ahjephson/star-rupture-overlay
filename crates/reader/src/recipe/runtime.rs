//! Build-pinned native widget hover and research-owner reads, never game calls.
use crate::{RuntimeCache, path, stamp};
use anyhow::{Context, Result, ensure};
use jmap_dumper::{
    mem::{Ctx, Ptr},
    objects::{FChunkedFixedUObjectArray, FUObjectArray, UObject, UStruct, ZBoolProperty},
};
use starrupture_observation_model::recipe::{Catalogue, Snapshot, progress};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
};
mod bag;
#[cfg(debug_assertions)]
mod diagnostics;
mod ui;
static REGISTRY_SCANS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub(super) fn registry_scans() -> u64 {
    REGISTRY_SCANS.load(std::sync::atomic::Ordering::Relaxed)
}

const FIELDS: &[(&str, &str, usize, i32)] = &[
    ("/Script/Chimera.CrGameStateBase", "CraftingRecipeOwner", 8, 840),
    ("/Script/Chimera.CrCraftingRecipeOwner", "UnlockableRecipesContainer", 288, 864),
    ("/Script/Chimera.CrUnlockableRecipesContainer", "RecipesData", 16, 264),
    ("/Script/Chimera.CrUnlockableRecipesContainer", "Owner", 8, 280),
    ("/Script/Chimera.CrUnlockableRecipeData", "Recipe", 8, 16),
    ("/Script/Chimera.CrUnlockableRecipeData", "OwnedItems", 16, 24),
    ("/Script/Chimera.CrCountItem", "Item", 8, 0),
    ("/Script/Chimera.CrCountItem", "Count", 4, 8),
    ("/Script/ChimeraUI.CrUW_RecipeTableRecipe", "Recipe", 8, 1024),
    ("/Script/UMG.Widget", "Visibility", 1, 252),
    ("/Script/UMG.UserWidget", "WidgetTree", 8, 824),
    ("/Script/UMG.Widget", "Slot", 8, 48),
    ("/Script/UMG.PanelSlot", "Parent", 8, 40),
    ("/Script/ChimeraUI.CrUW_RecipeTable", "CraftersBox", 8, 1520),
    ("/Script/Chimera.CrPlayerControllerBase", "PlayerMainCharacter", 8, 1928),
    ("/Script/Engine.LocalPlayer", "ViewportClient", 8, 120),
    ("/Script/Engine.GameViewportClient", "GameInstance", 8, 128),
    ("/Script/CommonGame.GameUIManagerSubsystem", "CurrentPolicy", 8, 48),
    ("/Script/CommonGame.GameUIPolicy", "RootViewportLayouts", 16, 88),
    ("/Script/CommonGame.RootViewportLayoutInfo", "LocalPlayer", 8, 0),
    ("/Script/CommonGame.RootViewportLayoutInfo", "RootLayout", 8, 8),
    ("/Script/CommonGame.RootViewportLayoutInfo", "bAddedToViewport", 1, 16),
    ("/Game/Chimera/UI/WBP_OverallUILayout.WBP_OverallUILayout_C", "Menu_Stack", 8, 1088),
    ("/Script/CommonUI.CommonActivatableWidgetContainerBase", "DisplayedWidget", 8, 464),
    ("/Script/UMG.WidgetTree", "RootWidget", 8, 48),
    ("/Script/UMG.PanelWidget", "Slots", 16, 400),
    ("/Script/UMG.PanelSlot", "Content", 8, 48),
    ("/Script/ChimeraUI.CrUW_InventoryScreen", "LeftWidget", 8, 1648),
    ("/Script/ChimeraUI.CrUW_BuildingUIWithTabs", "TabWidget", 8, 1528),
    ("/Script/ChimeraUI.CrUW_ButtonsTab", "ContentOverlay", 8, 1520),
    ("/Script/ChimeraUI.CrUW_RecipeTableCrafter", "RecipesBox", 8, 1000),
    ("/Script/CommonUI.CommonActivatableWidget", "bIsActive", 1, 1104),
    ("/Script/ChimeraUI.CrUW_InventoryScreen", "ActiveWidget", 8, 1656),
    ("/Script/Chimera.CrCharacterPlayerBase", "InventoryComponent", 8, 2800),
    ("/Script/Chimera.CrCharacterPlayerBase", "InventoryItemsStore", 8, 1976),
    ("/Script/AuItems.AuItemsComponent", "OwnedItems", 288, 200),
    ("/Script/AuItems.AuItemsComponent", "ItemsStore", 8, 1528),
    ("/Script/AuItems.AuItemsIdContainer", "Items", 16, 264),
    ("/Script/AuItems.AuItemsStoreComponent", "ItemsArray", 368, 184),
    ("/Script/AuItems.AuItemsArray", "Items", 16, 264),
    ("/Script/AuItems.AuItemEntry", "Handle", 16, 192),
    ("/Script/AuItems.AuItemEntry", "Amount", 4, 224),
    ("/Script/AuItems.AuItemEntryInternal", "ItemTypeGCHolder", 8, 32),
];
const SIZES: &[(&str, i32)] = &[
    ("/Script/AuItems.AuItemIdItem", 28),
    ("/Script/AuItems.AuItemEntryInternal", 40),
    ("/Script/CommonGame.RootViewportLayoutInfo", 24),
];
pub(crate) struct Binding {
    hover: u64,
    rows_class: u64,
    owner_class: u64,
    station_class: u64,
    inventory_screen_class: u64,
    widget_class: u64,
    user_widget_class: u64,
    ui: ui::UiBindings,
    recipe_hints: RefCell<BTreeMap<String, u64>>,
}
fn ptr(a: u64) -> bool {
    (0x10000..0x800000000000).contains(&a) && a.is_multiple_of(8)
}
async fn bytes(ctx: &Ctx, a: u64, n: usize) -> Result<Vec<u8>> {
    ensure!(ptr(a) && n <= 65536, "Bounded recipe read");
    let mut b = vec![0; n];
    ctx.read_buf(a, &mut b).await?;
    Ok(b)
}
fn u32at(b: &[u8], n: usize) -> u32 {
    u32::from_le_bytes(b[n..n + 4].try_into().unwrap())
}
fn u64at(b: &[u8], n: usize) -> u64 {
    u64::from_le_bytes(b[n..n + 8].try_into().unwrap())
}
async fn class_is(ctx: &Ctx, mut class: u64, wanted: u64) -> Result<bool> {
    for _ in 0..32 {
        if class == wanted {
            return Ok(true);
        }
        let parent = Ptr::<UStruct>::new(class, ctx.clone())?.super_struct().read().await?;
        match parent {
            Some(p) => class = p.address(),
            None => return Ok(false),
        }
    }
    anyhow::bail!("Recipe class ancestry bound")
}
async fn serial(ctx: &Ctx, object: u64) -> Result<(crate::ObjectStamp, i32)> {
    let s = stamp(ctx, object).await?;
    ensure!(ctx.struct_member("FUObjectItem", "SerialNumber") == 16, "Registry serial layout");
    let arr = Ptr::<FChunkedFixedUObjectArray>::new(
        ctx.uobjectarray + ctx.struct_member("FUObjectArray", "ObjObjects") as u64,
        ctx.clone(),
    )?;
    let p = arr.read_item_ptr(s.index as usize).await?;
    let v = ctx.read::<i32>(p.address() + 16).await?;
    ensure!(v >= 0, "Recipe allocation serial");
    Ok((s, v))
}
/// Bulk registry hints avoid repeated chunk-header reads per slot. A hint is
/// never target evidence: consumers revalidate the selected live allocation.
pub(super) async fn registered_objects(ctx: &Ctx) -> Result<Vec<(usize, u64)>> {
    REGISTRY_SCANS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    ensure!(ctx.struct_member("FUObjectItem", "Object") == 0, "Registry object layout");
    let registry = Ptr::<FChunkedFixedUObjectArray>::new(
        ctx.uobjectarray + ctx.struct_member("FUObjectArray", "ObjObjects") as u64,
        ctx.clone(),
    )?;
    let count = registry.num_elements().read().await?;
    let per_chunk = registry.elements_per_chunk().await?;
    let stride = ctx.get_struct("FUObjectItem").size as usize;
    ensure!(
        (1..=2_000_000).contains(&count) && per_chunk == 65536 && stride == 24,
        "Registry enumeration layout/bounds"
    );
    let chunks = registry.objects().read().await?;
    let mut result = Vec::new();
    for start in (0..count as usize).step_by(per_chunk) {
        let chunk = chunks.offset(start / per_chunk).read().await?;
        let n = (count as usize - start).min(per_chunk);
        let batch = 65536 / stride;
        for offset in (0..n).step_by(batch) {
            let take = (n - offset).min(batch);
            let data =
                bytes(ctx, chunk.address() + (offset * stride) as u64, take * stride).await?;
            for i in 0..take {
                let p = u64at(&data, i * stride);
                if p != 0 {
                    ensure!(ptr(p), "Registry object pointer");
                    result.push((start + offset + i, p));
                }
            }
        }
        ensure!(
            chunks.offset(start / per_chunk).read().await?.address() == chunk.address(),
            "Registry chunk changed"
        );
    }
    ensure!(
        registry.objects().read().await?.address() == chunks.address()
            && registry.num_elements().read().await? == count,
        "Registry changed during enumeration"
    );
    Ok(result)
}
impl Binding {
    pub(crate) async fn in_station(&self, ctx: &Ctx, base: &RuntimeCache) -> Result<bool> {
        Ok(self.current_station(ctx, base).await?.is_some())
    }
    async fn table_active(&self, ctx: &Ctx, table: u64, base: &RuntimeCache) -> Result<bool> {
        let stamp = serial(ctx, table).await?;
        ensure!(
            class_is(ctx, stamp.0.class, self.station_class).await?,
            "Recipe station widget class"
        );
        let active = ctx.read::<u8>(table + 1104).await?;
        ensure!(active <= 1, "Recipe station activation encoding");
        if active == 0 {
            ensure!(
                serial(ctx, table).await? == stamp && ctx.read::<u8>(table + 1104).await? == active,
                "Station activation changed"
            );
            return Ok(false);
        }
        self.widget_owner(ctx, table, base).await?;
        let mut widget = table;
        let mut seen = BTreeSet::new();
        let mut guards = Vec::new();
        let mut allocations = Vec::new();
        let root_active = loop {
            ensure!(seen.len() < 64 && seen.insert(widget), "Station widget hierarchy bound");
            let stamp = serial(ctx, widget).await?;
            allocations.push(stamp.clone());
            ensure!(
                class_is(ctx, stamp.0.class, self.widget_class).await?,
                "Station ancestor class"
            );
            let visibility = ctx.read::<u8>(widget + 252).await?;
            ensure!(visibility <= 4, "Station ancestor visibility encoding");
            if visibility == 1 || visibility == 2 {
                return Ok(false);
            }
            guards.push((widget + 252, vec![visibility]));
            let slot = ctx.read::<u64>(widget + 48).await?;
            guards.push((widget + 48, slot.to_le_bytes().to_vec()));
            let next = if slot != 0 {
                let ss = serial(ctx, slot).await?;
                allocations.push(ss.clone());
                let parent = ctx.read::<u64>(slot + 40).await?;
                ensure!(serial(ctx, slot).await? == ss, "Station slot changed");
                guards.push((slot + 40, parent.to_le_bytes().to_vec()));
                parent
            } else {
                let tree = ctx.read::<u64>(widget + 32).await?;
                let ts = serial(ctx, tree).await?;
                allocations.push(ts.clone());
                if !path(ctx, ts.0.class).await?.ends_with(".WidgetTree") {
                    ensure!(
                        class_is(ctx, stamp.0.class, self.inventory_screen_class).await?,
                        "Unsupported station screen root"
                    );
                    self.widget_owner(ctx, widget, base).await?;
                    let state = ctx.read::<u8>(widget + 1104).await?;
                    ensure!(state <= 1, "Station screen activation encoding");
                    guards.push((widget + 1104, vec![state]));
                    break state == 1;
                }
                let owner = ctx.read::<u64>(tree + 32).await?;
                let os = serial(ctx, owner).await?;
                allocations.push(os.clone());
                ensure!(
                    class_is(ctx, os.0.class, self.user_widget_class).await?
                        && ctx.read::<u64>(owner + 824).await? == tree
                        && serial(ctx, tree).await? == ts,
                    "Station widget-tree owner changed"
                );
                guards.push((tree + 32, owner.to_le_bytes().to_vec()));
                guards.push((owner + 824, tree.to_le_bytes().to_vec()));
                owner
            };
            ensure!(serial(ctx, widget).await? == stamp, "Station ancestor changed");
            widget = next;
        };
        for (address, expected) in guards {
            let mut current = vec![0; expected.len()];
            ctx.read_buf(address, &mut current).await?;
            ensure!(current == expected, "Station hierarchy changed during observation");
        }
        for allocation in allocations {
            ensure!(
                serial(ctx, allocation.0.address).await? == allocation,
                "Station ancestor allocation changed"
            );
        }
        ensure!(
            serial(ctx, table).await? == stamp && ctx.read::<u8>(table + 1104).await? == active,
            "Station activation changed"
        );
        Ok(root_active)
    }
    pub(crate) async fn discover(ctx: &Ctx) -> Result<Self> {
        let wanted: BTreeSet<_> = FIELDS
            .iter()
            .map(|(p, _, _, _)| p.rsplit('.').next().unwrap())
            .chain(SIZES.iter().map(|(p, _)| p.rsplit('.').next().unwrap()))
            .collect();
        let mut fields = BTreeSet::new();
        let mut classes = BTreeMap::new();
        let mut hover = None;
        let mut player_context_verified = false;
        let mut sizes = BTreeSet::new();
        let mut managers = Vec::new();
        for (i, address) in registered_objects(ctx).await? {
            let o = Ptr::<UObject>::new(address, ctx.clone())?;
            if o.internal_index().read().await.ok() != Some(i as i32) {
                continue;
            }
            let name = o.name_private().read().await?;
            if name.starts_with("CrUIManagerSubsystem_")
                && managers.len() < 16
                && let Ok(s) = serial(ctx, address).await
            {
                managers.push(s);
            }
            if name == "Default__Widget" {
                let vt = ctx.read::<u64>(o.address()).await?;
                let h = ctx.read::<u64>(vt + 0x2c8).await?;
                let b = bytes(ctx, h, 40).await?;
                ensure!(
                    b.starts_with(&[
                        0x40, 0x57, 0x48, 0x83, 0xec, 0x30, 0x48, 0x8d, 0x54, 0x24, 0x20, 0xe8
                    ]) && b.windows(4).any(|w| w == [0x0f, 0xb6, 0x7f, 0x3b]),
                    "Native widget hover binding changed"
                );
                hover = Some(h);
                let displacement = i32::from_le_bytes(b[12..16].try_into()?) as i64;
                let get_widget = (h as i64 + 16 + displacement) as u64;
                let code = bytes(ctx, get_widget, 336).await?;
                ensure!(
                    code.windows(8).any(|w| w == [0x48, 0x83, 0xb9, 0x28, 0x01, 0, 0, 0])
                        && code.windows(7).any(|w| w == [0x49, 0x8b, 0x82, 0x18, 0x01, 0, 0]),
                    "Cached widget native layout changed"
                );
            }
            if name == "Default__CrUW_RecipeTableRecipe" {
                let vt = ctx.read::<u64>(o.address()).await?;
                let getter = ctx.read::<u64>(vt + 0x2e0).await?;
                let code = bytes(ctx, getter, 32).await?;
                ensure!(
                    code.windows(7).any(|w| w == [0x48, 0x8d, 0x99, 0x88, 0x03, 0, 0]),
                    "Recipe owning-player context layout changed"
                );
                player_context_verified = true;
            }
            if wanted.contains(name.as_str()) {
                let p = o.path().await?;
                if !FIELDS.iter().any(|(c, _, _, _)| *c == p) && !SIZES.iter().any(|(c, _)| *c == p)
                {
                    continue;
                }
                if let Some((_, expected)) = SIZES.iter().find(|(c, _)| *c == p) {
                    ensure!(
                        o.cast::<UStruct>().properties_size().read().await? == *expected,
                        "Native record size changed"
                    );
                    sizes.insert(p.clone());
                }
                classes.insert(p.clone(), o.address());
                let mut ps = o.cast::<UStruct>().properties(false);
                let mut visits = 0;
                while let Some(prop) = ps.next().await {
                    visits += 1;
                    ensure!(visits <= 4096, "Field chain bound");
                    let prop = prop?;
                    let n = prop.zfield().name_private().read().await?;
                    if let Some((_, _, size, off)) =
                        FIELDS.iter().find(|(c, f, _, _)| *c == p && *f == n)
                    {
                        ensure!(
                            prop.element_size().read().await? == *size as i32
                                && prop.offset_internal().read().await? == *off
                                && prop.array_dim().read().await? == 1,
                            "Recipe reflected binding changed"
                        );
                        if n == "bIsActive" || n == "bAddedToViewport" {
                            let boolean = prop.cast::<ZBoolProperty>();
                            ensure!(
                                boolean.field_mask().read().await? == 255
                                    && boolean.byte_offset_().read().await? == 0,
                                "Station activation boolean layout"
                            );
                        }
                        fields.insert((p.clone(), n));
                    }
                }
            }
        }
        ensure!(fields.len() == FIELDS.len(), "Recipe reflected fields unavailable");
        ensure!(player_context_verified, "Recipe owning-player binding unavailable");
        ensure!(sizes.len() == SIZES.len(), "Native record layouts unavailable");
        ensure!(!managers.is_empty(), "UI manager is not ready for this connection");
        Ok(Self {
            hover: hover.context("Native hover unavailable")?,
            rows_class: classes["/Script/ChimeraUI.CrUW_RecipeTableRecipe"],
            owner_class: classes["/Script/Chimera.CrCraftingRecipeOwner"],
            station_class: classes["/Script/ChimeraUI.CrUW_RecipeTable"],
            inventory_screen_class: classes["/Script/ChimeraUI.CrUW_InventoryScreen"],
            widget_class: classes["/Script/UMG.Widget"],
            user_widget_class: classes["/Script/UMG.UserWidget"],
            ui: ui::UiBindings {
                managers,
                manager_class: classes["/Script/CommonGame.GameUIManagerSubsystem"],
                policy_class: classes["/Script/CommonGame.GameUIPolicy"],
                layout_class: classes["/Game/Chimera/UI/WBP_OverallUILayout.WBP_OverallUILayout_C"],
                viewport_class: classes["/Script/Engine.GameViewportClient"],
                container_class: classes["/Script/CommonUI.CommonActivatableWidgetContainerBase"],
                tree_class: classes["/Script/UMG.WidgetTree"],
                panel_class: classes["/Script/UMG.PanelWidget"],
                slot_class: classes["/Script/UMG.PanelSlot"],
                crafter_class: classes["/Script/ChimeraUI.CrUW_RecipeTableCrafter"],
                building_menu_class: classes["/Script/ChimeraUI.CrUW_BuildingUIWithTabs"],
                buttons_tab_class: classes["/Script/ChimeraUI.CrUW_ButtonsTab"],
            },
            recipe_hints: RefCell::new(BTreeMap::new()),
        })
    }
    async fn hovered(&self, ctx: &Ctx, row: u64, base: &RuntimeCache) -> Result<bool> {
        let s = serial(ctx, row).await?;
        ensure!(class_is(ctx, s.0.class, self.rows_class).await?, "Recipe row class");
        self.widget_hovered(ctx, row, base).await
    }
    async fn widget_owner(&self, ctx: &Ctx, row: u64, base: &RuntimeCache) -> Result<()> {
        // Native UUserWidget uses FLocalPlayerContext at +0x388. Its first weak
        // reference must resolve to the guarded current local player allocation.
        let player = ctx.read::<u64>(base.controller.address + base.fields.player).await?;
        let ps = serial(ctx, player).await?;
        let weak = bytes(ctx, row + 0x388, 8).await?;
        ensure!(
            u32at(&weak, 0) == ps.0.index as u32 && u32at(&weak, 4) == ps.1 as u32,
            "Recipe row belongs to another player"
        );
        ensure!(
            serial(ctx, player).await? == ps
                && ctx.read::<u64>(base.controller.address + base.fields.player).await? == player
                && bytes(ctx, row + 0x388, 8).await? == weak,
            "Recipe owning player changed"
        );
        Ok(())
    }
    async fn widget_hovered(&self, ctx: &Ctx, row: u64, base: &RuntimeCache) -> Result<bool> {
        let s = serial(ctx, row).await?;
        self.widget_owner(ctx, row, base).await?;
        let visibility = ctx.read::<u8>(row + 252).await?;
        if visibility == 1 || visibility == 2 {
            return Ok(false);
        }
        ensure!(visibility <= 4, "Widget visibility encoding");
        let vt = ctx.read::<u64>(row).await?;
        ensure!(
            ctx.read::<u64>(vt + 0x2c8).await? == self.hover,
            "Unsupported recipe hover override"
        );
        // Mirror GetCachedWidget's weak reference preference, without acquiring
        // a reference or writing its counters. Validate counters/pointers again.
        for off in [0x128, 0x118] {
            let h = bytes(ctx, row + off, 16).await?;
            let p = u64at(&h, 0);
            let c = u64at(&h, 8);
            if !ptr(p) || !ptr(c) {
                continue;
            }
            let strong = ctx.read::<i32>(c + 8).await?;
            if strong <= 0 {
                continue;
            }
            let hovered = ctx.read::<u8>(p + 0x3b).await?;
            ensure!(hovered <= 1, "Native hovered flag encoding");
            let again = bytes(ctx, row + off, 16).await?;
            ensure!(
                h == again
                    && ctx.read::<i32>(c + 8).await? > 0
                    && ctx.read::<u8>(p + 0x3b).await? == hovered
                    && serial(ctx, row).await? == s
                    && self.widget_owner(ctx, row, base).await.is_ok(),
                "Hovered widget changed during capture"
            );
            return Ok(hovered == 1);
        }
        Ok(false)
    }
    async fn station(&self, ctx: &Ctx, row: u64, base: &RuntimeCache) -> Result<bool> {
        let mut widget = row;
        let mut seen = BTreeSet::new();
        for _ in 0..64 {
            ensure!(seen.insert(widget), "Cyclic recipe widget hierarchy");
            let stamp = serial(ctx, widget).await?;
            ensure!(
                class_is(ctx, stamp.0.class, self.widget_class).await?,
                "Recipe widget parent type"
            );
            let visibility = ctx.read::<u8>(widget + 252).await?;
            if visibility == 1 || visibility == 2 {
                return Ok(false);
            }
            if class_is(ctx, stamp.0.class, self.station_class).await? {
                return Ok(self.table_active(ctx, widget, base).await?
                    && self.widget_hovered(ctx, widget, base).await?);
            }
            let slot = ctx.read::<u64>(widget + 48).await?;
            let next = if slot != 0 {
                let ss = serial(ctx, slot).await?;
                let parent = ctx.read::<u64>(slot + 40).await?;
                ensure!(
                    serial(ctx, slot).await? == ss && ctx.read::<u64>(widget + 48).await? == slot,
                    "Recipe slot changed"
                );
                parent
            } else {
                let tree = ctx.read::<u64>(widget + 32).await?;
                let ts = serial(ctx, tree).await?;
                let owner = ctx.read::<u64>(tree + 32).await?;
                let os = serial(ctx, owner).await?;
                ensure!(
                    class_is(ctx, os.0.class, self.user_widget_class).await?
                        && ctx.read::<u64>(owner + 824).await? == tree
                        && serial(ctx, tree).await? == ts,
                    "Recipe widget-tree owner changed"
                );
                owner
            };
            ensure!(serial(ctx, widget).await? == stamp, "Recipe widget parent changed");
            widget = next;
        }
        anyhow::bail!("Recipe station hierarchy bound")
    }
    pub(crate) async fn capture(
        &self,
        ctx: &Ctx,
        base: &RuntimeCache,
        catalogue: &Catalogue,
    ) -> Result<String> {
        ensure!(
            catalogue.game_hash == base.identity.executable_sha256,
            "Recipe catalogue belongs to another game build"
        );
        let mut hovered = Vec::new();
        for row in self.current_rows(ctx, base).await? {
            if self.hovered(ctx, row.0.address, base).await? {
                hovered.push(row.0.address);
            }
        }
        ensure!(
            hovered.len() == 1,
            "Hover over one recipe row in the recipe station, then try again."
        );
        self.capture_row(ctx, base, catalogue, hovered[0]).await
    }
    async fn capture_row(
        &self,
        ctx: &Ctx,
        base: &RuntimeCache,
        catalogue: &Catalogue,
        row: u64,
    ) -> Result<String> {
        ensure!(self.station(ctx, row, base).await?, "Recipe station is not active");
        let s = serial(ctx, row).await?;
        let recipe = ctx.read::<u64>(row + 1024).await?;
        let id = asset(ctx, recipe).await?;
        ensure!(
            catalogue.recipes.contains_key(&id),
            "Hovered recipe is not in the supported reference pack"
        );
        ensure!(
            self.hovered(ctx, row, base).await?
                && serial(ctx, row).await? == s
                && ctx.read::<u64>(row + 1024).await? == recipe,
            "Recipe hover changed before capture"
        );
        self.recipe_hints.borrow_mut().insert(id.clone(), recipe);
        Ok(id)
    }
    pub(crate) async fn bag_inventory(
        &self,
        ctx: &Ctx,
        base: &RuntimeCache,
    ) -> Result<BTreeMap<String, u32>> {
        bag::capture(ctx, base).await
    }
    pub(crate) async fn sample(
        &self,
        ctx: &Ctx,
        base: &RuntimeCache,
        world: u64,
        id: &str,
        catalogue: &Catalogue,
        generation: u64,
    ) -> Result<Option<Snapshot>> {
        ensure!(
            catalogue.game_hash == base.identity.executable_sha256,
            "Recipe catalogue belongs to another game build"
        );
        ensure!(catalogue.recipes.contains_key(id), "Unknown pinned recipe reference");
        let world_s = serial(ctx, world).await?;
        let gs = ctx.read::<u64>(world + 0x1b0).await?;
        let gs_s = serial(ctx, gs).await?;
        let owner = ctx.read::<u64>(gs + 840).await?;
        let owner_s = serial(ctx, owner).await?;
        ensure!(
            class_is(ctx, owner_s.0.class, self.owner_class).await?
                && ctx.read::<u64>(owner + 864 + 280).await? == owner,
            "Research owner binding"
        );
        let level = ctx.read::<u64>(owner + 32).await?;
        let level_s = serial(ctx, level).await?;
        ensure!(ctx.read::<u64>(level + 32).await? == world, "Research owner world");
        let h = bytes(ctx, owner + 864 + 264, 16).await?;
        let p = u64at(&h, 0);
        let n = u32at(&h, 8) as usize;
        let cap = u32at(&h, 12) as usize;
        ensure!(n <= 512 && cap >= n && cap <= 4096, "Research row bounds");
        let raw = if n == 0 {
            Vec::new()
        } else {
            bytes(ctx, p, n * 40).await?
        };
        let mut matches = Vec::new();
        let hint = self.recipe_hints.borrow().get(id).copied();
        if let Some(pointer) = hint {
            for (i, r) in raw.as_chunks::<40>().0.iter().enumerate() {
                if u64at(r, 16) == pointer && asset(ctx, pointer).await? == id {
                    matches.push((i, r));
                }
            }
        }
        if matches.is_empty() {
            for (i, r) in raw.as_chunks::<40>().0.iter().enumerate() {
                let recipe = u64at(r, 16);
                if recipe != 0 && asset(ctx, recipe).await? == id {
                    matches.push((i, r));
                }
            }
        }
        ensure!(matches.len() <= 1, "Ambiguous pinned research recipe");
        let ingredients = if let Some((_, r)) = matches.first().copied() {
            self.recipe_hints.borrow_mut().insert(id.into(), u64at(r, 16));
            let ip = u64at(r, 24);
            let ni = u32at(r, 32) as usize;
            let ic = u32at(r, 36) as usize;
            ensure!(ni <= 20 && ic >= ni && ic <= 256, "Research ingredient bounds");
            let items = if ni == 0 {
                Vec::new()
            } else {
                bytes(ctx, ip, ni * 16).await?
            };
            let mut submitted = Vec::new();
            for item in items.as_chunks::<16>().0 {
                let count = u32at(item, 8);
                ensure!(count <= i32::MAX as u32, "Research counter out of range");
                submitted.push((asset(ctx, u64at(item, 0)).await?, count));
            }
            let mut ingredients = progress(
                &catalogue.recipes.get(id).context("Recipe reference")?.requirements,
                &submitted,
            )
            .map_err(anyhow::Error::msg)?;
            if ingredients.iter().any(|i| i.remaining > 0)
                && let Ok(counts) = bag::capture(ctx, base).await
            {
                for item in &mut ingredients {
                    item.bag_count = Some(counts.get(&item.item_id).copied().unwrap_or(0));
                }
            }
            ensure!(
                ni == 0 || bytes(ctx, ip, items.len()).await? == items,
                "Research ingredients changed during observation"
            );
            Some(ingredients)
        } else {
            // A completed recipe can leave the active research list entirely.
            // Only publish absence after validating the whole list and its owner below.
            None
        };
        ensure!(
            (n == 0 || bytes(ctx, p, raw.len()).await? == raw)
                && bytes(ctx, owner + 864 + 264, 16).await? == h
                && serial(ctx, owner).await? == owner_s
                && serial(ctx, level).await? == level_s
                && serial(ctx, gs).await? == gs_s
                && serial(ctx, world).await? == world_s
                && ctx.read::<u64>(world + 0x1b0).await? == gs
                && ctx.read::<u64>(gs + 840).await? == owner
                && ctx.read::<u64>(owner + 864 + 280).await? == owner
                && ctx.read::<u64>(owner + 32).await? == level
                && ctx.read::<u64>(level + 32).await? == world,
            "Research changed during observation"
        );
        Ok(ingredients.map(|ingredients| Snapshot {
            recipe_id: id.into(),
            ingredients,
            observed_at_ms: super::now_ms(),
            generation,
            process: base.identity.pid,
        }))
    }
}
async fn asset(ctx: &Ctx, a: u64) -> Result<String> {
    ensure!(ptr(a), "Recipe asset pointer");
    // Item metadata may legitimately be a class-default object. This separate
    // metadata path never relaxes instance/owner/hover allocation guards.
    let object = Ptr::<UObject>::new(a, ctx.clone())?;
    let index = object.internal_index().read().await?;
    let registry = Ptr::<FUObjectArray>::new(ctx.uobjectarray, ctx.clone())?;
    ensure!(
        index >= 0
            && index < registry.num_elements().await?
            && registry.read_item_ptr(index as usize).await?.is_some_and(|p| p.address() == a),
        "Asset registry identity"
    );
    let flags = object.object_flags().read().await?.bits();
    ensure!(flags & (0x8000 | 0x10000) == 0, "Asset being destroyed");
    let class = object.class_private().read().await?.address();
    let name = ctx.read::<u64>(a + 24).await?;
    let outer = ctx.read::<u64>(a + 32).await?;
    let p = path(ctx, a).await?;
    ensure!(
        p.starts_with("/Game/")
            && object.internal_index().read().await? == index
            && object.class_private().read().await?.address() == class
            && ctx.read::<u64>(a + 24).await? == name
            && ctx.read::<u64>(a + 32).await? == outer
            && object.object_flags().read().await?.bits() == flags
            && registry.read_item_ptr(index as usize).await?.is_some_and(|p| p.address() == a),
        "Recipe asset identity"
    );
    Ok(p.split('.').next().context("Recipe asset package")?.into())
}
