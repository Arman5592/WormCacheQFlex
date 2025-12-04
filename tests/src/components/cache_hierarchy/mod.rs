// BSD 3-Clause License
//
// Copyright (c) 2024, Parallel Systems Architecture Laboratory (PARSA), EPFL.
// All rights reserved.
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this
//    list of conditions and the following disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice,
//    this list of conditions and the following disclaimer in the documentation
//    and/or other materials provided with the distribution.
//
// 3. Neither the name of the PARSA, EPFL
//    nor the names of its contributors may be used to endorse or promote
//    products derived from this software without specific prior written
//    permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
// AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
// CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
// OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
// OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

// This module defines the basic memory hierarchies using fine-grained locks.
// It contains the same logical memory hierarchy as the `memory`, but uses locks for shared communication.
// - TLB, which is private.
// - Private caches, with set locks.
// - Directory, with set locks.
// - Shared caches, with set locks.

pub mod common;
pub mod mmu;
mod parallel_hierarchy;
mod single_cache_hierarchy;

use common::CacheAccessType;
use common::CacheHierarchyAccessResult;
use mmu::MMUTranslationResult;
pub use parallel_hierarchy::ParallelCacheHierarchyPlugin;
pub use parallel_hierarchy::hierarchy;

pub use single_cache_hierarchy::SingleCacheHierarchyPlugin;

use crate::parameter;
use crate::parameter::ADJACENT_LINE_PREFETCHING;

#[derive(Clone)]
pub struct MemoryAccessRequest {
    pub core_id: u32,
    pub va: u64,
    pub access_type: CacheAccessType,
    pub is_os: bool,
}

impl MemoryAccessRequest {
    pub fn is_instruction(&self) -> bool {
        self.access_type == CacheAccessType::InstructionFetch
    }

    pub fn is_store(&self) -> bool {
        self.access_type == CacheAccessType::DataWrite
    }

    pub fn is_prefetch(&self) -> bool {
        self.access_type == CacheAccessType::PrefetchRead
            || self.access_type == CacheAccessType::PrefetchWrite
    }

    pub fn is_os(&self) -> bool {
        self.is_os
    }
}

#[derive(Clone)]
pub enum CacheAccessSource {
    Core(u32),
    Device,
}

#[derive(Clone)]
pub struct CacheBlockRequest {
    pub core_id: u32,
    pub block_id: u64,
    pub access_type: CacheAccessType,
    pub is_os: bool,
}

impl CacheBlockRequest {
    pub fn is_instruction(&self) -> bool {
        self.access_type == CacheAccessType::InstructionFetch
    }

    pub fn is_store(&self) -> bool {
        self.access_type == CacheAccessType::DataWrite
    }

    pub fn is_prefetch(&self) -> bool {
        self.access_type == CacheAccessType::PrefetchRead
            || self.access_type == CacheAccessType::PrefetchWrite
    }

    pub fn is_os(&self) -> bool {
        self.is_os
    }

    pub fn is_page_walk(&self) -> bool {
        self.access_type == CacheAccessType::PageWalkRead
    }
}

pub trait MemoryHierarchy {
    fn access_memory_pblock_id(
        &self,
        request: &CacheBlockRequest,
        ts: u64,
    ) -> CacheHierarchyAccessResult;

    fn access_from_device_with_pa(
        &self,
        paddr: u64,
        access_type: CacheAccessType,
        ts: u64,
    ) -> CacheHierarchyAccessResult;

    fn translate(&self, request: &MemoryAccessRequest, ts: u64) -> MMUTranslationResult;

    fn flush_mmu(&self, core_id: u32, info: mmu::MMUFlushMode);

    #[inline]
    fn access_memory_with_va_and_pa(
        &self,
        request: &MemoryAccessRequest,
        pa: Option<u64>,
        ts: u64,
    ) -> CacheHierarchyAccessResult {
        assert!(
            !(request.is_instruction() && request.is_store()),
            "Instruction and store permission cannot be used at the same time."
        );

        let translation = self.translate(request, ts);

        let translated_request = match translation {
            MMUTranslationResult::MissNotCacheable(paddr) | MMUTranslationResult::Hit(paddr, _) => {
                let block_id = paddr >> parameter::CACHE_LINE_SIZE.trailing_zeros();

                let block_id = if let Some(pa) = pa {
                    pa >> parameter::CACHE_LINE_SIZE.trailing_zeros()
                } else {
                    block_id
                };

                CacheBlockRequest {
                    core_id: request.core_id,
                    block_id,
                    access_type: request.access_type.clone(),
                    is_os: request.is_os,
                }
            }
            MMUTranslationResult::Miss(paddr, walk_trace) => {
                // walk.
                for pa in walk_trace {
                    if pa == u64::MAX {
                        break;
                    }
                    let pte_block_id = pa >> parameter::CACHE_LINE_SIZE.trailing_zeros();
                    let request = CacheBlockRequest {
                        core_id: request.core_id,
                        block_id: pte_block_id,
                        access_type: CacheAccessType::PageWalkRead,
                        is_os: request.is_os,
                    };
                    self.access_memory_pblock_id(&request, ts);
                }

                let block_id = paddr >> parameter::CACHE_LINE_SIZE.trailing_zeros();

                let block_id = if let Some(pa) = pa {
                    pa >> parameter::CACHE_LINE_SIZE.trailing_zeros()
                } else {
                    block_id
                };

                CacheBlockRequest {
                    core_id: request.core_id,
                    block_id,
                    access_type: request.access_type.clone(),
                    is_os: request.is_os,
                }
            }
        };

        let result = self.access_memory_pblock_id(&translated_request, ts);

        if ADJACENT_LINE_PREFETCHING {
            let mut prefetch_request = translated_request.clone();
            prefetch_request.block_id += 1;
            self.access_memory_pblock_id(&prefetch_request, ts);
        }
        result
    }

    #[inline]
    fn access_memory_with_va(
        &self,
        request: &MemoryAccessRequest,
        ts: u64,
    ) -> CacheHierarchyAccessResult {
        self.access_memory_with_va_and_pa(request, None, ts)
    }

    fn serialize(&self, name: &str, numa_node_id: usize);
    fn deserialize(&mut self, name: &str, numa_node_id: usize);
}
