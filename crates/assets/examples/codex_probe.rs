//! Standalone read-only asset research. Not called or packaged by the overlay.
//! Raw game data and indexes must remain in ignored local storage.
include!("../src/lib.rs");
#[path = "codex_probe/properties.rs"]
pub mod research_properties;

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let request_path = args.next().context("Cached extraction request path required")?;
    let output =
        std::path::PathBuf::from(args.next().context("Private output directory required")?);
    let selection = args.next();
    ensure!(args.next().is_none(), "Unexpected argument");
    fs::create_dir_all(&output)?;
    let output = output.canonicalize()?;
    let private = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../local").canonicalize()?;
    ensure!(output.starts_with(private), "Research output must stay in ignored local storage");
    let request: Request = serde_json::from_slice(&bounded(Path::new(&request_path), 16384)?)?;
    let mappings =
        usmap::Usmap::read(&mut Cursor::new(bounded(&request.mappings, 16 * 1024 * 1024)?))?;
    let archives = archives::preflight(&request.paks)?;
    let store = iostore::open(&request.paks, Arc::new(retoc::Config::default()))?;
    let paths: BTreeMap<_, _> =
        store.chunks().filter_map(|c| c.path().map(|p| (p, c.id().get_package_id()))).collect();
    ensure!(paths.len() <= 200000, "Archive index bound");
    let log = Log::no_log();
    let context = FZenPackageContext::create(store.as_ref(), None, &log, None);
    let mut native_paths = BTreeMap::new();
    for (path, package) in &paths {
        let native = if let Some(relative) = path.strip_prefix("../../../StarRupture/Content/") {
            Some(format!("/Game/{}", relative.trim_end_matches(".uasset")))
        } else if let Some(plugin) = path.strip_prefix("../../../StarRupture/Plugins/GameFeatures/")
        {
            plugin.split_once("/Content/").map(|(mount, relative)| {
                format!("/{mount}/{}", relative.trim_end_matches(".uasset"))
            })
        } else {
            None
        };
        if path.ends_with(".uasset")
            && let Some(native) = native
        {
            ensure!(
                native_paths.insert(native, *package).is_none(),
                "Ambiguous native package identity"
            );
        }
    }
    if let Some(selection) = selection {
        let selected: Vec<String> =
            serde_json::from_slice(&bounded(Path::new(&selection), 65536)?)?;
        ensure!(!selected.is_empty() && selected.len() <= 128, "Selection bound");
        let mut records = Vec::new();
        let mut texture_budget = 64 * 1024 * 1024;
        for id in &selected {
            ensure!(
                id.starts_with("/") && !id.contains("..") && native_paths.contains_key(id),
                "Selected native package must exist in the installed game index"
            );
            let result: Result<Value> = (|| {
                let p = load(&context, *native_paths.get(id).context("Selected package missing")?)?;
                ensure!(p.header.exports.len() <= 512, "Export count bound");
                let mut exports = Vec::new();
                for (index, export) in p.header.exports.iter().enumerate() {
                    let schema = properties::schema_for_export(&p.header, index)?;
                    let name = p.header.name_map.get(export.object_name)?.into_owned();
                    let export = &p.header.exports[index];
                    let start = usize::try_from(export.serial_offset)?
                        .checked_sub(p.header_len)
                        .context("Research export start")?;
                    let end = start
                        .checked_add(usize::try_from(export.serial_size)?)
                        .context("Research export size")?;
                    let mut r = research_properties::Reader {
                        bytes: p.exports.get(start..end).context("Research export bounds")?,
                        pos: 0,
                        package: &p.header,
                        schemas: &mappings,
                    };
                    let object_path = r.reference(i32::try_from(index + 1)?)?;
                    let parsed = r.properties(&schema, 0);
                    exports.push(match parsed {
                        Ok(props) => {
                            let property_bytes = r.pos;
                            let extra: Result<Value> = match schema.as_str() {
                                "StringTable" => r.string_table(),
                                "Texture2D" => r.texture_meta(),
                                _ => Ok(Value::Null),
                            };
                            if schema == "Texture2D"
                                && let Ok(meta) = &extra
                            {
                                ensure!(meta["format"] != "PF_FloatRGBA" || props.get("SRGB") == Some(&json!(false)), "Float icon must explicitly use linear colour");
                                write_research_texture(&r, meta, id, &output, &mut texture_budget)?;
                            }
                            let tail_ok = if extra.as_ref().is_ok_and(Value::is_null) {
                                r.object_tail().is_ok() && r.pos == r.bytes.len()
                            } else { false };
                            match extra {
                                Ok(extra) => json!({"index":index,"name":name,"objectPath":object_path,"schema":schema,"flags":export.object_flags,"properties":props,"extra":extra,"propertyBytes":property_bytes,"totalBytes":r.bytes.len(),"completeObjectTail":tail_ok}),
                                Err(e) => json!({"index":index,"name":name,"objectPath":object_path,"schema":schema,"flags":export.object_flags,"properties":props,"extraError":format!("{e:#}"),"propertyBytes":property_bytes,"totalBytes":r.bytes.len()}),
                            }
                        },
                        Err(e) => json!({"index":index,"name":name,"objectPath":object_path,"schema":schema,"flags":export.object_flags,"error":format!("{e:#}"),"consumedBytes":r.pos,"totalBytes":r.bytes.len()}),
                    });
                }
                Ok(json!({"asset":id,"exports":exports}))
            })();
            records.push(match result {
                Ok(v) => v,
                Err(e) => json!({"asset":id,"error":format!("{e:#}")}),
            });
        }
        fs::write(output.join("raw-assets.json"), serde_json::to_vec_pretty(&records)?)?;
        println!("{}", json!({"selected":records.len(),"output":output.join("raw-assets.json")}));
    } else {
        let paths: Vec<_> = paths.keys().collect();
        fs::write(output.join("asset-paths.json"), serde_json::to_vec(&paths)?)?;
        fs::write(output.join("schemas.json"), serde_json::to_vec(&mappings)?)?;
        fs::write(
            output.join("source.json"),
            serde_json::to_vec_pretty(
                &json!({"gameHash":request.game_hash,"paks":request.paks,"mappingsSha256":hex::encode(Sha256::digest(bounded(&request.mappings,16*1024*1024)?)),"archives":archives,"readOnlyGameAccess":true,"processMemoryRead":false,"gameCodeInvoked":false}),
            )?,
        )?;
        println!("{}", json!({"paths":paths.len(),"output":output}));
    }
    Ok(())
}

