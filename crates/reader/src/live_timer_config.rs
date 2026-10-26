//! Configured timer values, guarded by exact-build native control flow.
use anyhow::{Result, ensure};
use jmap_dumper::mem::Ctx;
use serde_json::{Value, json};
fn valid(interval: f32, warning: f32) -> bool {
    interval.is_finite()
        && warning.is_finite()
        && interval > 0.0
        && interval <= 86400.0
        && warning > 0.0
        && warning <= interval
}
pub async fn read(ctx: &Ctx, sub: u64, image: u64) -> Result<Value> {
    for (rva, expected) in [
        (124077696 + 0x9e, &[0xf3, 0x0f, 0x10, 0x76, 0x64][..]),
        (124077696 + 0x103, &[0x0f, 0x2f, 0x76, 0x7c][..]),
        (124053637, &[0xf3, 0x0f, 0x10, 0x46, 0x78][..]),
        (124010272, &[0x8b, 0x43, 0x3c, 0x89, 0x47, 0x78, 0x8b, 0x47, 0x78, 0x89, 0x47, 0x7c][..]),
    ] {
        let mut code = vec![0u8; expected.len()];
        ctx.read_buf(image + rva, &mut code).await?;
        ensure!(code == expected, "Timer configuration native instructions differ");
    }
    let interval = ctx.read::<f32>(sub + 0x78).await?;
    let warning = ctx.read::<f32>(sub + 0x64).await?;
    ensure!(valid(interval, warning), "Invalid configured interval/warning range");
    Ok(json!({
        "status": "ok",
        "nativeControlFlowVerified": true,
        "intervalSeconds": interval,
        "warningDurationSeconds": warning,
        "verification": "live_fields_with_native_control_flow",
        "intervalIncludesWarning": true,
        "sourceOffsets": {
            "interval": 120,
            "warning": 100,
        },
        "coverage": "Observed normal Heat2400/120 configuration; different values can be read but complete gameplay behaviour remains unverified.",
    }))
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
