//! Bounded observations from the current world's owned objects.
use super::*;
use crate::recipe::runtime::{Binding, SubmittedItems};
use std::time::{Duration, Instant};

type UpgradeSubmissions = BTreeMap<String, Vec<(String, u32)>>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Progress {
    pub buildings: BTreeSet<String>,
    pub recipes: BTreeSet<String>,
    pub research: BTreeSet<String>,
    pub upgrade_submissions: UpgradeSubmissions,
    pub owned_equipment: BTreeSet<String>,
    pub corporations: BTreeMap<String, u32>,
    pub corporation_reputation: BTreeMap<String, u32>,
    pub cores: Vec<super::Core>,
    pub highest_unlocked_core_tier: Option<u32>,
}

pub(super) struct Reference {
    pub(super) address: u64,
    pub(super) target: (crate::ObjectStamp, i32),
}
impl Reference {
    pub(super) async fn bind(ctx: &Ctx, owner: u64, name: &str) -> Result<Self> {
        let object = crate::recipe::allocation(ctx, owner).await?;
        let property = field(ctx, object.0.class, name).await?;
        ensure!(
            property.zfield().cast_flags().await?.contains(K::CASTCLASS_FObjectProperty)
                && property.element_size().read().await? == 8,
            "Invalid Codex reference type"
        );
        let address = owner + property.offset_internal().read().await? as u64;
        let target = crate::recipe::allocation(ctx, ctx.read(address).await?)
            .await
            .with_context(|| format!("Codex reference {name}"))?;
        let result = Self {
            address,
            target,
        };
        result.check(ctx).await?;
        Ok(result)
    }
    pub(super) async fn check(&self, ctx: &Ctx) -> Result<()> {
        ensure!(
            ctx.read::<u64>(self.address).await? == self.target.0.address
                && crate::recipe::allocation(ctx, self.target.0.address).await? == self.target,
            "Codex world reference changed"
        );

        Ok(())
    }
    pub(super) fn target(&self) -> u64 {
        self.target.0.address
    }
}

pub(super) struct Array {
    pub(super) address: u64,
    pub(super) stride: usize,
    pub(super) object_offset: usize,
}

impl Array {
    async fn bind(
        ctx: &Ctx,
        owner: u64,
        class: u64,
        name: &str,
        member: Option<&str>,
    ) -> Result<Self> {
        let p = field(ctx, class, name).await?;
        ensure!(
            p.zfield().cast_flags().await?.contains(K::CASTCLASS_FArrayProperty)
                && p.element_size().read().await? == 16,
            "Invalid Codex array property"
        );
        let inner = p.cast::<jmap_dumper::objects::ZArrayProperty>().inner().read().await?;
        let stride = usize::try_from(inner.element_size().read().await?)?;
        let item = if let Some(member) = member {
            ensure!(
                inner.zfield().cast_flags().await?.contains(K::CASTCLASS_FStructProperty),
                "Invalid Codex row type"
            );
            let st = inner.cast::<ZStructProperty>().struct_().read().await?;
            field(ctx, st.address(), member).await?
        } else {
            inner
        };
        let object_offset = usize::try_from(item.offset_internal().read().await?)?;
        ensure!(
            item.zfield().cast_flags().await?.contains(K::CASTCLASS_FObjectProperty)
                && item.element_size().read().await? == 8
                && stride <= 512
                && object_offset + 8 <= stride,
            "Invalid Codex array element"
        );
        Ok(Self {
            address: owner + p.offset_internal().read().await? as u64,
            stride,
            object_offset,
        })
    }

    pub(super) async fn bytes(&self, ctx: &Ctx) -> Result<(u64, [u8; 16], Vec<u8>)> {
        let mut header = [0; 16];
        ctx.read_buf(self.address, &mut header).await?;
        let pointer = u64::from_le_bytes(header[..8].try_into()?);
        let count = u32::from_le_bytes(header[8..12].try_into()?) as usize;
        let capacity = u32::from_le_bytes(header[12..].try_into()?) as usize;
        ensure!(count <= capacity && count <= 2048 && capacity <= 8192, "Codex array bound");
        let mut bytes = vec![0; count * self.stride];
        if count != 0 {
            ensure!(
                (0x10000..0x800000000000).contains(&pointer) && pointer % 8 == 0,
                "Codex array allocation"
            );
            ctx.read_buf(pointer, &mut bytes).await?;
        }
        Ok((pointer, header, bytes))
    }

