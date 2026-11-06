//! Independent navigation evidence and horizontal compass geometry.
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
#[derive(Default)]
pub struct Control {
    configured: AtomicBool,
    sampling: AtomicBool,
}
impl Control {
    /// Prepare metadata while configured, independently of gameplay visibility.
    pub fn configure(&self, enabled: bool) {
        self.configured.store(enabled, Ordering::Relaxed);
    }
    pub fn should_prepare(&self) -> bool {
        self.configured.load(Ordering::Relaxed) || self.enabled()
    }
    pub fn set(&self, enabled: bool) {
        self.sampling.store(enabled, Ordering::Relaxed);
    }
    pub fn enabled(&self) -> bool {
        self.sampling.load(Ordering::Relaxed)
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
/// Horizontal proximity latch with separate arrival/departure thresholds.
#[derive(Clone, Copy, Default)]
pub struct Arrival {
    target: Option<[u64; 2]>,
    arrived: bool,
}
impl Arrival {
    pub fn update(&mut self, pin: Option<Pin>) -> bool {
        self.update_with(pin, 25., 35.)
    }
    pub fn update_with(&mut self, pin: Option<Pin>, arrival: f64, departure: f64) -> bool {
        if !arrival.is_finite() || !departure.is_finite() || arrival <= 0. || departure <= arrival {
            *self = Self::default();
            return false;
        }
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
            pin.metres <= departure
        } else {
            pin.metres <= arrival
        };
        self.arrived
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn configured_compass_prepares_while_gameplay_sampling_is_hidden() {
        let control = Control::default();
        assert!(!control.should_prepare());
        control.configure(true);
        assert!(control.should_prepare());
        assert!(!control.enabled());
        control.set(true);
        assert!(control.enabled());
        control.set(false);
        assert!(control.should_prepare());
        control.configure(false);
        assert!(!control.should_prepare());
        // Existing finite diagnostics can request sampling directly.
        control.set(true);
        assert!(control.should_prepare());
    }
    #[test]
    fn arrival_has_hysteresis_and_resets_for_replaced_or_missing_pins() {
        let mut a = Arrival::default();
        let pin = Pin {
            position: [1., 2.],
            bearing: 0.,
            metres: 25.,
        };
        assert!(a.update(Some(pin)));
        assert!(a.update(Some(Pin {
            metres: 34.,
            ..pin
        })));
        assert!(!a.update(Some(Pin {
            metres: 36.,
            ..pin
        })));
        assert!(!a.update(Some(Pin {
            metres: 26.,
            ..pin
        })));
        assert!(a.update(Some(pin)));
        assert!(!a.update(Some(Pin {
            position: [3., 4.],
            metres: 26.,
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

#[cfg(test)]
mod configurable_arrival_tests {
    use super::*;
    #[test]
    fn configured_boundaries_and_invalid_settings_preserve_safe_latch_behavior() {
        let mut arrival = Arrival::default();
        let at = |metres| {
            Some(Pin {
                position: [1., 2.],
                bearing: 0.,
                metres,
            })
        };
        assert!(!arrival.update_with(at(50.1), 50., 65.));
        assert!(arrival.update_with(at(50.), 50., 65.));
        assert!(arrival.update_with(at(65.), 50., 65.));
        assert!(!arrival.update_with(at(65.1), 50., 65.));
        assert!(!arrival.update_with(at(60.), 50., 65.));
        assert!(!arrival.update_with(at(1.), f64::NAN, 65.));
        assert!(!arrival.update_with(at(1.), 65., 50.));
        assert!(arrival.update_with(at(25.), 25., 35.));
        assert!(arrival.update_with(at(35.), 25., 35.));
        assert!(!arrival.update_with(at(35.1), 25., 35.));
    }
}
