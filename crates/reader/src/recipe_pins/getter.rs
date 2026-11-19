//! Resolves the loaded-session FString through the native save-menu consumer without invoking it.
use crate::{
    RuntimeCache,
    native_lookup::{Lookup, SAVE_MENU},
    recipe,
};
use anyhow::{Context, Result, ensure};
use iced_x86::{Decoder, DecoderOptions, Mnemonic, OpKind};
use jmap_dumper::{
    mem::{Ctx, Ptr},
    objects::{UFunction, UObject, UStruct},
};
use std::collections::BTreeSet;

pub(super) async fn resolve(ctx: &Ctx, base: &RuntimeCache, lookup: &Lookup) -> Result<u64> {
    let mut menu = None;
    for (_, address) in lookup.resolve(ctx, base)? {
        if crate::path(ctx, address).await? == SAVE_MENU {
            ensure!(menu.replace(address).is_none(), "Duplicate native save-menu class");
        }
    }
    let menu = menu.context("Native save-menu class unavailable for recipe persistence")?;
    let menu_stamp = recipe::allocation(ctx, menu).await?;
    let mut field = Ptr::<UStruct>::new(menu, ctx.clone())?.children().read().await?;
    let mut visited = BTreeSet::new();
    let mut consumer = None;
    while let Some(child) = field {
        ensure!(
            visited.len() < 256 && visited.insert(child.address()),
            "Save-menu function chain bound"
        );
        let object = child.cast::<UObject>();
        if object.name_private().read().await? == "SaveButtonClicked" {
            let class = object.class_private().read().await?;
            ensure!(
                class.path().await? == "/Script/CoreUObject.Function"
                    && object
                        .outer_private()
                        .read()
                        .await?
                        .is_some_and(|outer| outer.address() == menu),
                "Native save-menu consumer ownership"
            );
            ensure!(
                consumer.replace(child.cast::<UFunction>()).is_none(),
                "Duplicate save-menu consumer"
            );
        }
        field = child.next().read().await?;
    }
    let consumer =
        consumer.context("Native save-menu consumer unavailable for recipe persistence")?;
    let stamp = recipe::allocation(ctx, consumer.address()).await?;
    let entry = consumer.func().read().await? as u64;
    let offset = read_calls(ctx, entry).await?;
    ensure!(
        recipe::allocation(ctx, menu).await? == menu_stamp
            && recipe::allocation(ctx, consumer.address()).await? == stamp
            && consumer.func().read().await? as u64 == entry,
        "Native save-menu consumer changed during lookup"
    );
    eprintln!("Recipe persistence resolved the native save-label getter.");
    Ok(offset)
}

fn copy_getter(code: &[u8]) -> Option<u64> {
    // Both encodings copy an FString field at [rcx + displacement] to the caller's buffer.
    const ENTRY: [u8; 11] = [0x53, 0x48, 0x83, 0xec, 0x20, 0x48, 0x8b, 0xda, 0x48, 0x8d, 0x91];
    let code = code.strip_prefix(&[0x40]).unwrap_or(code).strip_prefix(&ENTRY)?;
    let offset = u32::from_le_bytes(code.get(..4)?.try_into().ok()?) as u64;
    if !(0x100..=0x4000).contains(&offset)
        || !offset.is_multiple_of(8)
        || code.get(4..8)? != [0x48, 0x8b, 0xcb, 0xe8]
        || code.get(12..21)? != [0x48, 0x8b, 0xc3, 0x48, 0x83, 0xc4, 0x20, 0x5b, 0xc3]
    {
        return None;
    }
    Some(offset)
}

async fn read_calls(ctx: &Ctx, entry: u64) -> Result<u64> {
    let image = ctx.image_base_address;
    let in_image = |address: u64| address >= image && address - image < 0x20000000;
    let mut queue = vec![(entry, 0u8)];
    let mut visited = BTreeSet::new();
    let mut offsets = BTreeSet::new();
    while let Some((address, depth)) = queue.pop() {
        if !visited.insert(address) {
            continue;
        }
        ensure!(visited.len() <= 64 && in_image(address), "Save-menu call graph bound");
        let size = match depth {
            0 => 192,
            1 => 4096,
            _ => 64,
        };
        let mut code = vec![0; size];
        ctx.read_buf(address, &mut code).await?;
        if let Some(offset) = copy_getter(&code) {
            offsets.insert(offset);
            continue;
        }
        if depth >= 2 {
            continue;
        }
        let mut blocks = vec![0usize];
        let mut instructions = BTreeSet::new();
        while let Some(offset) = blocks.pop() {
            let mut decoder = Decoder::with_ip(
                64,
                &code[offset..],
                address + offset as u64,
                DecoderOptions::NONE,
            );
            while decoder.can_decode() {
                let instruction = decoder.decode();
                if instruction.is_invalid()
                    || !instructions.insert(instruction.ip())
                    || instruction.mnemonic() == Mnemonic::Ret
                {
                    break;
                }
                let target = instruction.near_branch_target();
                if instruction.mnemonic() == Mnemonic::Call
                    && instruction.op0_kind() == OpKind::NearBranch64
                    && in_image(target)
                {
                    queue.push((target, depth + 1));
                }
                if instruction.is_jcc_short_or_near() || instruction.is_jmp_short_or_near() {
                    if (address..address + size as u64).contains(&target) {
                        blocks.push((target - address) as usize);
                    } else if instruction.is_jmp_short_or_near() && in_image(target) {
                        queue.push((target, depth + 1));
                    }
                    if instruction.is_jmp_short_or_near() {
                        break;
                    }
                }
            }
        }
    }
    ensure!(offsets.len() == 1, "Native save-label getter missing or ambiguous");
    offsets.pop_first().context("Native save-label getter unavailable")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_the_calibrated_string_copy_shape() {
        let mut getter = [
            0x40, 0x53, 0x48, 0x83, 0xec, 0x20, 0x48, 0x8b, 0xda, 0x48, 0x8d, 0x91, 0xa0, 0x02, 0,
            0, 0x48, 0x8b, 0xcb, 0xe8, 0, 0, 0, 0, 0x48, 0x8b, 0xc3, 0x48, 0x83, 0xc4, 0x20, 0x5b,
            0xc3,
        ];
        assert_eq!(copy_getter(&getter), Some(0x2a0));
        assert_eq!(copy_getter(&getter[1..]), Some(0x2a0));
        assert_eq!(copy_getter(&getter[..30]), None);
        getter[12] = 0xa1;
        assert_eq!(copy_getter(&getter), None);
        getter[12] = 0xa8;
        assert_eq!(copy_getter(&getter), Some(0x2a8));
        getter[31] = 0x90;
        assert_eq!(copy_getter(&getter), None);
    }
}
