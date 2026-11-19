//! Persistent read-only context with live allocation and ownership guards.
use super::{Bindings, RuntimeBindings, discover, finite, read_context, world, write_bindings};
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
    record: std::sync::Arc<Bindings>,
    ctx: Ctx,
    anchors: Vec<StrongStamp>,
    wave_anchors: Option<[StrongStamp; 2]>,
    level: u64,
    persistent: u64,
    settings: u64,
    player: u64,
    state: u64,
    last_sim: f64,
}
impl Resident {
    pub(crate) fn record(&self) -> std::sync::Arc<Bindings> {
        self.record.clone()
    }
    pub(crate) fn proof(&self) -> Vec<(ObjectStamp, i32, u64)> {
        self.anchors.iter().map(|s| (s.basic.clone(), s.serial, s.name_bits)).collect()
    }
    pub(crate) fn matches_proof(&self, proof: &[(ObjectStamp, i32, u64)]) -> bool {
        self.anchors.len() == proof.len()
            && self
                .anchors
                .iter()
                .zip(proof)
                .all(|(a, b)| a.basic == b.0 && a.serial == b.1 && a.name_bits == b.2)
    }
    pub(crate) fn world_address(&self) -> u64 {
        self.bindings.world.address
    }
    pub(crate) fn context(&self) -> Ctx {
        self.ctx.clone()
    }
    pub(crate) async fn open(
        process: &platform::Process,
        base: &RuntimeCache,
        directory: &FilePath,
    ) -> Result<Self> {
        let file = directory.join("live-context.json");
        if let Ok(record) = crate::cache::read_json::<Bindings>(&file, 65536)
            && let Ok(resident) =
                Self::from_record(process, base, std::sync::Arc::new(record)).await
        {
            return Ok(resident);
        }
        let record = std::sync::Arc::new(discover(process, base).await?);
        let resident = Self::from_record(process, base, record.clone()).await?;
        write_bindings(&file, &record)?;
        Ok(resident)
    }
    pub(crate) async fn from_record(
        process: &platform::Process,
        base: &RuntimeCache,
        record: std::sync::Arc<Bindings>,
    ) -> Result<Self> {
        let b = RuntimeBindings::new(&record)?;
        b.validate(&process.identity)?;
        let ctx = jmap_dumper::connect_manual(process.clone(), base.config.clone(), None).await?;
        let (world, persistent) = world(&ctx, base, &b).await?;
        ensure!(stamp(&ctx, world).await? == b.world, "Resident world changed");
        ensure!(
            crate::path(&ctx, world).await?
                == "/Game/Chimera/Maps/ChimeraMain/ChimeraMain.ChimeraMain",
            "Unsupported world frame"
        );
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
        for address in [base.controller.address, world, level, persistent, settings, player, state]
        {
            anchors.push(strong_stamp(&ctx, address).await?);
        }
        let last_sim = finite(ctx.read::<f64>(world + b.clocks.simulation_seconds).await?)?;
        let mut resident = Self {
            bindings: b,
            record,
            ctx,
            anchors,
            wave_anchors: None,
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
                    == base.controller.address,
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
    pub(crate) async fn compass(
        &mut self,
        process: &platform::Process,
        base: &RuntimeCache,
        binding: &mut crate::compass::Binding,
    ) -> Result<starrupture_observation_model::compass::Sample> {
        let started = crate::overlay_stream::clock_ms();
        let menu = self.menu(process, base).await?.context("Navigation menu changed")?;
        let sim = self
            .ctx
            .read::<f64>(self.bindings.world.address + self.bindings.clocks.simulation_seconds)
            .await?;
        let origin = crate::compass::origin(
            &self.ctx,
            self.bindings.world.address + self.bindings.clocks.world_origin,
        )
        .await?;
        let sample = binding
            .sample(
                &self.ctx,
                base,
                crate::compass::View {
                    world: self.bindings.world.address,
                    state: self.state,
                    simulation: sim,
                    origin,
                    menu,
                    started,
                },
            )
            .await?;
        self.guard(process, base).await?;
        ensure!(
            crate::compass::origin(
                &self.ctx,
                self.bindings.world.address + self.bindings.clocks.world_origin
            )
            .await?
                == origin,
            "Navigation world origin changed"
        );
        Ok(sample)
    }
    pub(crate) fn has_wave_bindings(&self) -> bool {
        self.bindings.wave.is_some()
    }

    async fn guard_wave(&self, anchors: &[StrongStamp; 2]) -> Result<()> {
        for anchor in anchors {
            let address = anchor.basic.address;
            ensure!(
                strong_stamp(&self.ctx, address).await? == *anchor
                    && self.ctx.read::<u64>(address + 32).await? == self.world_address(),
                "Wave subsystem allocation or ownership changed"
            );
        }
        Ok(())
    }

    pub(crate) async fn snapshot(
        &mut self,
        process: &platform::Process,
        base: &RuntimeCache,
    ) -> Result<LiveContext> {
        self.menu(process, base).await?;
        if self.wave_anchors.is_none() {
            let wave =
                self.bindings.wave.as_ref().context("Validated wave subsystems unavailable")?;
            let timer = strong_stamp(&self.ctx, wave.timer_subsystem.address).await?;
            let phase = strong_stamp(&self.ctx, wave.wave_subsystem.address).await?;
            ensure!(
                timer.basic == wave.timer_subsystem && phase.basic == wave.wave_subsystem,
                "Wave subsystem identity changed"
            );
            let anchors = [timer, phase];
            self.guard_wave(&anchors).await?;
            self.bindings.bind_timer(&self.ctx, base.config.image_base).await?;
            self.wave_anchors = Some(anchors);
        }
        let anchors = self.wave_anchors.as_ref().context("Wave allocation guards unavailable")?;
        self.guard_wave(anchors).await?;
        let value = read_context(process, base, &self.bindings, self.ctx.clone()).await?;
        self.guard_wave(anchors).await?;
        self.menu(process, base).await?;
        Ok(value)
    }
}
