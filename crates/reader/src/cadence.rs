//! Validated sampling intervals for the embedded reader.
use std::time::Duration;
#[derive(Clone, Copy)]
pub(crate) struct Cadence {
    pub menu: Duration,
    pub world: Duration,
}
impl Cadence {
    pub fn new(menu_ms: u32, world_ms: u32) -> Result<Self, &'static str> {
        if !(25..=100).contains(&menu_ms) || !(250..=1000).contains(&world_ms) {
            return Err("Invalid sampling cadence bounds");
        }
        Ok(Self {
            menu: Duration::from_millis(u64::from(menu_ms)),
            world: Duration::from_millis(u64::from(world_ms)),
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sampling_intervals_preserve_supported_bounds() {
        assert!(Cadence::new(50, 250).is_ok());
        for (menu, world) in [(24, 250), (101, 250), (50, 249), (50, 1001)] {
            assert!(Cadence::new(menu, world).is_err());
        }
        assert!(Cadence::new(25, 250).is_ok());
        assert!(Cadence::new(100, 1000).is_ok());
    }
}
