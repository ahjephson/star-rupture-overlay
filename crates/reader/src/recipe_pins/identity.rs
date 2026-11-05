//! Loaded game-instance label plus an unambiguous read-only local save-folder identity.
use crate::{ObjectStamp, RuntimeCache, recipe};
use anyhow::{Context, Result, ensure};
use jmap_dumper::{
    mem::{Ctx, Ptr},
    objects::{UObject, UStruct},
};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};
pub(crate) struct Binding {
    world: (ObjectStamp, i32),
    instance: (ObjectStamp, i32),
    owner_offset: u64,
    network_offset: u64,
    label_offset: u64,
}
pub(crate) struct World {
    pub key: String,
    pub label: String,
}
fn label_offset(build: Option<&str>) -> Result<u64> {
    // Existing loaded-campaign calibration: this field is unreflected, not a
    // persistent GUID. Its explicit native-build scope is independent of timer support.
    match build {
        Some("++Earth20+Neon-CL-127004") => Ok(0x290),
        Some("++Earth20+Kobalt-PTR1-CL-128786") => Ok(0x2a0),
        _ => anyhow::bail!("Loaded-session layout is not calibrated for recipe persistence"),
    }
}
async fn native_class(ctx: &Ctx, mut class: u64, wanted: &str) -> Result<Ptr<UStruct>> {
    for _ in 0..32 {
        let object = Ptr::<UObject>::new(class, ctx.clone())?;
        if object.path().await? == wanted {
            return Ok(object.cast());
        }
        class = object
            .cast::<UStruct>()
            .super_struct()
            .read()
            .await?
            .context("Native identity class ancestry unavailable")?
            .address();
    }
    anyhow::bail!("Native identity class ancestry bound")
}
impl Binding {
    pub(crate) async fn discover(ctx: &Ctx, base: &RuntimeCache, world: u64) -> Result<Self> {
        let label_offset = label_offset(base.config.build_change_list.as_deref())?;
        let world = recipe::allocation(ctx, world).await?;
        let class = native_class(ctx, world.0.class, "/Script/Engine.World").await?;
        let size = class.properties_size().read().await?;
        ensure!((1..=262144).contains(&size), "World identity class size");
        let mut owner_offset = None;
        let mut network_offset = None;
        let mut props = class.properties(false);
        let mut visited = 0;
        while let Some(prop) = props.next().await {
            visited += 1;
            ensure!(visited <= 4096, "World identity field bound");
            let prop = prop?;
            let name = prop.zfield().name_private().read().await?;
            if name == "OwningGameInstance" || name == "NetDriver" {
                let off = prop.offset_internal().read().await?;
                ensure!(
                    off >= 0
                        && off + 8 <= size
                        && prop.element_size().read().await? == 8
                        && prop.array_dim().read().await? == 1,
                    "World identity reflected field"
                );
                if name == "OwningGameInstance" {
                    owner_offset = Some(off as u64)
                } else {
                    network_offset = Some(off as u64)
                }
            }
        }
        let owner_offset = owner_offset.context("World instance identity field")?;
        let network_offset = network_offset.context("World network identity field")?;
        let instance_address: u64 = ctx.read(world.0.address + owner_offset).await?;
        let instance = recipe::allocation(ctx, instance_address).await?;
        let class = native_class(ctx, instance.0.class, "/Script/Chimera.CrGameInstance").await?;
        let size = class.properties_size().read().await?;
        ensure!(
            (1..=262144).contains(&size) && label_offset + 16 <= size as u64,
            "Loaded session field bound"
        );
        Ok(Self {
            world,
            instance,
            owner_offset,
            network_offset,
            label_offset,
        })
    }
    pub(crate) async fn current(&self, ctx: &Ctx, world: u64, executable: &Path) -> Result<World> {
        ensure!(
            world == self.world.0.address
                && recipe::allocation(ctx, world).await? == self.world
                && ctx.read::<u64>(world + self.owner_offset).await? == self.instance.0.address
                && recipe::allocation(ctx, self.instance.0.address).await? == self.instance
                && ctx.read::<u64>(world + self.network_offset).await? == 0,
            "Loaded local world identity changed or is online"
        );
        let at = self.instance.0.address + self.label_offset;
        let (label, guard) = text(ctx, at).await?;
        ensure!(component(&label), "Invalid loaded campaign label");
        let folder = resolve_folder(&steam_roots(executable), &label)?;
        let key = folder_key(&folder)?;
        guard.check(ctx, at).await?;
        ensure!(
            recipe::allocation(ctx, world).await? == self.world
                && ctx.read::<u64>(world + self.owner_offset).await? == self.instance.0.address
                && recipe::allocation(ctx, self.instance.0.address).await? == self.instance
                && ctx.read::<u64>(world + self.network_offset).await? == 0,
            "Loaded local world changed during identity read"
        );
        Ok(World {
            key,
            label,
        })
    }
}
pub(super) fn component(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 240
        && s != "."
        && s != ".."
        && !s.ends_with(['.', ' '])
        && !s.chars().any(|c| {
            c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
        })
}
fn steam_roots(executable: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(root) = steam_registry() {
        roots.push(root);
    }
    for parent in executable.ancestors() {
        if parent.file_name().is_some_and(|n| n.eq_ignore_ascii_case("steamapps"))
            && let Some(root) = parent.parent()
        {
            roots.push(root.to_path_buf());
        }
    }
    roots.sort();
    roots.dedup();
    roots
}
fn steam_registry() -> Option<PathBuf> {
    use windows_sys::Win32::System::Registry::*;
    let key: Vec<_> = "Software\\Valve\\Steam".encode_utf16().chain([0]).collect();
    let value: Vec<_> = "SteamPath".encode_utf16().chain([0]).collect();
    let mut data = [0u16; 1024];
    let mut bytes = (data.len() * 2) as u32;
    // SAFETY: Fixed terminated key/value buffers and a bounded writable UTF-16 output
    // remain live for this synchronous read-only registry query. No key is opened or changed.
    let result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            data.as_mut_ptr().cast(),
            &mut bytes,
        )
    };
    if result != 0 || bytes < 2 || bytes as usize > data.len() * 2 || !bytes.is_multiple_of(2) {
        return None;
    }
    String::from_utf16(&data[..bytes as usize / 2 - 1]).ok().map(PathBuf::from)
}
pub(super) fn resolve_folder(roots: &[PathBuf], label: &str) -> Result<PathBuf> {
    ensure!(component(label), "Invalid session folder component");
    let mut candidates = Vec::new();
    for root in roots {
        let userdata = root.join("userdata");
        let Ok(accounts) = std::fs::read_dir(userdata) else {
            continue;
        };
        for (i, account) in accounts.enumerate() {
            ensure!(i < 64, "Steam account discovery bound");
            let account = account?;
            if !account.file_name().to_string_lossy().bytes().all(|b| b.is_ascii_digit()) {
                continue;
            }
            let saved = account.path().join("1631270/remote/Saved");
            let Ok(folders) = std::fs::read_dir(&saved) else {
                continue;
            };
            for (i, entry) in folders.enumerate() {
                ensure!(i < 32, "Save namespace discovery bound");
                let entry = entry?;
                if !entry.file_name().to_string_lossy().starts_with("SaveGames") {
                    continue;
                }
                let path = entry.path().join(label);
                if path.is_dir() {
                    candidates.push(path.canonicalize()?);
                }
            }
        }
    }
    candidates.sort();
    candidates.dedup();
    ensure!(candidates.len() == 1, "Local world save folder is missing or ambiguous");
    Ok(candidates.remove(0))
}
pub(super) fn folder_key(path: &Path) -> Result<String> {
    let path = path.canonicalize()?;
    let metadata = std::fs::metadata(&path)?;
    ensure!(metadata.is_dir(), "World path is not a directory");
    let created = metadata.created()?.duration_since(UNIX_EPOCH)?.as_nanos();
    Ok(format!(
        "{:x}",
        Sha256::digest(format!("{}\0{created}", path.to_string_lossy().to_lowercase()).as_bytes())
    ))
}
struct TextGuard {
    header: [u8; 16],
    bytes: Vec<u8>,
}
impl TextGuard {
    async fn check(&self, ctx: &Ctx, address: u64) -> Result<()> {
        let mut h = [0; 16];
        ctx.read_buf(address, &mut h).await?;
        let p = u64::from_le_bytes(h[..8].try_into()?);
        ensure!(h == self.header, "Session string allocation changed");
        let mut bytes = vec![0; self.bytes.len()];
        ctx.read_buf(p, &mut bytes).await?;
        ensure!(bytes == self.bytes, "Session string changed");
        Ok(())
    }
}
async fn text(ctx: &Ctx, address: u64) -> Result<(String, TextGuard)> {
    let mut header = [0; 16];
    ctx.read_buf(address, &mut header).await?;
    let p = u64::from_le_bytes(header[..8].try_into()?);
    let n = u32::from_le_bytes(header[8..12].try_into()?) as usize;
    let cap = u32::from_le_bytes(header[12..].try_into()?) as usize;
    ensure!(
        (1..=256).contains(&n)
            && cap >= n
            && cap <= 4096
            && (0x10000..0x800000000000).contains(&p)
            && p.is_multiple_of(2),
        "Session string bounds"
    );
    let mut bytes = vec![0; n * 2];
    ctx.read_buf(p, &mut bytes).await?;
    let units: Vec<_> = bytes.as_chunks::<2>().0.iter().map(|b| u16::from_le_bytes(*b)).collect();
    ensure!(units.last() == Some(&0) && !units[..n - 1].contains(&0), "Session string termination");
    Ok((
        String::from_utf16(&units[..n - 1])?,
        TextGuard {
            header,
            bytes,
        },
    ))
}
