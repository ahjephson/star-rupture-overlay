use starrupture_observation_model::{
    READER_CONTRACT_VERSION,
    observation::*,
    overlay_stream::{
        GAME_HASH, GameIdentity, MAX_OBSERVATION_BYTES, Message, Payload, Statistics, WorldSample,
    },
};
use starrupture_wave_core::{
    live::validate_observation,
    live_timing::{HEAT_SETTINGS, live_wave_timing},
};
fn wave(stage: usize, progress: f64) -> WaveObservation {
    let duration = [0.0, 120.0, 10.0, 80.0, 735.0][stage];
    WaveObservation {
        previous_stage: Some(NativeStage::try_from(stage as u8).unwrap()),
        configuration: Some(TimerConfiguration {
            native_control_flow_verified: true,
            interval_includes_warning: true,
            interval_seconds: 2400.0,
            warning_duration_seconds: 120.0,
        }),
        actor_next_time: Some(10000.0 + duration * (1.0 - progress)),
        actor_next_phase: [0, 0, 1, 2, 3][stage],
        phase: PhaseEvidence::Active(ActivePhase {
            getter_layouts_verified: true,
            stage: NativeStage::try_from(stage as u8).unwrap(),
            wave_type: 1,
            progress,
            inverse_duration: 1.0 / duration,
            native_update_binding_verified: true,
            delta_cap_seconds: 0.1,
            settings: HEAT_SETTINGS,
            wave_start_seconds: 10000.0 + duration * (1.0 - progress)
                - [0.0, 0.0, 10.0, 90.0, 825.0][stage],
        }),
        ..WaveObservation::default()
    }
}
fn fixture() -> RawObservation {
    RawObservation {
        schema_version: 1,
        contract_version: READER_CONTRACT_VERSION,
        source_is_live_memory: true,
        game_memory_writes: false,
        process: GameIdentity {
            pid: 123,
            creation_ticks: 134000000000000000,
            executable_sha256: GAME_HASH.into(),
        },
        sample: PlayerSample {
            position: Position {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            },
            observed_at_ms: 1780000000000,
            reader_is_elevated: false,
        },
        live: LiveContext {
            player: PlayerContext {
                dead: false,
                view_mode: ViewMode::BuilderDrone,
                main_character_name: "Synthetic character".into(),
                main_character_path: "Synthetic character".into(),
                controller_path: "Synthetic controller".into(),
                first_person_active: false,
                drone_active: true,
                drone_present: true,
            },
            world: WorldState {
                world_path: WORLD_PATH.into(),
                map_coordinates_compatible: true,
                ..WorldState::default()
            },
            clocks: Clocks {
                simulation_seconds: 10000.0,
                audio_seconds: 10000.0,
                real_seconds: 10000.0,
                unpaused_seconds: 10000.0,
                simulation_at_start: 10000.0,
                time_dilation: 1.0,
            },
            pause: PauseState {
                paused: Some(true),
                menu_open: true,
                pauser_present: true,
                ..PauseState::default()
            },
            wave: wave(4, 0.5),
            window: ReadWindow {
                started_ms: 1779999999990,
                completed_ms: 1780000000000,
                atomic: false,
            },
        },
    }
}
#[test]
fn active_timing_uses_native_progress_rate_and_keeps_actor_drift_diagnostic() {
    for stage in 1..=4 {
        for progress in [0.0, 0.5, 1.0] {
            let mut w = wave(stage, progress);
            w.actor_next_time = Some(2400.0);
            w.actor_next_phase = 99;
            let r = live_wave_timing(&w, 10000.0);
            let expected = [0.0, 120.0, 10.0, 80.0, 735.0][stage] * (1.0 - progress);
            assert_eq!(r.next_phase_remaining_seconds, Some(expected));
            assert_eq!(r.stage_verification, Verification::ProgressEstimate);
            assert!(!r.actor_next_phase_matches);
        }
    }
    let mut w = wave(4, 0.569159746170044);
    let PhaseEvidence::Active(ref mut p) = w.phase else {
        panic!("Expected active phase")
    };
    p.inverse_duration = f64::from(1.0f32 / 735.0);
    p.wave_start_seconds = 137901.28125;
    w.actor_next_time = Some(138726.28125);
    let r = live_wave_timing(&w, 138411.75120060518);
    assert!(r.actor_progress_drift_seconds.unwrap() > 2.0);
    assert_eq!(r.stage_verification, Verification::ProgressEstimate);
}
#[test]
fn invalid_phase_settings_pause_binding_and_cap_fail_closed() {
    for case in 0..19 {
        let mut w = wave(4, 0.5);
        let PhaseEvidence::Active(ref mut p) = w.phase else {
            panic!("Expected active phase")
        };
        match case {
            0 => w.specific_pause = true,
            1 => w.actor_paused = true,
            2 => p.wave_type = 2,
            3 => p.progress = 1.1,
            4 => p.inverse_duration = 0.0,
            5 => p.getter_layouts_verified = false,
            6 => p.native_update_binding_verified = false,
            7 => p.delta_cap_seconds = 0.2,
            8 => p.settings.pre_wave_duration = 30.0,
            9 => p.wave_start_seconds = 20000.0,
            10 => w.previous_stage = Some(NativeStage::Moving),
            11 => p.progress = f64::NAN,
            12 => p.inverse_duration = f64::INFINITY,
            13 => p.delta_cap_seconds = f64::NAN,
            14 => p.settings.wave_speed = f64::NAN,
            15 => w.previous_stage = None,
            16 => p.wave_start_seconds = -1.0,
            17 => p.wave_start_seconds = f64::NAN,
            18 => w.phase = PhaseEvidence::Unavailable,
            _ => unreachable!(),
        }
        let r = live_wave_timing(&w, 10000.0);
        assert_eq!(r.next_phase_remaining_seconds, None, "case {case}");
        assert_eq!(r.rupture_remaining_seconds, None, "case {case}");
    }
}
#[test]
fn waiting_and_following_cycle_use_live_configuration_without_defaults() {
    let mut w = wave(4, 0.5);
    w.configuration = None;
    let r = live_wave_timing(&w, 10000.0);
    assert_eq!(r.rupture_remaining_seconds, None);
    assert_eq!(r.next_phase_remaining_seconds, Some(367.5));
    w.waiting = true;
    w.phase = PhaseEvidence::Inactive;
    w.waiting_remaining_seconds = Some(524.4);
    w.native_waiting_countdown = true;
    let r = live_wave_timing(&w, 10000.0);
    assert_eq!(r.rupture_verification, Verification::Verified);
    assert_eq!(r.next_phase_remaining_seconds, None);
    w.configuration = Some(TimerConfiguration {
        native_control_flow_verified: true,
        interval_includes_warning: true,
        interval_seconds: 1800.0,
        warning_duration_seconds: 90.0,
    });
    let r = live_wave_timing(&w, 10000.0);
    assert!((r.next_phase_remaining_seconds.unwrap() - 434.4).abs() < 1e-9);
    assert_eq!(r.stage_verification, Verification::UnverifiedConfiguration);
}
#[test]
fn waiting_conflicts_invalid_countdowns_and_disabled_waves_fail_closed() {
    let mut w = wave(4, 0.5);
    w.waiting = true;
    w.waiting_remaining_seconds = Some(123.0);
    w.native_waiting_countdown = true;
    assert_eq!(live_wave_timing(&w, 10000.0).phase, Phase::Unknown);
    w.phase = PhaseEvidence::Inactive;
    for remaining in [None, Some(-1.0), Some(86401.0), Some(f64::NAN), Some(f64::INFINITY)] {
        w.waiting_remaining_seconds = remaining;
        assert_eq!(live_wave_timing(&w, 10000.0).rupture_remaining_seconds, None);
    }
    w.waiting_remaining_seconds = Some(123.0);
    w.disabled = true;
    assert_eq!(live_wave_timing(&w, 10000.0).phase, Phase::Disabled);
}
#[test]
fn live_identity_frame_window_and_map_suppression_are_independent_of_saves() {
    let r = fixture();
    let v = validate_observation(r.clone(), 1780000000000, 1780000000000).unwrap();
    assert_eq!(v.map_position, Some(r.sample.position));
    for case in 0..4 {
        let mut r = fixture();
        match case {
            0 => r.live.player.dead = true,
            1 => r.live.world.origin[0] = 1,
            2 => r.live.world.origin_rebasing_enabled = true,
            3 => r.live.world.map_coordinates_compatible = false,
            _ => unreachable!(),
        }
        let v = validate_observation(r, 1780000000000, 1780000000000).unwrap();
        assert_eq!(v.map_position, None);
    }
    for case in 0..16 {
        let mut r = fixture();
        match case {
            0 => r.process.executable_sha256 = "wrong".into(),
            1 => r.process.creation_ticks = 0,
            2 => r.sample.observed_at_ms = 1,
            3 => r.live.window.atomic = true,
            4 => r.live.world.world_path = "interior".into(),
            5 => r.sample.position.x = f64::NAN,
            6 => r.live.clocks.simulation_seconds = -1.0,
            7 => r.schema_version = 2,
            8 => r.contract_version = "unsupported",
            9 => r.source_is_live_memory = false,
            10 => r.game_memory_writes = true,
            11 => r.process.pid = 0,
            12 => r.process.creation_ticks = u64::MAX,
            13 => r.live.window.started_ms = r.live.window.completed_ms + 1,
            14 => r.live.window.completed_ms = r.sample.observed_at_ms + 1,
            15 => r.live.clocks.time_dilation = f64::NAN,
            _ => unreachable!(),
        }
        assert!(validate_observation(r, 1780000000000, 1780000000000).is_err(), "case {case}");
    }
}
#[test]
fn observation_bound_accounts_for_reserved_string_storage() {
    let mut raw = fixture();
    raw.live.player.main_character_path.reserve(MAX_OBSERVATION_BYTES);
    let m = Message {
        version: 1,
        reader_pid: 4,
        sequence: 1,
        generation: 1,
        sampled_at_unix_ms: 1780000000000,
        process: Some(raw.process.clone()),
        read_only: true,
        game_code_invoked: false,
        payload: Payload::World(Box::new(WorldSample {
            raw,
            read_started_unix_ms: 1780000000000,
            statistics: Statistics::default(),
        })),
    };
    assert!(!m.within_bound());
}

