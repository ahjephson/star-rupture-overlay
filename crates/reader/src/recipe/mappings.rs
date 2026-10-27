//! One-time native property schema for local asset preparation, never a HUD read.
use crate::{path, stamp};
use anyhow::{Context, Result, ensure};
use jmap_dumper::{
    mem::{Ctx, Ptr},
    objects::{UObject, UStruct},
};
use std::collections::{BTreeMap, BTreeSet};

fn name(path: &str) -> &str {
    path.rsplit(['.', ':']).next().unwrap_or(path)
}

fn inner(value: &jmap::PropertyType, depth: usize) -> Result<usmap::PropertyInner> {
    ensure!(depth < 32, "Native schema nesting bound");
    use jmap::PropertyType as T;
    use usmap::PropertyInner as U;
    Ok(match value {
        T::Struct {
            r#struct,
        } => U::Struct {
            name: name(r#struct).into(),
        },
        T::Str => U::Str,
        T::Name => U::Name,
        T::Text => U::Text,
        T::MulticastInlineDelegate {
            ..
        }
        | T::MulticastSparseDelegate {
            ..
        }
        | T::MulticastDelegate {
            ..
        } => U::MulticastDelegate,
        T::Delegate {
            ..
        } => U::Delegate,
        T::Bool {
            ..
        } => U::Bool,
        T::Array {
            inner: p,
        } => U::Array {
            inner: inner(&p.r#type, depth + 1)?.into(),
        },
        T::Enum {
            container,
            r#enum,
        } => U::Enum {
            inner: inner(&container.r#type, depth + 1)?.into(),
            name: r#enum.as_deref().map(name).unwrap_or("None").into(),
        },
        T::Map {
            key_prop,
            value_prop,
        } => U::Map {
            key: inner(&key_prop.r#type, depth + 1)?.into(),
            value: inner(&value_prop.r#type, depth + 1)?.into(),
        },
        T::Set {
            key_prop,
        } => U::Set {
            key: inner(&key_prop.r#type, depth + 1)?.into(),
        },
        T::Byte {
            r#enum: Some(e),
        } => U::Enum {
            inner: U::Byte.into(),
            name: name(e).into(),
        },
        T::Byte {
            r#enum: None,
        } => U::Byte,
        T::Float => U::Float,
        T::Double => U::Double,
        T::UInt16 => U::UInt16,
        T::UInt32 => U::UInt32,
        T::UInt64 => U::UInt64,
        T::Int8 => U::Int8,
        T::Int16 => U::Int16,
        T::Int => U::Int,
        T::Int64 => U::Int64,
        T::Object {
            ..
        }
        | T::Class {
            ..
        } => U::Object,
        T::WeakObject {
            ..
        } => U::WeakObject,
        T::SoftObject {
            ..
        }
        | T::SoftClass {
            ..
        } => U::SoftObject,
        T::LazyObject {
            ..
        } => U::LazyObject,
        T::Interface {
            ..
        } => U::Interface,
        T::FieldPath => U::FieldPath,
        T::Optional {
            inner: p,
        } => U::Optional {
            inner: inner(&p.r#type, depth + 1)?.into(),
        },
        T::Utf8Str => U::Utf8Str,
        T::AnsiStr => U::AnsiStr,
    })
}

pub(crate) async fn generate(ctx: &Ctx) -> Result<Vec<u8>> {
    let mut structs = BTreeMap::new();
    let mut enums = BTreeMap::new();
    let mut types = BTreeMap::new();
    // The checked registry reader batches pointers and rereads its headers.
    // This scan happens only when the build's derived asset cache is missing.
    for (index, address) in super::runtime::registered_objects(ctx).await? {
        let object = Ptr::<UObject>::new(address, ctx.clone())?;
        if object.internal_index().read().await.ok() != Some(index as i32) {
            continue;
        }
        let class = object.class_private().read().await?;
        let kind = if let Some(kind) = types.get(&class.address()) {
            *kind
        } else {
            let kind = class.cast::<UObject>().name_private().read().await?;
            let kind = match kind.as_str() {
                "Class" => 1,
                "ScriptStruct" => 2,
                "Enum" => 3,
                _ => 0,
            };
            types.insert(class.address(), kind);
            kind
        };
        if kind == 0
            || object.object_flags().read().await?.bits() & (0x10 | 0x20 | 0x8000 | 0x10000) != 0
        {
            continue;
        }
        let full = path(ctx, address).await?;
        if !full.starts_with("/Script/") {
            continue;
        }
        let before = stamp(ctx, address).await?;
        if kind == 3 {
            let e = jmap_dumper::read_enum(&object.cast()).await?;
            ensure!(e.names.len() <= 65536, "Native enum bound");
            let entries = e
                .names
                .into_iter()
                .map(|(n, v)| (v, name(&n).rsplit("::").next().unwrap_or(&n).to_string()))
                .collect();
            ensure!(
                enums
                    .insert(
                        name(&full).to_string(),
                        usmap::Enum {
                            name: name(&full).into(),
                            entries
                        }
                    )
                    .is_none(),
                "Duplicate native enum name"
            );
        } else {
            let structure = object.cast::<UStruct>();
            let parent = match structure.super_struct().read().await? {
                Some(p) => Some(name(&path(ctx, p.address()).await?).to_string()),
                None => None,
            };
            let mut fields = structure.properties(false);
            let mut properties = Vec::new();
            let mut index = 0usize;
            let mut visited = BTreeSet::new();
            while let Some(field) = fields.next().await {
                let field = field?;
                ensure!(
                    visited.insert(field.address()) && visited.len() <= 4096,
                    "Native property chain bound"
                );
                let p = jmap_dumper::read_prop_type(&field.cast()).await?;
                ensure!(
                    p.array_dim > 0
                        && p.array_dim <= 255
                        && index + p.array_dim <= u16::MAX as usize,
                    "Native property dimension bound"
                );
                properties.push(usmap::Property {
                    name: p.name,
                    array_dim: p.array_dim.try_into()?,
                    index: index.try_into()?,
                    inner: inner(&p.r#type, 0)?,
                });
                index += p.array_dim;
            }
            ensure!(
                structs
                    .insert(
                        name(&full).to_string(),
                        usmap::Struct {
                            name: name(&full).into(),
                            super_struct: parent,
                            properties
                        }
                    )
                    .is_none(),
                "Duplicate native structure name"
            );
        }
        ensure!(stamp(ctx, address).await? == before, "Native schema allocation changed");
        ensure!(structs.len() + enums.len() <= 32768, "Native schema type bound");
    }
    for required in ["Texture2D", "CrItemRecipeData"] {
        ensure!(structs.contains_key(required), "Missing native asset type {required}");
    }
    let schema = usmap::Usmap {
        structs: structs.into_values().collect(),
        enums: enums.into_values().collect(),
        cext: None,
        eatr: None,
        envp: None,
        ppth: None,
    };
    let mut output = std::io::Cursor::new(Vec::new());
    schema.write(&mut output).context("Writing local asset schema")?;
    let bytes = output.into_inner();
    ensure!(bytes.len() <= 16 * 1024 * 1024, "Native schema output bound");
    Ok(bytes)
}
