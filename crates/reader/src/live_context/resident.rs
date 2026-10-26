//! Persistent read-only context with live allocation and ownership guards.
use super::{Bindings, RuntimeBindings, finite, read_context, snapshot, world};
use crate::{ObjectStamp, RuntimeCache, platform, stamp};
use anyhow::{Context, Result, ensure};
use jmap_dumper::{
    mem::{Ctx, Ptr},
    objects::UObject,
};
use starrupture_observation_model::observation::LiveContext;
use std::path::Path as FilePath;

#[derive(PartialEq)]
struct StrongStamp {
    basic: ObjectStamp,
    serial: i32,
    name_bits: u64,
}
async fn strong_stamp(ctx: &Ctx, address: u64) -> Result<StrongStamp> {
    use jmap_dumper::objects::FChunkedFixedUObjectArray;
    let basic = stamp(ctx, address).await?;
    ensure!(ctx.struct_member("FUObjectItem", "SerialNumber") == 16, "Registry serial layout");
    let array = Ptr::<FChunkedFixedUObjectArray>::new(
        ctx.uobjectarray + ctx.struct_member("FUObjectArray", "ObjObjects") as u64,
        ctx.clone(),
    )?;
    let item = array.read_item_ptr(basic.index as usize).await?;
    Ok(StrongStamp {
        basic,
        serial: ctx.read::<i32>(item.address() + 16).await?,
        name_bits: ctx.read::<u64>(address + 24).await?,
    })
}
pub(crate) struct Resident {
    bindings: RuntimeBindings,
    ctx: Ctx,
    anchors: Vec<StrongStamp>,
    level: u64,
    persistent: u64,
    settings: u64,
    player: u64,
    state: u64,
    last_sim: f64,
}
impl Resident {
    pub(crate) fn context(&self) -> Ctx {
        self.ctx.clone()
    }
    pub(crate) async fn open(
        process: &platform::Process,
        base: &RuntimeCache,
        directory: &FilePath,
    ) -> Result<Self> {
        snapshot(process, base, directory).await?;
        let record: Bindings =
            crate::cache::read_json(&directory.join("live-context.json"), 65536)?;
        let b = RuntimeBindings::new(&record)?;
        b.validate(&process.identity)?;
        let ctx = jmap_dumper::connect_manual(process.clone(), base.config.clone(), None).await?;
        let (world, persistent) = world(&ctx, base, &b).await?;
        ensure!(stamp(&ctx, world).await? == b.world, "Resident world changed");
        let level = Ptr::<UObject>::new(base.controller.address, ctx.clone())?
            .outer_private()
            .read()
            .await?
            .context("Controller level")?
            .address();
        let settings = ctx.read::<u64>(persistent + b.fields.world_settings).await?;
        let state = ctx.read::<u64>(world + b.fields.game_state).await?;
        let player: u64 = ctx.read(base.controller.address + base.fields.player).await?;
        let mut anchors = Vec::new();
        for address in [
            base.controller.address,
            world,
            level,
            persistent,
            settings,
            player,
            state,
            b.timer_subsystem.address,
            b.wave_subsystem.address,
        ] {
            anchors.push(strong_stamp(&ctx, address).await?);
        }
        let last_sim = finite(ctx.read::<f64>(world + b.clocks.simulation_seconds).await?)?;
        let mut resident = Self {
            bindings: b,
            ctx,
            anchors,
            level,
            persistent,
            settings,
            player,
            state,
            last_sim,
        };
        resident.menu(process, base).await?;
        Ok(resident)
    }
    async fn guard(&self, process: &platform::Process, base: &RuntimeCache) -> Result<()> {
        process.ensure_alive()?;
        for anchor in &self.anchors {
            ensure!(
                strong_stamp(&self.ctx, anchor.basic.address).await? == *anchor,
                "Resident object/serial changed"
            );
        }
        let b = &self.bindings;
        let ctx = &self.ctx;
        let (world, persistent) = world(ctx, base, b).await?;
        ensure!(
            world == b.world.address
                && persistent == self.persistent
                && ctx.read::<u64>(base.controller.address + 32).await? == self.level
                && ctx.read::<u64>(persistent + b.fields.world_settings).await? == self.settings
                && ctx.read::<u64>(world + b.fields.game_state).await? == self.state
                && ctx.read::<u64>(base.controller.address + base.fields.player).await?
                    == self.player
                && ctx.read::<u64>(self.player + base.fields.player_controller).await?
                    == base.controller.address
                && ctx.read::<u64>(b.timer_subsystem.address + 32).await? == world
                && ctx.read::<u64>(b.wave_subsystem.address + 32).await? == world,
            "Resident world/player ownership changed"
        );
        Ok(())
    }
    pub(crate) async fn menu(
        &mut self,
        process: &platform::Process,
        base: &RuntimeCache,
    ) -> Result<Option<bool>> {
        self.guard(process, base).await?;
        let b = &self.bindings;
        let before = b.fields.menu_open.read(&self.ctx, base.controller.address).await?;
        let sim =
            finite(self.ctx.read::<f64>(b.world.address + b.clocks.simulation_seconds).await?)?;
        ensure!(sim >= self.last_sim, "Resident simulation clock regressed");
        self.guard(process, base).await?;
        if b.fields.menu_open.read(&self.ctx, base.controller.address).await? != before {
            return Ok(None);
        }
        self.last_sim = sim;
        Ok(Some(before))
    }
    pub(crate) async fn snapshot(
        &mut self,
        process: &platform::Process,
        base: &RuntimeCache,
    ) -> Result<LiveContext> {
        self.menu(process, base).await?;
        let value = read_context(process, base, &self.bindings, self.ctx.clone()).await?;
        self.menu(process, base).await?;
        Ok(value)
    }
}
