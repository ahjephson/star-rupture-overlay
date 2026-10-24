use std::collections::{BTreeMap, HashMap, HashSet};

use crate::mem::Ctx;
use anyhow::Result;
use futures_util::StreamExt;
use jmap::{Address, ObjectType};

pub async fn analyze_vtables(
    mem: &Ctx,
    objects: &mut BTreeMap<String, ObjectType>,
) -> BTreeMap<Address, Vec<Address>> {
    let mut class_vtables: HashMap<String, Address> = HashMap::new();
    let mut grouped: BTreeMap<Address, HashSet<&str>> = Default::default();
    for (path, obj) in &mut *objects {
        let object = obj.get_object();
        let vtable = object.vtable;
        let class = &object.class;
        if let Some(existing) = class_vtables.get(class) {
            if *existing != vtable {
                crate::warn!(
                    "conflicting vtable for Class={class:?} Object={path:?}: existing {existing}, new {vtable}"
                );
            }
        } else {
            class_vtables.insert(class.to_string(), vtable);
        }
        grouped.entry(vtable).or_default().insert(class);
    }

    // for (i, (vtable, classes)) in grouped.iter().enumerate() {
    //     println!("{i} {vtable:08x} {classes:?}");
    // }

    async fn read_ptr(mem: &Ctx, addr: u64) -> Result<u64> {
        let mut buf = [0; 8];
        mem.read_buf(addr, &mut buf).await?;
        Ok(u64::from_le_bytes(buf))
    }
    async fn is_valid(mem: &Ctx, addr: u64) -> bool {
        // TODO check for executable bit, not just valid memory
        let mut buf = [0; 1];
        mem.read_buf(addr, &mut buf).await.is_ok()
    }

    let mut vtables: BTreeMap<Address, Vec<Address>> = Default::default();

    let entries: Vec<(Address, Option<Address>)> = {
        let keys: Vec<Address> = grouped.keys().copied().collect();
        (0..keys.len())
            .map(|i| (keys[i], keys.get(i + 1).copied()))
            .collect()
    };

    let walks = futures_util::stream::iter(entries)
        .map(|(vtable, bound)| async move {
            let mut addr = vtable;
            let mut funcs = vec![];
            loop {
                if bound.is_some_and(|ptr| addr >= ptr) {
                    break;
                }
                // bad func ptr or unreadable: end of vtable (can't .await in a match guard, so check validity in the body)
                let Ok(ptr) = read_ptr(mem, addr.0).await else {
                    break;
                };
                if !is_valid(mem, ptr).await {
                    break;
                }
                funcs.push(ptr.into());
                addr.0 += 8;
            }
            (vtable, funcs)
        })
        .buffer_unordered(crate::dump_concurrency());
    let mut walks = std::pin::pin!(walks);
    while let Some((vtable, funcs)) = walks.next().await {
        assert!(vtables.insert(vtable, funcs).is_none());
    }

    // trim vtables as they must be bounded by size of child vtable
    for (path, obj) in &*objects {
        if obj.get_class().is_some() {
            let mut class = path.as_str();
            let Some(vtable_ptr) = class_vtables.get(class) else {
                // println!("no vtable found for class {class}");
                continue;
            };
            let mut vtable_len = vtables.get(vtable_ptr).unwrap().len();

            while let Some(parent) = objects
                .get(class)
                .unwrap()
                .get_struct()
                .unwrap()
                .super_struct
                .as_deref()
            {
                class = parent;
                let Some(vtable_ptr) = class_vtables.get(class) else {
                    // println!("no vtable found for class {class}");
                    continue;
                };
                let vtable = vtables.get_mut(vtable_ptr).unwrap();
                if vtable.len() > vtable_len {
                    // println!(
                    //     "trimming vtable {} -> {} ({}) for {class}",
                    //     vtable.len(),
                    //     vtable_len,
                    //     vtable.len() - vtable_len
                    // );
                    vtable.truncate(vtable_len);
                }
                vtable_len = vtable.len();
            }
        }
    }

    // update UClass::instance_vtable
    for (class, vtable) in class_vtables {
        if let Some(ObjectType::Class(class)) = objects.get_mut(&class) {
            class.instance_vtable = Some(vtable)
        }
    }

    // {
    //     fn get_class<'a>(
    //         objects: &'a BTreeMap<String, ObjectType>,
    //         class: &str,
    //     ) -> &'a jmap::Class {
    //         objects.get(class).unwrap().get_class().unwrap()
    //     }
    //     // let mut class = "/Script/FSD.EnemyTemperatureComponent";
    //     // let mut class = "/Script/FSD.FSDGameInstance";
    //     // let mut class = "/Script/FSD.TagVanitySeasonalEvent";
    //     let mut class = "/Script/FSD.FSDGameMode";
    //     let vtable_ptr = get_class(objects, class).instance_vtable.unwrap();
    //     let vtable = vtables.get(&vtable_ptr).unwrap();
    //     println!("vtable_ptr={:08x}", vtable_ptr.0);
    //     let mut funcs: Vec<(Address, &str)> = vtable.iter().map(|func| (*func, class)).collect();

    //     println!("hierarchy:");
    //     println!("{class}");
    //     while let Some(parent) = objects
    //         .get(class)
    //         .unwrap()
    //         .get_struct()
    //         .unwrap()
    //         .super_struct
    //         .as_deref()
    //     {
    //         class = parent;
    //         println!("{}", class);
    //         if let Some(vtable_ptr) = get_class(objects, class).instance_vtable {
    //             let vtable = vtables.get(&vtable_ptr).unwrap();
    //             for (i, func) in vtable.iter().enumerate() {
    //                 if funcs[i].0 == *func {
    //                     funcs[i].1 = class;
    //                 }
    //             }
    //         } else {
    //             println!("no vtable found for class {class}");
    //             break;
    //         }
    //     }

    //     for (i, (func, class)) in funcs.iter().enumerate() {
    //         println!("{i:>4} ptr={:08x} owner={class}", func.0);
    //     }
    // }

    vtables
}
