//! Independent navigation evidence and horizontal compass geometry.
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
#[derive(Default)]
pub struct Control(AtomicBool);
impl Control {
    pub fn set(&self, enabled: bool) {
        self.0.store(enabled, Ordering::Relaxed);
    }
    pub fn enabled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Sample {
    pub started_ms: u64,
    pub finished_ms: u64,
    pub heading: f64,
    pub pin: Option<Pin>,
    pub pin_known: bool,
    pub menu_open: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Pin {
    pub position: [f64; 2],
    pub bearing: f64,
    pub metres: f64,
}
impl Sample {
    pub fn valid(&self, now: u64) -> bool {
        self.started_ms <= self.finished_ms
            && self.finished_ms <= now
            && self.finished_ms - self.started_ms <= 100
            && now - self.finished_ms <= 250
            && self.heading.is_finite()
            && (0.0..360.0).contains(&self.heading)
            && (self.pin_known || self.pin.is_none())
            && self.pin.is_none_or(|p| {
                p.position.iter().all(|v| v.is_finite() && v.abs() < 1e7)
                    && p.bearing.is_finite()
                    && (0.0..360.0).contains(&p.bearing)
                    && p.metres.is_finite()
                    && (0.0..=300000.0).contains(&p.metres)
            })
    }
}
pub fn relative(bearing: f64, heading: f64) -> f64 {
    (bearing - heading + 180.0).rem_euclid(360.0) - 180.0
}
pub fn bearing(dx: f64, dy: f64) -> f64 {
    (dy.atan2(dx).to_degrees() + 90.0).rem_euclid(360.0)
}
/// Proximity latch: arrive at 10 m, leave beyond 15 m. A changed target resets it.
#[derive(Clone, Copy, Default)]
pub struct Arrival {
    target: Option<[u64; 2]>,
    arrived: bool,
}
impl Arrival {
    pub fn update(&mut self, pin: Option<Pin>) -> bool {
        let Some(pin) = pin.filter(|p| p.metres.is_finite() && p.metres >= 0.) else {
            *self = Self::default();
            return false;
        };
        let key = pin.position.map(f64::to_bits);
        if self.target != Some(key) {
            self.arrived = false;
            self.target = Some(key);
        }
        self.arrived = if self.arrived {
            pin.metres <= 15.
        } else {
            pin.metres <= 10.
        };
        self.arrived
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn arrival_has_hysteresis_and_resets_for_replaced_or_missing_pins() {
        let mut a = Arrival::default();
        let pin = Pin {
            position: [1., 2.],
            bearing: 0.,
            metres: 10.,
        };
        assert!(a.update(Some(pin)));
        assert!(a.update(Some(Pin {
            metres: 14.,
            ..pin
        })));
        assert!(!a.update(Some(Pin {
            metres: 16.,
            ..pin
        })));
        assert!(!a.update(Some(Pin {
            metres: 12.,
            ..pin
        })));
        assert!(a.update(Some(pin)));
        assert!(!a.update(Some(Pin {
            position: [3., 4.],
            metres: 12.,
            ..pin
        })));
        assert!(a.update(Some(pin)));
        assert!(!a.update(None));
    }
    use super::*;
    #[test]
    fn compass_wraps_and_cardinal_axes_match_world() {
        assert_eq!(relative(1., 359.), 2.);
        assert_eq!(relative(359., 1.), -2.);
        assert_eq!(bearing(0., -1.), 0.);
        assert_eq!(bearing(1., 0.), 90.);
        assert_eq!(bearing(0., 1.), 180.);
        assert_eq!(bearing(-1., 0.), 270.);
    }
    #[test]
    fn navigation_rejects_stale_torn_and_nonfinite_evidence() {
        let s = Sample {
            started_ms: 1000,
            finished_ms: 1005,
            heading: 0.,
            pin: None,
            pin_known: true,
            menu_open: false,
        };
        assert!(s.valid(1010));
        assert!(!s.valid(1300));
        assert!(!s.valid(1004));
        assert!(
            !Sample {
                heading: f64::NAN,
                ..s
            }
            .valid(1010)
        );
        assert!(
            !Sample {
                started_ms: 800,
                ..s
            }
            .valid(1010)
        );
        assert!(
            !Sample {
                pin: Some(Pin {
                    position: [0., 0.],
                    bearing: 0.,
                    metres: -1.
                }),
                ..s
            }
            .valid(1010)
        );
    }
}
