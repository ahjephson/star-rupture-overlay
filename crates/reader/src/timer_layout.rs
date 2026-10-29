//! Native save/load evidence for the unreflected timer fields.
use anyhow::{Context, Result, ensure};
use iced_x86::{Decoder, DecoderOptions, MemorySize, Mnemonic, OpKind, Register};
use jmap_dumper::{
    mem::{Ctx, Ptr},
    objects::{UFunction, UObject, UStruct},
};
use std::collections::{BTreeSet, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Fields {
    pub interval: u64,
    pub remaining: u64,
    pub waiting: u64,
    pub paused: u64,
    pub previous: u64,
    pub stopped: u64,
    pub waves: u64,
}
fn loads(bytes: &[u8], actor: u64) -> Result<Fields> {
    let mut decoder = Decoder::new(64, bytes, DecoderOptions::NONE);
    let (mut flags, mut floats, mut counters) = (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
    let mut original = false;
    while decoder.can_decode() {
        let i = decoder.decode();
        if i.mnemonic() == Mnemonic::Mov && i.op0_register() == Register::RBX {
            if i.op1_register() == Register::RCX && !original {
                original = true;
                continue;
            }
            if original {
                break;
            }
        }
        if !original
            || i.op1_kind() != OpKind::Memory
            || i.memory_base() != Register::RBX
            || i.memory_index() != Register::None
        {
            continue;
        }
        let offset = i.memory_displacement64();
        match i.mnemonic() {
            Mnemonic::Movzx if i.memory_size() == MemorySize::UInt8 => {
                flags.insert(offset);
            }
            Mnemonic::Mov
                if i.op0_register() == Register::RAX && i.memory_size() == MemorySize::UInt64 =>
            {
                counters.insert(offset);
            }
            Mnemonic::Movss => {
                floats.insert(offset);
            }
            _ => {}
        }
    }
    ensure!(
        flags.len() == 4 && floats.len() == 2 && counters.len() == 1,
        "Unrecognised timer save field pattern"
    );
    let flags: Vec<_> = flags.into_iter().collect();
    let floats: Vec<_> = floats.into_iter().collect();
    let waves = *counters.first().unwrap();
    ensure!(
        flags.windows(2).all(|p| p[1] == p[0] + 1)
            && floats[1] == floats[0] + 4
            && flags[0] == floats[1] + 4
            && waves > flags[3]
            && waves + 8 == actor
            && floats[0] >= 0x40
            && actor <= 0x400,
        "Timer save field relationships changed"
    );
    Ok(Fields {
        interval: floats[0],
        remaining: floats[1],
        waiting: flags[0],
        paused: flags[1],
        previous: flags[2],
        stopped: flags[3],
        waves,
    })
}
fn inverse(bytes: &[u8], fields: Fields) -> Result<()> {
    let mut decoder = Decoder::new(64, bytes, DecoderOptions::NONE);
    let mut writes = BTreeSet::new();
    let mut original = false;
    while decoder.can_decode() {
        let i = decoder.decode();
        if i.mnemonic() == Mnemonic::Ret {
            break;
        }
        if i.mnemonic() == Mnemonic::Mov && i.op0_register() == Register::RDI {
            if i.op1_register() == Register::RCX && !original {
                original = true;
                continue;
            }
            if original {
                break;
            }
        }
        if !original
            || i.op0_kind() != OpKind::Memory
            || i.memory_base() != Register::RDI
            || i.memory_index() != Register::None
        {
            continue;
        }
        let size = match i.mnemonic() {
            Mnemonic::Mov => i.memory_size().size(),
            Mnemonic::Movss => 4,
            _ => 0,
        };
        if size > 0 {
            writes.insert((i.memory_displacement64(), size));
        }
    }
    for (offset, size) in [
        (fields.waiting, 1),
        (fields.paused, 1),
        (fields.previous, 1),
        (fields.stopped, 1),
        (fields.waves, 8),
        (fields.interval, 4),
        (fields.remaining, 4),
    ] {
        ensure!(writes.contains(&(offset, size)), "Timer load does not corroborate save fields");
    }
    Ok(())
}
async fn entry(ctx: &Ctx, wrapper: u64, image: u64) -> Result<(u64, Vec<u8>)> {
    ensure!(wrapper > image && wrapper - image < 0x20000000, "Timer wrapper outside image");
    let mut bytes = vec![0; 32];
    ctx.read_buf(wrapper, &mut bytes).await?;
    let mut decoder = Decoder::with_ip(64, &bytes, wrapper, DecoderOptions::NONE);
    while decoder.can_decode() {
        let i = decoder.decode();
        if i.is_jmp_short_or_near() {
            let target = i.near_branch_target();
            ensure!(target > image && target - image < 0x20000000, "Timer method outside image");
            return Ok((target, bytes));
        }
        ensure!(i.mnemonic() != Mnemonic::Ret, "Timer wrapper does not delegate");
    }
    anyhow::bail!("Timer native wrapper changed")
}
pub(crate) struct Layout {
    pub fields: Fields,
    proofs: Vec<(u64, Vec<u8>)>,
}
impl Layout {
    pub async fn resolve(ctx: &Ctx, sub: u64, image: u64, actor: u64) -> Result<Self> {
        let object = Ptr::<UObject>::new(sub, ctx.clone())?;
        let class = object.class_private().read().await?;
        ensure!(
            class.path().await? == "/Script/Chimera.CrEnviroWaveTimerSubsystem",
            "Unexpected timer class"
        );
        let mut child = class.cast::<UStruct>().children().read().await?;
        let mut visited = HashSet::new();
        let (mut save, mut load) = (None, None);
        while let Some(f) = child {
            ensure!(
                visited.len() < 256 && visited.insert(f.address()),
                "Timer function chain bound"
            );
            let name = f.cast::<UObject>().name_private().read().await?;
            if name == "OnPreSaveStart" || name == "OnSaveLoaded" {
                let slot = f.cast::<UFunction>().func();
                let wrapper = slot.read().await? as u64;
                let (address, wrapper_bytes) = entry(ctx, wrapper, image).await?;
                let mut bytes = vec![0; 1024];
                ctx.read_buf(address, &mut bytes).await?;
                let record = (wrapper, wrapper_bytes, address, bytes, slot.address());
                if name == "OnPreSaveStart" {
                    ensure!(save.is_none(), "Duplicate timer save binding");
                    save = Some(record);
                } else {
                    ensure!(load.is_none(), "Duplicate timer load binding");
                    load = Some(record);
                }
            }
            child = f.next().read().await?;
        }
        let (s, l) = (
            save.context("Missing timer save method")?,
            load.context("Missing timer load method")?,
        );
        let fields = loads(&s.3, actor)?;
        inverse(&l.3, fields)?;
        Ok(Self {
            fields,
            proofs: vec![
                (s.0, s.1),
                (s.2, s.3),
                (s.4, s.0.to_le_bytes().to_vec()),
                (l.0, l.1),
                (l.2, l.3),
                (l.4, l.0.to_le_bytes().to_vec()),
            ],
        })
    }
    pub async fn verify(&self, ctx: &Ctx) -> Result<()> {
        for (address, expected) in &self.proofs {
            let mut bytes = vec![0; expected.len()];
            ctx.read_buf(*address, &mut bytes).await?;
            ensure!(bytes == *expected, "Timer native binding changed");
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(interval: u32, waiting: u32, waves: u32) -> (Vec<u8>, Vec<u8>) {
        let mut save = vec![0x48, 0x8b, 0xd9];
        let mut load = vec![0x48, 0x8b, 0xf9];
        for offset in waiting..waiting + 4 {
            save.extend([0x0f, 0xb6, 0x83]);
            save.extend(offset.to_le_bytes());
            load.extend([0x88, 0x87]);
            load.extend(offset.to_le_bytes());
        }
        save.extend([0x48, 0x8b, 0x83]);
        save.extend(waves.to_le_bytes());
        load.extend([0x48, 0x89, 0x87]);
        load.extend(waves.to_le_bytes());
        for offset in [interval, interval + 4] {
            save.extend([0xf3, 0x0f, 0x10, 0x83]);
            save.extend(offset.to_le_bytes());
            load.extend([0xf3, 0x0f, 0x11, 0x87]);
            load.extend(offset.to_le_bytes());
        }
        save.extend([0x48, 0x8b, 0x5c, 0x24, 0x20]);
        (save, load)
    }
    #[test]
    fn distinct_timer_layouts_require_matching_save_and_load_code() {
        for (interval, waiting, waves, actor) in
            [(0x78, 0x80, 0x88, 0x90), (0x84, 0x8c, 0x90, 0x98)]
        {
            let (s, l) = fixture(interval, waiting, waves);
            let f = loads(&s, actor).unwrap();
            assert_eq!(f.waves, u64::from(waves));
            assert_eq!(f.remaining, u64::from(interval + 4));
            inverse(&l, f).unwrap();
            let (_, wrong) = fixture(interval + 4, waiting, waves);
            assert!(inverse(&wrong, f).is_err());
            assert!(loads(&s, actor + 8).is_err());
        }
    }
}