    async fn read(
        &self,
        ctx: &Ctx,
        hints: &mut BTreeMap<u64, ((crate::ObjectStamp, i32), String)>,
        package: bool,
    ) -> Result<BTreeSet<String>> {
        Ok(self.read_with_submissions(ctx, hints, package, None).await?.0)
    }

    async fn read_with_submissions(
        &self,
        ctx: &Ctx,
        hints: &mut BTreeMap<u64, ((crate::ObjectStamp, i32), String)>,
        package: bool,
        upgrade_recipes: Option<&BTreeSet<String>>,
    ) -> Result<(BTreeSet<String>, UpgradeSubmissions)> {
        let (pointer, header, bytes) = self.bytes(ctx).await?;
        let mut values = BTreeSet::new();
        let mut submissions = BTreeMap::new();
        for row in bytes.chunks_exact(self.stride) {
            let address =
                u64::from_le_bytes(row[self.object_offset..self.object_offset + 8].try_into()?);
            if address == 0 {
                continue;
            }
            let proof =
                crate::recipe::allocation(ctx, address).await.context("Codex array object")?;
            let id = match hints.get(&address) {
                Some((current, id)) if *current == proof => id.clone(),
                _ => {
                    let id = crate::path(ctx, address).await?;
                    ensure!(
                        crate::recipe::allocation(ctx, address).await? == proof,
                        "Codex item identity changed"
                    );
                    hints.insert(address, (proof, id.clone()));
                    id
                }
            };
            if package {
                if let Some((package, _)) = id.split_once('.') {
                    let first = values.insert(package.to_owned());
                    if upgrade_recipes.is_some_and(|recipes| recipes.contains(package)) {
                        ensure!(first, "Duplicate upgrade research row");
                        let row = row.try_into().context("Upgrade research row layout")?;
                        // A torn ingredient read affects this progress entry only.
                        if let Ok(items) = SubmittedItems::read(ctx, row).await
                            && items.check(ctx).await.is_ok()
                        {
                            submissions.insert(package.to_owned(), items.counts);
                        }
                    }
                }
            } else {
                values.insert(id);
            }
        }
        ensure!(
            self.bytes(ctx).await? == (pointer, header, bytes),
            "Codex data changed during read"
        );
        Ok((values, submissions))
    }
}

