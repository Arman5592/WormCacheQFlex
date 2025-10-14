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

// This file defines the tests for the memory_delayed module.
// All these tests are taken from the input that triggers a bug.

use crate::components::cache_hierarchy::common::{
    CacheAccessType, CacheHierarchyAccessResult, InfiniteDirectory, SharedCacheAccessRequest,
    SharedCacheAccessSource,
};
use crate::components::cache_hierarchy::mmu::NoMMU;
use crate::components::cache_hierarchy::{CacheBlockRequest, MemoryHierarchy};
use crate::parameter;
use crate::util::get_monotonic_ts;

use super::super::super::common::{
    ParallelLRUSharedCache, ParallelUnifiedPrivateCache, statistics::ZeroSharedCacheSetStatistics,
};
use super::ParallelMemoryHierarchy;

const PCACHE_SET: usize = 1024;

type MH = ParallelMemoryHierarchy<
    NoMMU,
    ParallelUnifiedPrivateCache<32, { PCACHE_SET }, { parameter::UNIFIED_PRI_CACHE_ASSO }>,
    ParallelLRUSharedCache<
        ZeroSharedCacheSetStatistics,
        { parameter::SHARED_CACHE_SET },
        { parameter::SHARED_CACHE_ASSO },
        { parameter::SHARED_CACHE_EXCLUSIVE },
    >,
    InfiniteDirectory<32768>,
    { parameter::SHARED_CACHE_FILL_WITH_PRIVATE_CACHE },
    { parameter::SHARED_CACHE_FILL_ON_CLEAN_EVICTION },
    { parameter::SHARED_CACHE_FILL_ON_DIRTY_EVICTION },
    { parameter::SHARED_CACHE_FILL_ON_REPLICA_CREATION },
    32,
>;

#[test]
fn read_evict_and_other_core_read_back() {
    let mh = MH::new();
    let block_id = 1043;

    // core 0 reads a data at timestamp 10.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id,
                access_type: CacheAccessType::DataRead,
                is_os: false,
            },
            get_monotonic_ts(),
        ),
        CacheHierarchyAccessResult::Miss
    );

    // Now, evict the block from the cache.
    for l in 0..parameter::UNIFIED_PRI_CACHE_ASSO {
        let block_id: u64 = ((l + 1) * PCACHE_SET) as u64 + block_id;
        assert_eq!(
            mh.access_memory_pblock_id(
                &CacheBlockRequest {
                    core_id: 0,
                    block_id,
                    access_type: CacheAccessType::DataRead,
                    is_os: false,
                },
                get_monotonic_ts(),
            ),
            CacheHierarchyAccessResult::Miss
        );
    }

    // Then core 1 reads the cache line. It should hit in the shared cache.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 1,
                block_id,
                access_type: CacheAccessType::DataRead,
                is_os: false,
            },
            get_monotonic_ts(),
        ),
        CacheHierarchyAccessResult::HitInSharedCache
    );
}

#[test]
fn one_core_write_first_then_read() {
    let mh = MH::new();
    let block_id = 1043;

    // core 0 reads a data at timestamp 10.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id,
                access_type: CacheAccessType::DataWrite,
                is_os: false,
            },
            10
        ),
        CacheHierarchyAccessResult::Miss
    );

    // Then, core 0 writes the data at timestamp 20.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id,
                access_type: CacheAccessType::DataRead,
                is_os: false,
            },
            20
        ),
        CacheHierarchyAccessResult::HitInSelfPrivateCache
    );
}

#[test]
fn write_write_read_then_old_write() {
    // This bug is related to the coherence state reconstruction.
    let mh = MH::new();
    let block_id = 1043;

    // First, there should be a write permission, by core 0, at timestamp 100.
    assert_eq!(
        // mh.access_memory_pblock_id(0, block_id, 100, CacheAccessType::DataWrite, false, 0),
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id,
                access_type: CacheAccessType::DataWrite,
                is_os: false,
            },
            100
        ),
        CacheHierarchyAccessResult::Miss
    );

    // Second, core 0 writes the same data at timestamp 150. This won't update the write timestamp in the directory.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id,
                access_type: CacheAccessType::DataWrite,
                is_os: false,
            },
            150
        ),
        CacheHierarchyAccessResult::HitInSelfPrivateCache
    );

    // Second, core 1 reads the data at timestamp 200.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 1,
                block_id,
                access_type: CacheAccessType::DataRead,
                is_os: false,
            },
            200
        ),
        CacheHierarchyAccessResult::HitInOtherPrivateCache
    );

    // Third, core 2 writes the data at timestamp 125.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 2,
                block_id,
                access_type: CacheAccessType::DataWrite,
                is_os: false,
            },
            125
        ),
        CacheHierarchyAccessResult::Unknown
    );
}

