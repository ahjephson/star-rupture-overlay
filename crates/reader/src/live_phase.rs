//! Read-only active-phase readers with checked native arithmetic and controller proofs.
use anyhow::{Result, ensure};
use jmap_dumper::mem::Ctx;
use starrupture_observation_model::observation::{
    ActivePhase, HeatSettings, NativeStage, PhaseEvidence,
};
#[cfg(test)]
async fn read(ctx: &Ctx, subsystem: u64, image_base: u64) -> Result<(u64, PhaseEvidence)> {
    read_with_clock(ctx, subsystem, image_base, 0, &crate::phase_clock::Cache::default()).await
}
pub(crate) async fn read_with_clock(
    ctx: &Ctx,
    subsystem: u64,
    image_base: u64,
    settings: u64,
    clock: &crate::phase_clock::Cache,
) -> Result<(u64, PhaseEvidence)> {
    let implementation = ctx.read::<u64>(subsystem + 0x1f8).await?;
    if implementation == 0 {
        return Ok((0, PhaseEvidence::Inactive));
    }
    let vtable = ctx.read::<u64>(implementation).await?;
    let mut stages = [0u8; 3];
    for (slot, pattern) in [
        (0x18, &[0xf3, 0x0f, 0x10, 0x41, 0x10, 0xc3][..]),
        (0x20, &[][..]),
        (0x28, &[0x0f, 0xb6, 0x41, 0x18, 0xc3][..]),
        (0x30, &[0x48, 0x8d, 0x41, 0x1c, 0xc3][..]),
    ] {
        let address = ctx.read::<u64>(vtable + slot).await?;
        ensure!(
            address > image_base && address - image_base < 0x20000000,
            "Phase reader outside supported image"
        );
        let mut bytes = [0u8; 8];
        ctx.read_buf(address, &mut bytes).await?;
        if slot == 0x20 {
            stages.copy_from_slice(&bytes[..3]);
            ensure!(
                stages[0] == 0xb0 && (1..=4).contains(&stages[1]) && stages[2] == 0xc3,
                "Unknown phase enum reader"
            );
        } else {
            ensure!(bytes.starts_with(pattern), "Unsupported phase reader instructions");
        }
    }
    let stage = NativeStage::try_from(stages[1]).map_err(anyhow::Error::msg)?;
    let progress = ctx.read::<f32>(implementation + 0x10).await?;
    let inverse_duration = ctx.read::<f32>(implementation + 0x14).await?;
    ensure!(
        inverse_duration.is_finite() && inverse_duration > 0.0,
        "Invalid phase inverse duration"
    );
    let update = ctx.read::<u64>(vtable + 8).await?;
    ensure!(update > image_base && update - image_base < 0x20000000, "Phase update outside image");
    let mut code = [0; 64];
    ctx.read_buf(update, &mut code).await?;
    crate::phase_clock::update(&code)?;
    let delta_cap = clock.read(ctx, update, image_base, settings).await?;
    ensure!(delta_cap == 0.1f32, "Uncalibrated phase delta cap");
    ensure!(progress.is_finite() && (0.0..=1.0).contains(&progress), "Invalid phase progress");
    let wave_type = ctx.read::<u8>(implementation + 0x18).await?;
    // Consecutive native fields are named by HeatSettings::from_native_fields.
    let mut settings = [0.0; 11];
    for (i, setting) in settings.iter_mut().enumerate() {
        let v = ctx.read::<f32>(implementation + 0x1c + i as u64 * 4).await?;
        ensure!(v.is_finite(), "Non-finite phase setting");
        *setting = f64::from(v);
    }
    let mut after_code = [0; 64];
    ctx.read_buf(update, &mut after_code).await?;
    ensure!(
        after_code == code && ctx.read::<u64>(vtable + 8).await? == update,
        "Phase update binding changed"
    );
    let start = ctx.read::<f64>(subsystem + 0x210).await?;
    ensure!(stage.accepts_wave_start(start), "Invalid wave start clock");
    ensure!(
        ctx.read::<u64>(subsystem + 0x1f8).await? == implementation
            && ctx.read::<u64>(implementation).await? == vtable,
        "Phase changed during reader"
    );
    Ok((
        implementation,
        PhaseEvidence::Active(ActivePhase {
            getter_layouts_verified: true,
            stage,
            wave_type,
            progress: f64::from(progress),
            inverse_duration: f64::from(inverse_duration),
            native_update_binding_verified: true,
            delta_cap_seconds: f64::from(delta_cap),
            settings: HeatSettings::from_native_fields(settings),
            wave_start_seconds: start,
        }),
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use jmap_dumper::mem::{CtxInner, Mem};
    use std::collections::HashMap;
    struct Memory(Vec<u8>);
    #[async_trait(?Send)]
    impl Mem for Memory {
        async fn read_buf(&self, address: u64, buf: &mut [u8]) -> Result<()> {
            if address == 0x4000 + 0x765420f {
                let mut code = vec![0xf3, 0x0f, 0x5d, 0x35];
                code.extend_from_slice(&((0xaf40620i64 - 0x7654217) as i32).to_le_bytes());
                buf.copy_from_slice(&code);
                return Ok(());
            }
            if address == 0x4000 + 0xaf40620 {
                buf.copy_from_slice(&0.1f32.to_le_bytes());
                return Ok(());
            }
            if buf.len() == 65536 {
                buf.fill(0);
                return Ok(());
            }
            if [124071968u64, 124071664, 124071280, 124071568]
                .contains(&address.saturating_sub(0x4000))
                && buf.len() == 64
            {
                buf.fill(0xcc);
                let code = [
                    0xf3, 0x0f, 0x59, 0x49, 0x14, 0xf3, 0x0f, 0x58, 0x49, 0x10, 0xf3, 0x0f, 0x11,
                    0x49, 0x10,
                ];
                buf[..code.len()].copy_from_slice(&code);
                return Ok(());
            }
            let at = address as usize;
            let end = at.checked_add(buf.len()).ok_or_else(|| anyhow::anyhow!("overflow"))?;
            buf.copy_from_slice(self.0.get(at..end).ok_or_else(|| anyhow::anyhow!("unmapped"))?);
            Ok(())
        }
    }
    fn context(stage: u8, corrupt: bool, progress: f32) -> Ctx {
        context_rate(stage, corrupt, progress, 1.0f32 / 735.0)
    }
    fn context_rate(stage: u8, corrupt: bool, progress: f32, inverse: f32) -> Ctx {
        context_start(stage, corrupt, progress, inverse, 100.0)
    }
    fn context_start(stage: u8, corrupt: bool, progress: f32, inverse: f32, start: f64) -> Ctx {
        let mut bytes = vec![0u8; 0x9000];
        bytes[0x11f8..0x1200].copy_from_slice(&0x2000u64.to_le_bytes());
        bytes[0x1210..0x1218].copy_from_slice(&start.to_le_bytes());
        bytes[0x2000..0x2008].copy_from_slice(&0x3000u64.to_le_bytes());
        bytes[0x2010..0x2014].copy_from_slice(&progress.to_le_bytes());
        bytes[0x2014..0x2018].copy_from_slice(&inverse.to_le_bytes());
        if (1..=4).contains(&stage) {
            bytes[0x3008..0x3010].copy_from_slice(
                &(0x4000u64 + [0, 124071968, 124071664, 124071280, 124071568][stage as usize])
                    .to_le_bytes(),
            );
        }
        bytes[0x2018] = 1;
        for i in 0..11 {
            let at = 0x201c + i * 4;
            bytes[at..at + 4].copy_from_slice(&((i + 1) as f32).to_le_bytes());
        }
        let methods: [(usize, &[u8]); 4] = [
            (0x18, &[0xf3, 0x0f, 0x10, 0x41, 0x10, 0xc3]),
            (0x20, &[0xb0, stage, 0xc3]),
            (0x28, &[0x0f, 0xb6, 0x41, 0x18, 0xc3]),
            (0x30, &[0x48, 0x8d, 0x41, 0x1c, 0xc3]),
        ];
        for (i, (slot, code)) in methods.iter().enumerate() {
            let at = 0x4100 + i * 0x100;
            bytes[0x3000 + slot..0x3008 + slot].copy_from_slice(&(at as u64).to_le_bytes());
            bytes[at..at + code.len()].copy_from_slice(code);
        }
        if corrupt {
            bytes[0x4100] = 0x90;
        }
        Ctx::new(CtxInner {
            mem: Box::new(Memory(bytes)),
            fnamepool: 0,
            structs: HashMap::new(),
            version: (5, 6),
            build_config: Default::default(),
            uobjectarray: 0,
            image_base_address: 0x4000,
            build_change_list: None,
        })
    }
    #[test]
    fn unset_start_is_accepted_only_for_prewave_and_is_preserved() {
        smol::block_on(async {
            for stage in 1..=4 {
                let result =
                    read(&context_start(stage, false, 0.5, 1.0 / 120.0, f64::MIN), 0x1000, 0x4000)
                        .await;
                if stage == 1 {
                    let (_, PhaseEvidence::Active(p)) = result.unwrap() else {
                        panic!("Expected Pre-wave")
                    };
                    assert_eq!(p.wave_start_seconds, f64::MIN);
                } else {
                    assert!(result.is_err());
                }
                for invalid in [-1.0, f64::NEG_INFINITY, f64::INFINITY, f64::NAN] {
                    assert!(
                        read(
                            &context_start(stage, false, 0.5, 1.0 / 120.0, invalid),
                            0x1000,
                            0x4000
                        )
                        .await
                        .is_err()
                    );
                }
            }
        });
    }
    #[test]
    fn invalid_phase_rates_are_rejected() {
        smol::block_on(async {
            for inverse in [0.0, -1.0, f32::NAN, f32::INFINITY] {
                assert!(read(&context_rate(4, false, 0.5, inverse), 0x1000, 0x4000).await.is_err());
            }
        });
    }
    #[test]
    fn all_native_stage_readers_accept_only_proven_layouts() {
        smol::block_on(async {
            for stage in 1..=4 {
                let (_, r) = read(&context(stage, false, 0.5), 0x1000, 0x4000).await.unwrap();
                let PhaseEvidence::Active(r) = r else {
                    panic!("Expected active phase")
                };
                assert_eq!(r.stage as u8, stage);
                assert_eq!(r.progress, 0.5);
                assert_eq!(r.settings.wave_speed, 1.0);
                assert_eq!(r.settings.wave_start_position, 2.0);
                assert_eq!(r.settings.wave_end_position, 3.0);
                assert_eq!(r.settings.pre_wave_duration, 4.0);
                assert_eq!(r.settings.fadeout_fire_wave_duration, 5.0);
                assert_eq!(r.settings.fadeout_burning_duration, 6.0);
                assert_eq!(r.settings.fadeout_fading_duration, 7.0);
                assert_eq!(r.settings.growback_moon_phase_duration, 8.0);
                assert_eq!(r.settings.growback_regrowth_start_duration, 9.0);
                assert_eq!(r.settings.growback_regrowth_duration, 10.0);
                assert_eq!(r.settings.pre_wave_explosion_duration, 11.0);
            }
            assert!(read(&context(1, true, 0.5), 0x1000, 0x4000).await.is_err());
            assert!(read(&context(5, false, 0.5), 0x1000, 0x4000).await.is_err());
            assert!(read(&context(4, false, f32::NAN), 0x1000, 0x4000).await.is_err());
            assert!(read(&context(4, false, 1.1), 0x1000, 0x4000).await.is_err());
        });
    }
}
