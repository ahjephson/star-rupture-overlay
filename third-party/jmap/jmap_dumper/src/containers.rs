use crate::mem::{Ctx, Pod};
use anyhow::Result;
use derive_where::derive_where;
use std::collections::BTreeMap;

use alloc::*;

use crate::mem::Ptr;

#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct FString(pub TArray<u16>);
impl Ptr<FString> {
    pub async fn read(&self) -> Result<String> {
        let array = self.cast::<TArray<u16>>();
        Ok(if let Some(chars) = array.data().await? {
            let chars = chars.read_vec(array.len().await?).await?;
            let len = chars.iter().position(|c| *c == 0).unwrap_or(chars.len());
            String::from_utf16(&chars[..len])?
        } else {
            "".to_string()
        })
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct FUtf8String(pub TArray<u16>);
impl Ptr<FUtf8String> {
    pub async fn read(&self) -> Result<String> {
        let array = self.cast::<TArray<u8>>();
        Ok(if let Some(chars) = array.data().await? {
            let chars = chars.read_vec(array.len().await?).await?;
            String::from_utf8(chars)?
        } else {
            "".to_string()
        })
    }
}

#[derive_where(Debug, Clone, Copy; T, A::ForElementType<T>)]
#[repr(C)]
pub struct TArray<T, A: TAlloc = TSizedHeapAllocator<32>> {
    pub data: A::ForElementType<T>,
    pub num: u32,
    pub max: u32,
}
impl<T: Pod + Clone, A: TAlloc> Ptr<TArray<T, A>> {
    /// Reads the backing pointer, then yields element `Ptr`s (no further reads).
    pub async fn iter(&self) -> Result<impl Iterator<Item = Ptr<T>> + '_> {
        let data = self.data().await?;
        let len = self.len().await?;
        Ok((0..len).map(move |i| data.as_ref().unwrap().offset(i)))
    }
}
impl<T, A: TAlloc> Ptr<TArray<T, A>> {
    pub async fn data(&self) -> Result<Option<Ptr<T>>> {
        let alloc = self
            .byte_offset(std::mem::offset_of!(TArray<T, A>, data))
            .cast::<A::ForElementType<T>>();

        <A as TAlloc>::ForElementType::<T>::data(&alloc).await
    }
    pub async fn len(&self) -> Result<usize> {
        Ok(self
            .byte_offset(std::mem::offset_of!(TArray<T, A>, num))
            .cast::<u32>()
            .read()
            .await? as usize)
    }
}
impl<T: Pod, A: TAlloc> Ptr<TArray<T, A>> {
    pub async fn read_vec(&self) -> Result<Vec<T>> {
        if let Some(data) = self.data().await? {
            data.read_vec(self.len().await?).await
        } else {
            Ok(vec![])
        }
    }
}

pub mod alloc {
    use super::*;
    use crate::mem::Ptr;
    use async_trait::async_trait;
    use std::marker::PhantomData;

    pub trait TAlloc {
        type ForElementType<T>: TAllocImpl<T>;
    }
    #[async_trait(?Send)]
    pub trait TAllocImpl<T> {
        async fn data(this: &Ptr<Self>) -> Result<Option<Ptr<T>>>
        where
            Self: Sized;
    }