#[test]
fn write_read_then_early_read() {
    let mh = MH::new();
    let block_id = 1043;

    // First, there should be a write permission, by core 0, at timestamp 100.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id,
                access_type: CacheAccessType::DataWrite,
                is_os: false,
            },
            100
        ),
        CacheHierarchyAccessResult::Miss
    );

    // Second, core 1 reads the cache line. This can update the access timestamp but does not touch the write timestamp.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 1,
                block_id,
                access_type: CacheAccessType::DataRead,
                is_os: false,
            },
            150
        ),
        CacheHierarchyAccessResult::HitInOtherPrivateCache
    );

    // Third, core 2 reads the cache line at timestamp 50.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 2,
                block_id,
                access_type: CacheAccessType::DataRead,
                is_os: false,
            },
            50
        ),
        CacheHierarchyAccessResult::Unknown
    );
}

#[test]
fn read_then_write() {
    let mh = MH::new();
    let block_id = 1043;

    // First, there should be a read permission, by core 0, at timestamp 100.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id,
                access_type: CacheAccessType::DataRead,
                is_os: false,
            },
            100
        ),
        CacheHierarchyAccessResult::Miss
    );

    // Second, core 0 writes the same data at timestamp 150. This won't update the write timestamp in the directory.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id,
                access_type: CacheAccessType::DataWrite,
                is_os: false,
            },
            150
        ),
        if parameter::ENABLE_EXCLUSIVE_CACHE_STATE {
            CacheHierarchyAccessResult::HitInSelfPrivateCache
        } else {
            CacheHierarchyAccessResult::MissDueToPermission
        }
    );
}

#[test]
fn write_read_after_write() {
    let mh = MH::new();
    let block_id = 1043;

    // First, there should be a write permission, by core 0, at timestamp 100.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id,
                access_type: CacheAccessType::DataWrite,
                is_os: false,
            },
            100
        ),
        CacheHierarchyAccessResult::Miss
    );

    // Second, core 0 read the cache line. This can update the access timestamp but does not touch the write timestamp.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id,
                access_type: CacheAccessType::DataRead,
                is_os: false,
            },
            150
        ),
        CacheHierarchyAccessResult::HitInSelfPrivateCache
    );

    // Third, core 1 writes the data at timestamp 200.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 1,
                block_id,
                access_type: CacheAccessType::DataWrite,
                is_os: false,
            },
            200
        ),
        CacheHierarchyAccessResult::HitInOtherPrivateCache
    );
}

#[test]
fn later_read_after_write_cancel_sharers() {
    let mh = MH::new();
    let block_id = 1043;

    // Core 0 write, at 10.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id,
                access_type: CacheAccessType::DataWrite,
                is_os: false,
            },
            10
        ),
        CacheHierarchyAccessResult::Miss
    );

    // Core 0 write, at 20.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id,
                access_type: CacheAccessType::DataWrite,
                is_os: false,
            },
            20
        ),
        CacheHierarchyAccessResult::HitInSelfPrivateCache
    );

    // Core 1 read. at 30.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 1,
                block_id,
                access_type: CacheAccessType::DataRead,
                is_os: false,
            },
            30
        ),
        CacheHierarchyAccessResult::HitInOtherPrivateCache
    );

    // Core 2 read, at 15. We don't know the result.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 2,
                block_id,
                access_type: CacheAccessType::DataRead,
                is_os: false,
            },
            15
        ),
        CacheHierarchyAccessResult::Unknown
    );

    // But, Core 1's replica should not be invalidated.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 1,
                block_id,
                access_type: CacheAccessType::DataRead,
                is_os: false,
            },
            35
        ),
        CacheHierarchyAccessResult::HitInSelfPrivateCache
    );

    // Core 0 and Core 1 have the cache line.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id,
                access_type: CacheAccessType::DataRead,
                is_os: false,
            },
            40
        ),
        CacheHierarchyAccessResult::HitInSelfPrivateCache
    );

    // And also, core 1's entry is recorded in the directory.
    // This means when there is an eviction of this cache line in core'1, it should not trigger any panic.
    for i in 0..(parameter::UNIFIED_PRI_CACHE_ASSO + 1) {
        assert_eq!(
            mh.access_memory_pblock_id(
                &CacheBlockRequest {
                    core_id: 1,
                    block_id: block_id + ((i + 1) * parameter::UNIFIED_PRI_CACHE_SET) as u64,
                    access_type: CacheAccessType::DataRead,
                    is_os: false,
                },
                (50 + i) as u64
            ),
            CacheHierarchyAccessResult::Miss
        );
    }
}

