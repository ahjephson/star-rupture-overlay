//! Validated read-only context and explicit evidence for guarded phase calculations.
use crate::error::ObservationConflict;
use crate::{Identity, ObjectStamp, RuntimeCache, path, platform, stamp};
use anyhow::{Context, Result, ensure};
use iced_x86::{Decoder, DecoderOptions, Mnemonic, Register};
use jmap_dumper::{
    mem::{Ctx, Ptr},
    objects::{FUObjectArray, UFunction, UObject, UStruct, ZBoolProperty},
};
use serde::{Deserialize, Serialize};
use starrupture_observation_model::observation::{
    Clocks, LiveContext, NativeStage, PauseState, PhaseEvidence, PlayerContext, ReadWindow,
    ViewMode, WaveObservation, WorldState,
};
use std::{
    collections::{BTreeMap, HashSet},
    path::Path as FilePath,
    time::{SystemTime, UNIX_EPOCH},
};
mod runtime;
use runtime::RuntimeBindings;
const FIELDS: &[(&str, &str, usize)] = &[
    ("Engine.Level", "OwningWorld", 8),
    ("Engine.Level", "WorldSettings", 8),
    ("Engine.World", "PersistentLevel", 8),
    ("Engine.World", "GameState", 8),
    ("Engine.WorldSettings", "PauserPlayerState", 8),
    ("Engine.WorldSettings", "bEnableWorldOriginRebasing", 1),
    ("Engine.WorldSettings", "TimeDilation", 4),
    ("Chimera.CrPlayerControllerBase", "PlayerMainCharacter", 8),
    ("Chimera.CrPlayerControllerBase", "bReplInPauseMenu", 1),
    ("Chimera.CrCharacterPlayerBase", "bDead", 1),
    ("Chimera.CrCharacterPlayerBase", "BuildingDrone", 8),
    ("Chimera.CrCharacterPlayerBase", "FPPCamera", 8),
    ("Chimera.CrCharacterPlayerBase", "DroneCamera", 8),
    ("Engine.ActorComponent", "bIsActive", 1),
    ("Chimera.CrGameStateBase", "WaveTimerActor", 8),
    ("Chimera.CrGameStateBase", "bIsGamePaused", 1),
    ("Chimera.CrEnviroWaveTimerSubsystem", "TimerActor", 8),
    ("Chimera.CrEnviroWaveSubsystem", "CurrentPreWaveSubstage", 1),
    ("Chimera.CrEnviroWaveSubsystem", "CurrentFadeoutSubstage", 1),
    ("Chimera.CrEnviroWaveSubsystem", "CurrentGrowbackSubstage", 1),
    ("Chimera.CrWaveTimerActor", "NextTime", 4),
    ("Chimera.CrWaveTimerActor", "NextPhase", 4),
    ("Chimera.CrWaveTimerActor", "bPause", 1),
];
const GETTERS: &[&str] = &[
    "GetTimeSeconds",
    "GetAudioTimeSeconds",
    "GetRealTimeSeconds",
    "GetUnpausedTimeSeconds",
    "GetWorldOriginLocation",
];
#[derive(Serialize, Deserialize)]
struct Bindings {
    version: u32,
    identity: Identity,
    world: ObjectStamp,
    timer_subsystem: ObjectStamp,
    wave_subsystem: ObjectStamp,
    offsets: BTreeMap<String, u64>,
    masks: BTreeMap<String, u8>,
    getters: BTreeMap<String, u64>,
}
fn property_key(class: &str, field: &str) -> String {
    format!("/Script/{class}:{field}")
}
impl Bindings {
    fn offset(&self, c: &str, f: &str) -> Result<u64> {
        self.offsets.get(&property_key(c, f)).copied().context("Missing live field")
    }
    fn validate(&self, id: &Identity) -> Result<()> {
        ensure!(self.version == 2 && self.identity == *id, "Stale live-context cache");
        for &(c, f, _) in FIELDS {
            ensure!((8..=262144).contains(&self.offset(c, f)?), "Invalid context offset");
            if f.starts_with('b') {
                ensure!(
                    self.masks.get(&property_key(c, f)).is_some_and(|x| *x != 0),
                    "Invalid bool mask"
                );
            }
        }
        for g in GETTERS {
            ensure!(
                self.getters.get(*g).is_some_and(|o| (512..8192).contains(o)),
                "Invalid getter field"
            );
        }
        Ok(())
    }
}
async fn world(ctx: &Ctx, cache: &RuntimeCache, b: &RuntimeBindings) -> Result<(u64, u64)> {
    let controller = Ptr::<UObject>::new(cache.controller.address, ctx.clone())?;
    let level =
        controller.outer_private().read().await?.context("Player level unavailable")?.address();
    stamp(ctx, level).await?;
    let world = ctx.read::<u64>(level + b.fields.owning_world).await?;
    let persistent = ctx.read::<u64>(world + b.fields.persistent_level).await?;
    ensure!(
        ctx.read::<u64>(persistent + b.fields.owning_world).await? == world,
        "World/level mismatch"
    );
    Ok((world, persistent))
}
async fn discover(process: &platform::Process, base: &RuntimeCache) -> Result<Bindings> {
    let ctx = jmap_dumper::connect_manual(
        crate::discovery_memory::DiscoveryMemory::new(process.clone()),
        base.config.clone(),
        None,
    )
    .await?;
    let array = Ptr::<FUObjectArray>::new(ctx.uobjectarray, ctx.clone())?;
    let count = array.num_elements().await?;
    ensure!((1..=2_000_000).contains(&count), "Invalid registry size");
    let wanted: HashSet<&str> = FIELDS
        .iter()
        .map(|(c, _, _)| c.rsplit('.').next().unwrap())
        .chain(GETTERS.iter().copied())
        .chain(["CrEnviroWaveTimerSubsystem"])
        .collect();
    let mut offsets = BTreeMap::new();
    let mut masks = BTreeMap::new();
    let mut getters = BTreeMap::new();
    let mut subsystems = vec![];
    for index in 0..count as usize {
        let Some(obj) = array.read_item_ptr(index).await? else {
            continue;
        };
        if obj.internal_index().read().await.ok() != Some(index as i32) {
            continue;
        }
        let name = obj.name_private().read().await?;
        if !wanted.contains(name.as_str())
            && !name.starts_with("CrEnviroWaveTimerSubsystem_")
            && !name.starts_with("CrEnviroWaveSubsystem_")
        {
            continue;
        }
        let object_path = path(&ctx, obj.address()).await?;
        if GETTERS.contains(&name.as_str())
            && object_path == format!("/Script/Engine.GameplayStatics:{name}")
        {
            let function = obj.cast::<UFunction>().func().read().await? as u64;
            let mut bytes = [0u8; 192];
            ctx.read_buf(function, &mut bytes).await?;
            let mut decoder = Decoder::with_ip(64, &bytes, function, DecoderOptions::NONE);
            let mut found = vec![];
            while decoder.can_decode() {
                let i = decoder.decode();
                if i.memory_base() == Register::RAX
                    && ((name == "GetWorldOriginLocation" && i.mnemonic() == Mnemonic::Lea)
                        || (name != "GetWorldOriginLocation" && i.mnemonic() == Mnemonic::Movsd))
                {
                    found.push(i.memory_displacement64());
                }
                if i.mnemonic() == Mnemonic::Ret {
                    break;
                }
            }
            ensure!(
                found.len() == 1 && (512..8192).contains(&found[0]),
                "Unsupported native getter layout"
            );
            getters.insert(name.clone(), found[0]);
        }
        if FIELDS.iter().any(|(c, _, _)| object_path == format!("/Script/{c}")) {
            let mut props = obj.cast::<UStruct>().properties(false);
            let mut visits = 0;
            while let Some(prop) = props.next().await {
                visits += 1;
                ensure!(visits <= 4096, "Invalid property chain");
                let prop = prop?;
                let name = prop.zfield().name_private().read().await?;
                if let Some((c, f, size)) = FIELDS
                    .iter()
                    .find(|(c, f, _)| object_path == format!("/Script/{c}") && name == *f)
                {
                    ensure!(
                        prop.element_size().read().await? as usize == *size
                            && prop.array_dim().read().await? == 1,
                        "Unexpected live field layout"
                    );
                    let k = property_key(c, f);
                    offsets.insert(k.clone(), prop.offset_internal().read().await? as u64);
                    if f.starts_with('b') {
                        masks.insert(k, prop.cast::<ZBoolProperty>().field_mask().read().await?);
                    }
                }
            }
        }
        if (name.starts_with("CrEnviroWaveTimerSubsystem_")
            || name.starts_with("CrEnviroWaveSubsystem_"))
            && obj.object_flags().read().await?.bits() & (0x10 | 0x20 | 0x8000 | 0x10000) == 0
        {
            subsystems.push(obj.address());
        }
    }
    let mut b = Bindings {
        version: 2,
        identity: process.identity.clone(),
        world: base.controller.clone(),
        timer_subsystem: base.controller.clone(),
        wave_subsystem: base.controller.clone(),
        offsets,
        masks,
        getters,
    };
    b.validate(&process.identity)?;
    let (world, _) = world(&ctx, base, &RuntimeBindings::new(&b)?).await?;
    b.world = stamp(&ctx, world).await?;
    let matching: Vec<_> = subsystems.into_iter().filter(|a| *a != 0).collect();
    let mut selected = vec![];
    for address in matching {
        if Ptr::<UObject>::new(address, ctx.clone())?
            .outer_private()
            .read()
            .await?
            .is_some_and(|o| o.address() == world)
        {
            selected.push((address, stamp(&ctx, address).await?));
        }
    }
    ensure!(selected.len() == 2, "Live wave subsystems unavailable or ambiguous");
    b.timer_subsystem = selected
        .iter()
        .find(|(_, s)| s.name.starts_with("CrEnviroWaveTimerSubsystem_"))
        .context("Missing timer subsystem")?
        .1
        .clone();
    b.wave_subsystem = selected
        .iter()
        .find(|(_, s)| s.name.starts_with("CrEnviroWaveSubsystem_"))
        .context("Missing phase subsystem")?
        .1
        .clone();
    Ok(b)
}
fn finite(v: f64) -> Result<f64> {
    ensure!(v.is_finite() && v >= 0.0, "Invalid live clock/countdown");
    Ok(v)
}
fn signed_finite(v: f64) -> Result<f64> {
    ensure!(v.is_finite(), "Non-finite live value");
    Ok(v)
}
fn countdown(remaining: f64, waiting: bool, stopped: bool) -> Result<Option<f64>> {
    signed_finite(remaining)?;
    if waiting && !stopped {
        Ok(Some(finite(remaining)?))
    } else {
        Ok(None)
    }
}
async fn read(
    process: &platform::Process,
    base: &RuntimeCache,
    b: &Bindings,
) -> Result<LiveContext> {
    let runtime = RuntimeBindings::new(b)?;
    runtime.validate(&process.identity)?;
    let ctx = jmap_dumper::connect_manual(process.clone(), base.config.clone(), None).await?;
    read_context(process, base, &runtime, ctx).await
}
async fn read_context(
    process: &platform::Process,
    base: &RuntimeCache,
    b: &RuntimeBindings,
    ctx: Ctx,
) -> Result<LiveContext> {
    b.validate(&process.identity)?;
    let (world, persistent) = world(&ctx, base, b).await?;
    ensure!(
        stamp(&ctx, world).await? == b.world
            && stamp(&ctx, b.timer_subsystem.address).await? == b.timer_subsystem
            && stamp(&ctx, b.wave_subsystem.address).await? == b.wave_subsystem,
        "Live world/subsystem identity changed"
    );
    let world_path = path(&ctx, world).await?;
    ensure!(
        world_path == "/Game/Chimera/Maps/ChimeraMain/ChimeraMain.ChimeraMain",
        "Unsupported world frame"
    );
    let settings = ctx.read::<u64>(persistent + b.fields.world_settings).await?;
    let state = ctx.read::<u64>(world + b.fields.game_state).await?;
    let settings_stamp = stamp(&ctx, settings).await?;
    let state_stamp = stamp(&ctx, state).await?;
    let controller = base.controller.address;
    let character = ctx.read::<u64>(controller + b.fields.main_character).await?;
    let character_stamp = stamp(&ctx, character).await?;
    ensure!(
        ctx.read::<u64>(controller + base.fields.character).await? == character,
        "Main character and possession disagree"
    );
    let timer = ctx.read::<u64>(state + b.fields.wave_timer_actor).await?;
    let timer_stamp = stamp(&ctx, timer).await?;
    ensure!(
        ctx.read::<u64>(b.timer_subsystem.address + b.fields.timer_actor).await? == timer,
        "Independent timer references disagree"
    );
    let observed_start = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let time = finite(ctx.read::<f64>(world + b.clocks.simulation_seconds).await?)?;
    let audio = finite(ctx.read::<f64>(world + b.clocks.audio_seconds).await?)?;
    let real = finite(ctx.read::<f64>(world + b.clocks.real_seconds).await?)?;
    let unpaused = finite(ctx.read::<f64>(world + b.clocks.unpaused_seconds).await?)?;
    let origin_offset = b.clocks.world_origin;
    let origin = [
        ctx.read::<i32>(world + origin_offset).await?,
        ctx.read::<i32>(world + origin_offset + 4).await?,
        ctx.read::<i32>(world + origin_offset + 8).await?,
    ];
    let rebasing = b.fields.origin_rebasing.read(&ctx, settings).await?;
    let pauser = ctx.read::<u64>(settings + b.fields.pauser_player_state).await?;
    if pauser != 0 {
        stamp(&ctx, pauser).await?;
    }
    // Exact build pinned at entry. This native IsPaused offset/branch was disassembled and calibrated.
    let pause_delay = signed_finite(ctx.read::<f64>(world + 0x860).await?)?;
    let menu = b.fields.menu_open.read(&ctx, controller).await?;
    let game_flag = b.fields.game_paused.read(&ctx, state).await?;
    let fpp = ctx.read::<u64>(character + b.fields.first_person_camera).await?;
    let drone_camera = ctx.read::<u64>(character + b.fields.drone_camera).await?;
    stamp(&ctx, fpp).await?;
    stamp(&ctx, drone_camera).await?;
    let fpp_active = b.fields.camera_active.read(&ctx, fpp).await?;
    let drone_active = b.fields.camera_active.read(&ctx, drone_camera).await?;
    let drone = ctx.read::<u64>(character + b.fields.building_drone).await?;
    if drone != 0 {
        stamp(&ctx, drone).await?;
    }
    let dead = b.fields.player_dead.read(&ctx, character).await?;
    let view = if !dead && drone != 0 && drone_active && !fpp_active {
        ViewMode::BuilderDrone
    } else if !dead && drone == 0 && fpp_active && !drone_active {
        ViewMode::OnFoot
    } else {
        ViewMode::Unknown
    };
    let sub = b.timer_subsystem.address;
    let timing_configuration =
        crate::live_timer_config::read(&ctx, sub, base.config.image_base).await.ok();
    // These private fields were mapped through native OnPreSaveStart and exact paused-save joins.
    let remaining = signed_finite(ctx.read::<f32>(sub + 0x7c).await? as f64)?;
    ensure!(remaining <= 86400., "Waiting countdown out of supported range");
    let waiting = ctx.read::<u8>(sub + 0x80).await? != 0;
    let wave_pause = ctx.read::<u8>(sub + 0x81).await? != 0;
    let previous = ctx.read::<u8>(sub + 0x82).await?;
    let stopped = ctx.read::<u8>(sub + 0x83).await? != 0;
    let waves = ctx.read::<i64>(sub + 0x88).await?;
    ensure!((0..=10000000).contains(&waves), "Invalid wave counter");
    let actor_next_time = ctx.read::<f32>(timer + b.fields.actor_next_time).await?;
    let actor_next_phase = ctx.read::<i32>(timer + b.fields.actor_next_phase).await?;
    let actor_pause = b.fields.actor_paused.read(&ctx, timer).await?;
    let phase_pointer = ctx.read::<u64>(b.wave_subsystem.address + 0x1f8).await?;
    let phase_evidence =
        match crate::live_phase::read(&ctx, b.wave_subsystem.address, base.config.image_base).await
        {
            Ok((pointer, value)) => {
                ensure!(pointer == phase_pointer, ObservationConflict::Phase);
                value
            }
            Err(_) => PhaseEvidence::Unavailable,
        };
    let mut substages = [0u8; 3];
    for (index, offset) in
        [b.fields.pre_wave_substage, b.fields.fadeout_substage, b.fields.growback_substage]
            .into_iter()
            .enumerate()
    {
        substages[index] = ctx.read::<u8>(b.wave_subsystem.address + offset).await?;
    }
    smol::Timer::after(std::time::Duration::from_millis(30)).await;
    let end_time = finite(ctx.read::<f64>(world + b.clocks.simulation_seconds).await?)?;
    ensure!(
        end_time >= time && end_time - time < 2.,
        "Simulation clock jumped during live observation"
    );
    ensure!(
        ctx.read::<u64>(state + b.fields.wave_timer_actor).await? == timer
            && stamp(&ctx, world).await? == b.world
            && stamp(&ctx, settings).await? == settings_stamp
            && stamp(&ctx, state).await? == state_stamp
            && stamp(&ctx, timer).await? == timer_stamp
            && stamp(&ctx, character).await? == character_stamp,
        "Live identities changed during observation"
    );
    ensure!(
        ctx.read::<u64>(controller + b.fields.main_character).await? == character,
        "Player changed during observation"
    );
    process.ensure_alive()?;
    if let Some(configuration) = timing_configuration {
        ensure!(
            ctx.read::<f32>(sub + 0x78).await? as f64 == configuration.interval_seconds
                && ctx.read::<f32>(sub + 0x64).await? as f64
                    == configuration.warning_duration_seconds,
            ObservationConflict::TimerConfiguration
        );
    }
    ensure!(
        stamp(&ctx, b.wave_subsystem.address).await? == b.wave_subsystem
            && ctx.read::<u64>(b.wave_subsystem.address + 0x1f8).await? == phase_pointer
            && (ctx.read::<u8>(sub + 0x80).await? != 0) == waiting
            && ctx.read::<u8>(sub + 0x82).await? == previous,
        ObservationConflict::Wave
    );
    let paused = if pauser != 0 && pause_delay <= time {
        Some(true)
    } else if end_time > time {
        Some(false)
    } else {
        None
    };
    let countdown = countdown(remaining, waiting, stopped)?;
    let main_character_path = path(&ctx, character).await?;
    let controller_path = path(&ctx, controller).await?;
    let persistent_level_path = path(&ctx, persistent).await?;
    let time_dilation = f64::from(ctx.read::<f32>(settings + b.fields.time_dilation).await?);
    Ok(LiveContext {
        player: PlayerContext {
            dead,
            view_mode: view,
            main_character_name: character_stamp.name,
            main_character_path,
            controller_path,
            first_person_active: fpp_active,
            drone_active,
            drone_present: drone != 0,
        },
        world: WorldState {
            world_path,
            persistent_level_path,
            origin,
            origin_rebasing_enabled: rebasing,
            map_coordinates_compatible: origin == [0, 0, 0] && !rebasing,
        },
        clocks: Clocks {
            simulation_seconds: end_time,
            audio_seconds: audio,
            real_seconds: real,
            unpaused_seconds: unpaused,
            simulation_at_start: time,
            time_dilation,
        },
        pause: PauseState {
            paused,
            menu_open: menu,
            pauser_present: pauser != 0,
            pause_delay_seconds: pause_delay,
            game_state_pause_flag: game_flag,
        },
        wave: WaveObservation {
            waiting,
            disabled: stopped,
            specific_pause: wave_pause,
            wave_count: waves,
            previous_stage: NativeStage::try_from(previous).ok(),
            waiting_remaining_seconds: countdown,
            native_waiting_countdown: countdown.is_some(),
            actor_next_time: actor_next_time.is_finite().then_some(f64::from(actor_next_time)),
            actor_next_phase,
            actor_paused: actor_pause,
            phase: phase_evidence,
            configuration: timing_configuration,
            substages,
        },
        window: ReadWindow {
            started_ms: observed_start as u64,
            completed_ms: SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64,
            atomic: false,
        },
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_validated_waiting_has_eta() {
        assert_eq!(countdown(123.0, true, false).unwrap(), Some(123.0));
        assert_eq!(countdown(119.0, false, false).unwrap(), None);
        assert_eq!(countdown(-1.0, false, false).unwrap(), None);
        assert_eq!(countdown(123.0, true, true).unwrap(), None);
        assert!(countdown(-1.0, true, false).is_err());
        assert!(countdown(f64::NAN, false, false).is_err());
    }
}
pub async fn snapshot(
    process: &platform::Process,
    base: &RuntimeCache,
    directory: &FilePath,
) -> Result<LiveContext> {
    let file = directory.join("live-context.json");
    let cached = crate::cache::read_json::<Bindings>(&file, 65536).ok();
    if let Some(b) = cached.filter(|b| b.validate(&process.identity).is_ok())
        && let Ok(value) = read(process, base, &b).await
    {
        return Ok(value);
    }
    let b = discover(process, base).await?;
    let value = read(process, base, &b).await?;
    let temporary = directory.join(format!("live-{}.tmp", std::process::id()));
    std::fs::write(&temporary, serde_json::to_vec_pretty(&b)?)?;
    if file.exists() {
        std::fs::remove_file(&file)?;
    }
    std::fs::rename(temporary, file)?;
    Ok(value)
}

pub(crate) mod resident;
