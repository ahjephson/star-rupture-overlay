//! Validated read-only context and explicit evidence for guarded phase calculations.
use crate::{Cache, Identity, ObjectStamp, key, path, platform, stamp};
use anyhow::{Context, Result, ensure};
use iced_x86::{Decoder, DecoderOptions, Mnemonic, Register};
use jmap_dumper::{
    mem::{BlockCache, Ctx, Ptr},
    objects::{FUObjectArray, UFunction, UObject, UStruct, ZBoolProperty},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashSet},
    path::Path as FilePath,
    time::{SystemTime, UNIX_EPOCH},
};
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
    (
        "Chimera.CrEnviroWaveSubsystem",
        "CurrentGrowbackSubstage",
        1,
    ),
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
        self.offsets
            .get(&property_key(c, f))
            .copied()
            .context("Missing live field")
    }
    async fn ptr(&self, ctx: &Ctx, a: u64, c: &str, f: &str) -> Result<u64> {
        ctx.read(a + self.offset(c, f)?).await
    }
    async fn boolean(&self, ctx: &Ctx, a: u64, c: &str, f: &str) -> Result<bool> {
        let mask = self
            .masks
            .get(&property_key(c, f))
            .context("Missing boolean mask")?;
        Ok(ctx.read::<u8>(a + self.offset(c, f)?).await? & mask != 0)
    }
    fn validate(&self, id: &Identity) -> Result<()> {
        ensure!(
            self.version == 2 && self.identity == *id,
            "Stale live-context cache"
        );
        for &(c, f, _) in FIELDS {
            ensure!(
                (8..=262144).contains(&self.offset(c, f)?),
                "Invalid context offset"
            );
            if f.starts_with('b') {
                ensure!(
                    self.masks.get(&property_key(c, f)).is_some_and(|x| *x != 0),
                    "Invalid bool mask"
                );
            }
        }
        for g in GETTERS {
            ensure!(
                self.getters
                    .get(*g)
                    .is_some_and(|o| (512..8192).contains(o)),
                "Invalid getter field"
            );
        }
        Ok(())
    }
}
async fn world(ctx: &Ctx, cache: &Cache, b: &Bindings) -> Result<(u64, u64)> {
    let controller = Ptr::<UObject>::new(cache.controller.address, ctx.clone())?;
    let level = controller
        .outer_private()
        .read()
        .await?
        .context("Player level unavailable")?
        .address();
    stamp(ctx, level).await?;
    let world = b.ptr(ctx, level, "Engine.Level", "OwningWorld").await?;
    let persistent = b.ptr(ctx, world, "Engine.World", "PersistentLevel").await?;
    ensure!(
        b.ptr(ctx, persistent, "Engine.Level", "OwningWorld")
            .await?
            == world,
        "World/level mismatch"
    );
    Ok((world, persistent))
}
async fn discover(process: &platform::Process, base: &Cache) -> Result<Bindings> {
    let ctx =
        jmap_dumper::connect_manual(BlockCache::wrap(process.clone()), base.config.clone(), None)
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
        if FIELDS
            .iter()
            .any(|(c, _, _)| object_path == format!("/Script/{c}"))
        {
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
    let (world, _) = world(&ctx, base, &b).await?;
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
    ensure!(
        selected.len() == 2,
        "Live wave subsystems unavailable or ambiguous"
    );
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
async fn read(process: &platform::Process, base: &Cache, b: &Bindings) -> Result<Value> {
    b.validate(&process.identity)?;
    let ctx = jmap_dumper::connect_manual(process.clone(), base.config.clone(), None).await?;
    read_context(process,base,b,ctx).await
}
async fn read_context(process:&platform::Process,base:&Cache,b:&Bindings,ctx:Ctx)->Result<Value> {
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
    let settings = b
        .ptr(&ctx, persistent, "Engine.Level", "WorldSettings")
        .await?;
    let state = b.ptr(&ctx, world, "Engine.World", "GameState").await?;
    let settings_stamp = stamp(&ctx, settings).await?;
    let state_stamp = stamp(&ctx, state).await?;
    let controller = base.controller.address;
    let character = b
        .ptr(
            &ctx,
            controller,
            "Chimera.CrPlayerControllerBase",
            "PlayerMainCharacter",
        )
        .await?;
    let character_stamp = stamp(&ctx, character).await?;
    ensure!(
        ctx.read::<u64>(
            controller + base.offsets[&key("/Script/Engine.Controller", "Character")] as u64
        )
        .await?
            == character,
        "Main character and possession disagree"
    );
    let timer = b
        .ptr(&ctx, state, "Chimera.CrGameStateBase", "WaveTimerActor")
        .await?;
    let timer_stamp = stamp(&ctx, timer).await?;
    ensure!(
        b.ptr(
            &ctx,
            b.timer_subsystem.address,
            "Chimera.CrEnviroWaveTimerSubsystem",
            "TimerActor"
        )
        .await?
            == timer,
        "Independent timer references disagree"
    );
    let observed_start = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let time = finite(ctx.read::<f64>(world + b.getters["GetTimeSeconds"]).await?)?;
    let audio = finite(
        ctx.read::<f64>(world + b.getters["GetAudioTimeSeconds"])
            .await?,
    )?;
    let real = finite(
        ctx.read::<f64>(world + b.getters["GetRealTimeSeconds"])
            .await?,
    )?;
    let unpaused = finite(
        ctx.read::<f64>(world + b.getters["GetUnpausedTimeSeconds"])
            .await?,
    )?;
    let origin_offset = b.getters["GetWorldOriginLocation"];
    let origin = [
        ctx.read::<i32>(world + origin_offset).await?,
        ctx.read::<i32>(world + origin_offset + 4).await?,
        ctx.read::<i32>(world + origin_offset + 8).await?,
    ];
    let rebasing = b
        .boolean(
            &ctx,
            settings,
            "Engine.WorldSettings",
            "bEnableWorldOriginRebasing",
        )
        .await?;
    let pauser = b
        .ptr(&ctx, settings, "Engine.WorldSettings", "PauserPlayerState")
        .await?;
    if pauser != 0 {
        stamp(&ctx, pauser).await?;
    }
    // Exact build pinned at entry. This native IsPaused offset/branch was disassembled and calibrated.
    let pause_delay = signed_finite(ctx.read::<f64>(world + 0x860).await?)?;
    let menu = b
        .boolean(
            &ctx,
            controller,
            "Chimera.CrPlayerControllerBase",
            "bReplInPauseMenu",
        )
        .await?;
    let game_flag = b
        .boolean(&ctx, state, "Chimera.CrGameStateBase", "bIsGamePaused")
        .await?;
    let fpp = b
        .ptr(
            &ctx,
            character,
            "Chimera.CrCharacterPlayerBase",
            "FPPCamera",
        )
        .await?;
    let drone_camera = b
        .ptr(
            &ctx,
            character,
            "Chimera.CrCharacterPlayerBase",
            "DroneCamera",
        )
        .await?;
    stamp(&ctx, fpp).await?;
    stamp(&ctx, drone_camera).await?;
    let fpp_active = b
        .boolean(&ctx, fpp, "Engine.ActorComponent", "bIsActive")
        .await?;
    let drone_active = b
        .boolean(&ctx, drone_camera, "Engine.ActorComponent", "bIsActive")
        .await?;
    let drone = b
        .ptr(
            &ctx,
            character,
            "Chimera.CrCharacterPlayerBase",
            "BuildingDrone",
        )
        .await?;
    if drone != 0 {
        stamp(&ctx, drone).await?;
    }
    let dead = b
        .boolean(&ctx, character, "Chimera.CrCharacterPlayerBase", "bDead")
        .await?;
    let view = if !dead && drone != 0 && drone_active && !fpp_active {
        "builder_drone"
    } else if !dead && drone == 0 && fpp_active && !drone_active {
        "on_foot"
    } else {
        "unknown"
    };
    let sub = b.timer_subsystem.address;
    let timing_configuration =
        match crate::live_timer_config::read(&ctx, sub, base.config.image_base).await {
            Ok(value) => value,
            Err(error) => json!({"status":"unavailable","reason":format!("{error:#}")}),
        };
    // These private fields were mapped through native OnPreSaveStart and exact paused-save joins.
    let remaining = signed_finite(ctx.read::<f32>(sub + 0x7c).await? as f64)?;
    ensure!(
        remaining <= 86400.,
        "Waiting countdown out of supported range"
    );
    let waiting = ctx.read::<u8>(sub + 0x80).await? != 0;
    let wave_pause = ctx.read::<u8>(sub + 0x81).await? != 0;
    let previous = ctx.read::<u8>(sub + 0x82).await?;
    let stopped = ctx.read::<u8>(sub + 0x83).await? != 0;
    let waves = ctx.read::<i64>(sub + 0x88).await?;
    ensure!((0..=10000000).contains(&waves), "Invalid wave counter");
    let actor_next_time = ctx
        .read::<f32>(timer + b.offset("Chimera.CrWaveTimerActor", "NextTime")?)
        .await?;
    let actor_next_phase = ctx
        .read::<i32>(timer + b.offset("Chimera.CrWaveTimerActor", "NextPhase")?)
        .await?;
    let actor_pause = b
        .boolean(&ctx, timer, "Chimera.CrWaveTimerActor", "bPause")
        .await?;
    let phase_pointer = ctx.read::<u64>(b.wave_subsystem.address + 0x1f8).await?;
    let phase_evidence =
        match crate::live_phase::read(&ctx, b.wave_subsystem.address, base.config.image_base).await
        {
            Ok((pointer, value)) => {
                ensure!(pointer == phase_pointer, "Phase changed while sampling");
                value
            }
            Err(error) => json!({"status":"unavailable","reason":format!("{error:#}")}),
        };
    let mut substages = serde_json::Map::new();
    for field in [
        "CurrentPreWaveSubstage",
        "CurrentFadeoutSubstage",
        "CurrentGrowbackSubstage",
    ] {
        substages.insert(
            field.to_string(),
            json!(
                ctx.read::<u8>(
                    b.wave_subsystem.address + b.offset("Chimera.CrEnviroWaveSubsystem", field)?
                )
                .await?
            ),
        );
    }
    smol::Timer::after(std::time::Duration::from_millis(30)).await;
    let end_time = finite(ctx.read::<f64>(world + b.getters["GetTimeSeconds"]).await?)?;
    ensure!(
        end_time >= time && end_time - time < 2.,
        "Simulation clock jumped during live observation"
    );
    ensure!(
        b.ptr(&ctx, state, "Chimera.CrGameStateBase", "WaveTimerActor")
            .await?
            == timer
            && stamp(&ctx, world).await? == b.world
            && stamp(&ctx, settings).await? == settings_stamp
            && stamp(&ctx, state).await? == state_stamp
            && stamp(&ctx, timer).await? == timer_stamp
            && stamp(&ctx, character).await? == character_stamp,
        "Live identities changed during observation"
    );
    ensure!(
        b.ptr(
            &ctx,
            controller,
            "Chimera.CrPlayerControllerBase",
            "PlayerMainCharacter"
        )
        .await?
            == character,
        "Player changed during observation"
    );
    process.ensure_alive()?;
    if timing_configuration["status"] == "ok" {
        ensure!(
            ctx.read::<f32>(sub + 0x78).await? as f64
                == timing_configuration["intervalSeconds"].as_f64().unwrap()
                && ctx.read::<f32>(sub + 0x64).await? as f64
                    == timing_configuration["warningDurationSeconds"]
                        .as_f64()
                        .unwrap(),
            "Timer configuration changed during observation"
        );
    }
    ensure!(
        stamp(&ctx, b.wave_subsystem.address).await? == b.wave_subsystem
            && ctx.read::<u64>(b.wave_subsystem.address + 0x1f8).await? == phase_pointer
            && (ctx.read::<u8>(sub + 0x80).await? != 0) == waiting
            && ctx.read::<u8>(sub + 0x82).await? == previous,
        "Wave phase changed during observation"
    );
    let paused = if pauser != 0 && pause_delay <= time {
        Some(true)
    } else if end_time > time {
        Some(false)
    } else {
        None
    };
    let pause_basis = if paused == Some(true) {
        "native_pauser_delay_branch"
    } else if paused == Some(false) {
        "simulation_clock_advancing"
    } else {
        "not_established"
    };
    let countdown = countdown(remaining, waiting, stopped)?;
    Ok(
        json!({"player":{"dead":dead,"viewMode":view,"mainCharacterName":character_stamp.name,"mainCharacterPath":path(&ctx,character).await?,"controllerPath":path(&ctx,controller).await?,"cameraEvidence":{"firstPersonActive":fpp_active,"droneActive":drone_active,"droneReferencePresent":drone!=0}},
 "world":{"worldPath":world_path,"persistentLevelPath":path(&ctx,persistent).await?,"origin":{"x":origin[0],"y":origin[1],"z":origin[2]},"originRebasingEnabled":rebasing,"mapCoordinatesCompatible":origin==[0,0,0]&&!rebasing,"frameCoverage":"ChimeraMain only; other interiors and nonzero origins uncalibrated"},
 "clocks":{"simulationSeconds":end_time,"audioSeconds":audio,"realSeconds":real,"unpausedSeconds":unpaused,"simulationAtObservationStart":time,"timeDilation":ctx.read::<f32>(settings+b.offset("Engine.WorldSettings","TimeDilation")?).await?,"meaning":"Simulation/audio clocks stop during normal pause; real/unpaused clocks continue. Restored clocks can reset to saved values after reload."},
 "pause":{"paused":paused,"basis":pause_basis,"pauseMenuOpen":menu,"pauserPresent":pauser!=0,"pauseDelaySeconds":pause_delay,"gameStatePauseFlag":game_flag,"warning":"Game-state and wave-specific pause flags are not global pause truth; other native loading/network pause branches remain uncalibrated."},
 "wave":{"phase":if stopped{"disabled"}else if waiting{"waiting"}else{"active_unverified"},"ruptureRemainingSeconds":countdown,"countdownBasis":if countdown.is_some(){"native_waiting_countdown"}else{"unsupported_or_disabled"},"waitingForNextWave":waiting,"specificWavePauseFlag":wave_pause,"wavesDisabled":stopped,"waveCount":waves,"previousStageRaw":previous,"actor":{"nextTime":actor_next_time,"nextPhase":actor_next_phase,"specificPauseFlag":actor_pause},"phaseEvidence":phase_evidence,"timingConfiguration":timing_configuration,"substages":substages,"coverage":"Native stage/progress/settings/configuration readers; server calculates guarded timings and separate verification labels. Waiting timer must not be reused in active phases."},
 "observationWindow":{"startedAtUnixMilliseconds":observed_start,"completedAtUnixMilliseconds":SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis(),"atomic":false},
 "evidence":["Runtime native reflection and object registry joins","Clock/origin offsets decoded from live native getter instructions; no game code invocation","Normal-menu pause branch plus simulation clock behaviour","Timer private fields traced through native save callback and matched to saves","Actual restart and drone/on-foot validation for installed executable hash"]}),
    )
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
    base: &Cache,
    directory: &FilePath,
) -> Result<Value> {
    let file = directory.join("live-context.json");
    let cached = std::fs::read(&file)
        .ok()
        .filter(|b| b.len() <= 65536)
        .and_then(|b| serde_json::from_slice::<Bindings>(&b).ok());
    if let Some(b) = cached.filter(|b| b.validate(&process.identity).is_ok()) {
        if let Ok(value) = read(process, base, &b).await {
            return Ok(value);
        }
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

// Only the overlay binary uses this retained context.
#[allow(dead_code)]
pub(crate) mod resident;
