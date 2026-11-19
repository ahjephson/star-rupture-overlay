use super::*;

pub(super) struct Corporations {
    owner: live::Reference,
    rows: live::Array,
    name: usize,
    level: usize,
    hidden: usize,
    hidden_mask: u8,
}

impl Corporations {
    pub(super) async fn bind(ctx: &Ctx, state: u64) -> Result<Self> {
        let owner = live::Reference::bind(ctx, state, "CorporationsOwner").await?;
        let container = field(ctx, owner.target.0.class, "CorporationsContainer").await?;
        ensure!(
            container.zfield().cast_flags().await?.contains(K::CASTCLASS_FStructProperty),
            "Corporation container type"
        );
        let address = owner.target() + container.offset_internal().read().await? as u64;
        let class = container.cast::<ZStructProperty>().struct_().read().await?.address();
        let data = field(ctx, class, "CorporationsData").await?;
        ensure!(
            data.zfield().cast_flags().await?.contains(K::CASTCLASS_FArrayProperty)
                && data.element_size().read().await? == 16,
            "Corporation array type"
        );
        let inner = data.cast::<jmap_dumper::objects::ZArrayProperty>().inner().read().await?;
        ensure!(
            inner.zfield().cast_flags().await?.contains(K::CASTCLASS_FStructProperty),
            "Corporation row type"
        );
        let stride = usize::try_from(inner.element_size().read().await?)?;
        ensure!((1..=256).contains(&stride), "Corporation record size");
        let st = inner.cast::<ZStructProperty>().struct_().read().await?.address();
        let name = field(ctx, st, "Name").await?;
        let level = field(ctx, st, "Level").await?;
        let hidden = field(ctx, st, "bHidden").await?;
        ensure!(
            name.zfield().cast_flags().await?.contains(K::CASTCLASS_FNameProperty)
                && name.element_size().read().await? == 8
                && level.zfield().cast_flags().await?.contains(K::CASTCLASS_FIntProperty)
                && level.element_size().read().await? == 4
                && hidden.zfield().cast_flags().await?.contains(K::CASTCLASS_FBoolProperty)
                && hidden.element_size().read().await? == 1,
            "Corporation column types"
        );
        let hidden_mask = hidden.cast::<ZBoolProperty>().field_mask().read().await?;
        let name = name.offset_internal().read().await? as usize;
        let level = level.offset_internal().read().await? as usize;
        let hidden = hidden.offset_internal().read().await? as usize;
        ensure!(
            name + 8 <= stride && level + 4 <= stride && hidden < stride && hidden_mask != 0,
            "Corporation column bounds"
        );
        Ok(Self {
            owner,
            rows: live::Array {
                address: address + data.offset_internal().read().await? as u64,
                stride,
                object_offset: 0,
            },
            name,
            level,
            hidden,
            hidden_mask,
        })
    }

    pub(super) async fn read(&self, ctx: &Ctx) -> Result<BTreeMap<String, u32>> {
        self.owner.check(ctx).await?;
        let (data, header, bytes) = self.rows.bytes(ctx).await?;
        ensure!(bytes.len() / self.rows.stride <= 32, "Corporation count bound");
        let mut result = BTreeMap::new();
        for (i, row) in bytes.chunks_exact(self.rows.stride).enumerate() {
            if row[self.hidden] & self.hidden_mask != 0 {
                continue;
            }
            let name_at = data + (i * self.rows.stride + self.name) as u64;
            let name = Ptr::<FName>::new(name_at, ctx.clone())?.read().await?;
            let level = u32::from_le_bytes(row[self.level..self.level + 4].try_into()?);
            ensure!(
                name.len() <= 128 && level <= 128 && result.insert(name, level).is_none(),
                "Corporation level/identity bounds"
            );
        }
        ensure!(self.rows.bytes(ctx).await? == (data, header, bytes), "Corporations changed");
        self.owner.check(ctx).await?;
        Ok(result)
    }
}
