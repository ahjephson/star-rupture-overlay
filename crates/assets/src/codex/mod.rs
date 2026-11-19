//! On-demand, offline production catalogue. No native reads occur while browsing.
mod archive;
mod cache;
mod model;
mod normalize;
pub use cache::{Icon, Metrics, Prepared, Store};
pub use model::*;
pub use starrupture_observation_model::codex::NativeContext;
mod texture;
/// Same-executable offline preparation request; paths use the recipe setup confinement rules.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub assets: crate::Request,
    pub context: NativeContext,
}
pub fn run(request_file: &std::path::Path) -> anyhow::Result<()> {
    use anyhow::ensure;
    let request: Request = serde_json::from_slice(&crate::bounded(request_file, 128 * 1024)?)?;
    request.assets.validate(request_file)?;
    request.context.validate().map_err(anyhow::Error::msg)?;
    ensure!(
        request.context.game_hash == request.assets.game_hash,
        "Catalogue request build mismatch"
    );
    let result =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> anyhow::Result<()> {
            crate::status(&request.assets, "Preparing production catalogue...")?;
            let mappings = crate::bounded(&request.assets.mappings, 16 * 1024 * 1024)?;
            let p = Store::new(request.assets.output.clone()).prepare(
                &request.assets.paks,
                request.context,
                &mappings,
                &Default::default(),
            )?;
            std::fs::write(
                request.assets.output.join("preparation.json"),
                serde_json::to_vec(&p.metrics)?,
            )?;
            Ok(())
        }))
        .unwrap_or_else(|_| Err(anyhow::anyhow!("Unsupported or malformed catalogue assets")));
    if let Err(e) = &result {
        std::fs::write(
            &request.assets.error,
            format!("{e:#}").chars().take(1000).collect::<String>(),
        )?;
    }
    result
}
