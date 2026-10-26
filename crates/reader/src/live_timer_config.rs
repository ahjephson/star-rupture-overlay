//! Configured timer values, guarded by exact-build native control flow.
use anyhow::{Result, ensure};
use jmap_dumper::mem::Ctx;
use starrupture_observation_model::observation::TimerConfiguration;
fn valid(interval: f32, warning: f32) -> bool {
    interval.is_finite()
        && warning.is_finite()
        && interval > 0.0
        && interval <= 86400.0
        && warning > 0.0
        && warning <= interval
}
pub async fn read(ctx: &Ctx, sub: u64, image: u64) -> Result<TimerConfiguration> {
    for (rva, expected) in [
        (124077696 + 0x9e, &[0xf3, 0x0f, 0x10, 0x76, 0x64][..]),
        (124077696 + 0x103, &[0x0f, 0x2f, 0x76, 0x7c][..]),
        (124053637, &[0xf3, 0x0f, 0x10, 0x46, 0x78][..]),
        (124010272, &[0x8b, 0x43, 0x3c, 0x89, 0x47, 0x78, 0x8b, 0x47, 0x78, 0x89, 0x47, 0x7c][..]),
    ] {
        let mut code = [0u8; 12];
        ctx.read_buf(image + rva, &mut code[..expected.len()]).await?;
        ensure!(
            &code[..expected.len()] == expected,
            "Timer configuration native instructions differ"
        );
    }
    let interval = ctx.read::<f32>(sub + 0x78).await?;
    let warning = ctx.read::<f32>(sub + 0x64).await?;
    ensure!(valid(interval, warning), "Invalid configured interval/warning range");
    Ok(TimerConfiguration {
        native_control_flow_verified: true,
        interval_seconds: f64::from(interval),
        warning_duration_seconds: f64::from(warning),
        interval_includes_warning: true,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn configurations_reject_invalid_ranges() {
        assert!(valid(2400.0, 120.0));
        assert!(valid(1800.0, 120.0));
        assert!(!valid(0.0, 120.0));
        assert!(!valid(60.0, 120.0));
        assert!(!valid(f32::NAN, 120.0));
        assert!(!valid(2400.0, f32::INFINITY));
    }
}
