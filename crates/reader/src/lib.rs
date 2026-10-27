//! Embedded, bounded read-only wave reader; never writes or invokes game code.
mod bindings;
mod cache;
mod cadence;
mod discovery;
mod error;
mod live_context;
mod live_phase;
mod live_timer_config;
pub mod overlay_stream;
mod platform;
pub mod recipe;

use anyhow::{Context, Result, ensure};
use bindings::RuntimeCache;
use discovery::discover;
use jmap_dumper::{
    Config,
    mem::{Ctx, Mem, Ptr},
    objects::{FUObjectArray, UObject},
};
use serde::{Deserialize, Serialize};
use starrupture_observation_model::{
    observation::{PlayerSample, Position},
    overlay_stream::GAME_HASH as BUILD_HASH,
};
use std::{
    collections::{BTreeMap, HashSet},
    time::{SystemTime, UNIX_EPOCH},
};
const FIELDS: &[(&str, &str, usize)] = &[
    ("/Script/Engine.PlayerController", "Player", 8),
    ("/Script/Engine.PlayerController", "AcknowledgedPawn", 8),
    ("/Script/Engine.Controller", "Pawn", 8),
    ("/Script/Engine.Controller", "Character", 8),
    ("/Script/Engine.Player", "PlayerController", 8),
    ("/Script/Engine.Actor", "RootComponent", 8),
    ("/Script/Engine.SceneComponent", "AttachParent", 8),
    ("/Script/Engine.SceneComponent", "RelativeLocation", 24),
];
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct Identity {
    pid: u32,
    creation_ticks: u64,
    executable: String,
    executable_sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct ObjectStamp {
    address: u64,
    index: i32,
    class: u64,
    name: String,
}
#[derive(Serialize, Deserialize)]
struct Cache {
    version: u32,
    identity: Identity,
    config: Config,
    controller: ObjectStamp,
    offsets: BTreeMap<String, usize>,
}
fn key(class: &str, field: &str) -> String {
    format!("{class}:{field}")
}
fn validate_offsets(offsets: &BTreeMap<String, usize>) -> Result<()> {
    for &(class, field, _) in FIELDS {
        let off = offsets.get(&key(class, field)).context("Missing reflected field")?;
        ensure!((8..=262144).contains(off), "Invalid reflected offset");
    }
    Ok(())
}
fn cache_matches(cache: &Cache, id: &Identity) -> bool {
    cache.version == 1 && cache.identity == *id && validate_offsets(&cache.offsets).is_ok()
}
fn decode_position(bytes: &[u8]) -> Result<Position> {
    ensure!(bytes.len() == 24, "Position read has wrong length");
    let v: [f64; 3] = std::array::from_fn(|i| f64::from_le_bytes(bytes.as_chunks::<8>().0[i]));
    ensure!(v.iter().all(|x| x.is_finite() && x.abs() < 1e9), "Invalid coordinate values");
    Ok(Position {
        x: v[0],
        y: v[1],
        z: v[2],
    })
}
async fn stamp(ctx: &Ctx, address: u64) -> Result<ObjectStamp> {
    let obj = Ptr::<UObject>::new(address, ctx.clone())?;
    let index = obj.internal_index().read().await?;
    let array = Ptr::<FUObjectArray>::new(ctx.uobjectarray, ctx.clone())?;
    let count = array.num_elements().await?;
    ensure!(
        (1..=2_000_000).contains(&count) && index >= 0 && index < count,
        "Object index outside live registry"
    );
    ensure!(
        array
            .read_item_ptr(index as usize)
            .await?
            .context("Object no longer registered")?
            .address()
            == address,
        "Object registry address changed"
    );
    ensure!(
        obj.object_flags().read().await?.bits() & (0x10 | 0x20 | 0x8000 | 0x10000) == 0,
        "Template or destroyed object"
    );
    Ok(ObjectStamp {
        address,
        index,
        class: obj.class_private().read().await?.address(),
        name: obj.name_private().read().await?,
    })
}
async fn path(ctx: &Ctx, address: u64) -> Result<String> {
    let mut current = Some(Ptr::<UObject>::new(address, ctx.clone())?);
    let mut names = vec![];
    let mut seen = HashSet::new();
    while let Some(obj) = current {
        ensure!(seen.insert(obj.address()) && names.len() < 32, "Cyclic/deep outer chain");
        names.push(obj.name_private().read().await?);
        current = obj.outer_private().read().await?;
    }
    names.reverse();
    ensure!(!names.is_empty(), "Missing object path");
    let mut out = names.remove(0);
    for (i, n) in names.iter().enumerate() {
        out.push(if i == 0 {
            '.'
        } else {
            ':'
        });
        out.push_str(n);
    }
    Ok(out)
}
async fn sample(process: &platform::Process, cache: &RuntimeCache) -> Result<PlayerSample> {
    ensure!(cache.matches(&process.identity), "Stale session cache");
    let ctx = jmap_dumper::connect_manual(process.clone(), cache.config.clone(), None).await?;
    sample_context(process, cache, ctx).await
}
async fn sample_context(
    process: &platform::Process,
    cache: &RuntimeCache,
    ctx: Ctx,
) -> Result<PlayerSample> {
    ensure!(cache.matches(&process.identity), "Stale session cache");
    ensure!(
        stamp(&ctx, cache.controller.address).await? == cache.controller,
        "Controller identity changed"
    );
    let controller = cache.controller.address;
    let player: u64 = ctx.read(controller + cache.fields.player).await?;
    stamp(&ctx, player).await?;
    ensure!(
        ctx.read::<u64>(player + cache.fields.player_controller).await? == controller,
        "Local player roundtrip failed"
    );
    let pawn: u64 = ctx.read(controller + cache.fields.pawn).await?;
    let character: u64 = ctx.read(controller + cache.fields.character).await?;
    let ack: u64 = ctx.read(controller + cache.fields.acknowledged_pawn).await?;
    ensure!(
        pawn != 0 && pawn == character && ack == pawn,
        "Possession unsupported/changing; character not established"
    );
    let char_stamp = stamp(&ctx, character).await?;
    ensure!(char_stamp.name.starts_with("BP_PlayerCharacter_C_"), "Unsupported character type");
    let root: u64 = ctx.read(character + cache.fields.root_component).await?;
    let root_stamp = stamp(&ctx, root).await?;
    ensure!(
        ctx.read::<u64>(root + cache.fields.attach_parent).await? == 0,
        "Attached root: world coordinates not established"
    );
    let mut bytes = [0; 24];
    ctx.read_buf(root + cache.fields.relative_location, &mut bytes).await?;
    let first = decode_position(&bytes)?;
    let char_path = path(&ctx, character).await?;
    let package = char_path.split('.').next().context("Missing world package")?;
    ensure!(
        package == "/Game/Chimera/Maps/ChimeraMain/ChimeraMain",
        "Unsupported current world frame; refusing map coordinates"
    );
    ctx.read_buf(root + cache.fields.relative_location, &mut bytes).await?;
    let position = decode_position(&bytes)?;
    ensure!(
        ((first.x - position.x).powi(2)
            + (first.y - position.y).powi(2)
            + (first.z - position.z).powi(2))
        .sqrt()
            < 1000.,
        "Position changed too quickly; retry"
    );
    ensure!(
        ctx.read::<u64>(controller + cache.fields.pawn).await? == pawn
            && ctx.read::<u64>(character + cache.fields.root_component).await? == root
            && stamp(&ctx, character).await? == char_stamp
            && stamp(&ctx, root).await? == root_stamp,
        "References changed during sample"
    );
    process.ensure_alive()?;
    Ok(PlayerSample {
        position,
        observed_at_ms: SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64,
        reader_is_elevated: platform::is_elevated()?,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_coordinates() {
        assert!(decode_position(&[0; 23]).is_err());
        let mut b = [0; 24];
        b[..8].copy_from_slice(&f64::NAN.to_le_bytes());
        assert!(decode_position(&b).is_err());
        b[..8].copy_from_slice(&1e10f64.to_le_bytes());
        assert!(decode_position(&b).is_err());
    }
    #[test]
    fn negative_coordinates() {
        let mut b = [0; 24];
        b[..8].copy_from_slice(&(-269215.545f64).to_le_bytes());
        assert_eq!(
            decode_position(&b).unwrap(),
            Position {
                x: -269215.545,
                y: 0.,
                z: 0.
            }
        );
    }
    #[test]
    fn session_identity() {
        let id = Identity {
            pid: 12,
            creation_ticks: 42,
            executable: "game.exe".into(),
            executable_sha256: BUILD_HASH.into(),
        };
        let c = Cache {
            version: 1,
            identity: id.clone(),
            config: Config {
                guobject_array: 1,
                fname_pool: 2,
                engine_version: (5, 6),
                image_base: 0,
                build_change_list: None,
                build_config: Default::default(),
                target_triplet: jmap_dumper::structs::default_target_triplet(),
            },
            controller: ObjectStamp {
                address: 65536,
                index: 1,
                class: 65536,
                name: "test".into(),
            },
            offsets: FIELDS.iter().map(|(c, f, _)| (key(c, f), 320)).collect(),
        };
        assert!(cache_matches(&c, &id));
        for changed in [
            Identity {
                pid: 13,
                ..id.clone()
            },
            Identity {
                creation_ticks: 43,
                ..id.clone()
            },
            Identity {
                executable: "other".into(),
                ..id.clone()
            },
            Identity {
                executable_sha256: "different".into(),
                ..id.clone()
            },
        ] {
            assert!(!cache_matches(&c, &changed));
        }
        let bytes = serde_json::to_vec(&c).unwrap();
        let runtime = RuntimeCache::new(c).unwrap();
        assert!(runtime.matches(&id));
        assert_eq!(serde_json::to_vec(&runtime.record).unwrap(), bytes);
        assert!(!runtime.matches(&Identity {
            pid: 13,
            ..id
        }));
    }
    #[test]
    fn vendored_windows_reader_copies_owned_process_memory() {
        let expected = Box::new([0x41u8, 0x07, 0xa5, 0x5a, 0x11, 0x22, 0x33, 0x44]);
        let reader = jmap_dumper::mem::ProcessHandle::new(std::process::id() as i32);
        let mut actual = [0u8; 8];
        smol::block_on(jmap_dumper::mem::Mem::read_buf(
            &reader,
            expected.as_ptr() as u64,
            &mut actual,
        ))
        .unwrap();
        assert_eq!(actual, *expected);
        assert!(smol::block_on(jmap_dumper::mem::Mem::read_buf(&reader, 0, &mut actual)).is_err());
    }
}
