//! HUD presentation of validated world observations using simulation-based timing.
use crate::{
    observation::{Observation, OverlaySession, Phase, Verification, WaveTiming, WorldState},
    overlay_stream::GameIdentity,
};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CountdownFormat {
    #[default]
    Hours,
    Minutes,
}
fn countdown(seconds: u64, format: CountdownFormat) -> String {
    if format == CountdownFormat::Hours && seconds >= 3600 {
        format!("{}:{:02}:{:02}", seconds / 3600, seconds / 60 % 60, seconds % 60)
    } else {
        format!("{:02}:{:02}", seconds / 60, seconds % 60)
    }
}
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct DisplayFrame {
    pub state: &'static str,
    pub phase: &'static str,
    pub countdown: Option<String>,
    pub estimated: bool,
    pub paused: Option<bool>,
}
fn unavailable(state: &'static str) -> DisplayFrame {
    DisplayFrame {
        state,
        phase: "",
        countdown: None,
        estimated: false,
        paused: None,
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HudFrame {
    pub stage: &'static str,
    pub next_label: &'static str,
    pub countdown: String,
    pub colour: [u8; 3],
}
struct Presentation {
    stage: &'static str,
    next_label: &'static str,
    colour: [u8; 3],
    seconds: Option<u64>,
}
impl Presentation {
    fn frame(self, seconds: Option<u64>, format: CountdownFormat) -> HudFrame {
        HudFrame {
            stage: self.stage,
            next_label: self.next_label,
            colour: self.colour,
            countdown: seconds.map(|s| countdown(s, format)).unwrap_or_else(|| "--:--".into()),
        }
    }
}
struct Key {
    process: GameIdentity,
    session: OverlaySession,
    world: WorldState,
    wave_count: i64,
    phase: Phase,
    stage: &'static str,
    interval: Option<f64>,
    warning: Option<f64>,
}
impl Key {
    fn matches(&self, v: &Observation, stage: &str) -> bool {
        self.process == v.process
            && self.session == v.overlay_session
            && self.world == v.world
            && self.wave_count == v.wave.wave_count
            && self.phase == v.wave.phase
            && self.stage == stage
            && self.interval == v.wave.waiting_interval_seconds
            && self.warning == v.wave.warning_duration_seconds
    }
    fn new(v: &Observation, stage: &'static str) -> Self {
        Self {
            process: v.process.clone(),
            session: v.overlay_session,
            world: v.world.clone(),
            wave_count: v.wave.wave_count,
            phase: v.wave.phase,
            stage,
            interval: v.wave.waiting_interval_seconds,
            warning: v.wave.warning_duration_seconds,
        }
    }
}
/// Suppresses an isolated one-second upward correction until a new observation confirms it.
#[derive(Default)]
pub struct HudPresenter {
    key: Option<Key>,
    observation: Option<u64>,
    simulation: Option<f64>,
    seconds: Option<u64>,
    pending: Option<u64>,
}
impl HudPresenter {
    pub fn update(
        &mut self,
        value: Option<&Observation>,
        now_ms: u64,
        stale_after_ms: u64,
    ) -> HudFrame {
        self.update_with_format(value, now_ms, stale_after_ms, CountdownFormat::default())
    }
    pub fn update_with_format(
        &mut self,
        value: Option<&Observation>,
        now_ms: u64,
        stale_after_ms: u64,
        format: CountdownFormat,
    ) -> HudFrame {
        let frame = presentation(value, now_ms, stale_after_ms);
        let Some((v, seconds)) = value.zip(frame.seconds) else {
            *self = Self::default();
            return frame.frame(None, format);
        };
        let reset = !self.key.as_ref().is_some_and(|k| k.matches(v, frame.stage))
            || self.simulation.is_some_and(|s| v.clocks.simulation_seconds < s);
        if reset {
            *self = Self::default();
            self.key = Some(Key::new(v, frame.stage));
        }
        let observation = Some(v.window.completed_ms);
        if self.observation != observation || self.seconds.is_none() {
            if self.seconds.is_some_and(|p| seconds == p + 1) && self.pending != Some(seconds) {
                self.pending = Some(seconds);
            } else {
                self.seconds = Some(seconds);
                self.pending = None;
            }
            self.observation = observation;
            self.simulation = Some(v.clocks.simulation_seconds);
        }
        frame.frame(self.seconds, format)
    }
}
pub fn style(phase: &str) -> (&'static str, [u8; 3]) {
    match phase {
        "waiting" => ("STABLE", [0x79, 0xdd, 0xe5]),
        "PreWave" => ("PRE-WAVE", [0xff, 0xc4, 0x5c]),
        "Moving" => ("APPROACHING", [0xff, 0x71, 0x69]),
        "Fadeout" => ("FADEOUT", [0xff, 0xaa, 0x66]),
        "Growback" => ("GROWBACK", [0x8d, 0xdb, 0x9a]),
        _ => ("UNAVAILABLE", [0xa6, 0xaf, 0xb5]),
    }
}
fn state(v: &Observation, now_ms: u64, stale_after_ms: u64) -> &'static str {
    if v.reader_is_elevated || stale_after_ms == 0 || v.window.completed_ms > now_ms {
        "unavailable"
    } else if now_ms - v.window.completed_ms > stale_after_ms {
        "stale"
    } else if matches!(
        v.wave.phase,
        Phase::Waiting | Phase::PreWave | Phase::Moving | Phase::Fadeout | Phase::Growback
    ) {
        "current"
    } else {
        "unavailable"
    }
}
const INCINERATION_SECONDS: f64 = 7.0;
fn fadeout_presentation(w: &WaveTiming) -> Option<(&'static str, [u8; 3], f64)> {
    let duration = w.fadeout_duration_seconds?;
    let remaining = w.next_phase_remaining_seconds?;
    if !duration.is_finite()
        || !(INCINERATION_SECONDS..=604800.0).contains(&duration)
        || !remaining.is_finite()
        || !(0.0..=duration).contains(&remaining)
    {
        return None;
    }
    Some(if duration - remaining < INCINERATION_SECONDS {
        ("INCINERATING", [0xff, 0x71, 0x69], remaining - (duration - INCINERATION_SECONDS))
    } else {
        ("COOLING", [0xff, 0xaa, 0x66], remaining)
    })
}
/// Hide only when a current validated observation confirms waves are stopped.
pub fn waves_stopped(value: Option<&Observation>, now_ms: u64, stale_after_ms: u64) -> bool {
    value.is_some_and(|v| {
        !v.reader_is_elevated
            && stale_after_ms > 0
            && v.window.completed_ms <= now_ms
            && now_ms - v.window.completed_ms <= stale_after_ms
            && v.wave.phase == Phase::Disabled
    })
}
fn presentation(value: Option<&Observation>, now_ms: u64, stale_after_ms: u64) -> Presentation {
    let Some(v) = value.filter(|v| state(v, now_ms, stale_after_ms) == "current") else {
        let (stage, colour) = style("");
        return Presentation {
            stage,
            colour,
            next_label: "Next stage in",
            seconds: None,
        };
    };
    let w = &v.wave;
    let (mut stage, mut colour) = style(w.phase.as_str());
    let (mut time, verification) = if w.phase == Phase::Waiting {
        (w.rupture_remaining_seconds, w.rupture_verification)
    } else {
        (w.next_phase_remaining_seconds, w.stage_verification)
    };
    if w.phase == Phase::Fadeout && verification.supported() {
        if let Some((label, shade, remaining)) = fadeout_presentation(w) {
            stage = label;
            colour = shade;
            time = Some(remaining);
        } else {
            time = None;
        }
    }
    Presentation {
        stage,
        colour,
        next_label: if matches!(w.phase, Phase::Waiting | Phase::PreWave) {
            "Rupture in"
        } else {
            "Next stage in"
        },
        seconds: time
            .filter(|n| n.is_finite() && (0.0..=604800.0).contains(n) && verification.supported())
            .map(|n| n.floor() as u64),
    }
}
pub fn hud(v: &Observation, now_ms: u64, stale_after_ms: u64) -> HudFrame {
    let frame = presentation(Some(v), now_ms, stale_after_ms);
    let seconds = frame.seconds;
    frame.frame(seconds, CountdownFormat::default())
}
pub fn display(v: &Observation, now_ms: u64, stale_after_ms: u64) -> DisplayFrame {
    let state = state(v, now_ms, stale_after_ms);
    if state != "current" {
        return unavailable(state);
    }
    let w = &v.wave;
    let (time, verification) = if w.current_rupture_in_progress {
        (w.next_phase_remaining_seconds, w.stage_verification)
    } else {
        (w.rupture_remaining_seconds, w.rupture_verification)
    };
    let estimated = verification.estimated();
    let countdown = time
        .filter(|n| {
            n.is_finite()
                && (0.0..=604800.0).contains(n)
                && (verification == Verification::Verified || estimated)
        })
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
    DisplayFrame {
        state,
        phase: w.phase.as_str(),
        estimated: estimated && countdown.is_some(),
        countdown,
        paused: v.pause.paused,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::observation::*;
    use crate::overlay_stream::GAME_HASH;
    fn source() -> Observation {
        Observation {
            process: GameIdentity {
                pid: 7,
                creation_ticks: 123,
                executable_sha256: GAME_HASH.into(),
            },
            overlay_session: OverlaySession::default(),
            observed_at_ms: 1000,
            window: ReadWindow {
                completed_ms: 1000,
                ..ReadWindow::default()
            },
            reader_is_elevated: false,
            position: Position::default(),
            map_position: None,
            world: WorldState::default(),
            clocks: Clocks::default(),
            pause: PauseState {
                paused: Some(true),
                ..PauseState::default()
            },
            wave: WaveTiming {
                phase: Phase::Growback,
                rupture_remaining_seconds: Some(2936.6337),
                rupture_verification: Verification::ProgressEstimate,
                ..WaveTiming::default()
            },
        }
    }
    #[test]
    fn countdown_format_switches_at_hour_boundary_and_can_keep_total_minutes() {
        let mut v = source();
        v.wave.phase = Phase::Waiting;
        v.wave.rupture_verification = Verification::Verified;
        let mut p = HudPresenter::default();
        for (seconds, hours, minutes) in [
            (4800, "1:20:00", "80:00"),
            (3600, "1:00:00", "60:00"),
            (3599, "59:59", "59:59"),
            (0, "00:00", "00:00"),
        ] {
            v.wave.rupture_remaining_seconds = Some(seconds as f64);
            v.window.completed_ms += 1;
            assert_eq!(
                p.update_with_format(Some(&v), 1100, 5000, CountdownFormat::Hours).countdown,
                hours
            );
            assert_eq!(
                p.update_with_format(Some(&v), 1100, 5000, CountdownFormat::Minutes).countdown,
                minutes
            );
        }
    }
    #[test]
    fn agreed_hud_alias_colour_and_stage_countdown() {
        let mut v = source();
        v.wave.next_phase_remaining_seconds = Some(258.);
        v.wave.stage_verification = Verification::ConditionalEstimate;
        let frame = hud(&v, 1000, 5000);
        assert_eq!(frame.countdown, "04:18");
        assert_eq!(frame.colour, [0x8d, 0xdb, 0x9a]);
        v.wave.phase = Phase::Moving;
        assert_eq!(hud(&v, 1000, 5000).stage, "APPROACHING");
        assert_eq!(hud(&v, 1000, 5000), hud(&v, 4000, 5000));
        assert_eq!(hud(&v, 7000, 5000).countdown, "--:--");
        assert_eq!(style("unknown").0, "UNAVAILABLE");
        assert_ne!(style("PreWave").1, style("Fadeout").1);
    }
    #[test]
    fn fadeout_labels_follow_native_simulation_boundary_and_pause() {
        let mut v = source();
        v.wave.phase = Phase::Fadeout;
        v.wave.fadeout_duration_seconds = Some(80.0);
        v.wave.stage_verification = Verification::ProgressEstimate;
        let mut presenter = HudPresenter::default();
        for (index, (remaining, stage, countdown)) in [
            (80.0, "INCINERATING", "00:07"),
            (74.0, "INCINERATING", "00:01"),
            (73.001, "INCINERATING", "00:00"),
            (73.0, "COOLING", "01:13"),
            (72.999, "COOLING", "01:12"),
            (0.0, "COOLING", "00:00"),
        ]
        .into_iter()
        .enumerate()
        {
            v.wave.next_phase_remaining_seconds = Some(remaining);
            v.window.completed_ms = 1000 + index as u64;
            let frame = presenter.update(Some(&v), 1100, 5000);
            assert_eq!((frame.stage, frame.countdown.as_str()), (stage, countdown));
            assert_eq!(frame, presenter.update(Some(&v), 4000, 5000));
            assert_eq!(display(&v, 1100, 5000).phase, "Fadeout");
        }
        assert_eq!(hud(&v, 7000, 5000).stage, "UNAVAILABLE");
    }
    #[test]
    fn fadeout_split_rejects_unknown_or_invalid_timing() {
        let mut v = source();
        v.wave.phase = Phase::Fadeout;
        v.wave.stage_verification = Verification::ProgressEstimate;
        for (duration, remaining) in [
            (None, Some(75.0)),
            (Some(80.0), None),
            (Some(80.0), Some(-1.0)),
            (Some(80.0), Some(81.0)),
            (Some(6.0), Some(5.0)),
        ] {
            v.wave.fadeout_duration_seconds = duration;
            v.wave.next_phase_remaining_seconds = remaining;
            let frame = hud(&v, 1000, 5000);
            assert_eq!((frame.stage, frame.countdown.as_str()), ("FADEOUT", "--:--"));
        }
        v.wave.fadeout_duration_seconds = Some(80.0);
        v.wave.next_phase_remaining_seconds = Some(75.0);
        v.wave.stage_verification = Verification::Unknown;
        assert_eq!(hud(&v, 1000, 5000).stage, "FADEOUT");
    }
    #[test]
    fn hud_debounce_requires_new_observations_and_preserves_real_corrections() {
        let mut p = HudPresenter::default();
        let mut v = source();
        v.wave.phase = Phase::Waiting;
        v.wave.rupture_verification = Verification::Verified;
        v.wave.rupture_remaining_seconds = Some(60.);
        assert_eq!(p.update(Some(&v), 1000, 5000).countdown, "01:00");
        v.wave.rupture_remaining_seconds = Some(61.1);
        v.window.completed_ms = 1100;
        assert_eq!(p.update(Some(&v), 1100, 5000).countdown, "01:00");
        assert_eq!(p.update(Some(&v), 1200, 5000).countdown, "01:00");
        v.window.completed_ms = 1300;
        assert_eq!(p.update(Some(&v), 1300, 5000).countdown, "01:01");
        v.wave.rupture_remaining_seconds = Some(100.);
        v.window.completed_ms = 1400;
        assert_eq!(p.update(Some(&v), 1400, 5000).countdown, "01:40");
        v.overlay_session.generation = 1;
        v.wave.rupture_remaining_seconds = Some(101.);
        assert_eq!(p.update(Some(&v), 1400, 5000).countdown, "01:41");
        assert_eq!(p.update(Some(&v), 7000, 5000).countdown, "--:--");
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
        v.wave.phase = Phase::PreWave;
        v.wave.next_phase_remaining_seconds = Some(10.82);
        v.wave.stage_verification = Verification::ProgressEstimate;
        assert_eq!(hud(&v, 1000, 5000).countdown, "00:10");
        assert_eq!(hud(&v, 4000, 5000).countdown, "00:10");
        v.wave.next_phase_remaining_seconds = Some(0.99);
        assert_eq!(hud(&v, 1000, 5000).countdown, "00:00");
        v.wave.phase = Phase::Waiting;
        v.wave.rupture_remaining_seconds = Some(60.99);
        assert_eq!(hud(&v, 1000, 5000).countdown, "01:00");
    }
    #[test]
    fn stopped_waves_hide_only_with_current_evidence() {
        let mut v = source();
        assert!(!waves_stopped(Some(&v), 1000, 5000));
        v.wave.phase = Phase::Disabled;
        assert!(waves_stopped(Some(&v), 1000, 5000));
        assert!(!waves_stopped(Some(&v), 7000, 5000));
        assert!(!waves_stopped(Some(&v), 999, 5000));
        assert!(!waves_stopped(None, 1000, 5000));
        v.reader_is_elevated = true;
        assert!(!waves_stopped(Some(&v), 1000, 5000));
    }
    #[test]
    fn stale_and_errors_hide_old_countdown() {
        let mut v = source();
        assert_eq!(display(&v, 7000, 5000), unavailable("stale"));
        v.reader_is_elevated = true;
        assert_eq!(display(&v, 1000, 5000), unavailable("unavailable"));
    }
    #[test]
    fn unsupported_timing_stays_unknown() {
        let mut v = source();
        v.wave.rupture_verification = Verification::UnverifiedConfiguration;
        assert_eq!(display(&v, 1000, 5000).countdown, None);
    }
    #[test]
    fn active_wave_uses_stage_time() {
        let mut v = source();
        v.wave.current_rupture_in_progress = true;
        v.wave.phase = Phase::Fadeout;
        v.wave.next_phase_remaining_seconds = Some(12.5);
        v.wave.stage_verification = Verification::ProgressEstimate;
        assert_eq!(display(&v, 1000, 5000).countdown.as_deref(), Some("≈0:13"));
    }
    #[test]
    fn null_negative_and_future_data_never_become_zero() {
        let mut v = source();
        for t in [None, Some(-1.)] {
            v.wave.rupture_remaining_seconds = t;
            assert_eq!(display(&v, 1000, 5000).countdown, None);
        }
        assert_eq!(display(&v, 999, 5000), unavailable("unavailable"));
    }
    #[test]
    fn debounce_resets_for_identity_world_settings_clock_and_missing_observations() {
        for case in 0..10 {
            let mut v = source();
            v.wave.phase = Phase::Waiting;
            v.wave.rupture_verification = Verification::Verified;
            v.wave.rupture_remaining_seconds = Some(60.0);
            v.clocks.simulation_seconds = 10.0;
            let mut presenter = HudPresenter::default();
            assert_eq!(presenter.update(Some(&v), 1000, 5000).countdown, "01:00");
            v.wave.rupture_remaining_seconds = Some(61.0);
            v.window.completed_ms = 1100;
            match case {
                0 => v.process.creation_ticks += 1,
                1 => v.overlay_session.generation += 1,
                2 => v.world.origin[0] = 1,
                3 => v.world.world_path = "Other world".into(),
                4 => v.wave.wave_count += 1,
                5 => v.wave.waiting_interval_seconds = Some(2400.0),
                6 => v.wave.warning_duration_seconds = Some(120.0),
                7 => v.clocks.simulation_seconds = 1.0,
                8 => {
                    v.wave.phase = Phase::PreWave;
                    v.wave.next_phase_remaining_seconds = Some(61.0);
                    v.wave.stage_verification = Verification::ProgressEstimate;
                }
                9 => {
                    assert_eq!(presenter.update(None, 1100, 5000).countdown, "--:--");
                }
                _ => unreachable!(),
            }
            assert_eq!(presenter.update(Some(&v), 1100, 5000).countdown, "01:01", "case {case}");
        }
    }
}
