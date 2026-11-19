//! Finite read-only evidence for native wave fields; never invokes game code.
use super::*;
use iced_x86::{Decoder, DecoderOptions, Formatter, IntelFormatter};
use serde_json::{Value, json};
use std::sync::Arc;

async fn code(ctx: &Ctx, address: u64, image: u64, size: u64) -> Result<Value> {
    ensure!(
        address >= image && address.checked_add(512).is_some_and(|end| end <= image + size),
        "Native code outside image"
    );
    let mut bytes = [0u8; 512];
    ctx.read_buf(address, &mut bytes).await?;
    let mut decoder = Decoder::with_ip(64, &bytes, address, DecoderOptions::NONE);
    let mut formatter = IntelFormatter::new();
    let mut instructions = Vec::new();
    let mut calls = Vec::new();
    while decoder.can_decode() {
        let instruction = decoder.decode();
        let mut text = String::new();
        formatter.format(&instruction, &mut text);
        instructions.push(json!({"rva":instruction.ip()-image,"text":text}));
        if instruction.is_call_near() || instruction.is_jmp_short_or_near() {
            let target = instruction.near_branch_target();
            if target >= image && target < image + size && calls.len() < 24 {
                calls.push(target);
            }
        }
        if instruction.mnemonic() == Mnemonic::Ret || instruction.is_jmp_short_or_near() {
            break;
        }
    }
    Ok(
        json!({"rva":address-image,"hex":bytes.iter().map(|b| format!("{b:02x}")).collect::<String>(),"instructions":instructions,"targets":calls}),
    )
}

async fn object(ctx: &Ctx, address: u64, image: u64, size: u64) -> Result<Value> {
    let before = stamp(ctx, address).await?;
    let mut bytes = vec![0u8; 0x260];
    ctx.read_buf(address, &mut bytes).await?;
    let mut words = Vec::new();
    for offset in (0x40..0x250).step_by(4) {
        let f = f32::from_le_bytes(bytes[offset..offset + 4].try_into()?);
        words.push(json!({"offset":offset,"u32":u32::from_le_bytes(bytes[offset..offset+4].try_into()?),"i64":i64::from_le_bytes(bytes[offset..offset+8].try_into()?),"f32":if f.is_finite(){Some(f)}else{None}}));
    }
    let mut fields = Vec::new();
    let class = Ptr::<UStruct>::new(before.class, ctx.clone())?;
    let mut properties = class.properties(false);
    while let Some(property) = properties.next().await {
        ensure!(fields.len() < 256, "Wave property bound");
        let p = property?;
        fields.push(json!({"name":p.zfield().name_private().read().await?,"offset":p.offset_internal().read().await?,"size":p.element_size().read().await?}));
    }
    let mut functions = Vec::new();
    let mut child = class.children().read().await?;
    let mut visited = HashSet::new();
    while let Some(function) = child {
        ensure!(
            visited.len() < 256 && visited.insert(function.address()),
            "Wave function chain bound"
        );
        let name = function.cast::<UObject>().name_private().read().await?;
        let pointer = function.cast::<UFunction>().func().read().await? as u64;
        let native = if pointer >= image && pointer < image + size {
            {
                let mut value = code(ctx, pointer, image, size).await?;
                let targets = value["targets"]
                    .as_array()
                    .context("Native targets")?
                    .iter()
                    .take(8)
                    .filter_map(Value::as_u64)
                    .collect::<Vec<_>>();
                let mut children = Vec::new();
                for target in targets {
                    children.push(code(ctx, target, image, size).await?);
                }
                value["callees"] = json!(children);
                Some(value)
            }
        } else {
            None
        };
        functions.push(json!({"name":name,"native":native}));
        child = function.next().read().await?;
    }
    let vtable = ctx.read::<u64>(address).await?;
    ensure!(vtable >= image && vtable + 0x400 <= image + size, "Wave vtable image bound");
    let mut virtuals = Vec::new();
    for slot in (0..0x400).step_by(8) {
        let pointer = ctx.read::<u64>(vtable + slot).await?;
        if pointer >= image && pointer + 512 <= image + size {
            virtuals.push(json!({"slot":slot,"code":code(ctx,pointer,image,size).await?}));
        }
    }
    ensure!(stamp(ctx, address).await? == before, "Wave object changed during probe");
    Ok(
        json!({"headerHex":bytes[..128].iter().map(|b|format!("{b:02x}")).collect::<String>(),"stamp":before,"classSize":class.properties_size().read().await?,"fields":fields,"functions":functions,"virtuals":virtuals,"words":words}),
    )
}

