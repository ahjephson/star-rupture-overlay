//! Reflected camera/marker bindings, resolved once per resident session. Read-only.
use crate::{RuntimeCache, recipe, stamp};
use anyhow::{Context, Result, ensure};
use jmap_dumper::{
    mem::{Ctx, Ptr},
    objects::{UFunction, UObject, UStruct, ZArrayProperty, ZStructProperty},
};
use starrupture_observation_model::compass::{Pin, Sample, bearing};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};
const FIELDS: &[(&str, &str, usize)] = &[
    ("Engine.PlayerController", "PlayerCameraManager", 8),
    ("Engine.Controller", "PlayerState", 8),
    ("Engine.PlayerState", "PlayerNamePrivate", 16),
    ("Engine.PlayerCameraManager", "CameraCachePrivate", 0),
    ("Engine.CameraCacheEntry", "Timestamp", 4),
    ("Engine.CameraCacheEntry", "POV", 0),
    ("Engine.MinimalViewInfo", "Location", 24),
    ("Engine.MinimalViewInfo", "Rotation", 24),
    ("Chimera.CrGameStateBase", "PlayerPersonalMarkers", 16),
    ("Chimera.CrPlayerPersonalMarkerData", "MarkerPosition", 8),
    ("Chimera.CrPlayerPersonalMarkerData", "PlayerName", 16),
];
pub(crate) struct View {
    pub world: u64,
    pub state: u64,
    pub simulation: f64,
    pub origin: [i32; 3],
    pub menu: bool,
    pub started: u64,
}
struct CachedPin {
    player_state: u64,
    state: u64,
    at: Instant,
    position: Option<[f64; 2]>,
    known: bool,
}
pub(crate) struct Binding {
    pin_cache: Option<CachedPin>,
    fields: BTreeMap<(&'static str, &'static str), (u64, u64)>,
    custom_mask: u8,
    custom_flag: u64,
}
impl Binding {
    pub(crate) async fn discover(ctx: &Ctx) -> Result<Self> {
        let mut fields = BTreeMap::new();
        let mut native_guard = None;
        for (index, address) in recipe::registered_objects(ctx).await? {
            let obj = Ptr::<UObject>::new(address, ctx.clone())?;
            if obj.internal_index().read().await.ok() != Some(index as i32) {
                continue;
            }
            let name = obj.name_private().read().await?;
            if name == "GetPlayerName"
                && obj.path().await? == "/Script/Engine.PlayerState:GetPlayerName"
            {
                let function = obj.cast::<UFunction>().func().read().await? as u64;
                let mut code = [0; 192];
                ctx.read_buf(function, &mut code).await?;
                use iced_x86::{Decoder, DecoderOptions, Mnemonic, OpKind};
                for ins in Decoder::with_ip(64, &code, function, DecoderOptions::NONE) {
                    if ins.mnemonic() == Mnemonic::Ret {
                        break;
                    }
                    if ins.mnemonic() == Mnemonic::Call && ins.op0_kind() == OpKind::NearBranch64 {
                        let mut target = [0; 128];
                        ctx.read_buf(ins.near_branch_target(), &mut target).await?;
                        if let Some(guard) = name_guard(&target) {
                            ensure!(native_guard.is_none(), "Ambiguous owner getter");
                            native_guard = Some(guard);
                        }
                    }
                }
            }
            if !FIELDS.iter().any(|(c, _, _)| c.rsplit('.').next() == Some(name.as_str())) {
                continue;
            }
            let path = obj.path().await?;
            let Some(&(class, _, _)) =
                FIELDS.iter().find(|(c, _, _)| path == format!("/Script/{c}"))
            else {
                continue;
            };
            let size = obj.cast::<UStruct>().properties_size().read().await?;
            ensure!((1..=262144).contains(&size), "Navigation struct size");
            if class == "Chimera.CrPlayerPersonalMarkerData" {
                ensure!(size == 40, "Personal marker row layout")
            }
            let mut props = obj.cast::<UStruct>().properties(false);
            let mut visits = 0;
            while let Some(prop) = props.next().await {
                visits += 1;
                ensure!(visits <= 4096, "Navigation field chain bound");
                let prop = prop?;
                let name = prop.zfield().name_private().read().await?;
                let Some(&(_, field, wanted)) =
                    FIELDS.iter().find(|(c, f, _)| *c == class && *f == name)
                else {
                    continue;
                };
                let off = prop.offset_internal().read().await?;
                let bytes = prop.element_size().read().await?;
                ensure!(
                    off >= 0
                        && bytes > 0
                        && off.checked_add(bytes).is_some_and(|v| v <= size)
                        && prop.array_dim().read().await? == 1
                        && (wanted == 0 || bytes == wanted as i32),
                    "Navigation property bounds"
                );
                if field == "PlayerPersonalMarkers" {
                    let inner = prop.cast::<ZArrayProperty>().inner().read().await?;
                    ensure!(
                        inner.element_size().read().await? == 40
                            && inner
                                .cast::<ZStructProperty>()
                                .struct_()
                                .read()
                                .await?
                                .cast::<UObject>()
                                .path()
                                .await?
                                == "/Script/Chimera.CrPlayerPersonalMarkerData",
                        "Personal marker array type"
                    );
                }
                fields.insert((class, field), (off as u64, bytes as u64));
            }
        }
        ensure!(
            fields.len() == FIELDS.len() && native_guard.is_some(),
            "Navigation reflected fields unavailable: {:?}",
            FIELDS
                .iter()
                .filter(|(c, f, _)| !fields.contains_key(&(*c, *f)))
                .map(|(c, f, _)| format!("{c}:{f}"))
                .collect::<Vec<_>>()
        );
        let (custom_flag, custom_mask) =
            native_guard.context("Native player-name getter unavailable")?;
        ensure!(
            fields[&("Engine.PlayerState", "PlayerNamePrivate")].0 == 0x348,
            "Native name getter field mismatch"
        );
        let b = Self {
            fields,
            custom_mask,
            custom_flag,
            pin_cache: None,
        };
        for (f, off) in [("MarkerPosition", 0), ("PlayerName", 8)] {
            ensure!(
                b.off("Chimera.CrPlayerPersonalMarkerData", f) == off,
                "Personal marker row changed"
            )
        }
        b.child(
            ("Engine.PlayerCameraManager", "CameraCachePrivate"),
            ("Engine.CameraCacheEntry", "POV"),
        )?;
        b.child(("Engine.CameraCacheEntry", "POV"), ("Engine.MinimalViewInfo", "Rotation"))?;
        b.child(("Engine.CameraCacheEntry", "POV"), ("Engine.MinimalViewInfo", "Location"))?;
        Ok(b)
    }
    fn off(&self, c: &'static str, f: &'static str) -> u64 {
        self.fields[&(c, f)].0
    }
    fn child(
        &self,
        p: (&'static str, &'static str),
        c: (&'static str, &'static str),
    ) -> Result<u64> {
        let (o, s) = self.fields[&p];
        let (co, cs) = self.fields[&c];
        ensure!(co + cs <= s, "Navigation nested bounds");
        Ok(o + co)
    }
    pub(crate) async fn sample(
        &mut self,
        ctx: &Ctx,
        base: &RuntimeCache,
        view: View,
    ) -> Result<Sample> {
        let View {
            world,
            state,
            simulation: sim,
            origin,
            menu,
            started,
        } = view;
        ensure!(origin == [0; 3], "Rebased navigation frame is not calibrated");
        let controller = recipe::allocation(ctx, base.controller.address).await?;
        ensure!(controller.0 == base.controller, "Navigation controller changed");
        let manager_field =
            base.controller.address + self.off("Engine.PlayerController", "PlayerCameraManager");
        let manager: u64 = ctx.read(manager_field).await?;
        let manager_stamp = recipe::allocation(ctx, manager).await?;
        let cache = manager + self.off("Engine.PlayerCameraManager", "CameraCachePrivate");
        let timestamp_at = cache + self.off("Engine.CameraCacheEntry", "Timestamp");
        let at: f32 = ctx.read(timestamp_at).await?;
        ensure!(
            sim.is_finite() && at.is_finite() && (-0.05..=0.25).contains(&(sim - f64::from(at))),
            "Stale navigation camera"
        );
        let pov = cache + self.off("Engine.CameraCacheEntry", "POV");
        let rotation = vector(ctx, pov + self.off("Engine.MinimalViewInfo", "Rotation")).await?;
        let location = vector(ctx, pov + self.off("Engine.MinimalViewInfo", "Location")).await?;
        ensure!(
            rotation.iter().all(|v| v.is_finite() && v.abs() < 360000.)
                && rotation[0].to_radians().cos().abs() > 0.001
                && location.iter().all(|v| v.is_finite() && v.abs() < 1e9),
            "Navigation camera values"
        );
        let current_player_state: u64 = ctx
            .read(base.controller.address + self.off("Engine.Controller", "PlayerState"))
            .await?;
        let refresh = self.pin_cache.as_ref().is_none_or(|p| {
            p.state != state
                || p.player_state != current_player_state
                || p.at.elapsed() >= Duration::from_millis(500)
        });
        if refresh {
            let result = self.pin(ctx, base, state).await;
            let known = result.is_ok();
            self.pin_cache = Some(CachedPin {
                player_state: current_player_state,
                state,
                at: Instant::now(),
                position: result.unwrap_or_default(),
                known,
            });
        }
        let cached = &self.pin_cache;
        let cached = cached.as_ref().expect("Pin cache initialized above");
        let pin_known = cached.known;
        let pin = cached.position.map(|p| {
            let dx = p[0] - location[0];
            let dy = p[1] - location[1];
            Pin {
                bearing: bearing(dx, dy),
                metres: dx.hypot(dy) / 100.,
            }
        });
        ensure!(
            ctx.read::<f32>(timestamp_at).await? == at
                && ctx.read::<u64>(manager_field).await? == manager
                && recipe::allocation(ctx, manager).await? == manager_stamp
                && recipe::allocation(ctx, base.controller.address).await? == controller
                && stamp(ctx, world).await?.address == world,
            "Navigation camera allocation changed"
        );
        Ok(Sample {
            started_ms: started,
            finished_ms: super::overlay_stream::clock_ms(),
            heading: (rotation[1] + 90.).rem_euclid(360.),
            pin,
            pin_known,
            menu_open: menu,
        })
    }
    async fn pin(&self, ctx: &Ctx, base: &RuntimeCache, state: u64) -> Result<Option<[f64; 2]>> {
        let state_stamp = recipe::allocation(ctx, state).await?;
        let ps_at = base.controller.address + self.off("Engine.Controller", "PlayerState");
        let ps: u64 = ctx.read(ps_at).await?;
        let ps_stamp = recipe::allocation(ctx, ps).await?;
        let flag_at = ps + self.custom_flag;
        ensure!(
            ctx.read::<u8>(flag_at).await? & self.custom_mask == 0,
            "Custom player name unsupported"
        );
        let name_at = ps + self.off("Engine.PlayerState", "PlayerNamePrivate");
        let (name, name_guard) = text(ctx, name_at).await?;
        ensure!(!name.is_empty(), "Personal marker owner unavailable");
        let arr = state + self.off("Chimera.CrGameStateBase", "PlayerPersonalMarkers");
        let header = read_header(ctx, arr).await?;
        let n = (header[1] & 0xffffffff) as usize;
        let cap = (header[1] >> 32) as usize;
        ensure!(n <= 16 && cap >= n && cap <= 64, "Personal marker array bounds");
        let mut data = vec![0; n * 40];
        if n > 0 {
            ensure!(pointer(header[0]), "Marker data pointer");
            ctx.read_buf(header[0], &mut data).await?;
        }
        let mut selected = None;
        let mut matches = 0;
        for i in 0..n {
            let row = header[0] + (i * 40) as u64;
            let (owner, guard) = text(ctx, row + 8).await?;
            if owner == name {
                matches += 1;
                let pos = [ctx.read::<f32>(row).await?, ctx.read::<f32>(row + 4).await?];
                ensure!(
                    pos.iter().all(|v| v.is_finite() && v.abs() < 1e7),
                    "Marker coordinate bounds"
                );
                selected = Some([f64::from(pos[0]), f64::from(pos[1])]);
            }
            guard.check(ctx, row + 8).await?;
        }
        ensure!(matches <= 1, "Ambiguous personal marker owner");
        let mut again = vec![0; data.len()];
        if n > 0 {
            ctx.read_buf(header[0], &mut again).await?;
        }
        name_guard.check(ctx, name_at).await?;
        ensure!(
            again == data
                && read_header(ctx, arr).await? == header
                && ctx.read::<u64>(ps_at).await? == ps
                && recipe::allocation(ctx, ps).await? == ps_stamp
                && recipe::allocation(ctx, state).await? == state_stamp
                && ctx.read::<u8>(flag_at).await? & self.custom_mask == 0,
            "Personal marker changed during read"
        );
        Ok(selected)
    }
}
fn pointer(p: u64) -> bool {
    (0x10000..0x800000000000).contains(&p) && p.is_multiple_of(8)
}
struct TextGuard {
    header: [u64; 2],
    bytes: Vec<u8>,
}
impl TextGuard {
    async fn check(&self, ctx: &Ctx, at: u64) -> Result<()> {
        ensure!(read_header(ctx, at).await? == self.header, "Owner string allocation changed");
        let mut bytes = vec![0; self.bytes.len()];
        if !bytes.is_empty() {
            ctx.read_buf(self.header[0], &mut bytes).await?;
        }
        ensure!(bytes == self.bytes, "Owner name changed");
        Ok(())
    }
}
async fn text(ctx: &Ctx, at: u64) -> Result<(String, TextGuard)> {
    let header = read_header(ctx, at).await?;
    let n = (header[1] & 0xffffffff) as usize;
    let cap = (header[1] >> 32) as usize;
    ensure!(
        n <= 128 && cap >= n && cap <= 256 && (n == 0 || pointer(header[0])),
        "Owner text bounds"
    );
    let mut bytes = vec![0; n * 2];
    if n > 0 {
        ctx.read_buf(header[0], &mut bytes).await?;
    }
    let units: Vec<_> = bytes.as_chunks::<2>().0.iter().map(|b| u16::from_le_bytes(*b)).collect();
    let text = String::from_utf16(&units)?.trim_end_matches('\0').to_owned();
    Ok((
        text,
        TextGuard {
            header,
            bytes,
        },
    ))
}

