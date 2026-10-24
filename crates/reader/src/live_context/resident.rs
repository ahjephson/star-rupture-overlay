//! Overlay-only retained read-only context.
use super::*;

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
    bindings: Bindings,
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
        base: &Cache,
        directory: &FilePath,
    ) -> Result<Self> {
        snapshot(process, base, directory).await?;
        let b: Bindings = crate::cache::read_json(&directory.join("live-context.json"), 65536)?;
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
        let settings = b.ptr(&ctx, persistent, "Engine.Level", "WorldSettings").await?;
        let state = b.ptr(&ctx, world, "Engine.World", "GameState").await?;
        let player: u64 = ctx
            .read(
                base.controller.address
                    + base.offsets[&key("/Script/Engine.PlayerController", "Player")] as u64,
            )
            .await?;
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
        let last_sim = finite(ctx.read::<f64>(world + b.getters["GetTimeSeconds"]).await?)?;
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
    async fn guard(&self, process: &platform::Process, base: &Cache) -> Result<()> {
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
                && b.ptr(ctx, persistent, "Engine.Level", "WorldSettings").await? == self.settings
                && b.ptr(ctx, world, "Engine.World", "GameState").await? == self.state
                && ctx
                    .read::<u64>(
                        base.controller.address
                            + base.offsets[&key("/Script/Engine.PlayerController", "Player")]
                                as u64
                    )
                    .await?
                    == self.player
                && ctx
                    .read::<u64>(
                        self.player
                            + base.offsets[&key("/Script/Engine.Player", "PlayerController")]
                                as u64
                    )
                    .await?
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
        base: &Cache,
    ) -> Result<Option<bool>> {
        self.guard(process, base).await?;
        let b = &self.bindings;
        let before = b
            .boolean(
                &self.ctx,
                base.controller.address,
                "Chimera.CrPlayerControllerBase",
                "bReplInPauseMenu",
            )
            .await?;
        let sim =
            finite(self.ctx.read::<f64>(b.world.address + b.getters["GetTimeSeconds"]).await?)?;
        ensure!(sim >= self.last_sim, "Resident simulation clock regressed");
        self.guard(process, base).await?;
        if b.boolean(
            &self.ctx,
            base.controller.address,
            "Chimera.CrPlayerControllerBase",
            "bReplInPauseMenu",
        )
        .await?
            != before
        {
            return Ok(None);
        }
        self.last_sim = sim;
        Ok(Some(before))
    }
    pub(crate) async fn snapshot(
        &mut self,
        process: &platform::Process,
        base: &Cache,
    ) -> Result<Value> {
        self.menu(process, base).await?;
        let value = read_context(process, base, &self.bindings, self.ctx.clone()).await?;
        self.menu(process, base).await?;
        Ok(value)
    }
}
