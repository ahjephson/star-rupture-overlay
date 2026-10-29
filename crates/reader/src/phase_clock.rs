//! Session-owned proofs for active progress arithmetic and its controller delta cap.
use anyhow::{Context, Result, ensure};
use iced_x86::{Decoder, DecoderOptions, Instruction, Mnemonic, OpKind, Register};
use jmap_dumper::mem::Ctx;
use std::{cell::RefCell, rc::Rc};
#[derive(Default)]
pub(crate) struct Cache(RefCell<Option<Rc<Clock>>>);
struct Clock {
    proof: (u64, Vec<u8>),
    constant: u64,
    scaled: bool,
}
fn memory(i: &Instruction, base: Register, offset: u64) -> bool {
    i.memory_base() == base
        && i.memory_index() == Register::None
        && i.memory_displacement64() == offset
}
pub(crate) fn update(bytes: &[u8]) -> Result<()> {
    let mut decoder = Decoder::new(64, bytes, DecoderOptions::NONE);
    let (mut old, mut multiplied, mut added, mut stored) = (false, false, false, false);
    while decoder.can_decode() {
        let i = decoder.decode();
        match i.mnemonic() {
            Mnemonic::Movss
                if i.op0_register() == Register::XMM0 && memory(&i, Register::RCX, 0x10) =>
            {
                old = true
            }
            Mnemonic::Mulss
                if i.op0_register() == Register::XMM1
                    && memory(&i, Register::RCX, 0x14)
                    && !multiplied =>
            {
                multiplied = true
            }
            Mnemonic::Addss
                if multiplied
                    && !added
                    && i.op0_register() == Register::XMM1
                    && (memory(&i, Register::RCX, 0x10)
                        || old && i.op1_register() == Register::XMM0) =>
            {
                added = true
            }
            Mnemonic::Movss
                if added
                    && i.op0_kind() == OpKind::Memory
                    && memory(&i, Register::RCX, 0x10)
                    && i.op1_register() == Register::XMM1 =>
            {
                stored = true;
                break;
            }
            Mnemonic::Ret | Mnemonic::Call => break,
            _ if multiplied
                && matches!(
                    i.mnemonic(),
                    Mnemonic::Mulss
                        | Mnemonic::Addss
                        | Mnemonic::Subss
                        | Mnemonic::Divss
                        | Mnemonic::Maxss
                        | Mnemonic::Minss
                )
                && i.op0_register() == Register::XMM1 =>
            {
                anyhow::bail!("Unexpected phase progress operation")
            }
            _ if i.is_jcc_short_or_near() || i.is_jmp_short_or_near() => break,
            _ => {}
        }
    }
    ensure!(stored, "Unrecognised native phase progress arithmetic");
    Ok(())
}
fn controller(bytes: &[u8], address: u64) -> Result<u64> {
    let mut d = Decoder::with_ip(64, bytes, address, DecoderOptions::NONE);
    let (mut owner, mut delta, mut provider, mut cap, mut phase, mut clipped, mut argument) =
        (false, false, false, None, false, false, false);
    while d.can_decode() {
        let i = d.decode();
        match i.mnemonic() {
            Mnemonic::Mov
                if i.op0_register() == Register::R14 && i.op1_register() == Register::RCX =>
            {
                owner = true
            }
            Mnemonic::Movaps
                if i.op0_register() == Register::XMM6 && i.op1_register() == Register::XMM1 =>
            {
                delta = true
            }
            Mnemonic::Call
                if owner
                    && delta
                    && i.op0_kind() == OpKind::Memory
                    && memory(&i, Register::RDX, 0x778) =>
            {
                provider = true
            }
            Mnemonic::Mulss
                if provider
                    && i.op0_register() == Register::XMM0
                    && i.is_ip_rel_memory_operand() =>
            {
                cap = Some(i.ip_rel_memory_address())
            }
            Mnemonic::Mov
                if cap.is_some()
                    && i.op0_register() == Register::RCX
                    && memory(&i, Register::R14, 0x1f8) =>
            {
                phase = true
            }
            Mnemonic::Minss
                if phase
                    && i.op0_register() == Register::XMM0
                    && i.op1_register() == Register::XMM6 =>
            {
                clipped = true
            }
            Mnemonic::Movaps
                if clipped
                    && i.op0_register() == Register::XMM1
                    && i.op1_register() == Register::XMM0 =>
            {
                argument = true
            }
            Mnemonic::Call
                if argument && i.op0_kind() == OpKind::Memory && memory(&i, Register::RAX, 8) =>
            {
                return cap.context("Missing phase cap");
            }
            Mnemonic::Ret => break,
            _ => {}
        }
    }
    anyhow::bail!("Unrecognised scaled phase controller")
}
fn getter(bytes: &[u8], address: u64) -> Result<(u64, u8, [u64; 3], u64)> {
    let mut d = Decoder::with_ip(64, bytes, address, DecoderOptions::NONE);
    let test = d.decode();
    ensure!(
        test.mnemonic() == Mnemonic::Test
            && test.memory_base() == Register::RCX
            && test.memory_index() == Register::None
            && test.op1_kind() == OpKind::Immediate8,
        "Unknown effective time-dilation guard"
    );
    let branch = d.decode();
    ensure!(branch.mnemonic() == Mnemonic::Je, "Unknown time-dilation branch");
    let mut fields = [0; 3];
    for (index, field) in fields.iter_mut().enumerate() {
        let i = d.decode();
        ensure!(
            i.mnemonic()
                == if index == 0 {
                    Mnemonic::Movss
                } else {
                    Mnemonic::Mulss
                }
                && i.op0_register() == Register::XMM0
                && i.memory_base() == Register::RCX
                && i.memory_index() == Register::None,
            "Unknown time-dilation factors"
        );
        *field = i.memory_displacement64();
        ensure!(
            (0x28..0x1000).contains(field) && field.is_multiple_of(4),
            "Time-dilation factor offset"
        );
    }
    ensure!(
        d.decode().mnemonic() == Mnemonic::Ret && d.ip() == branch.near_branch_target(),
        "Unknown time-dilation fallback"
    );
    let fallback = d.decode();
    ensure!(
        fallback.mnemonic() == Mnemonic::Movss
            && fallback.op0_register() == Register::XMM0
            && fallback.is_ip_rel_memory_operand()
            && d.decode().mnemonic() == Mnemonic::Ret,
        "Unknown default time dilation"
    );
    Ok((test.memory_displacement64(), test.immediate8(), fields, fallback.ip_rel_memory_address()))
}
async fn effective(ctx: &Ctx, settings: u64, image: u64) -> Result<f32> {
    let vt = ctx.read::<u64>(settings).await?;
    let function = ctx.read::<u64>(vt + 0x778).await?;
    ensure!(
        function > image && function - image < 0x20000000,
        "Time-dilation getter outside image"
    );
    let mut bytes = [0; 64];
    ctx.read_buf(function, &mut bytes).await?;
    let (offset, mask, fields, fallback) = getter(&bytes, function)?;
    ensure!(
        offset < 0x1000 && mask != 0 && fallback > image && fallback - image < 0x20000000,
        "Time-dilation binding bounds"
    );
    ensure!(ctx.read::<f32>(fallback).await? == 1.0, "Changed default time dilation");
    let flag = ctx.read::<u8>(settings + offset).await?;
    let mut values = [0.0f32; 3];
    for (index, field) in fields.iter().enumerate() {
        values[index] = ctx.read::<f32>(settings + field).await?;
    }
    ensure!(
        values.iter().all(|v| v.is_finite() && *v > 0.0 && *v <= 100.0),
        "Invalid effective time-dilation factors"
    );
    let value = if flag & mask == 0 {
        1.0
    } else {
        values[0] * values[1] * values[2]
    };
    for (index, field) in fields.iter().enumerate() {
        ensure!(
            ctx.read::<f32>(settings + field).await?.to_bits() == values[index].to_bits(),
            "Time dilation changed during read"
        );
    }
    let mut after = [0; 64];
    ctx.read_buf(function, &mut after).await?;
    ensure!(
        after == bytes
            && ctx.read::<u64>(settings).await? == vt
            && ctx.read::<u64>(vt + 0x778).await? == function
            && ctx.read::<u8>(settings + offset).await? == flag,
        "Time-dilation binding changed"
    );
    Ok(value)
}
impl Clock {
    async fn resolve(ctx: &Ctx, update: u64, image: u64) -> Result<Self> {
        // A single bounded discovery near the owned phase methods, not a registry or image scan.
        let start = update.saturating_sub(32768).max(image);
        let mut bytes = vec![0; 65536];
        ctx.read_buf(start, &mut bytes).await?;
        let pattern = [0x48, 0x83, 0xb9, 0xf8, 1, 0, 0, 0];
        let mut matches = Vec::new();
        for (index, window) in bytes.windows(256).enumerate() {
            if window.starts_with(&pattern)
                && let Ok(constant) = controller(window, start + index as u64)
            {
                ensure!(
                    constant > image && constant - image < 0x20000000,
                    "Phase cap outside image"
                );
                ensure!(ctx.read::<f32>(constant).await? == 0.1, "Changed phase base delta cap");
                matches.push(Self {
                    proof: (start + index as u64, window.to_vec()),
                    constant,
                    scaled: true,
                });
            }
        }
        ensure!(matches.len() <= 1, "Ambiguous phase controller");
        if let Some(value) = matches.pop() {
            return Ok(value);
        }
        // Retain the independently verified original compiler form for the previous game layout.
        let address = image + 0x765420f;
        let mut code = vec![0; 8];
        ctx.read_buf(address, &mut code).await?;
        ensure!(code[..4] == [0xf3, 0x0f, 0x5d, 0x35], "Unknown phase delta controller");
        let constant =
            (address as i64 + 8 + i32::from_le_bytes(code[4..8].try_into().unwrap()) as i64) as u64;
        ensure!(
            constant == image + 0xaf40620 && ctx.read::<f32>(constant).await? == 0.1,
            "Unknown legacy phase delta cap"
        );
        Ok(Self {
            proof: (address, code),
            constant,
            scaled: false,
        })
    }
    async fn read(&self, ctx: &Ctx, settings: u64, image: u64) -> Result<f32> {
        let mut bytes = vec![0; self.proof.1.len()];
        ctx.read_buf(self.proof.0, &mut bytes).await?;
        ensure!(
            bytes == self.proof.1 && ctx.read::<f32>(self.constant).await? == 0.1,
            "Phase controller proof changed"
        );
        Ok(if self.scaled {
            0.1 * effective(ctx, settings, image).await?
        } else {
            0.1
        })
    }
}
impl Cache {
    pub async fn read(&self, ctx: &Ctx, update: u64, image: u64, settings: u64) -> Result<f32> {
        let existing = self.0.borrow().clone();
        let clock = match existing {
            Some(value) => value,
            None => {
                let value = Rc::new(Clock::resolve(ctx, update, image).await?);
                *self.0.borrow_mut() = Some(value.clone());
                value
            }
        };
        clock.read(ctx, settings, image).await
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn controller_code(address: u64, constant: u64) -> Vec<u8> {
        let mut b = vec![
            0x48, 0x83, 0xb9, 0xf8, 1, 0, 0, 0, 0x4c, 0x8b, 0xf1, 0x0f, 0x28, 0xf1, 0xff, 0x92,
            0x78, 7, 0, 0, 0xf3, 0x0f, 0x59, 5,
        ];
        b.extend(((constant as i64 - address as i64 - b.len() as i64 - 4) as i32).to_le_bytes());
        b.extend([
            0x49, 0x8b, 0x8e, 0xf8, 1, 0, 0, 0xf3, 0x0f, 0x5d, 0xc6, 0x48, 0x8b, 1, 0x0f, 0x28,
            0xc8, 0xff, 0x50, 8, 0xc3,
        ]);
        b
    }
    #[test]
    fn controller_requires_delta_source_clamp_and_owned_update_call() {
        let b = controller_code(0x1000, 0x4000);
        assert_eq!(controller(&b, 0x1000).unwrap(), 0x4000);
        for n in 0..b.len() - 1 {
            assert!(controller(&b[..n], 0x1000).is_err(), "length {n}");
        }
        for (offset, value) in [(13, 0xf9), (16, 0x80), (22, 0x58), (37, 0x5f), (47, 0x10)] {
            let mut bad = b.clone();
            bad[offset] = value;
            assert!(controller(&bad, 0x1000).is_err(), "offset {offset}");
        }
    }
    #[test]
    fn scaled_cap_is_read_from_checked_fields_and_discovery_is_session_cached() {
        use async_trait::async_trait;
        use jmap_dumper::mem::{CtxInner, Mem};
        use std::{cell::Cell, collections::HashMap};
        struct Memory(Rc<RefCell<Vec<u8>>>, Rc<Cell<usize>>);
        #[async_trait(?Send)]
        impl Mem for Memory {
            async fn read_buf(&self, address: u64, buf: &mut [u8]) -> Result<()> {
                if buf.len() == 65536 {
                    self.1.set(self.1.get() + 1)
                }
                let b = self.0.borrow();
                let at = address as usize;
                buf.copy_from_slice(b.get(at..at + buf.len()).context("Synthetic memory bound")?);
                Ok(())
            }
        }
        let mut b = vec![0; 0x30000];
        b[0x5000..0x5008].copy_from_slice(&0x11000u64.to_le_bytes());
        b[0x11778..0x11780].copy_from_slice(&0x12000u64.to_le_bytes());
        b[0x5070] = 2;
        for field in [0x5040, 0x5044, 0x5048] {
            b[field..field + 4].copy_from_slice(&1.0f32.to_le_bytes());
        }
        let mut g = vec![
            0xf6, 0x41, 0x70, 2, 0x74, 16, 0xf3, 0x0f, 0x10, 0x41, 0x40, 0xf3, 0x0f, 0x59, 0x41,
            0x44, 0xf3, 0x0f, 0x59, 0x41, 0x48, 0xc3, 0xf3, 0x0f, 0x10, 5,
        ];
        g.extend((0x13004i32 - 0x12000 - g.len() as i32 - 4).to_le_bytes());
        g.push(0xc3);
        b[0x12000..0x12000 + g.len()].copy_from_slice(&g);
        b[0x13000..0x13004].copy_from_slice(&0.1f32.to_le_bytes());
        b[0x13004..0x13008].copy_from_slice(&1.0f32.to_le_bytes());
        let tick = controller_code(0x14000, 0x13000);
        b[0x14000..0x14000 + tick.len()].copy_from_slice(&tick);
        let memory = Rc::new(RefCell::new(b));
        let scans = Rc::new(Cell::new(0));
        let ctx = Ctx::new(CtxInner {
            mem: Box::new(Memory(memory.clone(), scans.clone())),
            fnamepool: 0,
            structs: HashMap::new(),
            version: (5, 6),
            build_config: Default::default(),
            uobjectarray: 0,
            image_base_address: 0x10000,
            build_change_list: None,
        });
        smol::block_on(async {
            let cache = Cache::default();
            assert_eq!(cache.read(&ctx, 0x18000, 0x10000, 0x5000).await.unwrap(), 0.1);
            assert_eq!(cache.read(&ctx, 0x18000, 0x10000, 0x5000).await.unwrap(), 0.1);
            assert_eq!(scans.get(), 1);
            memory.borrow_mut()[0x5044..0x5048].copy_from_slice(&2.0f32.to_le_bytes());
            assert_eq!(cache.read(&ctx, 0x18000, 0x10000, 0x5000).await.unwrap(), 0.2);
            memory.borrow_mut()[0x5070] = 0;
            assert_eq!(cache.read(&ctx, 0x18000, 0x10000, 0x5000).await.unwrap(), 0.1);
            memory.borrow_mut()[0x14000] = 0x90;
            assert!(cache.read(&ctx, 0x18000, 0x10000, 0x5000).await.is_err());
            assert_eq!(scans.get(), 1);
        });
    }
    #[test]
    fn only_checked_progress_arithmetic_is_accepted() {
        let bytes = [
            0xf3, 0x0f, 0x59, 0x49, 0x14, 0xf3, 0x0f, 0x58, 0x49, 0x10, 0xf3, 0x0f, 0x11, 0x49,
            0x10,
        ];
        update(&bytes).unwrap();
        let moving = [
            0xf3, 0x0f, 0x10, 0x41, 0x10, 0xf3, 0x0f, 0x59, 0x49, 0x14, 0xf3, 0x0f, 0x58, 0xc8,
            0x0f, 0x2f, 0xce, 0xf3, 0x0f, 0x11, 0x49, 0x10,
        ];
        update(&moving).unwrap();
        let mut repeated = bytes[..10].to_vec();
        repeated.extend(&bytes[5..]);
        assert!(update(&repeated).is_err());
        for index in [4, 9, 14] {
            let mut bad = bytes;
            bad[index] = 0x18;
            assert!(update(&bad).is_err());
        }
        for n in 0..bytes.len() {
            assert!(update(&bytes[..n]).is_err());
        }
    }
}
