//! Bounded cooked-property decoding shared by recipe and codex preparation.
//! Format references: CUE4Parse ec6595e4
//! (Apache-2.0) and unreal_asset 0.1.16 (MIT). This is not a general UE parser.
use anyhow::{Context, Result, bail, ensure};
use retoc::legacy_asset::{FLegacyPackageHeader, FMinimalName};
use serde_json::{Map, Value, json};
use usmap::{PropertyInner as T, Usmap};
pub struct Reader<'a> {
    pub bytes: &'a [u8],
    pub pos: usize,
    pub package: &'a FLegacyPackageHeader,
    pub schemas: &'a Usmap,
}
impl<'a> Reader<'a> {
    pub fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(n).context("Length overflow")?;
        let b = self.bytes.get(self.pos..end).context("Export boundary exceeded")?;
        self.pos = end;
        Ok(b)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into()?))
    }
    pub fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into()?))
    }
    fn f32(&mut self) -> Result<f32> {
        let n = f32::from_le_bytes(self.take(4)?.try_into()?);
        ensure!(n.is_finite(), "Nonfinite float");
        Ok(n)
    }
    fn f64(&mut self) -> Result<f64> {
        let n = f64::from_le_bytes(self.take(8)?.try_into()?);
        ensure!(n.is_finite(), "Nonfinite double");
        Ok(n)
    }
    pub fn count(&mut self) -> Result<usize> {
        let n = self.i32()?;
        ensure!((0..=16384).contains(&n), "Collection count {n}");
        Ok(n as usize)
    }
    pub fn string(&mut self) -> Result<String> {
        let n = self.i32()?;
        ensure!(n.unsigned_abs() <= 65536, "String bound");
        if n == 0 {
            return Ok(String::new());
        }
        if n > 0 {
            let b = self.take(n as usize)?;
            ensure!(b.last() == Some(&0), "String terminator");
            Ok(String::from_utf8(b[..b.len() - 1].to_vec())?)
        } else {
            let b = self.take(n.unsigned_abs() as usize * 2)?;
            let w = b
                .as_chunks::<2>()
                .0
                .iter()
                .map(|x| u16::from_le_bytes([x[0], x[1]]))
                .collect::<Vec<_>>();
            ensure!(w.last() == Some(&0), "Wide terminator");
            Ok(String::from_utf16(&w[..w.len() - 1])?)
        }
    }
    fn name(&mut self) -> Result<String> {
        let index = self.i32()?;
        let number = self.i32()?;
        ensure!(
            index >= 0 && (index as usize) < self.package.name_map.num_names() && number >= 0,
            "Name bounds"
        );
        Ok(self
            .package
            .name_map
            .get(FMinimalName {
                index,
                number,
            })?
            .into_owned())
    }
    pub fn reference(&self, index: i32) -> Result<Value> {
        if index == 0 {
            return Ok(Value::Null);
        }
        let mut cursor = index;
        let mut names = Vec::new();
        for _ in 0..64 {
            if cursor == 0 {
                names.reverse();
                return Ok(json!(names.join(".")));
            }
            let (name, outer) = if cursor < 0 {
                let i = usize::try_from(-(cursor as i64) - 1)?;
                let v = self.package.imports.get(i).context("Import bound")?;
                (v.object_name, v.outer_index.index)
            } else {
                let v = self.package.exports.get(cursor as usize - 1).context("Export bound")?;
                (v.object_name, v.outer_index.index)
            };
            names.push(self.package.name_map.get(name)?.into_owned());
            cursor = outer;
        }
        bail!("Reference cycle/depth")
    }
    fn fields(&self, mut name: String) -> Result<Vec<usmap::Property>> {
        let mut fields = Vec::new();
        for _ in 0..64 {
            let s = self
                .schemas
                .structs
                .iter()
                .find(|s| s.name == name)
                .with_context(|| format!("Missing schema {name}"))?;
            let base = fields.len();
            for p in &s.properties {
                ensure!(p.index as usize == fields.len() - base, "Noncontiguous schema index");
                for _ in 0..p.array_dim {
                    fields.push(p.clone());
                }
            }
            match &s.super_struct {
                Some(parent) => name = parent.clone(),
                None => return Ok(fields),
            }
        }
        bail!("Schema inheritance cycle/depth")
    }
    pub fn properties(&mut self, schema: &str, depth: usize) -> Result<Value> {
        ensure!(depth <= 32, "Property recursion bound");
        let fields = self.fields(schema.to_owned())?;
        let mut fragments = Vec::new();
        let mut index = 0usize;
        let mut bits = 0usize;
        for _ in 0..512 {
            let f = self.u16()?;
            index += usize::from(f & 127);
            let count = usize::from(f >> 9);
            ensure!(
                index + count <= fields.len(),
                "Schema {schema} property index {} exceeds {}",
                index + count,
                fields.len()
            );
            let zeros = f & 128 != 0;
            fragments.push((index, count, zeros));
            index += count;
            if zeros {
                bits += count
            }
            if f & 256 != 0 {
                break;
            }
        }
        ensure!(fragments.len() < 512, "Fragment bound");
        let n = if bits == 0 {
            0
        } else if bits <= 8 {
            1
        } else if bits <= 16 {
            2
        } else {
            bits.div_ceil(32) * 4
        };
        let mask = self.take(n)?.to_vec();
        let mut bit = 0;
        let mut out = Map::new();
        for (start, count, zeros) in fragments {
            for field in &fields[start..start + count] {
                let zero = if zeros {
                    let z = mask[bit / 8] & (1 << (bit % 8)) != 0;
                    bit += 1;
                    z
                } else {
                    false
                };
                let value = self
                    .value(&field.inner, zero, depth + 1)
                    .with_context(|| format!("{schema}.{} at {}", field.name, self.pos))?;
                ensure!(out.insert(field.name.clone(), value).is_none(), "Duplicate property name");
            }
        }
        Ok(Value::Object(out))
    }
    fn value(&mut self, t: &T, zero: bool, depth: usize) -> Result<Value> {
        ensure!(depth <= 32, "Value recursion bound");
        if zero {
            return Ok(match t {
                T::Bool => json!(false),
                T::Object => Value::Null,
                T::Array {
                    ..
                }
                | T::Map {
                    ..
                }
                | T::Set {
                    ..
                } => json!([]),
                T::Struct {
                    ..
                } => json!({}),
                T::Str | T::Name => json!(""),
                _ => json!(0),
            });
        }
        Ok(match t {
            T::Byte => json!(self.u8()?),
            T::Bool => {
                let b = self.u8()?;
                ensure!(b <= 1, "Bool value");
                json!(b != 0)
            }
            T::Int => json!(self.i32()?),
            T::Float => json!(self.f32()?),
            T::Double => json!(self.f64()?),
            T::UInt32 => json!(u32::from_le_bytes(self.take(4)?.try_into()?)),
            T::Int64 => json!(i64::from_le_bytes(self.take(8)?.try_into()?)),
            T::UInt64 => json!(u64::from_le_bytes(self.take(8)?.try_into()?)),
            T::Name => json!(self.name()?),
            T::Str => json!(self.string()?),
            T::Object => {
                let index = self.i32()?;
                self.reference(index)?
            }
            T::Enum {
                inner,
                name,
            } => {
                let v = self.value(inner, false, depth + 1)?;
                let n = v.as_i64().context("Enum ordinal")?;
                let entry = self
                    .schemas
                    .enums
                    .iter()
                    .find(|e| &e.name == name)
                    .and_then(|e| e.entries.get(&n));
                match entry {
                    Some(e) => json!(format!("{name}::{e}")),
                    None => bail!("Unmapped enum {name}:{n}"),
                }
            }
            T::Array {
                inner,
            } => {
                let n = self.count()?;
                let mut v = Vec::new();
                for _ in 0..n {
                    if matches!(inner.as_ref(), T::Enum { .. }) {
                        // Cooked enum-array elements carry FNames, unlike scalar enum ordinals.
                        v.push(json!(self.name()?));
                    } else {
                        v.push(self.value(inner, false, depth + 1)?)
                    }
                }
                json!(v)
            }
            T::Map {
                key,
                value,
            } => {
                let removed = self.count()?;
                ensure!(removed == 0, "Map removals unsupported");
                let n = self.count()?;
                let mut v = Vec::new();
                for _ in 0..n {
                    // Cooked enum map keys use FNames; struct keys retain their property header.
                    let k = if matches!(key.as_ref(), T::Enum { .. }) {
                        json!(self.name()?)
                    } else {
                        self.value(key, false, depth + 1)?
                    };
                    v.push(json!({"Key":k,"Value":self.value(value,false,depth+1)?}))
                }
                json!(v)
            }
            T::SoftObject => {
                let package = self.name()?;
                let asset = self.name()?;
                let sub = self.string()?;
                json!({"package":package,"asset":asset,"subpath":sub})
            }
            // UE5 FFieldPath: bounded FName path followed by its serialized owner.
            // See CUE4Parse UE4/Objects/UObject/FFieldPath.cs (FFieldPathOwnerSerialization).
            T::FieldPath => {
                let count = self.count()?;
                ensure!(count <= 64, "Field path component bound");
                let mut path = Vec::with_capacity(count);
                for _ in 0..count {
                    path.push(self.name()?);
                }
                let owner = self.i32()?;
                json!({"Path":path,"ResolvedOwner":self.reference(owner)?})
            }
            T::Text => {
                let flags = self.i32()?;
                let history = self.u8()?;
                match history {
                    11 => json!({"flags":flags,"TableId":self.name()?,"Key":self.string()?}),
                    0 => {
                        json!({"flags":flags,"Namespace":self.string()?,"Key":self.string()?,"SourceString":self.string()?})
                    }
                    255 => {
                        let has = self.i32()?;
                        ensure!(has == 0 || has == 1, "Text boolean");
                        json!({"CultureInvariantString":if has==1{Some(self.string()?)}else{None}})
                    }
                    _ => bail!("Unsupported text history {history}"),
                }
            }
            T::Struct {
                name,
            } => match name.as_str() {
                "GameplayTagContainer" => {
                    let n = self.count()?;
                    let mut tags = Vec::new();
                    for _ in 0..n {
                        tags.push(json!({"TagName":self.name()?}))
                    }
                    json!(tags)
                }
                "Vector" => json!({"X":self.f64()?,"Y":self.f64()?,"Z":self.f64()?}),
                "Quat" => json!({"X":self.f64()?,"Y":self.f64()?,"Z":self.f64()?,"W":self.f64()?}),
                "Rotator" => json!({"Pitch":self.f64()?,"Yaw":self.f64()?,"Roll":self.f64()?}),
                "Box" => {
                    json!({"min":[self.f64()?,self.f64()?,self.f64()?],"max":[self.f64()?,self.f64()?,self.f64()?],"valid":self.u8()?})
                }
                "Guid" => {
                    json!(self.take(16)?.iter().map(|b| format!("{b:02x}")).collect::<String>())
                }
                "IntPoint" => json!({"X":self.i32()?,"Y":self.i32()?}),
                "Vector2D" | "DeprecateSlateVector2D" | "Vector2f" => {
                    json!({"X":self.f32()?,"Y":self.f32()?})
                }
                "LinearColor" => {
                    json!({"R":self.f32()?,"G":self.f32()?,"B":self.f32()?,"A":self.f32()?})
                }
                "Color" => json!(self.take(4)?),
                _ => self.properties(name, depth + 1)?,
            },
            _ => bail!("Unsupported property kind {t:?}"),
        })
    }
}
pub fn schema_for_export(package: &FLegacyPackageHeader, export: usize) -> Result<String> {
    let mut class = package.exports[export].class_index;
    for _ in 0..64 {
        if class.is_import() {
            let i =
                package.imports.get(class.to_import_index() as usize).context("Class import")?;
            return Ok(package.name_map.get(i.object_name)?.into_owned());
        }
        ensure!(class.is_export(), "Null class");
        class = package
            .exports
            .get(class.to_export_index() as usize)
            .context("Class export")?
            .super_index;
    }
    bail!("Class inheritance bound")
}