async fn read_header(ctx: &Ctx, at: u64) -> Result<[u64; 2]> {
    let mut b = [0; 16];
    ctx.read_buf(at, &mut b).await?;
    Ok([u64::from_le_bytes(b[..8].try_into()?), u64::from_le_bytes(b[8..].try_into()?)])
}
async fn vector(ctx: &Ctx, at: u64) -> Result<[f64; 3]> {
    let mut b = [0; 24];
    ctx.read_buf(at, &mut b).await?;
    Ok(std::array::from_fn(|i| f64::from_le_bytes(b.as_chunks::<8>().0[i])))
}
pub(crate) async fn origin(ctx: &Ctx, at: u64) -> Result<[i32; 3]> {
    let mut b = [0; 12];
    ctx.read_buf(at, &mut b).await?;
    Ok(std::array::from_fn(|i| i32::from_le_bytes(b.as_chunks::<4>().0[i])))
}
/// Finite read-only live diagnostic; never starts a normal overlay or mutates the game.
pub fn self_test(directory: &std::path::Path) -> Result<()> {
    use starrupture_observation_model::{
        compass::Control, overlay_stream::Payload, shutdown::StopSignal,
    };
    use std::sync::Arc;
    ensure!(directory.is_absolute(), "Absolute private diagnostic directory required");
    std::fs::create_dir_all(directory)?;
    let stop = Arc::new(StopSignal::default());
    let deadline = stop.clone();
    let timer = std::thread::spawn(move || {
        deadline.wait_until(Instant::now() + Duration::from_secs(120));
        deadline.request();
    });
    let enabled = Arc::new(Control::default());
    enabled.set(true);
    let mut samples = Vec::new();
    let mut unavailable = 0;
    let result = crate::overlay_stream::run_with_navigation(
        directory,
        stop.clone(),
        |m| {
            if let Payload::Compass(value) = m.payload {
                if let Some(value) = value {
                    samples.push(value);
                    if samples.len() >= 90 {
                        stop.request();
                    }
                } else {
                    unavailable += 1
                }
            }
            Ok(())
        },
        |_| {},
        None,
        None,
        Some(enabled),
    );
    stop.request();
    let _ = timer.join();
    result?;
    std::fs::write(directory.join("compass-samples.json"), serde_json::to_vec_pretty(&samples)?)?;
    ensure!(
        samples.len() >= 90,
        "No current compass observations; load a world and check diagnostic log"
    );
    let durations: Vec<_> = samples.iter().map(|s| s.finished_ms - s.started_ms).collect();
    println!(
        "{}",
        serde_json::json!({"status":"passed","samples":samples.len(),"unavailable":unavailable,
        "maxReadMs":durations.iter().max(),"meanReadMs":durations.iter().sum::<u64>() as f64/durations.len() as f64,
        "pinKnown":samples.iter().filter(|s|s.pin_known).count(),"pinPresent":samples.iter().filter(|s|s.pin.is_some()).count(),
        "pausedSamples":samples.iter().filter(|s|s.menu_open).count(),"gameMemoryWrites":false,"workerJoined":true})
    );
    Ok(())
}

