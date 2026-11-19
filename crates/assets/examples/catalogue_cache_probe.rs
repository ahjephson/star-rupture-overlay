//! Finite offline check of the production catalogue cache; never starts the overlay.
use anyhow::{Context, Result, ensure};
use starrupture_assets::codex::{NativeContext, Store};
fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let request = args.next().context("Recipe request")?;
    let context = args.next().context("Native context")?;
    let root = args.next().context("Private cache root")?;
    ensure!(args.next().is_none(), "Unexpected arguments");
    let r: starrupture_assets::Request = serde_json::from_slice(&std::fs::read(request)?)?;
    let context: NativeContext = serde_json::from_slice(&std::fs::read(context)?)?;
    let store = Store::new(root.into());
    let p = if let Some(p) = store.lookup(&r.paks, &context)? {
        p
    } else {
        store.prepare(&r.paks, context, &std::fs::read(r.mappings)?, &Default::default())?
    };
    let matches = p.search("calcium", 32)?;
    let icon = p.icon(&p.catalogue.items.values().next().context("Item")?.icon)?;
    println!(
        "{}",
        serde_json::json!({"metrics":p.metrics,"items":p.catalogue.items.len(),"recipes":p.catalogue.recipes.len(),"buildings":p.catalogue.buildings.len(),"coreTiers":p.catalogue.core_tiers.len(),"search":matches,"firstIcon":[icon.width,icon.height],"retainedIconBytes":p.retained_icon_bytes()})
    );
    Ok(())
}