impl Reader<'_> {
    pub fn object_tail(&mut self) -> Result<()> {
        ensure!(self.i32()? == 0, "Object GUID tail unsupported");
        Ok(())
    }
    pub fn data_table_rows(&mut self, row_schema: &str) -> Result<Vec<Value>> {
        self.object_tail()?;
        let count = self.count()?;
        ensure!(count <= 1024, "Data table row bound");
        let mut names = std::collections::BTreeSet::new();
        let mut rows = Vec::with_capacity(count);
        for _ in 0..count {
            ensure!(names.insert(self.name()?), "Duplicate data table row");
            rows.push(self.properties(row_schema, 0)?);
        }
        ensure!(self.pos == self.bytes.len(), "Data table tail");
        Ok(rows)
    }

    pub fn string_table(&mut self) -> Result<Value> {
        self.object_tail()?;
        let namespace = self.string()?;
        let n = self.count()?;
        let mut entries = Map::new();
        for _ in 0..n {
            let k = self.string()?;
            let v = self.string()?;
            ensure!(entries.insert(k, json!(v)).is_none(), "Duplicate string key");
        }
        let n = self.count()?;
        for _ in 0..n {
            self.string()?;
            let count = self.count()?;
            for _ in 0..count {
                self.name()?;
                self.string()?;
            }
        }
        ensure!(self.pos == self.bytes.len(), "String table tail");
        Ok(json!({"namespace":namespace,"entries":entries}))
    }
    pub fn texture_meta(&mut self) -> Result<Value> {
        self.object_tail()?;
        let strip_texture = self.take(2)?;
        ensure!(strip_texture[0] & 1 != 0, "Editor texture data unsupported");
        let strip_2d = self.take(2)?;
        ensure!(strip_2d[0] & 1 != 0, "Editor texture2D unsupported");
        ensure!(
            self.i32()? == 1 && self.i32()? == 1,
            "Only cooked serialized mip textures supported"
        );
        let format_name = self.name()?;
        let skip_start = self.pos;
        let skip = i64::from_le_bytes(self.take(8)?.try_into()?);
        ensure!(skip >= 8, "Texture skip offset");
        let end =
            skip_start.checked_add(usize::try_from(skip)?).context("Texture skip overflow")?;
        ensure!(end <= self.bytes.len(), "Texture skip bound");
        let placeholder = self.take(16)?;
        ensure!(placeholder[0] == 0, "Derived texture data unsupported");
        let width = self.i32()?;
        let height = self.i32()?;
        ensure!((1..=1024).contains(&width) && (1..=1024).contains(&height), "Texture dimensions");
        ensure!(self.i32()? == 1, "Non-simple texture unsupported");
        let format = self.string()?;
        ensure!(
            format == format_name
                && matches!(format.as_str(), "PF_DXT5" | "PF_B8G8R8A8" | "PF_FloatRGBA"),
            "Unsupported texture format {format}"
        );
        ensure!(self.i32()? == 0 && self.i32()? == 1, "Only first single mip supported");
        let index = self.i32()?;
        ensure!(
            index == 0 && self.package.data_resources.len() == 1,
            "Single bulk data resource required"
        );
        let resource = &self.package.data_resources[0];
        let offset = usize::try_from(resource.serial_offset)?;
        let size = usize::try_from(resource.serial_size)?;
        ensure!(self.pos == offset, "Inline bulk offset mismatch");
        ensure!(
            resource.raw_size == resource.serial_size && resource.legacy_bulk_data_flags == 0x48,
            "Unsupported bulk flags or compressed data"
        );
        self.take(size)?;
        ensure!(
            self.i32()? == width && self.i32()? == height && self.i32()? == 1,
            "Mip dimensions"
        );
        ensure!(self.i32()? == 0, "Virtual texture unsupported");
        ensure!(self.pos == end, "Texture platform tail mismatch");
        ensure!(self.name()? == "None" && self.pos == self.bytes.len(), "Texture end mismatch");
        Ok(json!({"width":width,"height":height,"format":format,"offset":offset,"size":size}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn schema(fields: usize) -> Usmap {
        Usmap {
            enums: vec![],
            structs: vec![usmap::Struct {
                name: "Test".into(),
                super_struct: None,
                properties: (0..fields)
                    .map(|i| usmap::Property {
                        name: format!("P{i}"),
                        array_dim: 1,
                        index: i as u16,
                        inner: T::Bool,
                    })
                    .collect(),
            }],
            cext: None,
            ppth: None,
            eatr: None,
            envp: None,
        }
    }
    fn read(bytes: &[u8], mappings: &Usmap) -> Result<(Value, usize)> {
        let package = FLegacyPackageHeader::default();
        let mut r = Reader {
            bytes,
            pos: 0,
            package: &package,
            schemas: mappings,
        };
        let v = r.properties("Test", 0)?;
        Ok((v, r.pos))
    }
    #[test]
    fn zero_mask_storage_sizes() {
        for n in [1usize, 8, 9, 16, 17, 63] {
            let header = 0x180u16 | ((n as u16) << 9);
            let mut b = header.to_le_bytes().to_vec();
            let mask_bytes = if n <= 8 {
                1
            } else if n <= 16 {
                2
            } else {
                n.div_ceil(32) * 4
            };
            b.extend(vec![255; mask_bytes]);
            let (v, p) = read(&b, &schema(n)).unwrap();
            assert_eq!(p, b.len());
            assert_eq!(v.as_object().unwrap().len(), n);
            assert!(v.as_object().unwrap().values().all(|v| v == false));
        }
    }
    #[test]
    fn skip_fragments_and_mixed_zero_values() {
        let mut b = Vec::new();
        b.extend_from_slice(&(2u16 | (1 << 9)).to_le_bytes());
        b.extend_from_slice(&(1u16 | 128 | 256 | (2 << 9)).to_le_bytes());
        b.extend_from_slice(&[1, 1, 1]);
        let (v, p) = read(&b, &schema(6)).unwrap();
        assert_eq!(p, b.len());
        assert_eq!(v, json!({"P2":true,"P4":false,"P5":true}));
    }
    #[test]
    fn wide_property_indices() {
        let mut b = Vec::new();
        for _ in 0..2 {
            b.extend_from_slice(&127u16.to_le_bytes());
        }
        b.extend_from_slice(&(7u16 | 256 | (1 << 9)).to_le_bytes());
        b.push(1);
        let (v, _) = read(&b, &schema(262)).unwrap();
        assert_eq!(v, json!({"P261":true}));
    }
    #[test]
    fn every_truncation_rejected() {
        let b = [0x80, 0x05, 0x00, 1, 1];
        for end in 0..b.len() {
            assert!(read(&b[..end], &schema(2)).is_err(), "prefix {end}");
        }
        assert!(read(&b, &schema(2)).is_ok());
    }
    #[test]
    fn malformed_index_and_boolean_rejected() {
        assert!(read(&[0x05, 0x03, 1], &schema(1)).is_err());
        assert!(read(&[0x00, 0x03, 2], &schema(1)).is_err());
    }
    #[test]
    fn inheritance_and_unknown_type_rejected() {
        let mut mappings = schema(1);
        mappings.structs[0].super_struct = Some("Missing".into());
        assert!(read(&[0, 1], &mappings).is_err());
        let mut mappings = schema(1);
        mappings.structs[0].properties[0].inner = T::Unknown;
        assert!(read(&[0, 3, 0], &mappings).is_err());
    }
}

#[cfg(test)]
mod research_tests {
    use super::*;
    use retoc::legacy_asset::FPackageNameMap;

    fn mappings() -> Usmap {
        Usmap {
            enums: vec![],
            structs: vec![],
            cext: None,
            ppth: None,
            eatr: None,
            envp: None,
        }
    }

    #[test]
    fn enum_map_names_and_field_paths_preserve_following_values() -> Result<()> {
        let package = FLegacyPackageHeader {
            name_map: FPackageNameMap::create_from_names(vec!["EPart::Dot".into(), "Ammo".into()]),
            ..Default::default()
        };
        let schemas = mappings();
        let mut bytes = 0i32.to_le_bytes().to_vec();
        bytes.extend_from_slice(&1i32.to_le_bytes());
        bytes.extend_from_slice(&0i32.to_le_bytes());
        bytes.extend_from_slice(&0i32.to_le_bytes());
        bytes.extend_from_slice(&7i32.to_le_bytes());
        bytes.extend_from_slice(&123i32.to_le_bytes());
        let ty = T::Map {
            key: Box::new(T::Enum {
                inner: Box::new(T::Byte),
                name: "EPart".into(),
            }),
            value: Box::new(T::Int),
        };
        let mut reader = Reader {
            bytes: &bytes,
            pos: 0,
            package: &package,
            schemas: &schemas,
        };
        assert_eq!(reader.value(&ty, false, 0)?, json!([{"Key":"EPart::Dot","Value":7}]));
        assert_eq!(reader.i32()?, 123);
        let mut bytes = 1i32.to_le_bytes().to_vec();
        bytes.extend_from_slice(&1i32.to_le_bytes());
        bytes.extend_from_slice(&0i32.to_le_bytes());
        bytes.extend_from_slice(&0i32.to_le_bytes());
        for end in 0..bytes.len() {
            let mut reader = Reader {
                bytes: &bytes[..end],
                pos: 0,
                package: &package,
                schemas: &schemas,
            };
            assert!(reader.value(&T::FieldPath, false, 0).is_err());
        }
        bytes.extend_from_slice(&456i32.to_le_bytes());
        let mut reader = Reader {
            bytes: &bytes,
            pos: 0,
            package: &package,
            schemas: &schemas,
        };
        assert_eq!(
            reader.value(&T::FieldPath, false, 0)?,
            json!({"Path":["Ammo"],"ResolvedOwner":null})
        );
        assert_eq!(reader.i32()?, 456);
        Ok(())
    }
    #[test]
    fn gameplay_tag_map_key_keeps_its_struct_header() -> Result<()> {
        let package = FLegacyPackageHeader {
            name_map: FPackageNameMap::create_from_names(vec!["Hud.Ammo".into()]),
            ..Default::default()
        };
        let mut schemas = mappings();
        schemas.structs.push(usmap::Struct {
            name: "GameplayTag".into(),
            super_struct: None,
            properties: vec![usmap::Property {
                name: "TagName".into(),
                array_dim: 1,
                index: 0,
                inner: T::Name,
            }],
        });
        let mut bytes = 0i32.to_le_bytes().to_vec();
        bytes.extend_from_slice(&1i32.to_le_bytes());
        bytes.extend_from_slice(&0x0300u16.to_le_bytes());
        bytes.extend_from_slice(&0i32.to_le_bytes());
        bytes.extend_from_slice(&0i32.to_le_bytes());
        bytes.extend_from_slice(&37i32.to_le_bytes());
        let ty = T::Map {
            key: Box::new(T::Struct {
                name: "GameplayTag".into(),
            }),
            value: Box::new(T::Int),
        };
        for end in 0..bytes.len() {
            let mut reader = Reader {
                bytes: &bytes[..end],
                pos: 0,
                package: &package,
                schemas: &schemas,
            };
            assert!(reader.value(&ty, false, 0).is_err());
        }
        bytes.extend_from_slice(&99i32.to_le_bytes());
        let mut reader = Reader {
            bytes: &bytes,
            pos: 0,
            package: &package,
            schemas: &schemas,
        };
        assert_eq!(
            reader.value(&ty, false, 0)?,
            json!([{"Key":{"TagName":"Hud.Ammo"},"Value":37}])
        );
        assert_eq!(reader.i32()?, 99);
        Ok(())
    }
    #[test]
    fn native_gameplay_tags_use_names_and_reject_truncation() -> Result<()> {
        let package = FLegacyPackageHeader {
            name_map: FPackageNameMap::create_from_names(vec!["Item.Ammo".into()]),
            ..Default::default()
        };
        let schemas = mappings();
        let mut bytes = 1i32.to_le_bytes().to_vec();
        bytes.extend_from_slice(&0i32.to_le_bytes());
        bytes.extend_from_slice(&0i32.to_le_bytes());
        let ty = T::Struct {
            name: "GameplayTagContainer".into(),
        };
        for end in 0..bytes.len() {
            let mut reader = Reader {
                bytes: &bytes[..end],
                pos: 0,
                package: &package,
                schemas: &schemas,
            };
            assert!(reader.value(&ty, false, 0).is_err());
        }
        bytes.extend_from_slice(&77i32.to_le_bytes());
        let mut reader = Reader {
            bytes: &bytes,
            pos: 0,
            package: &package,
            schemas: &schemas,
        };
        assert_eq!(reader.value(&ty, false, 0)?, json!([{"TagName":"Item.Ammo"}]));
        assert_eq!(reader.i32()?, 77);
        Ok(())
    }
    #[test]
    fn enum_array_uses_names_and_preserves_following_field() -> Result<()> {
        let package = FLegacyPackageHeader {
            name_map: FPackageNameMap::create_from_names(vec![
                "EExample::First".into(),
                "EExample::Second".into(),
            ]),
            ..Default::default()
        };
        let mut bytes = 2i32.to_le_bytes().to_vec();
        for index in [0i32, 1] {
            bytes.extend_from_slice(&index.to_le_bytes());
            bytes.extend_from_slice(&0i32.to_le_bytes());
        }
        bytes.extend_from_slice(&123i32.to_le_bytes());
        let schemas = mappings();
        let mut reader = Reader {
            bytes: &bytes,
            pos: 0,
            package: &package,
            schemas: &schemas,
        };
        let ty = T::Array {
            inner: Box::new(T::Enum {
                inner: Box::new(T::Byte),
                name: "EExample".into(),
            }),
        };
        assert_eq!(reader.value(&ty, false, 0)?, json!(["EExample::First", "EExample::Second"]));
        assert_eq!(reader.i32()?, 123);
        assert_eq!(reader.pos, bytes.len());
        Ok(())
    }

    #[test]
    fn native_vector_rejects_truncation_and_nonfinite_values() -> Result<()> {
        let package = FLegacyPackageHeader::default();
        let schemas = mappings();
        let bytes: Vec<_> = [12.0f64, -5.0, 70.0].into_iter().flat_map(f64::to_le_bytes).collect();
        let ty = T::Struct {
            name: "Vector".into(),
        };
        for end in 0..bytes.len() {
            let mut reader = Reader {
                bytes: &bytes[..end],
                pos: 0,
                package: &package,
                schemas: &schemas,
            };
            assert!(reader.value(&ty, false, 0).is_err());
        }
        let mut reader = Reader {
            bytes: &bytes,
            pos: 0,
            package: &package,
            schemas: &schemas,
        };
        assert_eq!(reader.value(&ty, false, 0)?, json!({"X":12.0,"Y":-5.0,"Z":70.0}));
        let invalid = f64::NAN.to_le_bytes();
        let mut reader = Reader {
            bytes: &invalid,
            pos: 0,
            package: &package,
            schemas: &schemas,
        };
        assert!(reader.value(&ty, false, 0).is_err());
        Ok(())
    }
}
