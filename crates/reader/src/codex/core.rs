use super::*;

/// Native indices are retained separately from the game's display numbering.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Core {
    pub id: u32,
    pub tier_index: u32,
    pub capacity: u32,
    pub submitted: Option<(u32, BTreeMap<String, u32>)>,
}

pub(super) struct Cores {
    owner: live::Reference,
    container_owner: u64,
    rows: live::Array,
    level: usize,
    capacity: usize,
    highest: Option<u64>,
    materials: Option<Materials>,
}

struct Materials {
    selected: usize,
    array: u64,
    stride: usize,
    item: usize,
    count: usize,
}
impl Materials {
    async fn bind(ctx: &Ctx, class: u64, row_size: usize) -> Result<Self> {
        let selected = field(ctx, class, "SelectedLevelToUpgrade").await?;
        let array = field(ctx, class, "ItemsAddedToUpgrade").await?;
        ensure!(
            selected.zfield().cast_flags().await?.contains(K::CASTCLASS_FInt8Property)
                && selected.element_size().read().await? == 1
                && array.zfield().cast_flags().await?.contains(K::CASTCLASS_FArrayProperty)
                && array.element_size().read().await? == 16,
            "Core submission layout"
        );
        let inner = array.cast::<jmap_dumper::objects::ZArrayProperty>().inner().read().await?;
        ensure!(
            inner.zfield().cast_flags().await?.contains(K::CASTCLASS_FStructProperty),
            "Core submission record type"
        );
        let stride = inner.element_size().read().await? as usize;
        let class = inner.cast::<ZStructProperty>().struct_().read().await?.address();
        let item = field(ctx, class, "Item").await?;
        let count = field(ctx, class, "Count").await?;
        ensure!(
            item.element_size().read().await? == 8
                && item.zfield().cast_flags().await?.contains(K::CASTCLASS_FObjectProperty)
                && count.element_size().read().await? == 4
                && count.zfield().cast_flags().await?.contains(K::CASTCLASS_FIntProperty),
            "Core submission fields"
        );
        let layout = Self {
            selected: selected.offset_internal().read().await? as usize,
            array: array.offset_internal().read().await? as u64,
            stride,
            item: item.offset_internal().read().await? as usize,
            count: count.offset_internal().read().await? as usize,
        };
        ensure!(
            stride <= 128
                && layout.item + 8 <= stride
                && layout.count + 4 <= stride
                && layout.selected < row_size
                && layout.array as usize + 16 <= row_size,
            "Core submission field bounds"
        );
        Ok(layout)
    }

