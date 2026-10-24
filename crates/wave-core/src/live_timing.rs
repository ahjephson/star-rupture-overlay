//! Native 0.8 live progress/rate timing, independent of saved-world calibrations.
use crate::facts::{merge, number};
use serde_json::{Value, json};
pub const HEAT_SETTINGS: [(&str, f64); 11] = [
    ("waveSpeed", 160000.0),
    ("waveStartPosition", -1200000.0),
    ("waveEndPosition", 400000.0),
    ("preWaveDuration", 120.0),
    ("waveFadeoutFireWaveDuration", 20.0),
    ("waveFadeoutBurningDuration", 30.0),
    ("waveFadeoutFadingDuration", 30.0),
    ("waveGrowbackMoonPhaseDuration", 600.0),
    ("waveGrowbackRegrowthStartDuration", 15.0),
    ("waveGrowbackRegrowthDuration", 120.0),
    ("wavePreWaveExplosionDuration", 118.0999984741211),
];
fn finite(v: &Value) -> bool {
    v.as_f64().is_some_and(f64::is_finite)
}
fn truthy(v: &Value) -> bool {
    !v.is_null() && v != false && v != "" && v.as_f64() != Some(0.0)
}
fn unknown(w: &Value, c: &Value, reason: &str) -> Value {
    merge(
        merge(w.clone(), c.clone()),
        json!({"phase":if truthy(&w["wavesDisabled"]){"disabled"}else{"unknown"},"ruptureRemainingSeconds":null,"nextPhaseRemainingSeconds":null,"stageTimingVerification":"unknown","ruptureTimingVerification":"unknown","timingReason":reason,"currentRuptureInProgress":null,"waitingIntervalAssumptionSeconds":null}),
    )
}
pub fn live_wave_timing(w: &Value, clock: f64) -> Value {
    let mut configuration = json!({"waitingIntervalSeconds":null,"warningDurationSeconds":null,"waitingIntervalVerification":"unknown","runtimeConfigurationCalibration":"unknown"});
    if truthy(&w["wavesDisabled"]) {
        return unknown(w, &configuration, "waves_disabled");
    }
    if !clock.is_finite() || clock < 0.0 {
        return unknown(w, &configuration, "invalid_simulation_clock");
    }
    if w["specificWavePauseFlag"] != false || w["actor"]["specificPauseFlag"] != false {
        return unknown(w, &configuration, "wave_specific_pause_or_unknown");
    }
    let p = &w["phaseEvidence"];
    let c = &w["timingConfiguration"];
    let valid = c["status"] == "ok"
        && c["nativeControlFlowVerified"] == true
        && c["intervalIncludesWarning"] == true
        && finite(&c["intervalSeconds"])
        && number(&c["intervalSeconds"]) > 0.0
        && number(&c["intervalSeconds"]) <= 86400.0
        && finite(&c["warningDurationSeconds"])
        && number(&c["warningDurationSeconds"]) > 0.0
        && number(&c["warningDurationSeconds"]) <= number(&c["intervalSeconds"]);
    let interval = if valid {
        c["intervalSeconds"].as_f64()
    } else {
        None
    };
    let warning = if valid {
        c["warningDurationSeconds"].as_f64()
    } else {
        None
    };
    let calibrated = interval == Some(2400.0) && warning == Some(120.0);
    configuration = json!({"waitingIntervalSeconds":interval,"warningDurationSeconds":warning,"waitingIntervalAssumptionSeconds":null,"waitingIntervalVerification":if valid{"live_fields_with_native_control_flow"}else{"unknown"},"runtimeConfigurationCalibration":if valid{if calibrated{"observed_normal_heat"}else{"unverified_runtime_configuration"}}else{"unknown"}});
    let substage = json!({"remainingSeconds":null,"verification":"unknown","reason":"Identifiers observed; exact substage deadlines are not calibrated."});
    let basis = "simulation observation; pause consumes no time; no wall-clock subtraction";
    if w["waitingForNextWave"] == true {
        if p["status"] != "inactive" {
            return unknown(w, &configuration, "waiting_and_phase_evidence_conflict");
        }
        if !finite(&w["ruptureRemainingSeconds"])
            || number(&w["ruptureRemainingSeconds"]) < 0.0
            || number(&w["ruptureRemainingSeconds"]) > 86400.0
            || w["countdownBasis"] != "native_waiting_countdown"
        {
            return unknown(w, &configuration, "invalid_waiting_countdown");
        }
        let remaining = warning.map(|v| (number(&w["ruptureRemainingSeconds"]) - v).max(0.0));
        let verification = if warning.is_none() {
            "unknown"
        } else if calibrated {
            "corroborated"
        } else {
            "unverified_runtime_configuration"
        };
        return merge(
            merge(w.clone(), configuration),
            json!({"phase":"waiting","nextPhaseRemainingSeconds":remaining,"stageTimingVerification":verification,"ruptureTimingVerification":"verified","timingReason":"native_waiting_countdown_and_live_warning_threshold","currentRuptureInProgress":false,"phaseDurations":null,"waitingWarningEstimate":{"remainingSeconds":remaining,"verification":verification,"warningDurationSeconds":warning,"warningDurationAssumptionSeconds":null},"substageTiming":substage,"timeBasis":basis}),
        );
    }
    if w["waitingForNextWave"] != false || p["status"] != "ok" || p["getterLayoutsVerified"] != true
    {
        return unknown(w, &configuration, "active_phase_reader_unavailable");
    }
    let stage = p["stage"]
        .as_f64()
        .filter(|n| n.fract() == 0.0 && (1.0..=4.0).contains(n))
        .map(|n| n as usize);
    if stage.is_none() || p["stage"].as_f64() != w["previousStageRaw"].as_f64() {
        return unknown(w, &configuration, "phase_transition_or_enum_conflict");
    }
    let stage = stage.unwrap();
    if p["waveType"].as_f64() != Some(1.0) {
        return unknown(w, &configuration, "unsupported_wave_type");
    }
    if !p["progress"]
        .as_f64()
        .is_some_and(|n| (0.0..=1.0).contains(&n))
    {
        return unknown(w, &configuration, "invalid_phase_progress");
    }
    if stage != 1
        && (!finite(&p["waveStartTimeSeconds"])
            || number(&p["waveStartTimeSeconds"]) < 0.0
            || number(&p["waveStartTimeSeconds"]) > clock)
    {
        return unknown(w, &configuration, "invalid_wave_start");
    }
    if p["settings"].is_null()
        || !HEAT_SETTINGS.iter().all(|(k, n)| {
            finite(&p["settings"][k]) && (number(&p["settings"][k]) - n).abs() < 0.001
        })
    {
        return unknown(w, &configuration, "uncalibrated_wave_settings");
    }
    if warning.is_some_and(|v| (v - number(&p["settings"]["preWaveDuration"])).abs() > 0.001) {
        return unknown(
            w,
            &configuration,
            "warning_duration_conflicts_with_active_heat_settings",
        );
    }
    let duration = [0.0, 120.0, 10.0, 80.0, 735.0][stage];
    let phase = &p["phaseClockEvidence"];
    if !p["inverseDuration"]
        .as_f64()
        .is_some_and(|v| v > 0.0 && (v * duration - 1.0).abs() <= 0.00001)
        || phase["status"] != "ok"
        || phase["nativeUpdateBindingVerified"] != true
        || !phase["deltaCapSeconds"]
            .as_f64()
            .is_some_and(|v| (v - 0.1).abs() <= 0.000001)
    {
        return unknown(w, &configuration, "invalid_or_unverified_phase_clock");
    }
    let remaining = (1.0 - number(&p["progress"])) / number(&p["inverseDuration"]);
    let actor = w["actor"]["nextTime"].as_f64();
    let actor_remaining = actor.map(|n| n - clock);
    let drift = actor_remaining.map(|v| remaining - v);
    let start_residual = if stage != 1 {
        p["waveStartTimeSeconds"]
            .as_f64()
            .zip(actor)
            .map(|(s, a)| (a - (s + [0.0, 0.0, 10.0, 90.0, 825.0][stage])).abs())
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
    let verification = "progress_based_estimate";
    let expected = [0.0, 0.0, 1.0, 2.0, 3.0][stage];
    merge(
        merge(w.clone(), configuration),
        json!({"phase":(["","PreWave","Moving","Fadeout","Growback"][stage]),"nextPhaseRemainingSeconds":remaining,"ruptureRemainingSeconds":until,"countdownBasis":if stage==1{"captured_heat_stage_timing"}else{"native_phase_and_live_waiting_interval"},"stageTimingVerification":verification,"ruptureTimingVerification":if stage==1{verification}else if interval.is_none(){"unknown"}else if calibrated{verification}else{"unverified_runtime_configuration"},"timingReason":"native_progress_and_live_inverse_duration","currentRuptureInProgress":stage==2||stage==3,"phaseDurations":{"preWave":120,"moving":10,"fadeout":80,"growback":735,"waitingIntervalIncludingWarning":interval},"progressRemainingSeconds":remaining,"phaseClockRemainingSeconds":remaining,"phaseClockVerification":"native_progress_rate_with_completion_anchor_corroboration","estimate":{"verification":"conditional_estimate","condition":"Future phase updates are not clipped by the live delta cap; float32 accumulation and transition ticks can add drift. Menu pause consumes no phase time.","deltaCapSeconds":phase["deltaCapSeconds"]},"actorDeadlineDiagnostic":{"nominalRemainingSeconds":actor_remaining,"progressMinusDeadlineSeconds":drift,"expectedNextPhase":expected,"nextPhaseMatches":w["actor"]["nextPhase"].as_f64()==Some(expected),"usedForTiming":false,"interpretation":"Positive drift means phase progress projects completion later than the nominal actor deadline. Actor may retain a duration or stale phase target."},"actorProgressResidualSeconds":drift.map(f64::abs),"actorStartResidualSeconds":start_residual,"substageTiming":substage,"preWaveExplosionCueRemainingSeconds":if stage==1{Some((number(&p["settings"]["wavePreWaveExplosionDuration"])-duration*number(&p["progress"])).max(0.0))}else{None},"preWaveExplosionCueVerification":if stage==1{Some("unverified_estimate")}else{None},"warningIncludedInWaitingInterval":true,"timeBasis":basis,"coverage":"Phase progress controls completion. Elapsed gameplay projections are conditional estimates; actor deadlines are diagnostic only. Interval/warning read live. Changed configuration, exact substages and local arrival remain limited."}),
    )
}
