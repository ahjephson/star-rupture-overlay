//! Current character bag only, using the Inspection helper's verified handle join.
use super::*;
async fn data(ctx: &Ctx, p: u64, n: usize) -> Result<Vec<u8>> {
    ensure!(n <= 4096 * 40, "Bag data budget");
    let mut result = Vec::with_capacity(n);
    for start in (0..n).step_by(65536) {
        result.extend(bytes(ctx, p + start as u64, (n - start).min(65536)).await?);
    }
    Ok(result)
}
async fn array(ctx: &Ctx, at: u64, stride: usize, max: usize) -> Result<(Vec<u8>, Vec<u8>)> {
    let header = bytes(ctx, at, 16).await?;
    let n = u32at(&header, 8) as usize;
    let cap = u32at(&header, 12) as usize;
    ensure!(n <= max && cap >= n && cap <= max * 2, "Bag array bounds");
    let values = if n == 0 {
        Vec::new()
    } else {
        data(ctx, u64at(&header, 0), n * stride).await?
    };
    Ok((header, values))
}
pub(super) async fn capture(ctx: &Ctx, base: &RuntimeCache) -> Result<BTreeMap<String, u32>> {
    let controller = base.controller.address;
    let character = ctx.read::<u64>(controller + 1928).await?;
    ensure!(
        ctx.read::<u64>(controller + base.fields.pawn).await? == character,
        "Bag character possession changed"
    );
    let bag = ctx.read::<u64>(character + 2800).await?;
    let store = ctx.read::<u64>(character + 1976).await?;
    let mut stamps = Vec::new();
    for p in [controller, character, bag, store] {
        stamps.push(serial(ctx, p).await?);
    }
    ensure!(
        ctx.read::<u64>(bag + 32).await? == character
            && ctx.read::<u64>(store + 32).await? == character
            && ctx.read::<u64>(bag + 1528).await? == store,
        "Bag owner/store association changed"
    );
    let (oh, owned) = array(ctx, bag + 464, 28, 1024).await?;
    let (sh, entries) = array(ctx, store + 448, 40, 4096).await?;
    let mut handles = BTreeMap::new();
    for row in entries.as_chunks::<40>().0 {
        let entry = u64at(row, 16);
        let handle = bytes(ctx, entry + 192, 16).await?;
        ensure!(
            handles.insert(handle, (entry, u64at(row, 32))).is_none(),
            "Ambiguous bag store handle"
        );
    }
    let mut seen = BTreeSet::new();
    let mut totals = BTreeMap::<String, u32>::new();
    let mut counts = Vec::new();
    let mut names = BTreeMap::<u64, String>::new();
    for row in owned.as_chunks::<28>().0 {
        let handle = row[12..28].to_vec();
        ensure!(seen.insert(handle.clone()), "Duplicate bag handle");
        let (entry, class) = handles
            .get(&handle)
            .copied()
            .context("Bag item missing from store; availability unknown")?;
        let count = ctx.read::<i32>(entry + 224).await?;
        ensure!((0..=1_000_000_000).contains(&count), "Bag amount bounds");
        let id = if let Some(id) = names.get(&class) {
            id.clone()
        } else {
            let id = asset(ctx, class).await?;
            names.insert(class, id.clone());
            id
        };
        let total = totals.entry(id).or_default();
        *total = total.checked_add(count as u32).context("Bag total overflow")?;
        counts.push((entry, handle, count));
    }
    for (entry, handle, count) in counts {
        ensure!(
            bytes(ctx, entry + 192, 16).await? == handle
                && ctx.read::<i32>(entry + 224).await? == count,
            "Bag item changed during observation"
        );
    }
    ensure!(
        bytes(ctx, bag + 464, 16).await? == oh && bytes(ctx, store + 448, 16).await? == sh,
        "Bag allocation changed"
    );
    if !owned.is_empty() {
        ensure!(data(ctx, u64at(&oh, 0), owned.len()).await? == owned, "Bag membership changed");
    }
    if !entries.is_empty() {
        ensure!(data(ctx, u64at(&sh, 0), entries.len()).await? == entries, "Bag store changed");
    }
    for s in stamps {
        ensure!(serial(ctx, s.0.address).await? == s, "Bag allocation identity changed");
    }
    ensure!(
        ctx.read::<u64>(controller + 1928).await? == character
            && ctx.read::<u64>(controller + base.fields.pawn).await? == character
            && ctx.read::<u64>(character + 2800).await? == bag
            && ctx.read::<u64>(character + 1976).await? == store
            && ctx.read::<u64>(bag + 1528).await? == store
            && ctx.read::<u64>(bag + 32).await? == character
            && ctx.read::<u64>(store + 32).await? == character,
        "Bag association changed"
    );
    Ok(totals)
}
