//! Private bounded research diagnostic, excluded from release builds.
use super::*;
use jmap_dumper::{
    mem::Ptr,
    objects::{FUObjectArray, UFunction, UStruct},
};
async fn code(ctx: &jmap_dumper::mem::Ctx, a: u64) -> Result<serde_json::Value> {
    use iced_x86::{Decoder, DecoderOptions, Formatter, IntelFormatter};
    let mut b = vec![0; 512];
    ctx.read_buf(a, &mut b).await?;
    let mut d = Decoder::with_ip(64, &b, a, DecoderOptions::NONE);
    let mut fmt = IntelFormatter::new();
    let mut ls = Vec::new();
    let mut targets = Vec::new();
    while d.can_decode() {
        let ins = d.decode();
        let mut l = String::new();
        fmt.format(&ins, &mut l);
        ls.push(format!("{:x}: {l}", ins.ip()));
        if ins.is_call_near()
            || (ins.is_jmp_short_or_near() && !(a..a + 512).contains(&ins.near_branch_target()))
        {
            targets.push(ins.near_branch_target())
        }
    }
    Ok(json!({"address":a,"instructions":ls,"targets":targets}))
}

/// Finite metadata/code observation; no windows, inputs or game-code calls.
pub fn probe(directory: &Path) -> Result<()> {
    std::fs::create_dir_all(directory)?;
    let stop = Arc::new(StopSignal::default());
    let budget = platform::ReadBudget::new(stop);
    let process = platform::find_game(budget.clone())?;

    let cache = std::path::PathBuf::from(std::env::var_os("LOCALAPPDATA").context("App data")?)
        .join("StarRuptureWaveOverlay/cache/session.json");
    let cached = crate::cache::read_json::<Cache>(&cache, 100000)
        .ok()
        .filter(|r| crate::cache_matches(r, &process.identity));
    let record = match cached {
        Some(r) => r,
        None => smol::block_on(crate::discover(&process))?,
    };
    let base = RuntimeCache::new(record)?;
    budget.renew(60000);
    let value = smol::block_on(async {
        let ctx = jmap_dumper::connect_manual(process.clone(), base.config.clone(), None).await?;
        let array = Ptr::<FUObjectArray>::new(ctx.uobjectarray, ctx.clone())?;
        let count = array.num_elements().await?;
        ensure!((1..=2_000_000).contains(&count), "Registry bound");
        let mut types = Vec::new();
        let mut functions = Vec::new();
        let mut instances = Vec::new();
        let mut extras = Vec::new();
        for i in 0..count as usize {
            let Some(o) = array.read_item_ptr(i).await? else {
                continue;
            };
            if o.internal_index().read().await.ok() != Some(i as i32) {
                continue;
            }
            let name = o.name_private().read().await?;
            if name == "Default__Widget"
                || name == "Default__CrUW_RecipeTableRecipe"
                || name == "Default__UserWidget"
            {
                let vt = ctx.read::<u64>(o.address()).await?;
                let a = ctx.read::<u64>(vt + 0x2c8).await?;
                let c = code(&ctx, a).await?;
                let lp = code(&ctx, ctx.read::<u64>(vt + 0x2e0).await?).await?;
                let mut context_codes = Vec::new();
                for target in
                    lp["targets"].as_array().context("Owning context code")?.iter().take(3)
                {
                    context_codes
                        .push(code(&ctx, target.as_u64().context("Owning context target")?).await?);
                }
                extras.push(json!({"name":name,"contextCode":context_codes,"hoverVtable":c,"owningPlayer":code(&ctx,ctx.read::<u64>(vt+0x2d8).await?).await?,"owningLocalPlayer":code(&ctx,ctx.read::<u64>(vt+0x2e0).await?).await?}));
            }
            if [
                "Widget",
                "UserWidget",
                "Button",
                "PanelWidget",
                "PanelSlot",
                "CrUW_RecipeTableRecipe",
                "CrUW_RecipeTableCrafter",
                "CrUW_RecipeTableRecipeRequirement",
            ]
            .contains(&name.as_str())
            {
                let path = o.path().await?;
                if path.starts_with("/Script/") {
                    let mut props = o.cast::<UStruct>().properties(false);
                    let mut fields = Vec::new();
                    while let Some(p) = props.next().await {
                        let p = p?;
                        ensure!(fields.len() < 1024, "Field budget");
                        fields.push(json!({"name":p.zfield().name_private().read().await?,"offset":p.offset_internal().read().await?,"size":p.element_size().read().await?}));
                    }
                    types.push(json!({"path":path,"fields":fields,"size":o.cast::<UStruct>().properties_size().read().await?}));
                }
            }
            if [
                "IsHovered",
                "GetCachedGeometry",
                "GetPaintSpaceGeometry",
                "GetTickSpaceGeometry",
                "IsInViewport",
                "GetOwningPlayer",
                "GetOwningLocalPlayer",
            ]
            .contains(&name.as_str())
            {
                let path = o.path().await?;
                if path.starts_with("/Script/UMG.") {
                    let address = o.cast::<UFunction>().func().read().await? as u64;
                    let mut bytes = vec![0; 256];
                    ctx.read_buf(address, &mut bytes).await?;
                    use iced_x86::{Decoder, DecoderOptions, Formatter, IntelFormatter, Mnemonic};
                    let mut dec = Decoder::with_ip(64, &bytes, address, DecoderOptions::NONE);
                    let mut fmt = IntelFormatter::new();
                    let mut lines = Vec::new();
                    let mut targets = Vec::new();
                    while dec.can_decode() {
                        let ins = dec.decode();
                        let mut line = String::new();
                        fmt.format(&ins, &mut line);
                        lines.push(format!("{:x}: {line}", ins.ip()));
                        if ins.is_call_near()
                            || (ins.is_jmp_short_or_near()
                                && !(address..address + 256).contains(&ins.near_branch_target()))
                        {
                            targets.push(ins.near_branch_target())
                        }
                        if ins.mnemonic() == Mnemonic::Ret {
                            break;
                        }
                    }
                    let mut calls = Vec::new();
                    for a in targets.into_iter().take(4) {
                        let c = code(&ctx, a).await?;
                        for target in
                            c["targets"].as_array().context("Code targets")?.iter().take(3)
                        {
                            extras
                                .push(code(&ctx, target.as_u64().context("Code address")?).await?);
                        }
                        calls.push(c);
                    }
                    functions.push(
                        json!({"path":path,"address":address,"instructions":lines,"calls":calls}),
                    );
                }
            }
            if name.starts_with("WBP_RecipeTableRecipe_C_") && instances.len() < 256 {
                let stamp = crate::stamp(&ctx, o.address()).await?;
                let mut b = vec![0; 1100];
                ctx.read_buf(o.address(), &mut b).await?;
                instances.push(json!({"stamp":stamp,"path":o.path().await?,"bytes":b}));
            }
        }
        let binding = Binding::discover(&ctx).await?;
        crate::sample_context(&process, &base, ctx.clone()).await?;
        let level = ctx.read::<u64>(base.controller.address + 32).await?;
        let world = ctx.read::<u64>(level + 32).await?;
        let exe = std::env::current_exe()?;
        let pack = std::fs::read(
            exe.parent().context("Executable directory")?.join("recipe-assets/catalogue.json"),
        )?;
        let catalogue: starrupture_observation_model::recipe::Catalogue =
            serde_json::from_slice(&pack)?;
        catalogue.validate().map_err(anyhow::Error::msg)?;
        let sample = binding
            .sample(
                &ctx,
                &base,
                world,
                "/Game/Chimera/Crafting/CR_QuantumTranqualizer",
                &catalogue,
                1,
            )
            .await?;
        let sample = sample.context("Probe recipe is absent from active research")?;
        let ingredients:Vec<_>=sample.ingredients.iter().map(|i|json!({"item":i.item_id,"submitted":i.submitted,"required":i.required,"remaining":i.remaining})).collect();
        let mut weak_codes = Vec::new();
        for extra in &extras {
            if extra["name"] == "Default__CrUW_RecipeTableRecipe" {
                for c in extra["contextCode"].as_array().context("Context code")? {
                    if let Some(t) =
                        c["targets"].as_array().and_then(|t| t.first()).and_then(|t| t.as_u64())
                    {
                        weak_codes.push(code(&ctx, t).await?);
                    }
                }
            }
        }
        Ok::<_, anyhow::Error>(
            json!({"readOnly":true,"gameCodeInvoked":false,"process":process.identity,"imageBase":base.config.image_base,"types":types,"functions":functions,"instances":instances,"extras":extras,"weakCodes":weak_codes,"research":{"recipe":sample.recipe_id,"ingredients":ingredients}}),
        )
    })?;
    std::fs::write(directory.join("recipe-probe.json"), serde_json::to_vec_pretty(&value)?)?;
    Ok(())
}
