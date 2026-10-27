//! Bounded temporary read cache for discovery; never retained by live sampling.
use anyhow::{Context, Result};
use async_trait::async_trait;
use jmap_dumper::mem::{BlockCache, Mem};
use std::{cell::RefCell, collections::HashSet};
const PAGE: u64 = 65536;
const PAGES: usize = 256;
pub(crate) struct DiscoveryMemory<M> {
    inner: M,
    cache: BlockCache<M>,
    pages: RefCell<HashSet<u64>>,
}
impl<M: Mem + Clone> DiscoveryMemory<M> {
    pub(crate) fn new(inner: M) -> Self {
        Self {
            cache: BlockCache::wrap(inner.clone()),
            inner,
            pages: RefCell::new(HashSet::new()),
        }
    }
}
#[async_trait(?Send)]
impl<M: Mem> Mem for DiscoveryMemory<M> {
    async fn read_buf(&self, address: u64, buf: &mut [u8]) -> Result<()> {
        if buf.is_empty() {
            return Ok(());
        }
        let end = address.checked_add(buf.len() as u64 - 1).context("Cache address overflow")?;
        let first = address / PAGE;
        let last = end / PAGE;
        // Large reads bypass the cache rather than expanding its resident budget.
        if last - first >= PAGES as u64 {
            return self.inner.read_buf(address, buf).await;
        }
        {
            let mut pages = self.pages.borrow_mut();
            let missing = (first..=last).filter(|p| !pages.contains(p)).count();
            if pages.len() + missing > PAGES {
                self.cache.clear_cache();
                pages.clear();
            }
            pages.extend(first..=last);
        }
        self.cache.read_buf(address, buf).await
    }
    fn clear_cache(&self) {
        self.cache.clear_cache();
        self.pages.borrow_mut().clear();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, rc::Rc};
    #[derive(Clone)]
    struct Synthetic(Rc<Cell<usize>>);
    #[async_trait(?Send)]
    impl Mem for Synthetic {
        async fn read_buf(&self, address: u64, buf: &mut [u8]) -> Result<()> {
            self.0.set(self.0.get() + 1);
            for (i, b) in buf.iter_mut().enumerate() {
                *b = ((address + i as u64) % 251) as u8;
            }
            Ok(())
        }
    }
    #[test]
    fn bounds_cache_and_preserves_cross_page_reads() {
        smol::block_on(async {
            let reads = Rc::new(Cell::new(0));
            let cache = DiscoveryMemory::new(Synthetic(reads.clone()));
            let mut b = [0; 32];
            for page in 0..(PAGES * 3) {
                let address = (page as u64 + 1) * PAGE - 9;
                cache.read_buf(address, &mut b).await.unwrap();
                for (i, v) in b.iter().enumerate() {
                    assert_eq!(*v, ((address + i as u64) % 251) as u8);
                }
                assert!(cache.pages.borrow().len() <= PAGES);
            }
            let before = reads.get();
            cache.read_buf(PAGES as u64 * 3 * PAGE - 9, &mut b).await.unwrap();
            assert_eq!(reads.get(), before);
            assert!(cache.read_buf(u64::MAX, &mut b).await.is_err());
            cache.clear_cache();
            assert!(cache.pages.borrow().is_empty());
            assert!(cache.write_buf(0, &[1]).await.is_err());
        });
    }
}
