mod live_context;
mod live_phase;
mod live_timer_config;
mod platform;
use anyhow::{Context, Result, ensure};
use jmap_dumper::{
    Config, ConfigOverrides,
    mem::{BlockCache, Ctx, Mem, Ptr},
    objects::{FUObjectArray, UObject, UStruct},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::{BTreeMap, HashSet},
    time::{Instant, SystemTime, UNIX_EPOCH},
};
const BUILD_HASH: &str = "95f95eee0b1913b8716de7fa5bf19877a91abe86e63bf2b6f0a29bd48d1d76f1";
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
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct Position {
    x: f64,
    y: f64,
    z: f64,
}
fn key(class: &str, field: &str) -> String {
    format!("{class}:{field}")
}
fn validate_offsets(offsets: &BTreeMap<String, usize>) -> Result<()> {
    for &(class, field, _) in FIELDS {
        let off = offsets
            .get(&key(class, field))
            .context("Missing reflected field")?;
        ensure!((8..=262144).contains(off), "Invalid reflected offset");
    }
    Ok(())
}
fn cache_matches(cache: &Cache, id: &Identity) -> bool {
    cache.version == 1 && cache.identity == *id && validate_offsets(&cache.offsets).is_ok()
}
fn decode_position(bytes: &[u8]) -> Result<Position> {
    ensure!(bytes.len() == 24, "Position read has wrong length");
    let v: Vec<f64> = bytes
        .chunks_exact(8)
        .map(|b| f64::from_le_bytes(b.try_into().unwrap()))
        .collect();
    ensure!(
        v.iter().all(|x| x.is_finite() && x.abs() < 1e9),
        "Invalid coordinate values"
    );
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
        ensure!(
            seen.insert(obj.address()) && names.len() < 32,
            "Cyclic/deep outer chain"
        );
        names.push(obj.name_private().read().await?);
        current = obj.outer_private().read().await?;
    }
    names.reverse();
    ensure!(!names.is_empty(), "Missing object path");
    let mut out = names.remove(0);
    for (i, n) in names.iter().enumerate() {
        out.push(if i == 0 { '.' } else { ':' });
        out.push_str(n);
    }
    Ok(out)
}
async fn discover(process: &platform::Process) -> Result<Cache> {
    eprintln!("Discovering current Unreal runtime references...");
    let image = patternsleuth::process::external::read_image_from_pid(process.identity.pid as i32)?;
    let memory = BlockCache::wrap(process.clone());
    let config = jmap_dumper::resolve_config(&memory, &image, &ConfigOverrides::default()).await?;
    ensure!(
        config.engine_version == (5, 6)
            && config.build_change_list.as_deref() == Some("++Earth20+Neon-CL-127004"),
        "Unsupported engine/build"
    );
    let ctx = jmap_dumper::connect_manual(memory, config.clone(), None).await?;
    let array = Ptr::<FUObjectArray>::new(ctx.uobjectarray, ctx.clone())?;
    let count = array.num_elements().await?;
    ensure!(
        (1..=2_000_000).contains(&count),
        "Invalid Unreal registry size"
    );
    let wanted: HashSet<&str> = FIELDS
        .iter()
        .map(|(c, _, _)| c.rsplit('.').next().unwrap())
        .collect();
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
        controller: stamp(&ctx, locals[0])
            .await
            .context("Validating discovered controller")?,
        offsets,
    })
}
async fn sample(process: &platform::Process, cache: &Cache) -> Result<serde_json::Value> {
    ensure!(
        cache_matches(cache, &process.identity),
        "Stale session cache"
    );
    let ctx = jmap_dumper::connect_manual(process.clone(), cache.config.clone(), None).await?;
    sample_context(process,cache,ctx).await
}
async fn sample_context(process:&platform::Process,cache:&Cache,ctx:Ctx)->Result<serde_json::Value> {
    ensure!(cache_matches(cache,&process.identity),"Stale session cache");
    ensure!(
        stamp(&ctx, cache.controller.address).await? == cache.controller,
        "Controller identity changed"
    );
    let off = |c: &str, f: &str| cache.offsets[&key(c, f)] as u64;
    let controller = cache.controller.address;
    let player: u64 = ctx
        .read(controller + off("/Script/Engine.PlayerController", "Player"))
        .await?;
    stamp(&ctx, player).await?;
    ensure!(
        ctx.read::<u64>(player + off("/Script/Engine.Player", "PlayerController"))
            .await?
            == controller,
        "Local player roundtrip failed"
    );
    let pawn: u64 = ctx
        .read(controller + off("/Script/Engine.Controller", "Pawn"))
        .await?;
    let character: u64 = ctx
        .read(controller + off("/Script/Engine.Controller", "Character"))
        .await?;
    let ack: u64 = ctx
        .read(controller + off("/Script/Engine.PlayerController", "AcknowledgedPawn"))
        .await?;
    ensure!(
        pawn != 0 && pawn == character && ack == pawn,
        "Possession unsupported/changing; character not established"
    );
    let char_stamp = stamp(&ctx, character).await?;
    ensure!(
        char_stamp.name.starts_with("BP_PlayerCharacter_C_"),
        "Unsupported character type"
    );
    let root: u64 = ctx
        .read(character + off("/Script/Engine.Actor", "RootComponent"))
        .await?;
    let root_stamp = stamp(&ctx, root).await?;
    ensure!(
        ctx.read::<u64>(root + off("/Script/Engine.SceneComponent", "AttachParent"))
            .await?
            == 0,
        "Attached root: world coordinates not established"
    );
    let mut bytes = [0; 24];
    ctx.read_buf(
        root + off("/Script/Engine.SceneComponent", "RelativeLocation"),
        &mut bytes,
    )
    .await?;
    let first = decode_position(&bytes)?;
    let char_path = path(&ctx, character).await?;
    let package = char_path
        .split('.')
        .next()
        .context("Missing world package")?;
    ensure!(
        package == "/Game/Chimera/Maps/ChimeraMain/ChimeraMain",
        "Unsupported current world frame; refusing map coordinates"
    );
    ctx.read_buf(
        root + off("/Script/Engine.SceneComponent", "RelativeLocation"),
        &mut bytes,
    )
    .await?;
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
        ctx.read::<u64>(controller + off("/Script/Engine.Controller", "Pawn"))
            .await?
            == pawn
            && ctx
                .read::<u64>(character + off("/Script/Engine.Actor", "RootComponent"))
                .await?
                == root
            && stamp(&ctx, character).await? == char_stamp
            && stamp(&ctx, root).await? == root_stamp,
        "References changed during sample"
    );
    process.ensure_alive()?;
    Ok(
        json!({"status":"ok","schemaVersion":1,"helperVersion":env!("CARGO_PKG_VERSION"),"observedAtUnixMilliseconds":SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis(),"source":"live_read_only_process_memory","position":position,"process":process.identity,"readerIsElevated":platform::is_elevated()?,"frame":{"worldPackage":package,"characterPath":char_path,"map":"ChimeraMain","worldUnitsPerMetre":100,"originShiftVerified":false},"validation":{"localPlayerRoundtrip":true,"registeredObjects":true,"unattachedRoot":true,"stableWithinSample":first==position},"limitations":["Main-map frame only; world-origin shifting independently unverified","Character position, not camera/drone position","No save snapshot or factory facts are refreshed"],"gameMemoryWrites":false}),
    )
}
fn run() -> Result<serde_json::Value> {
    let start = Instant::now();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut cache_directory = None;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--cache-dir" {
            ensure!(cache_directory.is_none(), "Duplicate --cache-dir");
            i += 1;
            let path = args
                .get(i)
                .context("--cache-dir requires an absolute directory")?;
            let path = std::path::PathBuf::from(path);
            ensure!(
                path.is_absolute(),
                "--cache-dir requires an absolute directory"
            );
            cache_directory = Some(path);
        } else {
            ensure!(
                ["--fresh", "--help", "--live"].contains(&args[i].as_str()),
                "Supported arguments: --fresh, --help, --live, --cache-dir DIRECTORY"
            );
        }
        i += 1;
    }
    if args.iter().any(|a| a == "--help") {
        return Ok(
            json!({"usage":"starrupture-live.exe [--fresh] [--live] [--cache-dir ABSOLUTE_DIRECTORY]","operation":"One read-only query, process-specific discovery cache. --live adds validated player/world/clock/pause/waiting-wave context. No watcher or service."}),
        );
    }
    // Discovery must not leave a caller waiting indefinitely on a damaged/runtime-changing layout.
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_secs(60));
        println!(
            "{}",
            json!({"status":"unavailable","schemaVersion":1,"position":null,"error":"Coordinate query timed out after 60 seconds","gameMemoryWrites":false})
        );
        std::process::exit(2);
    });
    let process = platform::find_game()?;
    ensure!(
        process.identity.executable_sha256 == BUILD_HASH,
        "Unsupported game executable hash; revalidate new build"
    );
    let dir = cache_directory.unwrap_or(
        std::env::current_exe()?
            .parent()
            .context("Executable directory")?
            .join("session-cache"),
    );
    std::fs::create_dir_all(&dir)?;
    let file = dir.join("session.json");
    let cached: Option<Cache> = if args.iter().any(|a| a == "--fresh") {
        None
    } else {
        std::fs::read(&file)
            .ok()
            .filter(|b| b.len() <= 100000)
            .and_then(|b| serde_json::from_slice(&b).ok())
    };
    jmap_dumper::diag::set_verbosity(jmap_dumper::diag::Verbosity::Quiet);
    smol::block_on(async {
        let mut hit = false;
        let (cache, mut value) =
            if let Some(cache) = cached.filter(|c| cache_matches(c, &process.identity)) {
                match sample(&process, &cache).await {
                    Ok(v) => {
                        hit = true;
                        (cache, v)
                    }
                    Err(e) => {
                        eprintln!("Cached references invalid ({e}); rediscovering once.");
                        let c = discover(&process).await?;
                        let v = sample(&process, &c).await?;
                        (c, v)
                    }
                }
            } else {
                let c = discover(&process).await?;
                let v = sample(&process, &c).await?;
                (c, v)
            };
        if !hit {
            let tmp = dir.join(format!("session-{}.tmp", std::process::id()));
            std::fs::write(&tmp, serde_json::to_vec_pretty(&cache)?)?;
            if file.exists() {
                std::fs::remove_file(&file)?;
            }
            std::fs::rename(tmp, &file)?;
        }
        if args.iter().any(|a| a == "--live") {
            if args.iter().any(|a| a == "--fresh") {
                let live_file = dir.join("live-context.json");
                if live_file.exists() {
                    std::fs::remove_file(live_file)?;
                }
            }
            let live = live_context::snapshot(&process, &cache, &dir).await?;
            value = sample(&process, &cache).await?;
            value["live"] = live;
        }
        value["process"]["creation_ticks"] = json!(process.identity.creation_ticks.to_string());
        value["observedAtUnixMilliseconds"] =
            json!(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis());
        value["discovery"] = json!({"cacheHit":hit,"durationMilliseconds":start.elapsed().as_millis(),"library":"jmap_dumper0.2.0; targeted reflection scan, no full dump"});
        Ok(value)
    })
}
#[allow(dead_code)]
fn finite_main() {
    let (value, exit) = match std::panic::catch_unwind(run) {
        Ok(Ok(v)) => (v, 0),
        Ok(Err(e)) => (
            json!({"status":"unavailable","schemaVersion":1,"position":null,"error":format!("{e:#}"),"gameMemoryWrites":false}),
            2,
        ),
        Err(_) => (
            json!({"status":"unavailable","schemaVersion":1,"position":null,"error":"Runtime discovery failed unexpectedly; no coordinates returned","gameMemoryWrites":false}),
            2,
        ),
    };
    println!("{}", value);
    std::process::exit(exit);
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
    }
}