#[test]
fn live_observation_accepts_another_well_formed_build_hash() {
    let mut raw = fixture();
    let hash = "a".repeat(64);
    raw.process.executable_sha256 = hash.clone().into();
    let observed = validate_observation(raw, 1780000000000, 1780000000000).unwrap();
    assert_eq!(observed.process.executable_sha256.as_ref(), hash);
}

#[test]
fn prewave_counts_down_before_the_first_rupture_start_is_set() {
    for stage in 1..=4 {
        let mut w = wave(stage, 0.5);
        let PhaseEvidence::Active(ref mut p) = w.phase else {
            panic!("Expected phase")
        };
        p.wave_start_seconds = f64::MIN;
        let timing = live_wave_timing(&w, 10000.0);
        if stage == 1 {
            assert_eq!(timing.phase, Phase::PreWave);
            assert_eq!(timing.next_phase_remaining_seconds, Some(60.0));
            assert_eq!(timing.rupture_remaining_seconds, Some(60.0));
            assert_eq!(timing.stage_verification, Verification::ProgressEstimate);
        } else {
            assert_eq!(timing.next_phase_remaining_seconds, None);
        }
    }
    for invalid in [-1.0, f64::NEG_INFINITY, f64::INFINITY, f64::NAN] {
        let mut w = wave(1, 0.5);
        let PhaseEvidence::Active(ref mut p) = w.phase else {
            panic!("Expected phase")
        };
        p.wave_start_seconds = invalid;
        assert_eq!(live_wave_timing(&w, 10000.0).next_phase_remaining_seconds, None);
    }
}