pub(crate) fn run(directory: &FilePath) -> Result<()> {
    ensure!(directory.is_absolute(), "Absolute private diagnostic directory required");
    ensure!(!platform::is_elevated()?, "Do not elevate diagnostic");
    std::fs::create_dir_all(directory)?;
    let stop = Arc::new(starrupture_observation_model::shutdown::StopSignal::default());
    let budget = platform::ReadBudget::new(stop);
    budget.renew(60000);
    let process = platform::find_game(budget)?;
    let cached = crate::cache::read_json::<crate::Cache>(&directory.join("session.json"), 100000)
        .ok()
        .filter(|r| crate::cache_matches(r, &process.identity));
    let result = smol::block_on(async {
        let record = match cached {
            Some(r) => r,
            None => crate::discover(&process).await?,
        };
        let base = RuntimeCache::new(record)?;
        crate::sample(&process, &base).await?;
        let bindings = discover(&process, &base).await?;
        bindings.validate(&process.identity)?;
        let ctx = jmap_dumper::connect_manual(process.clone(), base.config.clone(), None).await?;
        let (image, size) = process.main_module()?;
        let timer = object(&ctx, bindings.timer_subsystem.address, image, size as u64).await?;
        let mut phase = object(&ctx, bindings.wave_subsystem.address, image, size as u64).await?;
        let pointer = ctx.read::<u64>(bindings.wave_subsystem.address + 0x1f8).await?;
        let probe_runtime = RuntimeBindings::new(&bindings)?;
        let (_, probe_level) = world(&ctx, &base, &probe_runtime).await?;
        let probe_settings =
            ctx.read::<u64>(probe_level + probe_runtime.fields.world_settings).await?;
        phase["activeRead"] = match crate::live_phase::read_with_clock(
            &ctx,
            bindings.wave_subsystem.address,
            image,
            probe_settings,
            &probe_runtime.phase_clock,
        )
        .await
        {
            Ok((_, value)) => json!({"value":format!("{value:?}")}),
            Err(error) => json!({"error":format!("{error:#}")}),
        };
        if pointer != 0 {
            ensure!(
                (0x10000..0x800000000000).contains(&pointer) && pointer.is_multiple_of(8),
                "Active implementation pointer"
            );
            let vt = ctx.read::<u64>(pointer).await?;
            ensure!(vt >= image && vt + 0x40 <= image + size as u64, "Active vtable image bound");
            let mut data = vec![0u8; 128];
            ctx.read_buf(pointer, &mut data).await?;
            let mut methods = Vec::new();
            for slot in (0..0x38).step_by(8) {
                let function = ctx.read::<u64>(vt + slot).await?;
                methods
                    .push(json!({"slot":slot,"code":code(&ctx,function,image,size as u64).await?}));
            }
            let owner = ctx.read::<u64>(pointer + 8).await?;
            ensure!(
                (0x10000..0x800000000000).contains(&owner) && owner.is_multiple_of(8),
                "Phase owner pointer"
            );
            let owner_vt = ctx.read::<u64>(owner).await?;
            let mut owner_data = vec![0u8; 128];
            ctx.read_buf(owner, &mut owner_data).await?;
            let mut owner_methods = Vec::new();
            if owner_vt >= image && owner_vt + 8 * 16 <= image + size as u64 {
                for slot in (0..8 * 16).step_by(8) {
                    let f = ctx.read::<u64>(owner_vt + slot).await?;
                    if f >= image && f + 512 <= image + size as u64 {
                        owner_methods.push(
                            json!({"slot":slot,"code":code(&ctx,f,image,size as u64).await?}),
                        );
                    }
                }
            }
            phase["implementationOwner"] = json!({"pointer":owner,"vtable":owner_vt,"hex":owner_data.iter().map(|b|format!("{b:02x}")).collect::<String>(),"methods":owner_methods});
            phase["implementation"] = json!({"pointer":pointer,"vtableRva":vt-image,"hex":data.iter().map(|b|format!("{b:02x}")).collect::<String>(),"methods":methods});
            ensure!(
                ctx.read::<u64>(bindings.wave_subsystem.address + 0x1f8).await? == pointer
                    && ctx.read::<u64>(pointer).await? == vt,
                "Active phase changed during diagnostic"
            );
        }
        let mut tables = Vec::new();
        for offset in (0x28..0x68).step_by(8) {
            let vt = ctx.read::<u64>(bindings.wave_subsystem.address + offset).await?;
            if vt >= image && vt + 8 * 12 <= image + size as u64 {
                let mut methods = Vec::new();
                for slot in (0..8 * 12).step_by(8) {
                    let f = ctx.read::<u64>(vt + slot).await?;
                    if f >= image && f + 512 <= image + size as u64 {
                        methods.push(
                            json!({"slot":slot,"code":code(&ctx,f,image,size as u64).await?}),
                        );
                    }
                }
                tables.push(json!({"offset":offset,"vtableRva":vt-image,"methods":methods}));
            }
        }
        phase["secondaryTables"] = json!(tables);
        let controller_vt = ctx.read::<u64>(base.controller.address).await?;
        let runtime = RuntimeBindings::new(&bindings)?;
        let (_, level) = world(&ctx, &base, &runtime).await?;
        let settings = ctx.read::<u64>(level + runtime.fields.world_settings).await?;
        let svt = ctx.read::<u64>(settings).await?;
        phase["worldSettings778"] =
            code(&ctx, ctx.read::<u64>(svt + 0x778).await?, image, size as u64).await?;
        phase["worldSettingsStamp"] = json!(stamp(&ctx, settings).await?);
        phase["controller778"] =
            code(&ctx, ctx.read::<u64>(controller_vt + 0x778).await?, image, size as u64).await?;
        let world_class = Ptr::<UObject>::new(bindings.world.address, ctx.clone())?
            .class_private()
            .read()
            .await?;
        let mut props = world_class.cast::<UStruct>().properties(false);
        let mut world_properties = Vec::new();
        while let Some(property) = props.next().await {
            ensure!(world_properties.len() < 512, "World diagnostic field bound");
            let property = property?;
            let name = property.zfield().name_private().read().await?;
            if name == "AuthorityGameMode" || name == "GameState" {
                let offset = property.offset_internal().read().await? as u64;
                let address = ctx.read::<u64>(bindings.world.address + offset).await?;
                let mut entry = json!({"name":name,"offset":offset,"address":address});
                if address != 0 {
                    entry["stamp"] = json!(stamp(&ctx, address).await?);
                    let vt = ctx.read::<u64>(address).await?;
                    entry["getter778"] =
                        code(&ctx, ctx.read::<u64>(vt + 0x778).await?, image, size as u64).await?;
                }
                world_properties.push(entry);
            }
        }
        phase["worldGameplayActors"] = json!(world_properties);
        process.ensure_alive()?;
        Ok::<_, anyhow::Error>(
            json!({"readOnly":true,"gameCodeInvoked":false,"identity":process.identity,"config":base.config,"bindings":bindings,"timer":timer,"phase":phase}),
        )
    })?;
    let bytes = serde_json::to_vec_pretty(&result)?;
    ensure!(bytes.len() <= 16 * 1024 * 1024, "Wave diagnostic output bound");
    std::fs::write(directory.join("wave-layout.json"), bytes)?;
    Ok(())
}
