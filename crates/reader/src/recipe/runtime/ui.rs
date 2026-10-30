//! Player-owned UI references. No object-registry discovery is allowed here.
use super::*;

type Allocation = (crate::ObjectStamp, i32);
pub(super) struct UiBindings {
    pub managers: Vec<Allocation>,
    pub manager_class: u64,
    pub policy_class: u64,
    pub layout_class: u64,
    pub viewport_class: u64,
    pub container_class: u64,
    pub tree_class: u64,
    pub panel_class: u64,
    pub slot_class: u64,
    pub crafter_class: u64,
    pub building_menu_class: u64,
    pub buttons_tab_class: u64,
}

#[derive(Default)]
struct Read {
    objects: BTreeMap<u64, Allocation>,
    fields: Vec<(u64, Vec<u8>)>,
}
impl Read {
    async fn object(&mut self, ctx: &Ctx, address: u64, class: Option<u64>) -> Result<Allocation> {
        let allocation = serial(ctx, address).await?;
        if let Some(class) = class {
            ensure!(class_is(ctx, allocation.0.class, class).await?, "UI reference class changed");
        }
        if let Some(previous) = self.objects.insert(address, allocation.clone()) {
            ensure!(previous == allocation, "UI allocation changed during lookup");
        }
        Ok(allocation)
    }
    async fn pointer(&mut self, ctx: &Ctx, address: u64) -> Result<u64> {
        let value = ctx.read::<u64>(address).await?;
        self.fields.push((address, value.to_le_bytes().to_vec()));
        Ok(value)
    }
    async fn byte(&mut self, ctx: &Ctx, address: u64) -> Result<u8> {
        let value = ctx.read::<u8>(address).await?;
        self.fields.push((address, vec![value]));
        Ok(value)
    }
    async fn visible(&mut self, ctx: &Ctx, widget: u64) -> Result<bool> {
        let visibility = self.byte(ctx, widget + 252).await?;
        ensure!(visibility <= 4, "UI visibility encoding");
        Ok(visibility != 1 && visibility != 2)
    }
    async fn array(
        &mut self,
        ctx: &Ctx,
        address: u64,
        stride: usize,
        limit: usize,
    ) -> Result<Vec<u8>> {
        let header = bytes(ctx, address, 16).await?;
        let (data, count) = array_shape(&header, stride, limit)?;
        let value = if count == 0 {
            vec![]
        } else {
            bytes(ctx, data, count * stride).await?
        };
        self.fields.push((address, header));
        if count != 0 {
            self.fields.push((data, value.clone()));
        }
        Ok(value)
    }
    async fn finish(self, ctx: &Ctx) -> Result<()> {
        for (address, previous) in self.fields {
            let mut current = vec![0; previous.len()];
            ctx.read_buf(address, &mut current).await?;
            ensure!(current == previous, "Player UI changed during lookup");
        }
        for (address, previous) in self.objects {
            ensure!(serial(ctx, address).await? == previous, "Player UI allocation changed");
        }
        Ok(())
    }
}
fn array_shape(header: &[u8], stride: usize, limit: usize) -> Result<(u64, usize)> {
    ensure!(header.len() == 16 && stride > 0, "UI collection header");
    let count = u32at(header, 8) as usize;
    let capacity = u32at(header, 12) as usize;
    ensure!(
        count <= capacity && capacity <= limit && count * stride <= 65536,
        "UI collection bounds"
    );
    let data = u64at(header, 0);
    ensure!(count == 0 || ptr(data), "UI collection pointer");
    Ok((data, count))
}
fn local_layout(records: &[u8], player: u64) -> Result<u64> {
    ensure!(records.len().is_multiple_of(24), "Local-player UI record size");
    let selected: Vec<_> =
        records.as_chunks::<24>().0.iter().filter(|r| u64at(r.as_slice(), 0) == player).collect();
    ensure!(selected.len() == 1, "Current player's UI layout unavailable or ambiguous");
    let record = selected[0];
    ensure!(record[16] == 1 && ptr(u64at(record, 8)), "Current player's UI layout is not attached");
    Ok(u64at(record, 8))
}

