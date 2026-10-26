//! Guarded native phase timing from observed progress and configuration.
use starrupture_observation_model::observation::{
    Phase, PhaseEvidence, Verification, WaveObservation, WaveTiming,
};
pub const HEAT_SETTINGS: [f64; 11] = [
    160000.0,          // waveSpeed
    -1200000.0,        // waveStartPosition
    400000.0,          // waveEndPosition
    120.0,             // preWaveDuration
    20.0,              // waveFadeoutFireWaveDuration
    30.0,              // waveFadeoutBurningDuration
    30.0,              // waveFadeoutFadingDuration
    600.0,             // waveGrowbackMoonPhaseDuration
    15.0,              // waveGrowbackRegrowthStartDuration
    120.0,             // waveGrowbackRegrowthDuration
    118.0999984741211, // wavePreWaveExplosionDuration
];
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
    if !(1..=4).contains(&p.stage) || p.stage != w.previous_stage {
        return unknown(w, "phase_transition_or_enum_conflict");
    }
    // Native waveType 1 identifies Heat.
    if p.wave_type != 1 {
        return unknown(w, "unsupported_wave_type");
    }
    if !(0.0..=1.0).contains(&p.progress) {
        return unknown(w, "invalid_phase_progress");
    }
    if p.stage != 1
        && (!p.wave_start_seconds.is_finite()
            || p.wave_start_seconds < 0.0
            || p.wave_start_seconds > clock)
    {
        return unknown(w, "invalid_wave_start");
    }
    if !p.settings.iter().zip(HEAT_SETTINGS).all(|(a, b)| a.is_finite() && (a - b).abs() < 0.001) {
        return unknown(w, "uncalibrated_wave_settings");
    }
    // Slot 3 is preWaveDuration in the native settings layout.
    if warning.is_some_and(|v| (v - p.settings[3]).abs() > 0.001) {
        return unknown(w, "warning_duration_conflicts_with_active_heat_settings");
    }
    let stage = p.stage as usize;
    // Native stage indices: unused, PreWave, Moving, Fadeout, Growback; seconds.
    let duration = [0.0, 120.0, 10.0, 80.0, 735.0][stage];
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
    let start_residual = if stage != 1 {
        actor.map(|a| (a - (p.wave_start_seconds + [0.0, 0.0, 10.0, 90.0, 825.0][stage])).abs())
    } else {
        None
    };
    let until = if stage == 1 {
        Some(remaining)
    } else {
        interval.map(|v| {
            remaining
                + if stage == 2 {
                    815.0
                } else if stage == 3 {
                    735.0
                } else {
                    0.0
                }
                + v
        })
    };
    WaveTiming {
        phase: [Phase::Unknown, Phase::PreWave, Phase::Moving, Phase::Fadeout, Phase::Growback]
            [stage],
        wave_count: w.wave_count,
        next_phase_remaining_seconds: Some(remaining),
        rupture_remaining_seconds: until,
        stage_verification: Verification::ProgressEstimate,
        rupture_verification: if stage == 1 {
            Verification::ProgressEstimate
        } else if interval.is_none() {
            Verification::Unknown
        } else if calibrated {
            Verification::ProgressEstimate
        } else {
            Verification::UnverifiedConfiguration
        },
        current_rupture_in_progress: stage == 2 || stage == 3,
        waiting_interval_seconds: interval,
        warning_duration_seconds: warning,
        fadeout_duration_seconds: Some(80.0),
        actor_progress_drift_seconds: drift,
        actor_start_residual_seconds: start_residual,
        actor_next_phase_matches: w.actor_next_phase == [0, 0, 1, 2, 3][stage],
        timing_reason: "native_progress_and_live_inverse_duration",
    }
}
