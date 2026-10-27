//! Named runtime bindings resolved once from a checked discovery record.

use super::Bindings;
use crate::{Identity, ObjectStamp};
use anyhow::{Context, Result, ensure};
use jmap_dumper::mem::Ctx;
#[derive(Clone, Copy)]
pub(super) struct BooleanField {
    offset: u64,
    mask: u8,
}
impl BooleanField {
    pub async fn read(self, ctx: &Ctx, address: u64) -> Result<bool> {
        Ok(ctx.read::<u8>(address + self.offset).await? & self.mask != 0)
    }
}
pub(super) struct RuntimeBindings {
    version: u32,
    pub identity: Identity,
    pub world: ObjectStamp,
    pub timer_subsystem: ObjectStamp,
    pub wave_subsystem: ObjectStamp,
    pub fields: ContextFields,
    pub clocks: ClockFields,
}
pub(super) struct ContextFields {
    pub owning_world: u64,
    pub world_settings: u64,
    pub persistent_level: u64,
    pub game_state: u64,
    pub pauser_player_state: u64,
    pub origin_rebasing: BooleanField,
    pub time_dilation: u64,
    pub main_character: u64,
    pub menu_open: BooleanField,
    pub player_dead: BooleanField,
    pub building_drone: u64,
    pub first_person_camera: u64,
    pub drone_camera: u64,
    pub camera_active: BooleanField,
    pub wave_timer_actor: u64,
    pub game_paused: BooleanField,
    pub timer_actor: u64,
    pub pre_wave_substage: u64,
    pub fadeout_substage: u64,
    pub growback_substage: u64,
    pub actor_next_time: u64,
    pub actor_next_phase: u64,
    pub actor_paused: BooleanField,
}
pub(super) struct ClockFields {
    pub simulation_seconds: u64,
    pub audio_seconds: u64,
    pub real_seconds: u64,
    pub unpaused_seconds: u64,
    pub world_origin: u64,
}
impl RuntimeBindings {
    pub fn new(record: &Bindings) -> Result<Self> {
        record.validate(&record.identity)?;
        let boolean = |class, field| -> Result<BooleanField> {
            Ok(BooleanField {
                offset: record.offset(class, field)?,
                mask: *record
                    .masks
                    .get(&super::property_key(class, field))
                    .context("Missing boolean mask")?,
            })
        };
        let fields = ContextFields {
            owning_world: record.offset("Engine.Level", "OwningWorld")?,
            world_settings: record.offset("Engine.Level", "WorldSettings")?,
            persistent_level: record.offset("Engine.World", "PersistentLevel")?,
            game_state: record.offset("Engine.World", "GameState")?,
            pauser_player_state: record.offset("Engine.WorldSettings", "PauserPlayerState")?,
            origin_rebasing: boolean("Engine.WorldSettings", "bEnableWorldOriginRebasing")?,
            time_dilation: record.offset("Engine.WorldSettings", "TimeDilation")?,
            main_character: record
                .offset("Chimera.CrPlayerControllerBase", "PlayerMainCharacter")?,
            menu_open: boolean("Chimera.CrPlayerControllerBase", "bReplInPauseMenu")?,
            player_dead: boolean("Chimera.CrCharacterPlayerBase", "bDead")?,
            building_drone: record.offset("Chimera.CrCharacterPlayerBase", "BuildingDrone")?,
            first_person_camera: record.offset("Chimera.CrCharacterPlayerBase", "FPPCamera")?,
            drone_camera: record.offset("Chimera.CrCharacterPlayerBase", "DroneCamera")?,
            camera_active: boolean("Engine.ActorComponent", "bIsActive")?,
            wave_timer_actor: record.offset("Chimera.CrGameStateBase", "WaveTimerActor")?,
            game_paused: boolean("Chimera.CrGameStateBase", "bIsGamePaused")?,
            timer_actor: record.offset("Chimera.CrEnviroWaveTimerSubsystem", "TimerActor")?,
            pre_wave_substage: record
                .offset("Chimera.CrEnviroWaveSubsystem", "CurrentPreWaveSubstage")?,
            fadeout_substage: record
                .offset("Chimera.CrEnviroWaveSubsystem", "CurrentFadeoutSubstage")?,
            growback_substage: record
                .offset("Chimera.CrEnviroWaveSubsystem", "CurrentGrowbackSubstage")?,
            actor_next_time: record.offset("Chimera.CrWaveTimerActor", "NextTime")?,
            actor_next_phase: record.offset("Chimera.CrWaveTimerActor", "NextPhase")?,
            actor_paused: boolean("Chimera.CrWaveTimerActor", "bPause")?,
        };
        let clocks = ClockFields {
            simulation_seconds: *record
                .getters
                .get("GetTimeSeconds")
                .context("Missing getter field")?,
            audio_seconds: *record
                .getters
                .get("GetAudioTimeSeconds")
                .context("Missing getter field")?,
            real_seconds: *record
                .getters
                .get("GetRealTimeSeconds")
                .context("Missing getter field")?,
            unpaused_seconds: *record
                .getters
                .get("GetUnpausedTimeSeconds")
                .context("Missing getter field")?,
            world_origin: *record
                .getters
                .get("GetWorldOriginLocation")
                .context("Missing getter field")?,
        };
        Ok(Self {
            version: record.version,
            identity: record.identity.clone(),
            world: record.world.clone(),
            timer_subsystem: record.timer_subsystem.clone(),
            wave_subsystem: record.wave_subsystem.clone(),
            fields,
            clocks,
        })
    }
    pub fn validate(&self, identity: &Identity) -> Result<()> {
        ensure!(self.version == 2 && self.identity == *identity, "Stale live-context cache");
        ensure!(
            [
                self.fields.owning_world,
                self.fields.world_settings,
                self.fields.persistent_level,
                self.fields.game_state,
                self.fields.pauser_player_state,
                self.fields.origin_rebasing.offset,
                self.fields.time_dilation,
                self.fields.main_character,
                self.fields.menu_open.offset,
                self.fields.player_dead.offset,
                self.fields.building_drone,
                self.fields.first_person_camera,
                self.fields.drone_camera,
                self.fields.camera_active.offset,
                self.fields.wave_timer_actor,
                self.fields.game_paused.offset,
                self.fields.timer_actor,
                self.fields.pre_wave_substage,
                self.fields.fadeout_substage,
                self.fields.growback_substage,
                self.fields.actor_next_time,
                self.fields.actor_next_phase,
                self.fields.actor_paused.offset
            ]
            .into_iter()
            .all(|v| (8..=262144).contains(&v)),
            "Invalid context offset"
        );
        ensure!(
            [
                self.fields.origin_rebasing.mask,
                self.fields.menu_open.mask,
                self.fields.player_dead.mask,
                self.fields.camera_active.mask,
                self.fields.game_paused.mask,
                self.fields.actor_paused.mask
            ]
            .into_iter()
            .all(|v| v != 0),
            "Invalid bool mask"
        );
        ensure!(
            [
                self.clocks.simulation_seconds,
                self.clocks.audio_seconds,
                self.clocks.real_seconds,
                self.clocks.unpaused_seconds,
                self.clocks.world_origin
            ]
            .into_iter()
            .all(|v| (512..8192).contains(&v)),
            "Invalid getter field"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live_context::{FIELDS, GETTERS, property_key};
    fn record() -> Bindings {
        let identity = Identity {
            pid: 123,
            creation_ticks: 456,
            executable: "Synthetic game".into(),
            executable_sha256: crate::BUILD_HASH.into(),
        };
        let stamp = ObjectStamp {
            address: 65536,
            index: 1,
            class: 131072,
            name: "Synthetic object".into(),
        };
        Bindings {
            version: 2,
            identity,
            world: stamp.clone(),
            timer_subsystem: stamp.clone(),
            wave_subsystem: stamp,
            offsets: FIELDS
                .iter()
                .enumerate()
                .map(|(i, (class, field, _))| (property_key(class, field), 100 + i as u64 * 8))
                .collect(),
            masks: FIELDS
                .iter()
                .filter(|(_, field, _)| field.starts_with('b'))
                .map(|(class, field, _)| (property_key(class, field), 16))
                .collect(),
            getters: GETTERS
                .iter()
                .enumerate()
                .map(|(i, name)| (name.to_string(), 512 + i as u64 * 8))
                .collect(),
        }
    }
    #[test]
    fn resolving_cached_bindings_preserves_records_and_process_guards() {
        let record = record();
        let bytes = serde_json::to_vec(&record).unwrap();
        let runtime = RuntimeBindings::new(&record).unwrap();
        runtime.validate(&record.identity).unwrap();
        assert_eq!(serde_json::to_vec(&record).unwrap(), bytes);
        assert_eq!(runtime.fields.owning_world, 100);
        assert_eq!(runtime.fields.menu_open.mask, 16);
        assert_eq!(runtime.clocks.simulation_seconds, 512);
        let changed = Identity {
            creation_ticks: 457,
            ..record.identity.clone()
        };
        assert!(runtime.validate(&changed).is_err());
    }
    #[test]
    fn missing_out_of_range_or_unmasked_cached_fields_are_rejected_before_sampling() {
        for &(class, field, _) in FIELDS {
            for value in [None, Some(7), Some(262145)] {
                let mut record = record();
                let key = property_key(class, field);
                if let Some(value) = value {
                    record.offsets.insert(key, value);
                } else {
                    record.offsets.remove(&key);
                }
                assert!(RuntimeBindings::new(&record).is_err(), "{class}:{field} = {value:?}");
            }
            if field.starts_with('b') {
                let mut record = record();
                record.masks.insert(property_key(class, field), 0);
                assert!(RuntimeBindings::new(&record).is_err(), "{class}:{field}");
            }
        }
        for getter in GETTERS {
            for value in [511, 8192] {
                let mut record = record();
                record.getters.insert((*getter).into(), value);
                assert!(RuntimeBindings::new(&record).is_err(), "{getter} = {value}");
            }
        }
        let mut wrong_version = record();
        wrong_version.version = 1;
        assert!(RuntimeBindings::new(&wrong_version).is_err());
    }
}
