//! Fixed-build, read-only active-phase readers. Never invoke native methods.
use anyhow::{Result, ensure};
use jmap_dumper::mem::Ctx;
use serde_json::{Value, json};
pub async fn read(ctx: &Ctx, subsystem: u64, image_base: u64) -> Result<(u64, Value)> {
    let implementation = ctx.read::<u64>(subsystem + 0x1f8).await?;
    if implementation == 0 {
        return Ok((0, json!({"status":"inactive"})));
    }
    let vtable = ctx.read::<u64>(implementation).await?;
    let mut stages = [0u8; 3];
    let mut rvas = vec![];
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
            ensure!(
                bytes.starts_with(pattern),
                "Unsupported phase reader instructions"
            );
        }
        rvas.push(address - image_base);
    }
    let progress = ctx.read::<f32>(implementation + 0x10).await?;
    let inverse_duration = ctx.read::<f32>(implementation + 0x14).await?;
    ensure!(
        inverse_duration.is_finite() && inverse_duration > 0.0,
        "Invalid phase inverse duration"
    );
    // Exact executable is independently hash-pinned. Check its active update binding
    // and controller clamp before exposing the researched phase-clock semantics.
    let update_rva = [0, 124071968, 124071664, 124071280, 124071568][stages[1] as usize];
    ensure!(
        ctx.read::<u64>(vtable + 8).await? == image_base + update_rva,
        "Unknown phase update binding"
    );
    let mut clamp = [0u8; 8];
    ctx.read_buf(image_base + 0x765420f, &mut clamp).await?;
    let displacement = i32::from_le_bytes(clamp[4..8].try_into().unwrap()) as i64;
    ensure!(
        clamp[..4] == [0xf3, 0x0f, 0x5d, 0x35] && (0x7654217i64 + displacement) == 0xaf40620,
        "Unknown phase delta clamp"
    );
    let delta_cap = ctx.read::<f32>(image_base + 0xaf40620).await?;
    ensure!(delta_cap == 0.1f32, "Changed phase delta cap");
    ensure!(
        progress.is_finite() && (0.0..=1.0).contains(&progress),
        "Invalid phase progress"
    );
    let wave_type = ctx.read::<u8>(implementation + 0x18).await?;
    let keys = [
        "waveSpeed",
        "waveStartPosition",
        "waveEndPosition",
        "preWaveDuration",
        "waveFadeoutFireWaveDuration",
        "waveFadeoutBurningDuration",
        "waveFadeoutFadingDuration",
        "waveGrowbackMoonPhaseDuration",
        "waveGrowbackRegrowthStartDuration",
        "waveGrowbackRegrowthDuration",
        "wavePreWaveExplosionDuration",
    ];
    let mut settings = serde_json::Map::new();
    for (i, k) in keys.iter().enumerate() {
        let v = ctx
            .read::<f32>(implementation + 0x1c + i as u64 * 4)
            .await?;
        ensure!(v.is_finite(), "Non-finite phase setting");
        settings.insert(k.to_string(), json!(v));
    }
    let start = ctx.read::<f64>(subsystem + 0x210).await?;
    ensure!(
        start.is_finite() && start >= 0.0,
        "Invalid wave start clock"
    );
    ensure!(
        ctx.read::<u64>(subsystem + 0x1f8).await? == implementation
            && ctx.read::<u64>(implementation).await? == vtable,
        "Phase changed during reader"
    );
    Ok((
        implementation,
        json!({"status":"ok","getterLayoutsVerified":true,"stage":stages[1],"waveType":wave_type,"progress":progress,"inverseDuration":inverse_duration,"phaseClockEvidence":{"status":"ok","nativeUpdateBindingVerified":true,"updateRva":update_rva,"deltaCapSeconds":delta_cap,"semantics":"progress += min(delta, cap) * inverseDuration; float32 and transition tick rounding apply"},"settings":settings,"waveStartTimeSeconds":start,"readerRvas":rvas,"progressVerification":"Native update control flow and retrospective completion anchors corroborate progress; future elapsed-time projections remain estimates."}),
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
            let at = address as usize;
            let end = at
                .checked_add(buf.len())
                .ok_or_else(|| anyhow::anyhow!("overflow"))?;
            buf.copy_from_slice(
                self.0
                    .get(at..end)
                    .ok_or_else(|| anyhow::anyhow!("unmapped"))?,
            );
            Ok(())
        }
    }
    fn context(stage: u8, corrupt: bool, progress: f32) -> Ctx {
        context_rate(stage, corrupt, progress, 1.0f32 / 735.0)
    }
    fn context_rate(stage: u8, corrupt: bool, progress: f32, inverse: f32) -> Ctx {
        let mut bytes = vec![0u8; 0x9000];
        bytes[0x11f8..0x1200].copy_from_slice(&0x2000u64.to_le_bytes());
        bytes[0x1210..0x1218].copy_from_slice(&100f64.to_le_bytes());
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
            build_config: serde_json::from_value(json!({})).unwrap(),
            uobjectarray: 0,
            image_base_address: 0x4000,
            build_change_list: None,
        })
    }
    #[test]
    fn invalid_phase_rates_are_rejected() {
        smol::block_on(async {
            for inverse in [0.0, -1.0, f32::NAN, f32::INFINITY] {
                assert!(
                    read(&context_rate(4, false, 0.5, inverse), 0x1000, 0x4000)
                        .await
                        .is_err()
                );
            }
        });
    }
    #[test]
    fn all_native_stage_readers_accept_only_proven_layouts() {
        smol::block_on(async {
            for stage in 1..=4 {
                let (_, r) = read(&context(stage, false, 0.5), 0x1000, 0x4000)
                    .await
                    .unwrap();
                assert_eq!(r["stage"], stage);
                assert_eq!(r["progress"], 0.5);
            }
            assert!(read(&context(1, true, 0.5), 0x1000, 0x4000).await.is_err());
            assert!(read(&context(5, false, 0.5), 0x1000, 0x4000).await.is_err());
            assert!(
                read(&context(4, false, f32::NAN), 0x1000, 0x4000)
                    .await
                    .is_err()
            );
            assert!(read(&context(4, false, 1.1), 0x1000, 0x4000).await.is_err());
        });
    }
}