#[test]
fn write_evict_read_write() {
    let mh = MH::new();
    let block_id = 1043;
    // First, there is a write access from core 0, at timestamp 10.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id,
                access_type: CacheAccessType::DataWrite,
                is_os: false,
            },
            10
        ),
        CacheHierarchyAccessResult::Miss
    );

    // We evict the cache line from the cache.
    for i in 0..parameter::UNIFIED_PRI_CACHE_ASSO {
        let block_id = block_id + (i + 1) as u64 * PCACHE_SET as u64;
        assert_eq!(
            mh.access_memory_pblock_id(
                &CacheBlockRequest {
                    core_id: 0,
                    block_id,
                    access_type: CacheAccessType::DataWrite,
                    is_os: false,
                },
                20 + i as u64 * 10
            ),
            CacheHierarchyAccessResult::Miss
        );
    }

    // OK, we read it back, by another core
    // This should not trigger any assertion failure.
    // But it creates an replica on chip. Now it should create a replica.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 1,
                block_id,
                access_type: CacheAccessType::DataRead,
                is_os: false,
            },
            1024
        ),
        CacheHierarchyAccessResult::HitInSharedCache
    );

    // what if we have a write access before the first one, from core 1?
    // Will this trigger an assertion failure?
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 2,
                block_id,
                access_type: CacheAccessType::DataWrite,
                is_os: false,
            },
            5
        ),
        CacheHierarchyAccessResult::Unknown
    );
}

#[test]
fn share_directory_entry_inseter_ts_update() {
    let mh = MH::new();
    let block_id = 1043;

    // Core 0 reads, at 30.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id,
                access_type: CacheAccessType::DataRead,
                is_os: false,
            },
            30
        ),
        CacheHierarchyAccessResult::Miss
    );

    // Core 1 reads, at 40.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 1,
                block_id,
                access_type: CacheAccessType::DataRead,
                is_os: false,
            },
            40
        ),
        CacheHierarchyAccessResult::HitInOtherPrivateCache
    );

    // Core 2 reads, at 20.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 2,
                block_id,
                access_type: CacheAccessType::DataRead,
                is_os: false,
            },
            20
        ),
        CacheHierarchyAccessResult::Unknown
    );

    // The insert timestamp right now is 30, because reads to share block are not updating the timestamp.

    // Now, core 2 writes at 25. It will trigger the assertion failure.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 2,
                block_id,
                access_type: CacheAccessType::DataWrite,
                is_os: false,
            },
            25
        ),
        CacheHierarchyAccessResult::MissDueToPermission
    );
}

#[test]
fn write_to_llc_cannot_invalidate_larger_ts() {
    use crate::components::cache_hierarchy::common::SharedCache;

    let mh = MH::new();
    let block_id = 1043;

    // Core 0 reads, at 30.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id,
                access_type: CacheAccessType::DataRead,
                is_os: false,
            },
            30
        ),
        CacheHierarchyAccessResult::Miss
    );

    // Core 0 evicts the cache line in the following requests.
    // Note that after this operation, the cache line is in the shared cache with a timestamp larger than 30.
    for i in 0..parameter::UNIFIED_PRI_CACHE_ASSO {
        let block_id = block_id + (i + 1) as u64 * PCACHE_SET as u64;
        assert_eq!(
            mh.access_memory_pblock_id(
                &CacheBlockRequest {
                    core_id: 0,
                    block_id,
                    access_type: CacheAccessType::DataRead,
                    is_os: false,
                },
                40 + i as u64 * 10
            ),
            CacheHierarchyAccessResult::Miss
        );
    }

    // Then core 1 writes the cache line at the timestamp 10.
    // It should not be a hit in the shared cache, because the result is unknown.
    // But still, the cache line is taken away into the private cache of core 1, because of the get exclusive.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 1,
                block_id,
                access_type: CacheAccessType::DataWrite,
                is_os: false,
            },
            10
        ),
        CacheHierarchyAccessResult::Unknown
    );

    // And the cache line is not in the LLC.
    assert!(!mh.shared_cache.peek(&SharedCacheAccessRequest {
        source: SharedCacheAccessSource::Core(0),
        block_id,
        access_type: CacheAccessType::DataRead,
        is_os: false,
    }));
}