    async fn read(
        &self,
        ctx: &Ctx,
        row: &[u8],
        row_address: u64,
        current: u32,
    ) -> Result<Option<(u32, BTreeMap<String, u32>)>> {
        let selected = row[self.selected] as i8;
        if selected < 0 || selected as u32 <= current {
            return Ok(None);
        }
        let list = live::Array {
            address: row_address + self.array,
            stride: self.stride,
            object_offset: 0,
        };
        let (pointer, header, bytes) = list.bytes(ctx).await?;
        ensure!(bytes.len() / self.stride <= 32, "Core submission count");
        let mut counts = BTreeMap::new();
        for entry in bytes.chunks_exact(self.stride) {
            let asset = u64::from_le_bytes(entry[self.item..self.item + 8].try_into()?);
            let proof = crate::recipe::allocation(ctx, asset).await?;
            let path = crate::path(ctx, asset).await?;
            let id = path.split_once('.').context("Core submission item identity")?.0;
            let count =
                u32::try_from(i32::from_le_bytes(entry[self.count..self.count + 4].try_into()?))?;
            ensure!(
                starrupture_observation_model::codex::asset_id(id)
                    && counts.insert(id.to_owned(), count).is_none()
                    && crate::recipe::allocation(ctx, asset).await? == proof,
                "Core submission item changed"
            );
        }
        ensure!(list.bytes(ctx).await? == (pointer, header, bytes), "Core submissions changed");
        Ok(Some((selected as u32, counts)))
    }
}
impl Cores {
    pub(super) async fn bind(ctx: &Ctx, state: u64) -> Result<Self> {
        let owner = live::Reference::bind(ctx, state, "BaseCoreReplicationHelper").await?;
        let container = field(ctx, owner.target.0.class, "BaseCoreDataContainer").await?;
        ensure!(
            container.zfield().cast_flags().await?.contains(K::CASTCLASS_FStructProperty),
            "Core container type"
        );
        let address = owner.target() + container.offset_internal().read().await? as u64;
        let class = container.cast::<ZStructProperty>().struct_().read().await?.address();
        let data = field(ctx, class, "BaseCoreData").await?;
        let container_owner = field(ctx, class, "Owner").await?;
        ensure!(
            data.zfield().cast_flags().await?.contains(K::CASTCLASS_FArrayProperty)
                && data.element_size().read().await? == 16
                && container_owner
                    .zfield()
                    .cast_flags()
                    .await?
                    .contains(K::CASTCLASS_FObjectProperty)
                && container_owner.element_size().read().await? == 8,
            "Core container fields"
        );
        let inner = data.cast::<jmap_dumper::objects::ZArrayProperty>().inner().read().await?;
        ensure!(
            inner.zfield().cast_flags().await?.contains(K::CASTCLASS_FStructProperty),
            "Core record type"
        );
        let stride = usize::try_from(inner.element_size().read().await?)?;
        ensure!(matches!(stride, 72 | 80), "Unknown Core record size");
        let class = inner.cast::<ZStructProperty>().struct_().read().await?.address();
        // Core rows carry a world-scoped persistent Mass entity ID.
        let entity = field(ctx, class, "BaseCore").await?;
        ensure!(
            entity.zfield().cast_flags().await?.contains(K::CASTCLASS_FStructProperty)
                && entity.offset_internal().read().await? == 16
                && entity.element_size().read().await? == 24,
            "Core entity layout"
        );
        let entity_type = entity.cast::<ZStructProperty>().struct_().read().await?;
        let member = field(ctx, entity_type.address(), "Entity").await?;
        ensure!(
            member.zfield().cast_flags().await?.contains(K::CASTCLASS_FStructProperty)
                && member.offset_internal().read().await? == 8
                && member.element_size().read().await? == 16,
            "Core persistent identity layout"
        );
        let persistent_type = member.cast::<ZStructProperty>().struct_().read().await?;
        let id_field = field(ctx, persistent_type.address(), "ID").await?;
        ensure!(
            id_field.offset_internal().read().await? == 0
                && id_field.element_size().read().await? == 4,
            "Core persistent ID field"
        );
        let level = field(ctx, class, "UpgradeLevel").await?;
        let capacity = field(ctx, class, "CurrentCoolingCapacity").await?;
        ensure!(
            level.zfield().cast_flags().await?.contains(K::CASTCLASS_FByteProperty)
                && level.element_size().read().await? == 1
                && capacity.zfield().cast_flags().await?.contains(K::CASTCLASS_FFloatProperty)
                && capacity.element_size().read().await? == 4,
            "Core record fields"
        );
        let level = level.offset_internal().read().await? as usize;
        let capacity = capacity.offset_internal().read().await? as usize;
        ensure!(level < stride && capacity + 4 <= stride, "Core record bounds");
        let highest = async {
            let property = field(ctx, owner.target.0.class, "HighestBaseCoreLevelEverBeen").await?;
            ensure!(
                property.element_size().read().await? == 4
                    && property.zfield().cast_flags().await?.contains(K::CASTCLASS_FIntProperty),
                "Shared Core progression field layout"
            );
            let offset = property.offset_internal().read().await? as u64;
            Ok::<_, anyhow::Error>(owner.target() + offset)
        }
        .await
        .ok();
        Ok(Self {
            owner,
            highest,
            container_owner: address + container_owner.offset_internal().read().await? as u64,
            rows: live::Array {
                address: address + data.offset_internal().read().await? as u64,
                stride,
                object_offset: 0,
            },
            level,
            capacity,
            materials: Materials::bind(ctx, class, stride).await.ok(),
        })
    }

    pub(super) async fn read(&self, ctx: &Ctx) -> Result<(Vec<Core>, Option<u32>)> {
        self.owner.check(ctx).await?;
        let highest = if let Some(address) = self.highest {
            ctx.read::<i32>(address).await.ok()
        } else {
            None
        };
        let (ptr, header, bytes) = self.rows.bytes(ctx).await?;
        ensure!(bytes.len() / self.rows.stride <= 256, "Core count bound");
        let mut unique = BTreeSet::new();
        let mut cores = Vec::new();
        for (index, row) in bytes.chunks_exact(self.rows.stride).enumerate() {
            let id = u32::from_le_bytes(row[24..28].try_into()?);
            let capacity = f32::from_le_bytes(row[self.capacity..self.capacity + 4].try_into()?);
            ensure!(id != 0 && unique.insert(id), "Core identity missing or duplicated");
            ensure!(
                (0.0..=1e8).contains(&capacity) && capacity.fract() == 0.0,
                "Core cooling capacity"
            );
            let tier_index = u32::from(row[self.level]);
            let submitted = if let Some(materials) = &self.materials {
                materials
                    .read(ctx, row, ptr + (index * self.rows.stride) as u64, tier_index)
                    .await
                    .unwrap_or_default()
            } else {
                None
            };
            cores.push(Core {
                id,
                tier_index,
                submitted,
                capacity: capacity as u32,
            });
        }
        ensure!(
            self.rows.bytes(ctx).await? == (ptr, header, bytes)
                && ctx.read::<u64>(self.container_owner).await? == self.owner.target(),
            "Core observation changed"
        );
        let highest = if let (Some(address), Some(value)) = (self.highest, highest) {
            (ctx.read::<i32>(address).await.ok() == Some(value)
                && (-1..64).contains(&value)
                && cores.iter().all(|core| core.tier_index <= value.max(0) as u32))
            .then_some(value.max(0) as u32)
        } else {
            None
        };
        self.owner.check(ctx).await?;
        cores.sort_by_key(|core| core.id);
        Ok((cores, highest))
    }
}