// Reviewed native getter: zero AL, compare the packed byte against zero, and use
// the signed default branch; the alternative calls the custom-name virtual slot.
// Match the bounded code shape and reflected FString loads, never invoke it.
fn name_guard(code: &[u8]) -> Option<(u64, u8)> {
    if !code.starts_with(&[
        0x40, 0x53, 0x48, 0x83, 0xec, 0x60, 0x33, 0xc0, 0x48, 0x8b, 0xda, 0x38, 0x81,
    ]) || code.get(17..19) != Some(&[0x7d, 0x23])
        || !code.windows(7).any(|v| v == [0x48, 0x8b, 0xb1, 0x48, 0x03, 0, 0])
        || !code.windows(7).any(|v| v == [0x48, 0x63, 0xb9, 0x50, 0x03, 0, 0])
    {
        return None;
    }
    let offset = u32::from_le_bytes(code.get(13..17)?.try_into().ok()?);
    (0x28..0x348).contains(&offset).then_some((u64::from(offset), 0x80))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn private_owner_flag_requires_reviewed_native_getter_shape() {
        let mut code = vec![
            0x40, 0x53, 0x48, 0x83, 0xec, 0x60, 0x33, 0xc0, 0x48, 0x8b, 0xda, 0x38, 0x81, 0xb2,
            0x02, 0, 0, 0x7d, 0x23,
        ];
        code.extend([0x48, 0x8b, 0xb1, 0x48, 0x03, 0, 0, 0x48, 0x63, 0xb9, 0x50, 0x03, 0, 0]);
        assert_eq!(name_guard(&code), Some((0x2b2, 0x80)));
        code[17] = 0x75;
        assert_eq!(name_guard(&code), None);
        assert_eq!(name_guard(&[]), None);
    }
}
