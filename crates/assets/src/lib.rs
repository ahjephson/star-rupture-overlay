mod properties;
use anyhow::{Context, Result, ensure};
use retoc::{
    FileWriterTrait, UEPath,
    asset_conversion::{FZenPackageContext, build_legacy},
    iostore,
    legacy_asset::FLegacyPackageHeader,
    logging::Log,
};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Cursor,
    path::Path,
    sync::{Arc, Mutex},
    time::Instant,
};
#[derive(Default)]
struct Writer(Mutex<BTreeMap<String, Vec<u8>>>);
impl FileWriterTrait for Writer {
    fn write_file(&self, path: String, _: bool, data: Vec<u8>) -> Result<()> {
        ensure!(data.len() <= 4 * 1024 * 1024, "Package bound");
        ensure!(path == "probe.uasset" || path == "probe.uexp", "Unexpected conversion output");
        ensure!(self.0.lock().unwrap().insert(path, data).is_none(), "Duplicate output");
        Ok(())
    }
}
struct Package {
    header: FLegacyPackageHeader,
    exports: Vec<u8>,
    header_len: usize,
}
fn load(context: &FZenPackageContext<'_>, id: retoc::FPackageId) -> Result<Package> {
    let writer = Writer::default();
    build_legacy(context, id, UEPath::new("probe.uasset"), &writer)?;
    let mut files = writer.0.into_inner().unwrap();
    let header = files.remove("probe.uasset").context("Header")?;
    let exports = files.remove("probe.uexp").context("Exports")?;
    let header_len = header.len();
    let header = FLegacyPackageHeader::deserialize(
        &mut Cursor::new(header),
        Some(retoc::version::EngineVersion::UE5_6.package_file_version()),
    )?;
    Ok(Package {
        header,
        exports,
        header_len,
    })
}
fn object<'a>(
    package: &'a Package,
    index: usize,
    mappings: &'a usmap::Usmap,
) -> Result<properties::Reader<'a>> {
    let export = &package.header.exports[index];
    let start = usize::try_from(export.serial_offset)?
        .checked_sub(package.header_len)
        .context("Export offset")?;
    let end = start.checked_add(usize::try_from(export.serial_size)?).context("Export end")?;
    let bytes = package.exports.get(start..end).context("Export bounds")?;
    Ok(properties::Reader {
        bytes,
        pos: 0,
        package: &package.header,
        schemas: mappings,
    })
}
fn asset(value: &Value) -> Result<String> {
    let s = value.as_str().context("Asset reference")?;
    let package = s.split('.').next().context("Asset package")?;
    ensure!(package.starts_with("/Game/Chimera/"), "Asset namespace");
    Ok(package.to_owned())
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub paks: std::path::PathBuf,
    pub mappings: std::path::PathBuf,
    pub output: std::path::PathBuf,
    pub progress: std::path::PathBuf,
    pub error: std::path::PathBuf,
    pub game_hash: String,
}
impl Request {
    fn validate(&self, request_file: &Path) -> Result<()> {
        ensure!(
            self.game_hash == starrupture_observation_model::overlay_stream::GAME_HASH,
            "Unsupported game asset build"
        );
        ensure!(
            self.output.is_absolute() && self.paks.is_absolute(),
            "Absolute asset paths required"
        );
        let output = self.output.canonicalize()?;
        ensure!(
            request_file.canonicalize()?.parent() == Some(output.as_path()),
            "Request must be inside its output generation"
        );
        for file in [&self.mappings, &self.progress, &self.error] {
            ensure!(
                file.is_absolute()
                    && file.parent().context("Asset output parent")?.canonicalize()? == output,
                "Asset output escapes generation"
            );
        }
        let paks = self.paks.canonicalize()?;
        let game = paks.parent().and_then(Path::parent).context("Game directory")?;
        ensure!(!output.starts_with(game), "Asset output must not be inside game files");
        Ok(())
    }
}
fn bounded(path: &Path, limit: u64) -> Result<Vec<u8>> {
    use std::io::Read;
    let file = fs::File::open(path)?;
    ensure!(file.metadata()?.len() <= limit, "Asset input bound");
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= limit, "Asset input grew beyond bound");
    Ok(bytes)
}
pub fn run(request_file: &Path) -> Result<()> {
    let request: Request = serde_json::from_slice(&bounded(request_file, 16 * 1024)?)?;
    request.validate(request_file)?;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| extract(&request)))
        .unwrap_or_else(|_| Err(anyhow::anyhow!("Unsupported or malformed game assets")));
    if let Err(error) = &result {
        fs::write(&request.error, format!("{error:#}").chars().take(1000).collect::<String>())?;
    }
    result
}
fn status(request: &Request, message: &str) -> Result<()> {
    fs::write(&request.progress, message)?;
    Ok(())
}
fn extract(request: &Request) -> Result<()> {
    let started = Instant::now();
    let output = &request.output;
    status(request, "Reading native recipe types...")?;
    let schema_bytes = bounded(&request.mappings, 16 * 1024 * 1024)?;
    ensure!(schema_bytes.get(3) == Some(&0), "Uncompressed native schema required");
    let mappings = usmap::Usmap::read(&mut Cursor::new(&schema_bytes))?;
    let paks = &request.paks;
    // Only use the exact uncompressed TOCs verified by the separate POC preflight.
    for (file, hash) in [
        ("global.utoc", "1606b953a319e60133ab8f4395cc9e17a4fa11eb2be7ae7b50d091ffe71584b0"),
        (
            "pakchunk0-Windows.utoc",
            "b15f7a1494ab29c20dd787e688c5d45e0253d5aa3c562b6b030e23c83dea32f0",
        ),
    ] {
        ensure!(
            format!("{:x}", Sha256::digest(bounded(&paks.join(file), 32 * 1024 * 1024)?)) == hash,
            "Archive identity changed; rerun preflight investigation"
        );
    }
    let store = iostore::open(paks, Arc::new(retoc::Config::default()))?;
    let paths: BTreeMap<_, _> =
        store.chunks().filter_map(|c| c.path().map(|p| (p, c.id().get_package_id()))).collect();
    let lookup = |asset: &str| -> Result<retoc::FPackageId> {
        let path = format!(
            "../../../StarRupture/Content/{}.uasset",
            asset.strip_prefix("/Game/").context("Game prefix")?
        );
        paths.get(&path).copied().with_context(|| format!("Package missing {asset}"))
    };
    let log = Log::no_log();
    let context = FZenPackageContext::create(store.as_ref(), None, &log, None);
    let mut tables = BTreeMap::new();
    let mut failures = Vec::new();
    let mut recipes = Map::new();
    let mut items = Map::new();
    let mut icons = BTreeSet::new();
    status(request, "Reading recipe names...")?;
    let table_candidates = paths
        .keys()
        .filter_map(|p| {
            p.strip_prefix("../../../StarRupture/Content/Chimera/Localization/StringTables/")
                .and_then(|p| p.strip_suffix(".uasset"))
        })
        .map(|p| format!("/Game/Chimera/Localization/StringTables/{p}"))
        .collect::<Vec<_>>();
    ensure!(
        !table_candidates.is_empty() && table_candidates.len() <= 128,
        "String table discovery bound"
    );
    for id in &table_candidates {
        let p = load(&context, lookup(id)?)?;
        ensure!(p.header.exports.len() == 1, "String table export count");
        let name = p.header.name_map.get(p.header.exports[0].object_name)?.into_owned();
        let mut r = object(&p, 0, &mappings)?;
        ensure!(
            properties::schema_for_export(&p.header, 0)? == "StringTable",
            "Discovered string table class"
        );
        r.properties("StringTable", 0)?;
        let table = r.string_table()?;
        ensure!(tables.insert(format!("{id}.{name}"), table).is_none(), "Duplicate table identity");
    }
    status(request, "Discovering research recipes...")?;
    let candidates = paths
        .keys()
        .filter_map(|p| {
            p.strip_prefix("../../../StarRupture/Content/Chimera/Crafting/")
                .and_then(|p| p.strip_suffix(".uasset"))
        })
        .filter(|p| p.rsplit('/').next().is_some_and(|n| n.starts_with("CR_")))
        .map(|p| format!("/Game/Chimera/Crafting/{p}"))
        .collect::<Vec<_>>();
    ensure!(!candidates.is_empty() && candidates.len() <= 4096, "Recipe discovery bound");
    let mut discovered = BTreeSet::new();
    let mut wanted = BTreeSet::new();
    let mut excluded = Vec::new();
    for id in &candidates {
        let p = load(&context, lookup(id)?)?;
        ensure!(p.header.exports.len() == 1, "Recipe candidate export count");
        let schema = properties::schema_for_export(&p.header, 0)?;
        if schema != "CrItemRecipeData" {
            excluded.push(json!({"id":id,"reason":"Different class","schema":schema}));
            continue;
        }
        let mut r = object(&p, 0, &mappings)?;
        let props = r.properties(&schema, 0).with_context(|| format!("Discovering {id}"))?;
        r.object_tail()?;
        ensure!(r.pos == r.bytes.len(), "Discovered recipe tail");
        if props["UnlockType"] != "ECrRecipeUnlockType::RecipeTable" {
            excluded.push(json!({"id":id,"reason":"Not RecipeTable unlock type","unlock_type":props["UnlockType"]}));
            continue;
        }
        for entry in props["UnlockRequirements"].as_array().context("Discovered requirements")? {
            wanted.insert(asset(&entry["Key"])?);
        }
        wanted.insert(asset(&props["OutputItem"]["Item"])?);
        ensure!(discovered.insert(id.clone()), "Duplicate recipe");
    }
    let text = |v: &Value| -> Result<String> {
        if let Some(s) = v["SourceString"].as_str() {
            return Ok(s.to_owned());
        }
        if let Some(s) = v["CultureInvariantString"].as_str() {
            return Ok(s.to_owned());
        }
        let table = v["TableId"].as_str().context("Text table")?;
        let key = v["Key"].as_str().context("Text key")?;
        Ok(tables.get(table).context("String table not loaded")?["entries"][key]
            .as_str()
            .context("Missing localized key")?
            .to_owned())
    };
    for category in ["recipes", "items"] {
        status(
            request,
            if category == "recipes" {
                "Reading research requirements..."
            } else {
                "Reading resource icons..."
            },
        )?;
        let ids = if category == "recipes" {
            &discovered
        } else {
            &wanted
        };
        for id in ids {
            let attempt = (|| -> Result<Value> {
                let p = load(&context, lookup(id)?)?;
                let index = if category == "recipes" {
                    ensure!(p.header.exports.len() == 1, "Recipe export count");
                    0
                } else {
                    let indices = p
                        .header
                        .exports
                        .iter()
                        .enumerate()
                        .filter(|(_, e)| e.object_flags & 0x10 != 0)
                        .map(|(i, _)| i)
                        .collect::<Vec<_>>();
                    ensure!(indices.len() == 1, "Single CDO required");
                    indices[0]
                };
                let schema = properties::schema_for_export(&p.header, index)?;
                let mut r = object(&p, index, &mappings)?;
                let props = r.properties(&schema, 0)?;
                r.object_tail()?;
                ensure!(r.pos == r.bytes.len(), "Unparsed object tail");
                if category == "recipes" {
                    let mut requirements = Map::new();
                    for entry in props["UnlockRequirements"].as_array().context("Requirements")? {
                        let k = asset(&entry["Key"])?;
                        ensure!(
                            entry["Value"].as_i64().is_some_and(|n| n > 0),
                            "Positive requirement"
                        );
                        ensure!(
                            requirements.insert(k, entry["Value"].clone()).is_none(),
                            "Duplicate requirement"
                        );
                    }
                    Ok(
                        json!({"name":text(&props["DisplayText"])?,"itemId":asset(&props["OutputItem"]["Item"])?,"requirements":requirements}),
                    )
                } else {
                    let icon = asset(&props["ItemIcon"]["ResourceObject"])?;
                    icons.insert(icon.clone());
                    Ok(
                        json!({"name":text(&props["ItemName"])?,"image":format!("{}.bgra",&format!("{:x}",Sha256::digest(icon.as_bytes()))[..24])}),
                    )
                }
            })();
            match attempt {
                Ok(v) => {
                    if category == "recipes" {
                        recipes.insert(id.clone(), v);
                    } else {
                        items.insert(id.clone(), v);
                    }
                }
                Err(e) => failures.push(json!({"id":id,"error":format!("{e:#}")})),
            };
        }
    }
    let mut name_differences = Vec::new();
    for (id, recipe) in recipes.iter_mut() {
        let item = recipe["itemId"].as_str().context("Recipe output")?;
        let display = items.get(item).context("Recipe item name")?["name"].clone();
        if recipe["name"] != display {
            name_differences.push(json!({"id":id,"recipe_display_text":recipe["name"],"output_item_name":display,"rule":"Existing overlay uses output item name as recipe heading"}));
        }
        recipe["name"] = display;
        // Existing presentation alias in the committed overlay preparation script.
        // Keep raw native text in definitions.json and disclose the correction.
        if id == "/Game/Chimera/Crafting/CR_QuantumTranqualizer" {
            ensure!(recipe["name"] == "Quantum Tranqualizer", "Native spelling changed");
            name_differences.push(json!({"id":id,"native_name":recipe["name"],"display_name":"Quantum Tranquilizer","rule":"Existing overlay presentation spelling correction"}));
            recipe["name"] = json!("Quantum Tranquilizer");
        }
    }
    for icon in [
        "/Game/Chimera/UI/Research/Textures/T_SR_unlockedItemIcon_128x128",
        "/Game/Chimera/UI/Research/RecipeTable/Textures/SR_recipeTableRecipeIcon_frame",
        "/Game/Chimera/UI/Research/RecipeTable/Textures/SR_recipeTableRecipeIcon_bkgd",
    ] {
        icons.insert(icon.to_owned());
    }
    status(request, "Preparing recipe graphics...")?;
    let mut texture_results = Vec::new();
    let mut total_pixels = 0usize;
    for id in &icons {
        let attempt = (|| -> Result<Value> {
            let p = load(&context, lookup(id)?)?;
            ensure!(p.header.exports.len() == 1, "Texture export count");
            let mut r = object(&p, 0, &mappings)?;
            r.properties("Texture2D", 0)?;
            let meta = r.texture_meta()?;
            let width = meta["width"].as_u64().unwrap() as usize;
            let height = meta["height"].as_u64().unwrap() as usize;
            let offset = meta["offset"].as_u64().unwrap() as usize;
            let size = meta["size"].as_u64().unwrap() as usize;
            let data = &r.bytes[offset..offset + size];
            total_pixels =
                total_pixels.checked_add(width * height * 4).context("Texture byte overflow")?;
            ensure!(total_pixels <= 64 * 1024 * 1024, "Texture output byte bound");
            let mut pixels = vec![0u8; width * height * 4];
            if meta["format"] == "PF_B8G8R8A8" {
                ensure!(data.len() == pixels.len(), "Raw texture size");
                pixels.copy_from_slice(data)
            } else {
                ensure!(
                    width.is_multiple_of(4)
                        && height.is_multiple_of(4)
                        && data.len() == width * height,
                    "BC3 size"
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
                let a = p[3] as u32;
                for c in &mut p[..3] {
                    *c = ((*c as u32 * a + 127) / 255) as u8;
                }
            }
            let filename =
                format!("{}.bgra", &format!("{:x}", Sha256::digest(id.as_bytes()))[..24]);
            let mut bytes = Vec::new();
            bytes.extend_from_slice(&(width as u32).to_le_bytes());
            bytes.extend_from_slice(&(height as u32).to_le_bytes());
            bytes.extend_from_slice(&pixels);
            fs::write(output.join(filename), bytes)?;
            Ok(json!({"id":id,"meta":meta}))
        })();
        match attempt {
            Ok(v) => texture_results.push(v),
            Err(e) => failures.push(json!({"id":id,"error":format!("{e:#}")})),
        };
    }
    let mut font_archive = fs::File::open(paks.join("pakchunk0-Windows.pak"))?;
    let pak = repak::PakBuilder::new().reader(&mut font_archive)?;
    let fonts =
        pak.files().into_iter().filter(|p| p.ends_with("/Nippo-Regular.ufont")).collect::<Vec<_>>();
    ensure!(fonts.len() == 1, "One discovered Nippo font");
    let wrapped = pak.get(&fonts[0], &mut font_archive)?;
    ensure!(wrapped.len() >= 16 && &wrapped[4..8] == b"OTTO", "Font wrapper");
    let font = &wrapped[4..wrapped.len() - 4];
    fs::write(output.join("Nippo-Regular.otf"), font)?;
    ensure!(font.len() <= 4 * 1024 * 1024, "Font size bound");
    let image =
        |id: &str| format!("{}.bgra", &format!("{:x}", Sha256::digest(id.as_bytes()))[..24]);
    let catalogue = serde_json::json!({"gameHash":request.game_hash,"recipes":recipes,"items":items,"tick":image("/Game/Chimera/UI/Research/Textures/T_SR_unlockedItemIcon_128x128"),"frame":image("/Game/Chimera/UI/Research/RecipeTable/Textures/SR_recipeTableRecipeIcon_frame"),"background":image("/Game/Chimera/UI/Research/RecipeTable/Textures/SR_recipeTableRecipeIcon_bkgd"),"font":"Nippo-Regular.otf"});
    let validated: starrupture_observation_model::recipe::Catalogue =
        serde_json::from_value(catalogue.clone())?;
    validated.validate().map_err(anyhow::Error::msg)?;
    fs::write(output.join("catalogue.json"), serde_json::to_vec(&catalogue)?)?;
    fs::write(
        output.join("extraction.json"),
        serde_json::to_vec_pretty(
            &json!({"schema":1,"recipeCandidates":candidates.len(),"excludedRecipes":excluded.len(),"stringTables":table_candidates.len(),"recipes":recipes.len(),"items":items.len(),"graphics":texture_results.len(),"schemaSha256":format!("{:x}",Sha256::digest(&schema_bytes)),"elapsedMs":started.elapsed().as_millis(),"recipeHeadingDifferences":name_differences,"failures":failures}),
        )?,
    )?;
    ensure!(
        failures.is_empty(),
        "Game assets could not be read completely: {}",
        failures.first().unwrap_or(&Value::Null)
    );
    ensure!(recipes.len() == 122 && items.len() == 177 && icons.len() == 179, "Coverage mismatch");
    Ok(())
}

#[cfg(test)]
mod offline_tests {
    #[test]
    fn oodle_is_rejected_without_loading_a_helper() {
        use retoc::compression::{CompressionMethod, compress, decompress};
        let mut output = [0u8; 16];
        assert!(
            decompress(CompressionMethod::Oodle, &[0; 16], &mut output)
                .unwrap_err()
                .to_string()
                .contains("unsupported")
        );
        assert!(
            compress(CompressionMethod::Oodle, &[0; 16], Vec::<u8>::new())
                .unwrap_err()
                .to_string()
                .contains("unsupported")
        );
    }
}

#[cfg(test)]
mod dependency_tests {
    #[test]
    fn resolved_graph_contains_no_downloader_or_tls_stack() {
        let lock = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.lock"),
        )
        .unwrap();
        for package in [
            "oodle_loader",
            "ureq",
            "reqwest",
            "rustls",
            "rustls-webpki",
            "hyper",
            "native-tls",
            "openssl",
        ] {
            assert!(
                !lock.lines().any(|line| line == format!("name = \"{package}\"")),
                "Forbidden resolved dependency {package}"
            );
        }
    }
}

#[cfg(test)]
mod request_tests {
    use super::*;
    #[test]
    fn request_rejects_unrecognised_fields_and_foreign_builds_before_io() {
        assert!(serde_json::from_str::<Request>(r#"{"paks":"x","mappings":"x","output":"x","progress":"x","error":"x","gameHash":"x","download":true}"#).is_err());
        let request:Request=serde_json::from_str(r#"{"paks":"x","mappings":"x","output":"x","progress":"x","error":"x","gameHash":"unsupported"}"#).unwrap();
        assert!(
            request.validate(Path::new("unused")).unwrap_err().to_string().contains("Unsupported")
        );
    }
}