fn write_research_texture(
    r: &research_properties::Reader<'_>,
    meta: &Value,
    id: &str,
    output: &Path,
    texture_budget: &mut usize,
) -> Result<()> {
    let number = |key: &str| -> Result<usize> {
        usize::try_from(meta[key].as_u64().with_context(|| format!("Texture {key}"))?)
            .map_err(Into::into)
    };
    let (width, height, offset, size) =
        (number("width")?, number("height")?, number("offset")?, number("size")?);
    ensure!(
        (1..=2048).contains(&width) && (1..=2048).contains(&height),
        "Research texture dimensions"
    );
    let end = offset.checked_add(size).context("Texture range overflow")?;
    let data = r.bytes.get(offset..end).context("Texture payload bounds")?;
    let required = width
        .checked_mul(height)
        .and_then(|n| n.checked_mul(4))
        .context("Texture byte budget overflow")?;
    ensure!(required <= *texture_budget, "Research texture output budget exhausted");
    *texture_budget -= required;
    let mut pixels = vec![0; required];
    if meta["format"] == "PF_B8G8R8A8" {
        ensure!(data.len() == pixels.len(), "Texture raw size");
        pixels.copy_from_slice(data);
    } else if meta["format"] == "PF_FloatRGBA" {
        ensure!(data.len() == width * height * 8, "Float texture payload shape");
        for (input, output) in data.as_chunks::<8>().0.iter().zip(pixels.as_chunks_mut::<4>().0) {
            let mut rgba = [0.0f32; 4];
            for (i, component) in input.as_chunks::<2>().0.iter().enumerate() {
                rgba[i] = half_float(u16::from_le_bytes(*component))?;
            }
            output.copy_from_slice(&[
                linear_color(rgba[2]),
                linear_color(rgba[1]),
                linear_color(rgba[0]),
                (rgba[3].clamp(0.0, 1.0) * 255.0).round() as u8,
            ]);
        }
    } else {
        ensure!(
            meta["format"] == "PF_DXT5"
                && width.is_multiple_of(4)
                && height.is_multiple_of(4)
                && data.len() == width * height,
            "Research texture compression unsupported"
        );
        for by in 0..height / 4 {
            for bx in 0..width / 4 {
                let source = (by * (width / 4) + bx) * 16;
                let mut block = [0; 64];
                bcdec_rs::bc3(&data[source..source + 16], &mut block, 16);
                for y in 0..4 {
                    for x in 0..4 {
                        let s = (y * 4 + x) * 4;
                        let d = ((by * 4 + y) * width + bx * 4 + x) * 4;
                        pixels[d..d + 4].copy_from_slice(&[
                            block[s + 2],
                            block[s + 1],
                            block[s],
                            block[s + 3],
                        ]);
                    }
                }
            }
        }
    }
    for p in pixels.as_chunks_mut::<4>().0 {
        let alpha = p[3] as u32;
        for c in &mut p[..3] {
            *c = ((*c as u32 * alpha + 127) / 255) as u8;
        }
    }
    let icons = output.join("icons");
    fs::create_dir_all(&icons)?;
    ensure!(icons.canonicalize()?.starts_with(output), "Icon output escaped research directory");
    let mut bytes = Vec::with_capacity(8 + pixels.len());
    bytes.extend_from_slice(&(width as u32).to_le_bytes());
    bytes.extend_from_slice(&(height as u32).to_le_bytes());
    bytes.extend_from_slice(&pixels);
    fs::write(
        icons.join(format!("{}.bgra", &hex::encode(Sha256::digest(id.as_bytes()))[..24])),
        bytes,
    )?;
    Ok(())
}
fn half_float(bits: u16) -> Result<f32> {
    let exponent = i32::from((bits >> 10) & 31);
    let fraction = f32::from(bits & 1023) / 1024.0;
    ensure!(exponent != 31, "Nonfinite half-float texture component");
    let value = if exponent == 0 {
        fraction * 2.0f32.powi(-14)
    } else {
        (1.0 + fraction) * 2.0f32.powi(exponent - 15)
    };
    Ok(if bits & 0x8000 == 0 {
        value
    } else {
        -value
    })
}
fn linear_color(value: f32) -> u8 {
    let v = value.clamp(0.0, 1.0);
    let srgb = if v <= 0.0031308 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    };
    (srgb * 255.0).round() as u8
}
#[cfg(test)]
mod float_texture_tests {
    use super::*;
    #[test]
    fn half_float_color_preserves_endpoints_and_rejects_nonfinite() -> Result<()> {
        assert_eq!(half_float(0)?, 0.0);
        assert_eq!(half_float(0x3c00)?, 1.0);
        assert_eq!(half_float(0x3800)?, 0.5);
        assert_eq!(half_float(0xbc00)?, -1.0);
        assert_eq!(half_float(1)?, 2.0f32.powi(-24));
        assert!(half_float(0x7c00).is_err());
        assert!(half_float(0x7e00).is_err());
        assert_eq!(linear_color(-1.0), 0);
        assert_eq!(linear_color(0.0), 0);
        assert_eq!(linear_color(1.0), 255);
        assert_eq!(linear_color(0.5), 188);
        Ok(())
    }
}
