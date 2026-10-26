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
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActivePhase {
    pub getter_layouts_verified: bool,
    pub stage: u8,
    pub wave_type: u8,
    pub progress: f64,
    pub inverse_duration: f64,
    pub native_update_binding_verified: bool,
    pub delta_cap_seconds: f64,
    pub wave_start_seconds: f64,
    /// Native float32 fields at offsets 0x1c through 0x44: speed, start, end,
    /// pre-wave, fire wave, burning, fading, moon phase, regrowth start,
    /// regrowth and pre-wave explosion durations, promoted to float64.
    pub settings: [f64; 11],
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
    pub previous_stage: u8,
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
