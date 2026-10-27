//! Finite development-only observations for station classification.
use super::*;
use jmap_dumper::objects::{ZArrayProperty, ZBoolProperty, ZStructProperty};
use serde_json::{Value, json};

impl Binding {
    async fn root_references(
        &self,
        ctx: &Ctx,
        base: &RuntimeCache,
        extras: Vec<u64>,
    ) -> Result<Value> {
        let mut queue: Vec<_> = extras.into_iter().map(|a| (a, 0usize)).collect();
        queue.push((base.controller.address, 0usize));
        let mut seen = BTreeSet::new();
        let mut result = Vec::new();
        let mut index = 0;
        while index < queue.len() && index < 96 {
            let (address, depth) = queue[index];
            index += 1;
            if !seen.insert(address) {
                continue;
            }
            let stamp = serial(ctx, address).await?;
            let mut fields = Vec::new();
            let mut class = Some(Ptr::<UStruct>::new(stamp.0.class, ctx.clone())?);
            for _ in 0..24 {
                let Some(c) = class else {
                    break;
                };
                let mut props = c.properties(false);
                let mut count = 0;
                while let Some(p) = props.next().await {
                    count += 1;
                    ensure!(count <= 2048, "UI root property bound");
                    let p = p?;
                    let name = p.zfield().name_private().read().await?;
                    if ![
                        "Player",
                        "HUD",
                        "UI",
                        "Root",
                        "Widget",
                        "Screen",
                        "Subsystem",
                        "Viewport",
                        "GameInstance",
                        "Layer",
                        "Stack",
                        "Layout",
                        "Policy",
                        "Manager",
                        "Slots",
                        "Content",
                    ]
                    .iter()
                    .any(|s| name.contains(s))
                    {
                        continue;
                    }
                    let offset = p.offset_internal().read().await?;
                    let size = p.element_size().read().await?;
                    let flags = format!("{:?}", p.zfield().cast_flags().await?);
                    let mut f = json!({"ownerClass":c.path().await?,"name":name,"offset":offset,"size":size,"flags":format!("{flags:?}")});
                    if size == 8
                        && flags.contains("CASTCLASS_FObjectProperty")
                        && !flags.contains("CASTCLASS_FClassProperty")
                    {
                        let target = ctx.read::<u64>(address + offset as u64).await?;
                        if target != 0
                            && let Ok(s) = serial(ctx, target).await
                        {
                            f["target"] = json!(path(ctx, target).await?);
                            f["targetClass"] = json!(path(ctx, s.0.class).await?);
                            if depth < 8
                                && queue.len() < 96
                                && ![
                                    "PlayerMainCharacter",
                                    "PlayerCameraManager",
                                    "PlayerInput",
                                    "PlayerState",
                                    "ProfessionSelectionMenuWidget",
                                ]
                                .contains(&name.as_str())
                            {
                                queue.push((target, depth + 1));
                            }
                        }
                    }
                    if name == "RootViewportLayouts" {
                        let inner = p.cast::<ZArrayProperty>().inner().read().await?;
                        let ty = inner.cast::<ZStructProperty>().struct_().read().await?;
                        let stride = inner.element_size().read().await? as usize;
                        ensure!(stride > 0 && stride <= 128, "UI layout record bound");
                        let mut ps = ty.cast::<UStruct>().properties(false);
                        let mut members = Vec::new();
                        while let Some(member) = ps.next().await {
                            let member = member?;
                            members.push((
                                member.zfield().name_private().read().await?,
                                member.offset_internal().read().await?,
                                member.element_size().read().await?,
                                format!("{:?}", member.zfield().cast_flags().await?),
                            ));
                            ensure!(members.len() <= 32, "UI layout member bound");
                        }
                        let header = bytes(ctx, address + offset as u64, 16).await?;
                        let count = u32at(&header, 8) as usize;
                        ensure!(count <= 16, "Local-player layout bound");
                        let data = if count == 0 {
                            vec![]
                        } else {
                            bytes(ctx, u64at(&header, 0), count * stride).await?
                        };
                        let mut records = Vec::new();
                        for i in 0..count {
                            let mut values = Vec::new();
                            for (name, off, size, flags) in &members {
                                let mut value =
                                    json!({"name":name,"offset":off,"size":size,"flags":flags});
                                if *size == 8 && flags.contains("CASTCLASS_FObjectProperty") {
                                    let target = u64at(&data, i * stride + *off as usize);
                                    if target != 0
                                        && let Ok(s) = serial(ctx, target).await
                                    {
                                        value["target"] = json!(path(ctx, target).await?);
                                        value["targetClass"] = json!(path(ctx, s.0.class).await?);
                                        if name == "RootLayout" && queue.len() < 96 {
                                            queue.push((target, depth + 1));
                                        }
                                    }
                                }
                                values.push(value);
                            }
                            records.push(values);
                        }
                        f["recordType"] = json!(ty.path().await?);
                        f["stride"] = json!(stride);
                        f["records"] = json!(records);
                    }
                    if (name == "Slots" || name == "WidgetList")
                        && flags.contains("CASTCLASS_FArrayProperty")
                    {
                        let header = bytes(ctx, address + offset as u64, 16).await?;
                        let count = u32at(&header, 8) as usize;
                        ensure!(count <= 256, "UI widget collection bound");
                        let data = if count == 0 {
                            vec![]
                        } else {
                            bytes(ctx, u64at(&header, 0), count * 8).await?
                        };
                        let mut children = Vec::new();
                        for i in 0..count {
                            let target = u64at(&data, i * 8);
                            if target != 0
                                && let Ok(s) = serial(ctx, target).await
                            {
                                children.push(json!({"object":path(ctx,target).await?,"class":path(ctx,s.0.class).await?}));
                                if depth < 8 && queue.len() < 96 {
                                    queue.push((target, depth + 1));
                                }
                            }
                        }
                        f["children"] = json!(children);
                    }
                    fields.push(f);
                }
                class = c.super_struct().read().await?;
            }
            ensure!(serial(ctx, address).await? == stamp, "UI root object changed");
            result.push(json!({"object":path(ctx,address).await?,"class":path(ctx,stamp.0.class).await?,"fields":fields}));
        }
        Ok(json!(result))
    }
    pub(crate) async fn diagnostic(&self, ctx: &Ctx, base: &RuntimeCache) -> Result<Value> {
        let mut interactions = Vec::new();
        for offset in [3448, 3464, 4472] {
            let address = ctx.read::<u64>(base.controller.address + offset).await?;
            let value = if address == 0 {
                Value::Null
            } else {
                let s = serial(ctx, address).await?;
                json!({"object":path(ctx,address).await?,"class":path(ctx,s.0.class).await?})
            };
            interactions.push(json!({"offset":offset,"value":value}));
        }
        let mut ui_instances = Vec::new();
        let mut tables = Vec::new();
        let mut rows = Vec::new();
        for (_, address) in registered_objects(ctx).await? {
            let o = Ptr::<UObject>::new(address, ctx.clone())?;
            let name = o.name_private().read().await?;
            if ["UIManager", "UIPolicy", "PrimaryGameLayout", "RootLayout", "GameUILayout"]
                .iter()
                .any(|s| name.contains(s))
                && ui_instances.len() < 16
                && serial(ctx, address).await.is_ok()
            {
                ui_instances.push(address);
            }
            if name.starts_with("WBP_RecipeTable_C_") && tables.len() < 16 {
                tables.push(address);
            }
            if name.starts_with("WBP_RecipeTableRecipe_C_") && rows.len() < 512 {
                rows.push(address);
            }
        }
        let ui_roots = self.root_references(ctx, base, ui_instances).await?;
        let row_count = rows.len();
        let mut samples = Vec::new();
        for row in rows {
            let hovered = self.hovered(ctx, row, base).await;
            if samples.len() < 2 || matches!(hovered, Ok(true)) {
                samples.push(json!({"name":path(ctx,row).await?,"hover":format!("{hovered:?}"),"station":format!("{:?}",self.station(ctx,row,base).await)}));
            }
        }
        let mut chains = Vec::new();
        for table in tables {
            let mut nodes = Vec::new();
            let mut current = table;
            let mut seen = BTreeSet::new();
            for _ in 0..64 {
                if !seen.insert(current) {
                    break;
                }
                let s = serial(ctx, current).await?;
                let mut fields = Vec::new();
                let mut class = Some(Ptr::<UStruct>::new(s.0.class, ctx.clone())?);
                for _ in 0..16 {
                    let Some(c) = class else {
                        break;
                    };
                    let mut properties = c.properties(false);
                    let mut visits = 0;
                    while let Some(p) = properties.next().await {
                        visits += 1;
                        ensure!(visits <= 1024, "Diagnostic property chain bound");
                        let p = p?;
                        let name = p.zfield().name_private().read().await?;
                        if ["Active", "Visible", "Screen", "Menu", "Widget", "Stack", "Viewport"]
                            .iter()
                            .any(|s| name.contains(s))
                        {
                            let offset = p.offset_internal().read().await?;
                            let size = p.element_size().read().await?;
                            let mut field = json!({"class":c.path().await?,"name":name,"offset":offset,"size":size});
                            if name == "bIsActive" {
                                field["mask"] =
                                    json!(p.cast::<ZBoolProperty>().field_mask().read().await?);
                                field["byteOffset"] =
                                    json!(p.cast::<ZBoolProperty>().byte_offset_().read().await?);
                                field["raw"] =
                                    json!(ctx.read::<u8>(current + offset as u64).await?);
                            }
                            fields.push(field);
                        }
                        ensure!(fields.len() < 512, "Diagnostic property bound");
                    }
                    class = c.super_struct().read().await?;
                }
                let slot = ctx.read::<u64>(current + 48).await?;
                let user = class_is(ctx, s.0.class, self.user_widget_class).await?;
                let outer = ctx.read::<u64>(current + 32).await?;
                let mut node = json!({"object":path(ctx,current).await?,"class":path(ctx,s.0.class).await?,"visibility":ctx.read::<u8>(current+252).await?,"slot":slot,"outer":path(ctx,outer).await?,"fields":fields});
                if user {
                    node["hover"] =
                        json!(format!("{:?}", self.widget_hovered(ctx, current, base).await));
                    node["playerContext"] = json!(bytes(ctx, current + 0x388, 8).await?);
                }
                nodes.push(node);
                if slot != 0 {
                    current = ctx.read::<u64>(slot + 40).await?;
                    if current == 0 {
                        break;
                    }
                } else {
                    let ts = serial(ctx, outer).await?;
                    if !path(ctx, ts.0.class).await?.ends_with(".WidgetTree") {
                        break;
                    }
                    current = ctx.read::<u64>(outer + 32).await?;
                }
            }
            chains.push(nodes);
        }
        Ok(
            json!({"uiRoots":ui_roots,"interactions":interactions,"rowCount":row_count,"rowSamples":samples,"stationChains":chains}),
        )
    }
}
