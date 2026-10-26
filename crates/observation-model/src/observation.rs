//! Typed observations shared by the reader, timing calculation and HUD.
use crate::overlay_stream::GameIdentity;
use serde::Serialize;

pub const WORLD_PATH: &str = "/Game/Chimera/Maps/ChimeraMain/ChimeraMain.ChimeraMain";

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct Position {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ViewMode {
    OnFoot,
    BuilderDrone,
    #[default]
    Unknown,
}

#[derive(Clone, Debug)]
pub struct PlayerContext {
    pub dead: bool,
    pub view_mode: ViewMode,
    pub main_character_name: String,
    pub main_character_path: String,
    pub controller_path: String,
    pub first_person_active: bool,
    pub drone_active: bool,
    pub drone_present: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorldState {
    pub world_path: String,
    pub persistent_level_path: String,
    /// World-origin coordinates in x, y, z order.
    pub origin: [i32; 3],
    pub origin_rebasing_enabled: bool,
    pub map_coordinates_compatible: bool,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Clocks {
    pub simulation_seconds: f64,
    pub audio_seconds: f64,
    pub real_seconds: f64,
    pub unpaused_seconds: f64,
    pub simulation_at_start: f64,
    pub time_dilation: f64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PauseState {
    pub paused: Option<bool>,
    pub menu_open: bool,
    pub pauser_present: bool,
    pub pause_delay_seconds: f64,
    pub game_state_pause_flag: bool,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReadWindow {
    pub started_ms: u64,
    pub completed_ms: u64,
    pub atomic: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimerConfiguration {
    pub native_control_flow_verified: bool,
    pub interval_includes_warning: bool,
    pub interval_seconds: f64,
    pub warning_duration_seconds: f64,
}
/// Calibrated Heat fields in the native layout at offsets 0x1c through 0x44.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HeatSettings {
    pub wave_speed: f64,
    pub wave_start_position: f64,
    pub wave_end_position: f64,
    pub pre_wave_duration: f64,
    pub fadeout_fire_wave_duration: f64,
    pub fadeout_burning_duration: f64,
    pub fadeout_fading_duration: f64,
    pub growback_moon_phase_duration: f64,
    pub growback_regrowth_start_duration: f64,
    pub growback_regrowth_duration: f64,
    pub pre_wave_explosion_duration: f64,
}
impl HeatSettings {
    /// Decode consecutive native float32 fields promoted to float64.
    pub const fn from_native_fields(values: [f64; 11]) -> Self {
        let [
            wave_speed,
            wave_start_position,
            wave_end_position,
            pre_wave_duration,
            fadeout_fire_wave_duration,
            fadeout_burning_duration,
            fadeout_fading_duration,
            growback_moon_phase_duration,
            growback_regrowth_start_duration,
            growback_regrowth_duration,
            pre_wave_explosion_duration,
        ] = values;
        Self {
            wave_speed,
            wave_start_position,
            wave_end_position,
            pre_wave_duration,
            fadeout_fire_wave_duration,
            fadeout_burning_duration,
            fadeout_fading_duration,
            growback_moon_phase_duration,
            growback_regrowth_start_duration,
            growback_regrowth_duration,
            pre_wave_explosion_duration,
        }
    }
    pub const fn native_fields(self) -> [f64; 11] {
        [
            self.wave_speed,
            self.wave_start_position,
            self.wave_end_position,
            self.pre_wave_duration,
            self.fadeout_fire_wave_duration,
            self.fadeout_burning_duration,
            self.fadeout_fading_duration,
            self.growback_moon_phase_duration,
            self.growback_regrowth_start_duration,
            self.growback_regrowth_duration,
            self.pre_wave_explosion_duration,
        ]
    }
}
/// Native active-stage identifiers for the supported executable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum NativeStage {
    PreWave = 1,
    Moving = 2,
    Fadeout = 3,
    Growback = 4,
}
impl TryFrom<u8> for NativeStage {
    type Error = &'static str;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::PreWave),
            2 => Ok(Self::Moving),
            3 => Ok(Self::Fadeout),
            4 => Ok(Self::Growback),
            _ => Err("Unknown native wave stage"),
        }
    }
}
impl NativeStage {
    pub const fn phase(self) -> Phase {
        match self {
            Self::PreWave => Phase::PreWave,
            Self::Moving => Phase::Moving,
            Self::Fadeout => Phase::Fadeout,
            Self::Growback => Phase::Growback,
        }
    }
    pub const fn duration_seconds(self) -> f64 {
        match self {
            Self::PreWave => 120.0,
            Self::Moving => 10.0,
            Self::Fadeout => 80.0,
            Self::Growback => 735.0,
        }
    }
    pub const fn end_from_wave_start_seconds(self) -> f64 {
        match self {
            Self::PreWave => 0.0,
            Self::Moving => 10.0,
            Self::Fadeout => 90.0,
            Self::Growback => 825.0,
        }
    }
    pub const fn following_active_seconds(self) -> f64 {
        match self {
            Self::PreWave | Self::Growback => 0.0,
            Self::Moving => 815.0,
            Self::Fadeout => 735.0,
        }
    }
    /// The timer actor numbers its next-phase targets from zero.
    pub const fn actor_next_phase(self) -> i32 {
        self as i32 - 1
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActivePhase {
    pub getter_layouts_verified: bool,
    pub stage: NativeStage,
    pub wave_type: u8,
    pub progress: f64,
    pub inverse_duration: f64,
    pub native_update_binding_verified: bool,
    pub delta_cap_seconds: f64,
    pub wave_start_seconds: f64,
    pub settings: HeatSettings,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum PhaseEvidence {
    Inactive,
    Active(ActivePhase),
    #[default]
    Unavailable,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WaveObservation {
    pub waiting: bool,
    pub disabled: bool,
    pub specific_pause: bool,
    pub wave_count: i64,
    pub previous_stage: Option<NativeStage>,
    pub waiting_remaining_seconds: Option<f64>,
    pub native_waiting_countdown: bool,
    pub actor_next_time: Option<f64>,
    pub actor_next_phase: i32,
    pub actor_paused: bool,
    pub phase: PhaseEvidence,
    pub configuration: Option<TimerConfiguration>,
    /// CurrentPreWaveSubstage, CurrentFadeoutSubstage, CurrentGrowbackSubstage.
    pub substages: [u8; 3],
}
#[derive(Clone, Debug)]
pub struct LiveContext {
    pub player: PlayerContext,
    pub world: WorldState,
    pub clocks: Clocks,
    pub pause: PauseState,
    pub wave: WaveObservation,
    pub window: ReadWindow,
}
impl LiveContext {
    pub fn heap_bytes(&self) -> Option<usize> {
        [
            self.player.main_character_name.capacity(),
            self.player.main_character_path.capacity(),
            self.player.controller_path.capacity(),
            self.world.world_path.capacity(),
            self.world.persistent_level_path.capacity(),
        ]
        .into_iter()
        .try_fold(0usize, usize::checked_add)
    }
}
#[derive(Clone, Copy, Debug)]
pub struct PlayerSample {
    pub position: Position,
    pub observed_at_ms: u64,
    pub reader_is_elevated: bool,
}
#[derive(Clone, Debug)]
pub struct RawObservation {
    pub schema_version: u32,
    pub contract_version: &'static str,
    pub source_is_live_memory: bool,
    pub game_memory_writes: bool,
    pub process: GameIdentity,
    pub sample: PlayerSample,
    pub live: LiveContext,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    Waiting,
    PreWave,
    Moving,
    Fadeout,
    Growback,
    Disabled,
    #[default]
    Unknown,
}
impl Phase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Waiting => "waiting",
            Self::PreWave => "PreWave",
            Self::Moving => "Moving",
            Self::Fadeout => "Fadeout",
            Self::Growback => "Growback",
            Self::Disabled => "disabled",
            Self::Unknown => "unknown",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Verification {
    Verified,
    Corroborated,
    ProgressEstimate,
    ConditionalEstimate,
    UnverifiedConfiguration,
    #[default]
    Unknown,
}
impl Verification {
    pub fn supported(self) -> bool {
        matches!(
            self,
            Self::Verified
                | Self::Corroborated
                | Self::ProgressEstimate
                | Self::ConditionalEstimate
        )
    }
    pub fn estimated(self) -> bool {
        matches!(self, Self::ProgressEstimate | Self::ConditionalEstimate)
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WaveTiming {
    pub phase: Phase,
    pub wave_count: i64,
    pub rupture_remaining_seconds: Option<f64>,
    pub next_phase_remaining_seconds: Option<f64>,
    pub rupture_verification: Verification,
    pub stage_verification: Verification,
    pub current_rupture_in_progress: bool,
    pub waiting_interval_seconds: Option<f64>,
    pub warning_duration_seconds: Option<f64>,
    pub fadeout_duration_seconds: Option<f64>,
    pub actor_progress_drift_seconds: Option<f64>,
    pub actor_start_residual_seconds: Option<f64>,
    pub actor_next_phase_matches: bool,
    pub timing_reason: &'static str,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OverlaySession {
    pub reader_pid: u32,
    pub generation: u64,
}

#[derive(Clone, Debug)]
pub struct Observation {
    pub process: GameIdentity,
    pub overlay_session: OverlaySession,
    pub observed_at_ms: u64,
    pub window: ReadWindow,
    pub reader_is_elevated: bool,
    pub position: Position,
    pub map_position: Option<Position>,
    pub world: WorldState,
    pub clocks: Clocks,
    pub pause: PauseState,
    pub wave: WaveTiming,
}
