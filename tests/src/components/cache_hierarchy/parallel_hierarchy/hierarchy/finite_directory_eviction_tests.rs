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

use crate::components::cache_hierarchy::{
    CacheBlockRequest, MemoryHierarchy,
    common::{
        CacheAccessType, CacheHierarchyAccessResult, FiniteDirectory, ParallelLRUSharedCache,
        ParallelUnifiedPrivateCache, statistics::ZeroSharedCacheSetStatistics,
    },
    mmu::NoMMU,
};
use crate::util::get_monotonic_ts;

const DIR_SET: usize = 1;
const DIR_WAY: usize = 2;
const PCACHE_SET: usize = 1024;
const PCACHE_ASSO: usize = 8;
const SCACHE_SET: usize = 2048;
const SCACHE_ASSO: usize = 16;
const CORE_COUNT: usize = 4;

type MH = super::ParallelMemoryHierarchy<
    NoMMU,
    ParallelUnifiedPrivateCache<CORE_COUNT, PCACHE_SET, PCACHE_ASSO>,
    ParallelLRUSharedCache<ZeroSharedCacheSetStatistics, SCACHE_SET, SCACHE_ASSO, false>,
    FiniteDirectory<DIR_SET, DIR_WAY>,
    true, // FILL_SCACHE_ON_FILLING_PCACHE
    true, // FILL_SCACHE_ON_PCACHE_CLEAN_EVICTION
    true, // FILL_SCACHE_ON_PCACHE_DIRTY_EVICTION
    true, // FILL_SCACHE_ON_PCACHE_REPLICA_CREATION
    CORE_COUNT,
>;

#[test]
fn test_finite_directory_eviction_on_allocation() {
    let mh = MH::new();

    // These block IDs will map to the same directory set (set 0)
    let block_id_0 = 0;
    let block_id_1 = DIR_SET as u64;
    let block_id_2 = 2 * DIR_SET as u64;

    // Access block 0, should be a miss and allocate a directory entry.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id: block_id_0,
                access_type: CacheAccessType::DataWrite,
                is_os: false,
            },
            get_monotonic_ts(),
        ),
        CacheHierarchyAccessResult::Miss
    );

    // Access block 1, should be a miss and allocate another directory entry.
    // The directory set is now full.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id: block_id_1,
                access_type: CacheAccessType::DataWrite,
                is_os: false,
            },
            get_monotonic_ts(),
        ),
        CacheHierarchyAccessResult::Miss
    );

    // Access block 2. This should cause an eviction in the directory.
    // The access itself is a miss. The key is that it doesn't panic.
    // The logic inside `access_memory_pblock_id` should handle the `evicted`
    // value returned from `get_or_create`.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id: block_id_2,
                access_type: CacheAccessType::DataWrite,
                is_os: false,
            },
            get_monotonic_ts(),
        ),
        CacheHierarchyAccessResult::Miss
    );

    // According to the result of the directory eviction, the evicted block should stay in the shared cache.
    // Read it and we should get a hit in the shared cache.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id: block_id_0,
                access_type: CacheAccessType::DataWrite,
                is_os: false,
            },
            get_monotonic_ts(),
        ),
        CacheHierarchyAccessResult::HitInSharedCache
    );
}

#[test]
fn test_directory_eviction_with_multiple_replicas() {
    let mh = MH::new();

    // Block 0 will be shared across multiple cores
    let shared_block_id = 0;

    // Core 0 reads the block
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id: shared_block_id,
                access_type: CacheAccessType::DataRead,
                is_os: false,
            },
            get_monotonic_ts(),
        ),
        CacheHierarchyAccessResult::Miss
    );

    // Core 1 reads the same block, creating a second replica
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 1,
                block_id: shared_block_id,
                access_type: CacheAccessType::DataRead,
                is_os: false,
            },
            get_monotonic_ts(),
        ),
        CacheHierarchyAccessResult::HitInOtherPrivateCache
    );

    // Verify both cores have the block
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id: shared_block_id,
                access_type: CacheAccessType::DataRead,
                is_os: false,
            },
            get_monotonic_ts(),
        ),
        CacheHierarchyAccessResult::HitInSelfPrivateCache
    );
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 1,
                block_id: shared_block_id,
                access_type: CacheAccessType::DataRead,
                is_os: false,
            },
            get_monotonic_ts(),
        ),
        CacheHierarchyAccessResult::HitInSelfPrivateCache
    );

    // Fill the rest of the directory set with other blocks
    // The directory has WAY=2, and shared_block_id is already taking one slot.
    let filler_block_id = DIR_SET as u64;
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 2,
                block_id: filler_block_id,
                access_type: CacheAccessType::DataRead,
                is_os: false,
            },
            get_monotonic_ts(),
        ),
        CacheHierarchyAccessResult::Miss
    );

    // This access will evict the directory entry for `shared_block_id`
    let evictor_block_id = 2 * DIR_SET as u64;
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 2,
                block_id: evictor_block_id,
                access_type: CacheAccessType::DataRead,
                is_os: false,
            },
            get_monotonic_ts(),
        ),
        CacheHierarchyAccessResult::Miss
    );

    // Now, the replicas for `shared_block_id` in core 0 and 1 should be invalidated.
    // Accessing it again should result in a shared cache hit, as the evicted entry
    // should have been written back.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id: shared_block_id,
                access_type: CacheAccessType::DataRead,
                is_os: false,
            },
            get_monotonic_ts(),
        ),
        CacheHierarchyAccessResult::HitInSharedCache
    );

    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 1,
                block_id: shared_block_id,
                access_type: CacheAccessType::DataRead,
                is_os: false,
            },
            get_monotonic_ts(),
        ),
        CacheHierarchyAccessResult::HitInOtherPrivateCache // Hit in core 0's p-cache now
    );
}
