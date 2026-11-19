use super::super::{load, load_bounded, object, properties};
use anyhow::{Context, Result, ensure};
use retoc::{
    FPackageId, asset_conversion::FZenPackageContext, iostore::IoStoreTrait, logging::Log,
};
use serde_json::Value;
use starrupture_observation_model::shutdown::StopSignal;
use std::collections::BTreeMap;
#[derive(Clone)]
pub(super) struct Export {
    pub name: String,
    pub path: String,
    pub props: Value,
}
pub(super) struct Archive<'a> {
    context: FZenPackageContext<'a>,
    pub paths: BTreeMap<String, FPackageId>,
    schemas: usmap::Usmap,
    cache: BTreeMap<(String, String), Vec<Export>>,
    stop: &'a StopSignal,
    pub packages: usize,
    bytes: usize,
}
pub(super) fn native_path(path: &str) -> Option<String> {
    let value = if let Some(s) = path.strip_prefix("../../../StarRupture/Content/") {
        format!("/Game/{s}")
    } else {
        let s = path.strip_prefix("../../../StarRupture/Plugins/GameFeatures/")?;
        let (mount, relative) = s.split_once("/Content/")?;
        format!("/{mount}/{relative}")
    };
    value.strip_suffix(".uasset").map(str::to_owned)
}
impl<'a> Archive<'a> {
    pub fn new(
        store: &'a dyn IoStoreTrait,
        schemas: usmap::Usmap,
        log: &'a Log,
        stop: &'a StopSignal,
    ) -> Result<Self> {
        let mut paths = BTreeMap::new();
        for chunk in store.chunks() {
            ensure!(!stop.is_requested(), "Catalogue indexing cancelled");
            if let Some(id) = chunk.path().as_deref().and_then(native_path) {
                ensure!(
                    paths.insert(id, chunk.id().get_package_id()).is_none(),
                    "Duplicate native archive package"
                );
            }
        }
        ensure!(paths.len() <= 200000, "Native package index bound");
        Ok(Self {
            context: FZenPackageContext::create(store, None, log, None),
            paths,
            schemas,
            cache: BTreeMap::new(),
            stop,
            packages: 0,
            bytes: 0,
        })
    }
    pub fn exports(&mut self, id: &str, schema: &str) -> Result<Vec<Export>> {
        ensure!(!self.stop.is_requested(), "Catalogue preparation cancelled");
        let key = (id.to_owned(), schema.to_owned());
        if let Some(value) = self.cache.get(&key) {
            return Ok(value.clone());
        }
        ensure!(self.packages < 4096, "Catalogue package work bound");
        self.packages += 1;
        let package = load(
            &self.context,
            *self.paths.get(id).with_context(|| format!("Missing catalogue package {id}"))?,
        )?;
        ensure!(package.header.exports.len() <= 512, "Catalogue export bound");
        let mut result = Vec::new();
        for (i, e) in package.header.exports.iter().enumerate() {
            let actual = properties::schema_for_export(&package.header, i)?;
            if if schema == "@CDO" {
                e.object_flags & 16 == 0
            } else {
                actual != schema
            } {
                continue;
            }
            let mut reader = object(&package, i, &self.schemas)?;
            let props = reader
                .properties(&actual, 0)
                .with_context(|| format!("Catalogue {id} {actual}"))?;
            reader.object_tail()?;
            ensure!(reader.pos == reader.bytes.len(), "Catalogue required object tail: {id}");
            self.bytes = self
                .bytes
                .checked_add(serde_json::to_vec(&props)?.len())
                .context("Catalogue metadata byte overflow")?;
            ensure!(self.bytes <= 24 * 1024 * 1024, "Catalogue decoded metadata bound");
            result.push(Export {
                name: package.header.name_map.get(e.object_name)?.into_owned(),
                path: reader
                    .reference(i32::try_from(i + 1)?)?
                    .as_str()
                    .context("Export identity")?
                    .into(),
                props,
            });
        }
        self.cache.insert(key, result.clone());
        Ok(result)
    }
    pub fn one(&mut self, id: &str, schema: &str) -> Result<Export> {
        let mut exports = self.exports(id, schema)?;
        ensure!(exports.len() == 1, "Expected one {schema} export in {id}");
        Ok(exports.remove(0))
    }
    pub fn table(&mut self, id: &str) -> Result<(String, Value)> {
        ensure!(!self.stop.is_requested(), "Catalogue preparation cancelled");
        ensure!(self.packages < 4096, "Catalogue package work bound");
        self.packages += 1;
        let p = load(&self.context, *self.paths.get(id).context("String table package")?)?;
        ensure!(p.header.exports.len() == 1, "String table export count");
        let mut r = object(&p, 0, &self.schemas)?;
        r.properties("StringTable", 0)?;
        let table = r.string_table()?;
        Ok((format!("{id}.{}", p.header.name_map.get(p.header.exports[0].object_name)?), table))
    }
    pub fn icon(&mut self, id: &str) -> Result<Vec<u8>> {
        ensure!(!self.stop.is_requested(), "Catalogue icon preparation cancelled");
        ensure!(self.packages < 4096, "Catalogue package work bound");
        self.packages += 1;
        // A 1024px BGRA texture alone occupies 4 MiB, before metadata or any smaller mip.
        let p = load_bounded(
            &self.context,
            *self.paths.get(id).context("Icon package")?,
            6 * 1024 * 1024,
        )?;
        ensure!(p.header.exports.len() == 1, "Icon export count");
        let mut r = object(&p, 0, &self.schemas)?;
        let props = r.properties("Texture2D", 0)?;
        let meta = r.texture_meta()?;
        super::texture::decode(&r, &meta, &props)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_mounts_are_resolved_from_archive_paths() {
        assert_eq!(native_path("../../../StarRupture/Plugins/GameFeatures/ChimeraWeapon/Content/Data/I_Weapon.uasset").as_deref(),Some("/ChimeraWeapon/Data/I_Weapon"));
        assert!(native_path("other/Content/x.uasset").is_none());
        assert!(native_path("../../../StarRupture/Content/x.ubulk").is_none());
    }
}
