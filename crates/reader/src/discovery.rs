//! Discover reflected fields and the unique local controller for the supported build.
use crate::{Cache, FIELDS, key, path, platform, stamp, validate_offsets};
use anyhow::{Context, Result, ensure};
use jmap_dumper::{
    ConfigOverrides,
    mem::Ptr,
    objects::{FUObjectArray, UStruct},
};
use std::collections::{BTreeMap, HashSet};

pub(super) async fn discover(process: &platform::Process) -> Result<Cache> {
    eprintln!("Discovering current Unreal runtime references...");
    let config = {
        let image = crate::process_image::read(process).await?;
        let memory = crate::discovery_memory::DiscoveryMemory::new(process.clone());
        jmap_dumper::resolve_config(&memory, &image, &ConfigOverrides::default()).await?
    };
    let memory = crate::discovery_memory::DiscoveryMemory::new(process.clone());
    ensure!(
        config.engine_version == (5, 6)
            && config.build_change_list.as_deref() == Some("++Earth20+Neon-CL-127004"),
        "Unsupported engine/build"
    );
    let ctx = jmap_dumper::connect_manual(memory, config.clone(), None).await?;
    let array = Ptr::<FUObjectArray>::new(ctx.uobjectarray, ctx.clone())?;
    let count = array.num_elements().await?;
    ensure!((1..=2_000_000).contains(&count), "Invalid Unreal registry size");
    let wanted: HashSet<&str> =
        FIELDS.iter().map(|(c, _, _)| c.rsplit('.').next().unwrap()).collect();
    let mut offsets = BTreeMap::new();
    let mut candidates = vec![];
    for index in 0..count as usize {
        let Some(obj) = array
            .read_item_ptr(index)
            .await
            .with_context(|| format!("Registry slot {index}/{count}"))?
        else {
            continue;
        };
        // Unreal's global array can contain stale entries while objects are collected.
        if obj.internal_index().read().await.ok() != Some(index as i32) {
            continue;
        }
        let name = obj
            .name_private()
            .read()
            .await
            .with_context(|| format!("Registry object name at {index}"))?;
        if wanted.contains(name.as_str()) {
            let p = path(&ctx, obj.address()).await?;
            if FIELDS.iter().any(|(c, _, _)| *c == p) {
                let mut props = obj.cast::<UStruct>().properties(false);
                let mut visits = 0;
                while let Some(prop) = props.next().await {
                    visits += 1;
                    ensure!(visits <= 4096, "Invalid reflected property chain");
                    let prop = prop.with_context(|| format!("Reflected property chain for {p}"))?;
                    let n = prop.zfield().name_private().read().await?;
                    if let Some((_, _, size)) = FIELDS.iter().find(|(c, f, _)| *c == p && *f == n) {
                        ensure!(
                            prop.element_size().read().await? as usize == *size
                                && prop.array_dim().read().await? == 1,
                            "Unexpected field layout"
                        );
                        offsets.insert(key(&p, &n), prop.offset_internal().read().await? as usize);
                    }
                }
            }
        }
        if name.starts_with("PC_PlayerControllerBase_C_") {
            candidates.push(obj.address());
        }
    }
    validate_offsets(&offsets).context("Resolving required native fields")?;
    let mut locals = vec![];
    for address in candidates {
        let player: u64 = ctx
            .read(address + offsets[&key("/Script/Engine.PlayerController", "Player")] as u64)
            .await?;
        if player != 0 {
            let reverse: u64 = ctx
                .read(player + offsets[&key("/Script/Engine.Player", "PlayerController")] as u64)
                .await?;
            if reverse == address {
                locals.push(address);
            }
        }
    }
    ensure!(
        locals.len() == 1,
        "Expected exactly one local controller; world not loaded or ambiguous"
    );
    Ok(Cache {
        version: 1,
        identity: process.identity.clone(),
        config,
        controller: stamp(&ctx, locals[0]).await.context("Validating discovered controller")?,
        offsets,
    })
}
