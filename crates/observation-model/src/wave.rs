//! Presentation of already-validated native world-tool responses, not a reader
//! or scheduler. Never advance a simulation timer using wall-clock subtraction.
use serde::Serialize;
use serde_json::Value;
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct DisplayFrame {
    pub state: String,
    pub phase: String,
    pub countdown: Option<String>,
    pub estimated: bool,
    pub paused: Option<bool>,
}
fn unavailable(state: &str) -> DisplayFrame {
    DisplayFrame {
        state: state.into(),
        phase: String::new(),
        countdown: None,
        estimated: false,
        paused: None,
    }
}

/// Agreed compact HUD presentation; native phase names remain intact elsewhere.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HudFrame {
    pub stage: &'static str,
    pub next_label: &'static str,
    pub countdown: String,
    pub colour: [u8; 3],
}
/// Display-only debounce of an isolated one-second upward rounding correction.
/// No elapsed wall time is subtracted. A second independent observation confirms
/// the correction; larger changes, phase/session/settings changes and clock
/// regressions are shown immediately. Raw evidence is never rewritten.
#[derive(Default)]
pub struct HudPresenter {
    key: String,
    observation: Option<u64>,
    simulation: Option<f64>,
    seconds: Option<u64>,
    pending: Option<u64>,
}
impl HudPresenter {
    pub fn update(&mut self, v: &Value, now_ms: u64, stale_after_ms: u64) -> HudFrame {
        let mut frame = hud(v, now_ms, stale_after_ms);
        let seconds = frame
            .countdown
            .split_once(':')
            .and_then(|(m, s)| Some(m.parse::<u64>().ok()? * 60 + s.parse::<u64>().ok()?));
        let Some(seconds) = seconds else {
            *self = Self::default();
            return frame;
        };
        let wave = &v["data"]["wave"];
        let key = serde_json::json!([
            v["observation"]["sessionId"],
            v["observation"]["overlaySession"],
            v["data"]["world"],
            wave["waveCount"],
            wave["phase"],
            wave["waitingIntervalSeconds"],
            wave["warningDurationSeconds"]
        ])
        .to_string();
        let observation = v["observation"]["window"]["completedAtUnixMilliseconds"].as_u64();
        let simulation = v["data"]["clocks"]["simulationSeconds"].as_f64();
        let reset = key != self.key || simulation.zip(self.simulation).is_some_and(|(a, b)| a < b);
        if reset {
            *self = Self::default();
            self.key = key;
        }
        if self.observation != observation || self.seconds.is_none() {
            let previous = self.seconds;
            if previous.is_some_and(|p| seconds == p + 1) && self.pending != Some(seconds) {
                self.pending = Some(seconds);
            } else {
                self.seconds = Some(seconds);
                self.pending = None;
            }
            self.observation = observation;
            self.simulation = simulation;
        }
        if let Some(seconds) = self.seconds {
            frame.countdown = format!("{:02}:{:02}", seconds / 60, seconds % 60);
        }
        frame
    }
}
pub fn style(phase: &str) -> (&'static str, [u8; 3]) {
    match phase {
        "waiting" => ("WAITING", [0x79, 0xdd, 0xe5]),
        "PreWave" => ("PRE-WAVE", [0xff, 0xc4, 0x5c]),
        "Moving" => ("INCINERATING", [0xff, 0x71, 0x69]),
        "Fadeout" => ("FADEOUT", [0xff, 0xaa, 0x66]),
        "Growback" => ("GROWBACK", [0x8d, 0xdb, 0x9a]),
        _ => ("UNAVAILABLE", [0xa6, 0xaf, 0xb5]),
    }
}
pub fn hud(v: &Value, now_ms: u64, stale_after_ms: u64) -> HudFrame {
    let validated = display(v, now_ms, stale_after_ms);
    if validated.state != "current" {
        let (stage, colour) = style("");
        return HudFrame {
            stage,
            next_label: "Next stage in",
            countdown: "--:--".into(),
            colour,
        };
    }
    let phase = &validated.phase;
    let (stage, colour) = style(phase);
    let w = &v["data"]["wave"];
    let (time, verification) = if phase == "waiting" {
        (&w["ruptureRemainingSeconds"], &w["ruptureTimingVerification"])
    } else {
        (&w["nextPhaseRemainingSeconds"], &w["stageTimingVerification"])
    };
    let supported = ["verified", "corroborated", "progress_based_estimate", "conditional_estimate"]
        .iter()
        .any(|s| verification == *s);
    let countdown = time
        .as_f64()
        .filter(|n| n.is_finite() && *n >= 0. && *n <= 604800. && supported)
        .map(|n| {
            // Match the whole-seconds portion of the game's fractional timer.
            // Rounding up adds as much as one second to read/presentation delay.
            let seconds = n.floor() as u64;
            format!("{:02}:{:02}", seconds / 60, seconds % 60)
        })
        .unwrap_or_else(|| "--:--".into());
    HudFrame {
        stage,
        colour,
        next_label: if phase == "waiting" || phase == "PreWave" {
            "Rupture in"
        } else {
            "Next stage in"
        },
        countdown,
    }
}
pub fn display(v: &Value, now_ms: u64, stale_after_ms: u64) -> DisplayFrame {
    if v["status"] != "ok"
        || v["observation"]["source"] != "live_process_memory"
        || v["observation"]["readerIsElevated"] != false
    {
        return unavailable("unavailable");
    }
    let Some(end) = v["observation"]["window"]["completedAtUnixMilliseconds"].as_u64() else {
        return unavailable("unavailable");
    };
    if stale_after_ms == 0 || end > now_ms {
        return unavailable("unavailable");
    }
    if now_ms - end > stale_after_ms {
        return unavailable("stale");
    }
    let w = &v["data"]["wave"];
    let phase = w["phase"].as_str().unwrap_or("unknown");
    if !["waiting", "PreWave", "Moving", "Fadeout", "Growback"].contains(&phase) {
        return unavailable("unavailable");
    }
    let active = w["currentRuptureInProgress"] == true;
    let (time, verification) = if active {
        (&w["nextPhaseRemainingSeconds"], &w["stageTimingVerification"])
    } else {
        (&w["ruptureRemainingSeconds"], &w["ruptureTimingVerification"])
    };
    let verified = verification == "verified";
    let estimated =
        verification == "progress_based_estimate" || verification == "conditional_estimate";
    let countdown = time
        .as_f64()
        .filter(|v| v.is_finite() && *v >= 0. && *v <= 604800. && (verified || estimated))
        .map(|seconds| {
            let s = seconds.ceil() as u64;
            format!(
                "{}{}:{:02}",
                if estimated {
                    "≈"
                } else {
                    ""
                },
                s / 60,
                s % 60
            )
        });
    let has_countdown = countdown.is_some();
    DisplayFrame {
        state: "current".into(),
        phase: phase.into(),
        countdown,
        estimated: estimated && has_countdown,
        paused: v["data"]["pause"]["paused"].as_bool(),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn source() -> Value {
        json!({"status":"ok","observation":{"source":"live_process_memory","readerIsElevated":false,"window":{"completedAtUnixMilliseconds":1000}},"data":{"pause":{"paused":true},"wave":{"phase":"Growback","currentRuptureInProgress":false,"ruptureRemainingSeconds":2936.6337,"ruptureTimingVerification":"progress_based_estimate"}}})
    }
    #[test]
    fn agreed_hud_alias_colour_and_stage_countdown() {
        let mut v = source();
        v["data"]["wave"]["nextPhaseRemainingSeconds"] = json!(258.);
        v["data"]["wave"]["stageTimingVerification"] = json!("conditional_estimate");
        let frame = hud(&v, 1000, 5000);
        assert_eq!(frame.countdown, "04:18");
        assert_eq!(frame.colour, [0x8d, 0xdb, 0x9a]);
        v["data"]["wave"]["phase"] = json!("Moving");
        assert_eq!(hud(&v, 1000, 5000).stage, "INCINERATING");
        assert_eq!(hud(&v, 1000, 5000), hud(&v, 4000, 5000));
        assert_eq!(hud(&v, 7000, 5000).countdown, "--:--");
        assert_eq!(style("unknown").0, "UNAVAILABLE");
        assert_ne!(style("PreWave").1, style("Fadeout").1);
    }
    #[test]
    fn hud_debounce_requires_new_observations_and_preserves_real_corrections() {
        let mut p = HudPresenter::default();
        let mut v = source();
        v["data"]["wave"]["phase"] = json!("waiting");
        v["data"]["wave"]["ruptureTimingVerification"] = json!("verified");
        v["data"]["wave"]["ruptureRemainingSeconds"] = json!(60.);
        assert_eq!(p.update(&v, 1000, 5000).countdown, "01:00");
        v["data"]["wave"]["ruptureRemainingSeconds"] = json!(61.1);
        v["observation"]["window"]["completedAtUnixMilliseconds"] = json!(1100);
        assert_eq!(p.update(&v, 1100, 5000).countdown, "01:00");
        assert_eq!(p.update(&v, 1200, 5000).countdown, "01:00");
        v["observation"]["window"]["completedAtUnixMilliseconds"] = json!(1300);
        assert_eq!(p.update(&v, 1300, 5000).countdown, "01:01");
        v["data"]["wave"]["ruptureRemainingSeconds"] = json!(100.);
        v["observation"]["window"]["completedAtUnixMilliseconds"] = json!(1400);
        assert_eq!(p.update(&v, 1400, 5000).countdown, "01:40");
        v["observation"]["sessionId"] = json!("new");
        v["data"]["wave"]["ruptureRemainingSeconds"] = json!(101.);
        assert_eq!(p.update(&v, 1400, 5000).countdown, "01:41");
        assert_eq!(p.update(&v, 7000, 5000).countdown, "--:--");
    }
    #[test]
    fn pause_never_decrements_from_wall_time() {
        let v = source();
        let a = display(&v, 1000, 5000);
        let b = display(&v, 3000, 5000);
        assert_eq!(a, b);
        assert_eq!(a.countdown.as_deref(), Some("≈48:57"));
        assert_eq!(a.paused, Some(true));
    }
    #[test]
    fn hud_matches_fractional_timer_whole_seconds_without_wall_time_projection() {
        let mut v = source();
        v["data"]["wave"]["phase"] = json!("PreWave");
        v["data"]["wave"]["nextPhaseRemainingSeconds"] = json!(10.82);
        v["data"]["wave"]["stageTimingVerification"] = json!("progress_based_estimate");
        assert_eq!(hud(&v, 1000, 5000).countdown, "00:10");
        assert_eq!(hud(&v, 4000, 5000).countdown, "00:10");
        v["data"]["wave"]["nextPhaseRemainingSeconds"] = json!(0.99);
        assert_eq!(hud(&v, 1000, 5000).countdown, "00:00");
        v["data"]["wave"]["phase"] = json!("waiting");
        v["data"]["wave"]["ruptureRemainingSeconds"] = json!(60.99);
        assert_eq!(hud(&v, 1000, 5000).countdown, "01:00");
    }
    #[test]
    fn stale_and_errors_hide_old_countdown() {
        let mut v = source();
        assert_eq!(display(&v, 7000, 5000), unavailable("stale"));
        v["status"] = json!("unavailable");
        assert_eq!(display(&v, 1000, 5000), unavailable("unavailable"));
    }
    #[test]
    fn unsupported_timing_stays_unknown() {
        let mut v = source();
        v["data"]["wave"]["ruptureTimingVerification"] = json!("unverified_runtime_configuration");
        assert_eq!(display(&v, 1000, 5000).countdown, None);
    }
    #[test]
    fn active_wave_uses_stage_time() {
        let mut v = source();
        v["data"]["wave"]["currentRuptureInProgress"] = json!(true);
        v["data"]["wave"]["phase"] = json!("Fadeout");
        v["data"]["wave"]["nextPhaseRemainingSeconds"] = json!(12.5);
        v["data"]["wave"]["stageTimingVerification"] = json!("progress_based_estimate");
        assert_eq!(display(&v, 1000, 5000).countdown.as_deref(), Some("≈0:13"));
    }
    #[test]
    fn null_negative_and_future_data_never_become_zero() {
        let mut v = source();
        for t in [Value::Null, json!(-1.)] {
            v["data"]["wave"]["ruptureRemainingSeconds"] = t;
            assert_eq!(display(&v, 1000, 5000).countdown, None);
        }
        assert_eq!(display(&v, 999, 5000), unavailable("unavailable"));
    }
}
