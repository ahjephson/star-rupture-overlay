//! Guarded native phase timing from observed progress and configuration.
use starrupture_observation_model::observation::{
    HeatSettings, NativeStage, Phase, PhaseEvidence, Verification, WaveObservation, WaveTiming,
};
pub const HEAT_SETTINGS: HeatSettings = HeatSettings {
    wave_speed: 160000.0,
    wave_start_position: -1200000.0,
    wave_end_position: 400000.0,
    pre_wave_duration: 120.0,
    fadeout_fire_wave_duration: 20.0,
    fadeout_burning_duration: 30.0,
    fadeout_fading_duration: 30.0,
    growback_moon_phase_duration: 600.0,
    growback_regrowth_start_duration: 15.0,
    growback_regrowth_duration: 120.0,
    pre_wave_explosion_duration: 118.0999984741211,
};
fn unknown(w: &WaveObservation, reason: &'static str) -> WaveTiming {
    WaveTiming {
        phase: if w.disabled {
            Phase::Disabled
        } else {
            Phase::Unknown
        },
        wave_count: w.wave_count,
        timing_reason: reason,
        ..WaveTiming::default()
    }
}
pub fn live_wave_timing(w: &WaveObservation, clock: f64) -> WaveTiming {
    if w.disabled {
        return unknown(w, "waves_disabled");
    }
    if !clock.is_finite() || clock < 0.0 {
        return unknown(w, "invalid_simulation_clock");
    }
    if w.specific_pause || w.actor_paused {
        return unknown(w, "wave_specific_pause_or_unknown");
    }
    let configuration = w.configuration.filter(|c| {
        c.native_control_flow_verified
            && c.interval_includes_warning
            && c.interval_seconds.is_finite()
            && c.interval_seconds > 0.0
            && c.interval_seconds <= 86400.0
            && c.warning_duration_seconds.is_finite()
            && c.warning_duration_seconds > 0.0
            && c.warning_duration_seconds <= c.interval_seconds
    });
    let interval = configuration.map(|c| c.interval_seconds);
    let warning = configuration.map(|c| c.warning_duration_seconds);
    let calibrated = interval == Some(2400.0) && warning == Some(120.0);
    if w.waiting {
        if w.phase != PhaseEvidence::Inactive {
            return unknown(w, "waiting_and_phase_evidence_conflict");
        }
        let Some(remaining) =
            w.waiting_remaining_seconds.filter(|v| v.is_finite() && (0.0..=86400.0).contains(v))
        else {
            return unknown(w, "invalid_waiting_countdown");
        };
        if !w.native_waiting_countdown {
            return unknown(w, "invalid_waiting_countdown");
        }
        return WaveTiming {
            phase: Phase::Waiting,
            wave_count: w.wave_count,
            rupture_remaining_seconds: Some(remaining),
            next_phase_remaining_seconds: warning.map(|v| (remaining - v).max(0.0)),
            rupture_verification: Verification::Verified,
            stage_verification: if warning.is_none() {
                Verification::Unknown
            } else if calibrated {
                Verification::Corroborated
            } else {
                Verification::UnverifiedConfiguration
            },
            waiting_interval_seconds: interval,
            warning_duration_seconds: warning,
            timing_reason: "native_waiting_countdown_and_live_warning_threshold",
            ..WaveTiming::default()
        };
    }
    let PhaseEvidence::Active(p) = w.phase else {
        return unknown(w, "active_phase_reader_unavailable");
    };
    if !p.getter_layouts_verified {
        return unknown(w, "active_phase_reader_unavailable");
    }
    if Some(p.stage) != w.previous_stage {
        return unknown(w, "phase_transition_or_enum_conflict");
    }
    // Native waveType 1 identifies Heat.
    if p.wave_type != 1 {
        return unknown(w, "unsupported_wave_type");
    }
    if !(0.0..=1.0).contains(&p.progress) {
        return unknown(w, "invalid_phase_progress");
    }
    if p.stage != NativeStage::PreWave
        && (!p.wave_start_seconds.is_finite()
            || p.wave_start_seconds < 0.0
            || p.wave_start_seconds > clock)
    {
        return unknown(w, "invalid_wave_start");
    }
    if !p
        .settings
        .native_fields()
        .iter()
        .zip(HEAT_SETTINGS.native_fields())
        .all(|(a, b)| a.is_finite() && (a - b).abs() < 0.001)
    {
        return unknown(w, "uncalibrated_wave_settings");
    }
    if warning.is_some_and(|v| (v - p.settings.pre_wave_duration).abs() > 0.001) {
        return unknown(w, "warning_duration_conflicts_with_active_heat_settings");
    }
    let stage = p.stage;
    let duration = stage.duration_seconds();
    if !p.inverse_duration.is_finite()
        || p.inverse_duration <= 0.0
        || (p.inverse_duration * duration - 1.0).abs() > 0.00001
        || !p.native_update_binding_verified
        || !p.delta_cap_seconds.is_finite()
        || (p.delta_cap_seconds - 0.1).abs() > 0.000001
    {
        return unknown(w, "invalid_or_unverified_phase_clock");
    }
    let remaining = (1.0 - p.progress) / p.inverse_duration;
    let actor = w.actor_next_time.filter(|v| v.is_finite());
    let drift = actor.map(|v| remaining - (v - clock));
    let start_residual = if stage != NativeStage::PreWave {
        actor.map(|a| (a - (p.wave_start_seconds + stage.end_from_wave_start_seconds())).abs())
    } else {
        None
    };
    let until = if stage == NativeStage::PreWave {
        Some(remaining)
    } else {
        interval.map(|v| remaining + stage.following_active_seconds() + v)
    };
    WaveTiming {
        phase: stage.phase(),
        wave_count: w.wave_count,
        next_phase_remaining_seconds: Some(remaining),
        rupture_remaining_seconds: until,
        stage_verification: Verification::ProgressEstimate,
        rupture_verification: if stage == NativeStage::PreWave {
            Verification::ProgressEstimate
        } else if interval.is_none() {
            Verification::Unknown
        } else if calibrated {
            Verification::ProgressEstimate
        } else {
            Verification::UnverifiedConfiguration
        },
        current_rupture_in_progress: matches!(stage, NativeStage::Moving | NativeStage::Fadeout),
        waiting_interval_seconds: interval,
        warning_duration_seconds: warning,
        fadeout_duration_seconds: Some(80.0),
        actor_progress_drift_seconds: drift,
        actor_start_residual_seconds: start_residual,
        actor_next_phase_matches: w.actor_next_phase == stage.actor_next_phase(),
        timing_reason: "native_progress_and_live_inverse_duration",
    }
}