pub fn observe(
    directory: &Path,
    stop: Arc<StopSignal>,
    pid: Option<u32>,
    context: &NativeContext,
    upgrade_recipes: &BTreeSet<String>,
    mut publish: impl FnMut(bool, Option<Progress>),
) -> Result<()> {
    let budget = crate::platform::ReadBudget::new(stop.clone());
    let process = crate::platform::find_game(budget.clone())?;
    ensure!(
        pid.is_none_or(|pid| process.identity.pid == pid)
            && process.identity.executable_sha256 == context.game_hash,
        "Codex process changed"
    );
    budget.renew(5_000);
    smol::block_on(async {
        let base = crate::overlay_stream::load_base(&process, directory).await?;
        let ctx = jmap_dumper::connect_manual(process.clone(), base.config.clone(), None).await?;
        let ui = Binding::from_lookup(&ctx, &base, &crate::native_lookup::Lookup::new(directory))
            .await?;
        let level = ctx.read::<u64>(base.controller.address + 32).await?;
        let world = Reference::bind(&ctx, level, "OwningWorld").await?;
        let state = Reference::bind(&ctx, world.target(), "GameState").await?;
        ensure!(
            crate::path(&ctx, state.target.0.class).await? == context.state_class,
            "Codex game state class changed"
        );
        let technology = Reference::bind(&ctx, state.target(), "TechnologyKeeper").await?;
        ensure!(
            crate::path(&ctx, technology.target.0.class).await? == context.keeper_class,
            "Codex technology class changed"
        );
        let corporations = super::corporations::Corporations::bind(&ctx, state.target()).await?;
        let cores = super::core::Cores::bind(&ctx, state.target()).await.ok();
        let recipe_owner = Reference::bind(&ctx, state.target(), "CraftingRecipeOwner").await?;
        let owner = recipe_owner.target();
        let p = field(&ctx, recipe_owner.target.0.class, "UnlockableRecipesContainer").await?;
        ensure!(
            p.zfield().cast_flags().await?.contains(K::CASTCLASS_FStructProperty),
            "Codex research container type"
        );
        let st = p.cast::<ZStructProperty>().struct_().read().await?;
        let research = Array::bind(
            &ctx,
            owner + p.offset_internal().read().await? as u64,
            st.address(),
            "RecipesData",
            Some("Recipe"),
        )
        .await?;
        ensure!(
            research.stride == 40 && research.object_offset == 16,
            "Upgrade research row layout"
        );
        let buildings = Array::bind(
            &ctx,
            technology.target(),
            technology.target.0.class,
            "AvailableBuildings",
            None,
        )
        .await?;
        let recipes =
            Array::bind(&ctx, technology.target(), technology.target.0.class, "AllRecipes", None)
                .await?;

        // The build's previously owned quest entries are direct class references. An
        // unknown owner is reported as unavailable by leaving all equipment unowned.
        let mut quest = None;
        if let Ok(objects) = crate::native_lookup::Lookup::new(directory).resolve_codex(&ctx, &base)
        {
            for (_, address) in objects {
                if crate::path(&ctx, address).await?.ends_with(".SharedQuestItemsListComponent") {
                    let (component, _) =
                        component(&ctx, state.target(), address, &context.game_hash, directory)
                            .await?;
                    let proof = crate::recipe::allocation(&ctx, component).await?;
                    let entries = Array::bind(
                        &ctx,
                        component,
                        proof.0.class,
                        "PreviouslyOwnedQuestItems",
                        None,
                    )
                    .await?;
                    quest = Some((proof, entries));
                    break;
                }
            }
        }

        let pause = field(&ctx, base.controller.class, "bReplInPauseMenu").await?;
        ensure!(
            pause.zfield().cast_flags().await?.contains(K::CASTCLASS_FBoolProperty)
                && pause.element_size().read().await? == 1,
            "Codex pause flag type"
        );
        let pause_address = base.controller.address + pause.offset_internal().read().await? as u64;
        let pause_mask = pause.cast::<ZBoolProperty>().field_mask().read().await?;
        ensure!(pause_mask != 0, "Codex pause flag mask");

        let mut hints = BTreeMap::new();
        let mut next_progress = Instant::now();
        while !stop.is_requested() {
            budget.renew(750);
            process.ensure_alive()?;
            ensure!(
                ctx.read::<u64>(base.controller.address + 32).await? == level,
                "Codex world changed"
            );
            for link in [&world, &state, &technology, &recipe_owner] {
                link.check(&ctx).await?;
            }
            let gameplay = ctx.read::<u8>(pause_address).await? & pause_mask == 0
                && ui.gameplay(&ctx, &base).await.unwrap_or(false);
            if !gameplay {
                publish(false, None);
                next_progress = Instant::now();
            } else if Instant::now() >= next_progress {
                let owned_equipment = if let Some((proof, entries)) = &quest {
                    let address = proof.0.address;
                    ensure!(
                        crate::recipe::allocation(&ctx, address).await? == *proof
                            && ctx.read::<u64>(address + 32).await? == state.target(),
                        "Codex equipment owner changed"
                    );
                    entries.read(&ctx, &mut hints, false).await.context("Codex weapon ownership")?
                } else {
                    BTreeSet::new()
                };
                let (placed_cores, highest_unlocked_core_tier) = if let Some(cores) = &cores {
                    cores.read(&ctx).await.unwrap_or_default()
                } else {
                    (Vec::new(), None)
                };
                let (research, upgrade_submissions) = research
                    .read_with_submissions(&ctx, &mut hints, true, Some(upgrade_recipes))
                    .await
                    .context("Codex research list")?;
                let corporation_state =
                    corporations.read(&ctx).await.context("Codex corporation progress")?;
                let progress = Progress {
                    buildings: buildings
                        .read(&ctx, &mut hints, true)
                        .await
                        .context("Codex buildings list")?,
                    recipes: recipes
                        .read(&ctx, &mut hints, true)
                        .await
                        .context("Codex recipes list")?,
                    research,
                    upgrade_submissions,
                    owned_equipment,
                    corporations: corporation_state.levels,
                    corporation_reputation: corporation_state.reputation,
                    cores: placed_cores,
                    highest_unlocked_core_tier,
                };
                let gameplay = ctx.read::<u8>(pause_address).await? & pause_mask == 0
                    && ui.gameplay(&ctx, &base).await.unwrap_or(false);
                publish(gameplay, gameplay.then_some(progress));
                next_progress = Instant::now() + Duration::from_secs(1);
            } else {
                publish(true, None);
            }
            stop.wait_until(Instant::now() + Duration::from_millis(80));
        }
        Ok(())
    })
}