    #[derive(Debug, Clone, Copy)]
    pub struct TSizedHeapAllocator<const N: usize>;
    impl<const N: usize> TAlloc for TSizedHeapAllocator<N> {
        type ForElementType<T> = THeapAlloc_ForElementType<N, T>;
    }
    #[derive(Debug, Clone, Copy)]
    #[repr(C)]
    pub struct THeapAlloc_ForElementType<const N: usize, T> {
        data: usize,
        _phantom: PhantomData<T>,
    }
    #[async_trait(?Send)]
    impl<const N: usize, T> TAllocImpl<T> for THeapAlloc_ForElementType<N, T> {
        async fn data(this: &Ptr<Self>) -> Result<Option<Ptr<T>>>
        where
            Self: Sized,
        {
            this.cast::<Option<Ptr<T>>>().read().await
        }
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct FNameEntryId {}
impl Ptr<FNameEntryId> {
    pub fn value(&self) -> Ptr<u32> {
        self.byte_offset(0).cast()
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct FName;
impl Ptr<FName> {
    pub fn comparison_index(&self) -> Ptr<FNameEntryId> {
        let offset = self.ctx().struct_member("FName", "ComparisonIndex");
        self.byte_offset(offset).cast()
    }
    pub fn number(&self) -> Ptr<u32> {
        let offset = self.ctx().struct_member("FName", "Number");
        self.byte_offset(offset).cast()
    }
    pub async fn read(&self) -> Result<String> {
        let number = self.number().read().await?;
        let comparison_index = self.comparison_index().value().read().await?;
        resolve_fname(self.ctx(), comparison_index, number).await
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct FNameEntryAllocator;
impl Ptr<FNameEntryAllocator> {
    pub fn blocks(&self) -> Ptr<Ptr<u8>> {
        let offset = self.ctx().struct_member("FNameEntryAllocator", "Blocks");
        self.byte_offset(offset).cast()
    }
}

pub async fn resolve_fname(ctx: &Ctx, comparison_index: u32, number: u32) -> Result<String> {
    let fnamepool = ctx.fnamepool;
    let case_preserving = ctx.build_config.case_preserving;

    if ctx.ue_version() < (4, 22) {
        let chunks = Ptr::<Ptr<Ptr<Ptr<()>>>>::new(fnamepool, ctx.clone())?
            .read()
            .await?;

        let per_chunk = 0x4000;

        let chunk = comparison_index / per_chunk;
        let offset = comparison_index % per_chunk;

        let chunk = chunks.offset(chunk as usize).read().await?;
        let entry = chunk.offset(offset as usize).read().await?;

        let index = entry.cast::<u32>().read().await?;
        let is_wide = (index & 1) == 1;
        let char_data = entry.byte_offset(0x10);

        let base = if is_wide {
            let mut data = vec![];
            let char_data = char_data.cast::<u16>();
            for i in 0.. {
                let next = char_data.offset(i).read().await?;
                if next == 0 {
                    break;
                }
                data.push(next);
            }
            String::from_utf16(&data)?
        } else {
            let mut data = vec![];
            let char_data = char_data.cast::<u8>();
            for i in 0.. {
                let next = char_data.offset(i).read().await?;
                if next == 0 {
                    break;
                }
                data.push(next);
            }
            String::from_utf8(data)?
        };
        return Ok(if number == 0 {
            base
        } else {
            format!("{base}_{}", number - 1)
        });
    }

    let entries = Ptr::<FNameEntryAllocator>::new(fnamepool, ctx.clone())?;
    let blocks = entries.blocks();

    let block_index = (comparison_index >> 16) as usize;
    let offset = if case_preserving {
        (comparison_index & 0xffff) as usize * 4 + 4
    } else {
        (comparison_index & 0xffff) as usize * 2
    };

    let block = blocks.offset(block_index).read().await?;
    let header = block.offset(offset).cast::<u16>().read().await?;

    let len = if case_preserving {
        (header >> 1) as usize
    } else {
        (header >> 6) as usize
    };
    let is_wide = header & 1 != 0;

    let data = block.offset(offset + 2);
    let base = if is_wide {
        String::from_utf16(
            &data
                .read_vec(len * 2)
                .await?
                .chunks(2)
                .map(|chunk| u16::from_le_bytes(chunk.try_into().unwrap()))
                .collect::<Vec<_>>(),
        )?
    } else {
        String::from_utf8(data.read_vec(len).await?)?
    };
    Ok(if number == 0 {
        base
    } else {
        format!("{base}_{}", number - 1)
    })
}

pub async fn extract_fnames(ctx: &Ctx) -> Result<BTreeMap<u32, String>> {
    let mut names = BTreeMap::new();

    let fname_pool_address = ctx.fnamepool;
    let ue_version = ctx.ue_version();
    let case_preserving = ctx.build_config.case_preserving;

    if ue_version < (4, 22) {
        let per_chunk: usize = 0x4000;
        let chunk_table = ctx.read::<u64>(fname_pool_address).await?;

        for chunk_index in 0..0x400usize {
            let chunk_ptr = ctx
                .read::<u64>(chunk_table + (chunk_index as u64) * 8)
                .await?;
            if chunk_ptr == 0 {
                break;
            }

            let slots = match ctx.read_vec::<u64>(chunk_ptr, per_chunk).await {
                Ok(s) => s,
                Err(_) => break,
            };

            for (slot_index, &entry_ptr) in slots.iter().enumerate() {
                if entry_ptr == 0 {
                    continue;
                }
                let comparison_index = (chunk_index * per_chunk + slot_index) as u32;
                if let Ok(name) = resolve_fname(ctx, comparison_index, 0).await {
                    names.insert(comparison_index, name);
                }
            }
        }
    } else {
        // versions >= 4.22
        let stride: usize = if case_preserving { 4 } else { 2 };
        let header_off: usize = if case_preserving { 4 } else { 0 };
        let data_off: usize = if case_preserving { 6 } else { 2 };
        let block_size: usize = stride * 0x10000;

        let current_block_off = ctx.struct_member("FNameEntryAllocator", "CurrentBlock") as u64;
        let byte_cursor_off = ctx.struct_member("FNameEntryAllocator", "CurrentByteCursor") as u64;
        let blocks_off = ctx.struct_member("FNameEntryAllocator", "Blocks") as u64;

        let current_block = ctx
            .read::<u32>(fname_pool_address + current_block_off)
            .await? as usize;
        let current_byte_cursor = ctx
            .read::<u32>(fname_pool_address + byte_cursor_off)
            .await? as usize;

        for block_index in 0..=current_block {
            let block_ptr = ctx
                .read::<u64>(fname_pool_address + blocks_off + (block_index as u64) * 8)
                .await?;
            if block_ptr == 0 {
                continue;
            }

            let read_len = if block_index == current_block {
                current_byte_cursor
            } else {
                block_size
            };
            if read_len == 0 {
                continue;
            }

            let mut chunk = vec![0u8; read_len];
            if ctx.read_buf(block_ptr, &mut chunk).await.is_err() {
                continue;
            }

            let mut cursor = 0usize;
            while cursor + header_off + 2 <= chunk.len() {
                let h = cursor + header_off;
                let header = u16::from_le_bytes([chunk[h], chunk[h + 1]]);

                let len = if case_preserving {
                    (header >> 1) as usize
                } else {
                    (header >> 6) as usize
                };
                let is_wide = (header & 1) != 0;

                if len == 0 {
                    break;
                }

                let char_size = if is_wide { 2 } else { 1 };
                let data_start = cursor + data_off;
                let data_end = data_start + len * char_size;
                if data_end > chunk.len() {
                    break;
                }

                let decoded = if is_wide {
                    let units: Vec<u16> = chunk[data_start..data_end]
                        .chunks_exact(2)
                        .map(|c| u16::from_le_bytes([c[0], c[1]]))
                        .collect();
                    String::from_utf16(&units).ok()
                } else {
                    String::from_utf8(chunk[data_start..data_end].to_vec()).ok()
                };

                if let Some(name) = decoded {
                    let value = ((block_index << 16) | (cursor / stride)) as u32;
                    names.insert(value, name);
                }

                let size = (data_off + len * char_size).next_multiple_of(stride);
                if size == 0 {
                    break;
                }
                cursor += size;
            }
        }
    }

    Ok(names)
}

// FScriptArray - untyped array for runtime element access
#[derive(Debug, Clone, Copy)]
pub struct FScriptArray;
impl Ptr<FScriptArray> {
    pub fn data(&self) -> Ptr<Option<Ptr<u8>>> {
        let alloc_offset = self
            .ctx()
            .struct_member("FScriptArray", "AllocatorInstance");
        let data_offset = self
            .ctx()
            .struct_member("FDefaultAllocatorInstance", "Data");
        self.byte_offset(alloc_offset + data_offset).cast()
    }
    pub fn num(&self) -> Ptr<i32> {
        let offset = self.ctx().struct_member("FScriptArray", "ArrayNum");
        self.byte_offset(offset).cast()
    }
}

// FScriptBitArray - for reading allocation flags in sparse arrays
// Uses FDefaultBitArrayAllocator which has 4 inline DWORDs (128 bits) + overflow pointer
#[derive(Debug, Clone, Copy)]
pub struct FScriptBitArray;

impl Ptr<FScriptBitArray> {
    /// Get pointer to inline data (first 4 DWORDs)
    pub fn inline_data(&self) -> Ptr<u32> {
        let alloc_offset = self
            .ctx()
            .struct_member("FScriptBitArray", "AllocatorInstance");
        let inline_offset = self
            .ctx()
            .struct_member("FDefaultBitArrayAllocator", "InlineData");
        self.byte_offset(alloc_offset + inline_offset).cast()
    }

    /// Get pointer to secondary (heap) data for overflow
    pub fn secondary_data(&self) -> Ptr<Option<Ptr<u32>>> {
        let alloc_offset = self
            .ctx()
            .struct_member("FScriptBitArray", "AllocatorInstance");
        let secondary_offset = self
            .ctx()
            .struct_member("FDefaultBitArrayAllocator", "SecondaryData");
        self.byte_offset(alloc_offset + secondary_offset).cast()
    }

    pub fn num_bits(&self) -> Ptr<i32> {
        let offset = self.ctx().struct_member("FScriptBitArray", "NumBits");
        self.byte_offset(offset).cast()
    }

    /// Check if a specific index is allocated (bit is set)
    pub async fn is_allocated(&self, index: usize) -> Result<bool> {
        let num_bits = self.num_bits().read().await?;
        if num_bits <= 0 || index >= num_bits as usize {
            return Ok(false);
        }

        let word_index = index / 32;
        let bit_index = index % 32;

        // If secondary pointer is set, ALL data is on heap
        // Otherwise, ALL data is inline
        let word = if let Some(secondary_ptr) = self.secondary_data().read().await? {
            secondary_ptr.offset(word_index).read().await?
        } else {
            self.inline_data().offset(word_index).read().await?
        };

        Ok((word & (1 << bit_index)) != 0)
    }
}

// FScriptSparseArray - for iterating valid entries
#[derive(Debug, Clone, Copy)]
pub struct FScriptSparseArray;
impl Ptr<FScriptSparseArray> {
    pub fn data(&self) -> Ptr<FScriptArray> {
        let offset = self.ctx().struct_member("FScriptSparseArray", "Data");
        self.byte_offset(offset).cast()
    }
    pub fn allocation_flags(&self) -> Ptr<FScriptBitArray> {
        let offset = self
            .ctx()
            .struct_member("FScriptSparseArray", "AllocationFlags");
        self.byte_offset(offset).cast()
    }
    /// Get the maximum index (Data.ArrayNum)
    pub async fn get_max_index(&self) -> Result<usize> {
        Ok(self.data().num().read().await? as usize)
    }
    /// Check if an index is valid (allocated, not free)
    pub async fn is_valid_index(&self, index: usize) -> Result<bool> {
        self.allocation_flags().is_allocated(index).await
    }
    /// Get pointer to element data at index (caller must know element size)
    pub async fn get_data(&self, index: usize, element_size: usize) -> Result<Ptr<u8>> {
        let data_ptr = self.data().data().read().await?.expect("sparse array data");
        Ok(data_ptr.byte_offset(index * element_size))
    }
}

// FScriptSet - for iterating set elements
#[derive(Debug, Clone, Copy)]
pub struct FScriptSet;
impl Ptr<FScriptSet> {
    pub fn elements(&self) -> Ptr<FScriptSparseArray> {
        let offset = self.ctx().struct_member("FScriptSet", "Elements");
        self.byte_offset(offset).cast()
    }
}

// FScriptMap - for iterating map pairs (same layout as Set, stores pairs)
#[derive(Debug, Clone, Copy)]
pub struct FScriptMap;
impl Ptr<FScriptMap> {
    pub fn pairs(&self) -> Ptr<FScriptSet> {
        let offset = self.ctx().struct_member("FScriptMap", "Pairs");
        self.byte_offset(offset).cast()
    }
}