impl Binding {
    pub(super) async fn mouse_target(&self, ctx: &Ctx, row: u64) -> Result<bool> {
        let mut read = Read::default();
        read.object(ctx, row, Some(self.rows_class)).await?;
        let mut hovered = false;
        for offset in [976, 984] {
            let widget = read.pointer(ctx, row + offset).await?;
            if widget == 0 {
                continue;
            }
            let allocation = read.object(ctx, widget, Some(self.widget_class)).await?;
            if self.attached_to(ctx, widget, row, &mut read).await?
                && self.native_hovered(ctx, widget, allocation).await?
            {
                hovered = true;
            }
        }
        read.finish(ctx).await?;
        Ok(hovered)
    }
    async fn menu_screen(
        &self,
        ctx: &Ctx,
        base: &RuntimeCache,
        read: &mut Read,
    ) -> Result<Option<u64>> {
        let controller = read.object(ctx, base.controller.address, None).await?;
        ensure!(controller.0 == base.controller, "UI controller changed");
        let player = read.pointer(ctx, base.controller.address + base.fields.player).await?;
        read.object(ctx, player, None).await?;
        let viewport = read.pointer(ctx, player + 120).await?;
        read.object(ctx, viewport, Some(self.ui.viewport_class)).await?;
        let game = read.pointer(ctx, viewport + 128).await?;
        read.object(ctx, game, None).await?;
        let mut managers = Vec::new();
        for hint in &self.ui.managers {
            if serial(ctx, hint.0.address).await.ok().as_ref() == Some(hint) {
                let manager = read.object(ctx, hint.0.address, Some(self.ui.manager_class)).await?;
                if read.pointer(ctx, manager.0.address + 32).await? == game {
                    managers.push(manager.0.address);
                }
            }
        }
        ensure!(managers.len() == 1, "Current game-instance UI manager unavailable or ambiguous");
        let manager = managers[0];
        let policy = read.pointer(ctx, manager + 48).await?;
        read.object(ctx, policy, Some(self.ui.policy_class)).await?;
        ensure!(read.pointer(ctx, policy + 32).await? == manager, "UI policy owner changed");
        let layouts = read.array(ctx, policy + 88, 24, 16).await?;
        let layout = local_layout(&layouts, player)?;
        read.object(ctx, layout, Some(self.ui.layout_class)).await?;
        ensure!(read.pointer(ctx, layout + 32).await? == game, "UI layout owner changed");
        if !read.visible(ctx, layout).await? {
            return Ok(None);
        }
        let tree = read.pointer(ctx, layout + 824).await?;
        read.object(ctx, tree, Some(self.ui.tree_class)).await?;
        ensure!(read.pointer(ctx, tree + 32).await? == layout, "UI layout tree owner changed");
        let stack = read.pointer(ctx, layout + 1088).await?;
        read.object(ctx, stack, Some(self.ui.container_class)).await?;
        ensure!(read.pointer(ctx, stack + 32).await? == tree, "Menu stack owner changed");
        if !read.visible(ctx, stack).await? {
            return Ok(None);
        }
        let screen = read.pointer(ctx, stack + 464).await?;
        if screen == 0 {
            return Ok(None);
        }
        let screen_stamp = read.object(ctx, screen, Some(self.user_widget_class)).await?;
        if !class_is(ctx, screen_stamp.0.class, self.inventory_screen_class).await? {
            return Ok(None);
        }
        if !read.visible(ctx, screen).await? {
            return Ok(None);
        }
        ensure!(read.byte(ctx, screen + 1104).await? == 1, "Inventory menu is transitioning");
        self.widget_owner(ctx, screen, base).await?;
        Ok(Some(screen))
    }
    async fn attached_to(
        &self,
        ctx: &Ctx,
        mut widget: u64,
        root: u64,
        read: &mut Read,
    ) -> Result<bool> {
        let mut seen = BTreeSet::new();
        for _ in 0..64 {
            if widget == root {
                return Ok(true);
            }
            ensure!(seen.insert(widget), "Cyclic UI ancestry");
            read.object(ctx, widget, Some(self.widget_class)).await?;
            let visibility = read.byte(ctx, widget + 252).await?;
            ensure!(visibility <= 4, "UI visibility encoding");
            if visibility == 1 || visibility == 2 {
                return Ok(false);
            }
            let slot = read.pointer(ctx, widget + 48).await?;
            if slot != 0 {
                read.object(ctx, slot, Some(self.ui.slot_class)).await?;
                ensure!(read.pointer(ctx, slot + 48).await? == widget, "UI slot content changed");
                widget = read.pointer(ctx, slot + 40).await?;
                read.object(ctx, widget, Some(self.ui.panel_class)).await?;
            } else {
                let outer = read.pointer(ctx, widget + 32).await?;
                let stamp = read.object(ctx, outer, None).await?;
                if !class_is(ctx, stamp.0.class, self.ui.tree_class).await? {
                    return Ok(false);
                }
                if read.pointer(ctx, outer + 48).await? != widget {
                    return Ok(false);
                }
                widget = read.pointer(ctx, outer + 32).await?;
                read.object(ctx, widget, Some(self.user_widget_class)).await?;
                ensure!(read.pointer(ctx, widget + 824).await? == outer, "UI tree owner changed");
            }
        }
        anyhow::bail!("UI ancestry bound")
    }
    async fn panel_children(&self, ctx: &Ctx, panel: u64, read: &mut Read) -> Result<Vec<u64>> {
        read.object(ctx, panel, Some(self.ui.panel_class)).await?;
        let slots = read.array(ctx, panel + 400, 8, 512).await?;
        let mut children = Vec::new();
        for entry in slots.as_chunks::<8>().0 {
            let slot = u64at(entry, 0);
            read.object(ctx, slot, Some(self.ui.slot_class)).await?;
            ensure!(read.pointer(ctx, slot + 40).await? == panel, "UI panel slot parent changed");
            let child = read.pointer(ctx, slot + 48).await?;
            if child != 0 {
                read.object(ctx, child, Some(self.widget_class)).await?;
                ensure!(read.pointer(ctx, child + 48).await? == slot, "UI child slot changed");
                children.push(child);
            }
        }
        Ok(children)
    }
    async fn find_station(
        &self,
        ctx: &Ctx,
        base: &RuntimeCache,
        read: &mut Read,
    ) -> Result<Option<u64>> {
        let Some(screen) = self.menu_screen(ctx, base, read).await? else {
            return Ok(None);
        };
        let mut building_menus = BTreeSet::new();
        for offset in [1648, 1656] {
            let child = read.pointer(ctx, screen + offset).await?;
            if child == 0 {
                continue;
            }
            let allocation = read.object(ctx, child, Some(self.widget_class)).await?;
            if class_is(ctx, allocation.0.class, self.ui.building_menu_class).await?
                && self.attached_to(ctx, child, screen, read).await?
            {
                building_menus.insert(child);
            }
        }
        let mut found = BTreeSet::new();
        for building in building_menus {
            let tab = read.pointer(ctx, building + 1528).await?;
            read.object(ctx, tab, Some(self.ui.buttons_tab_class)).await?;
            ensure!(
                self.attached_to(ctx, tab, building, read).await?,
                "Building tab is not attached"
            );
            let content = read.pointer(ctx, tab + 1520).await?;
            read.object(ctx, content, Some(self.ui.panel_class)).await?;
            ensure!(
                self.attached_to(ctx, content, tab, read).await?,
                "Building tab content is not attached"
            );
            for widget in self.panel_children(ctx, content, read).await? {
                let allocation = read.object(ctx, widget, Some(self.widget_class)).await?;
                if !class_is(ctx, allocation.0.class, self.station_class).await? {
                    continue;
                }
                let active = read.byte(ctx, widget + 1104).await?;
                ensure!(active <= 1, "Recipe table activation encoding");
                if active == 1 && self.attached_to(ctx, widget, screen, read).await? {
                    self.widget_owner(ctx, widget, base).await?;
                    found.insert(widget);
                }
            }
        }
        ensure!(found.len() <= 1, "Ambiguous active recipe tables");
        Ok(found.first().copied())
    }
    pub(crate) async fn current_station(
        &self,
        ctx: &Ctx,
        base: &RuntimeCache,
    ) -> Result<Option<Allocation>> {
        let mut read = Read::default();
        let table = self.find_station(ctx, base, &mut read).await?;
        let result = if let Some(table) = table {
            Some(read.object(ctx, table, Some(self.station_class)).await?)
        } else {
            None
        };
        read.finish(ctx).await?;
        Ok(result)
    }
    pub(crate) async fn current_rows(
        &self,
        ctx: &Ctx,
        base: &RuntimeCache,
    ) -> Result<Vec<Allocation>> {
        let mut read = Read::default();
        let table =
            self.find_station(ctx, base, &mut read).await?.context("Recipe station is not open")?;
        let panel = read.pointer(ctx, table + 1520).await?;
        let mut rows = Vec::new();
        for crafter in self.panel_children(ctx, panel, &mut read).await? {
            read.object(ctx, crafter, Some(self.ui.crafter_class)).await?;
            let recipes = read.pointer(ctx, crafter + 1000).await?;
            for row in self.panel_children(ctx, recipes, &mut read).await? {
                ensure!(rows.len() < 512, "Recipe row collection bound");
                rows.push(read.object(ctx, row, Some(self.rows_class)).await?);
            }
        }
        read.finish(ctx).await?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn record(player: u64, root: u64, attached: u8) -> [u8; 24] {
        let mut r = [0; 24];
        r[..8].copy_from_slice(&player.to_le_bytes());
        r[8..16].copy_from_slice(&root.to_le_bytes());
        r[16] = attached;
        r
    }
    #[test]
    fn selects_only_the_current_players_attached_layout() {
        let data = [record(1, 0x10000, 1), record(2, 0x20000, 1)].concat();
        assert_eq!(local_layout(&data, 2).unwrap(), 0x20000);
    }
    #[test]
    fn missing_or_ambiguous_layout_is_unknown_never_closed() {
        assert!(local_layout(&[], 1).is_err());
        assert!(local_layout(&record(2, 0x10000, 1), 1).is_err());
        assert!(local_layout(&[record(1, 0x10000, 1), record(1, 0x20000, 1)].concat(), 1).is_err());
    }
    #[test]
    fn unattached_or_invalid_root_is_unknown() {
        assert!(local_layout(&record(1, 0x10000, 0), 1).is_err());
        assert!(local_layout(&record(1, 0, 1), 1).is_err());
    }
    #[test]
    fn ui_collection_bounds_reject_torn_or_oversized_headers() {
        let mut h = [0; 16];
        assert_eq!(array_shape(&h, 8, 16).unwrap(), (0, 0));
        h[8..12].copy_from_slice(&1u32.to_le_bytes());
        assert!(array_shape(&h, 8, 16).is_err());
        h[12..16].copy_from_slice(&32u32.to_le_bytes());
        assert!(array_shape(&h, 8, 16).is_err());
        h[12..16].copy_from_slice(&1u32.to_le_bytes());
        assert!(array_shape(&h, 8, 16).is_err());
    }
}
