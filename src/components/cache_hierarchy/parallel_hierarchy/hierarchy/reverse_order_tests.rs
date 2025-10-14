// BSD 3-Clause License
//
// Copyright (c) 2024, Parallel Systems Architecture Laboratory (PARSA), EPFL.
// All rights reserved.
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provideed that the following conditions are met:
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

use std::collections::HashMap;

use crate::{
    components::cache_hierarchy::{
        CacheBlockRequest, MemoryHierarchy,
        common::{
            CacheAccessType, CacheHierarchyAccessResult, InfiniteDirectory, ParallelLRUSharedCache,
            ParallelUnifiedPrivateCache, PrivateCache, SharedCache, SharedCacheAccessRequest,
            SharedCacheAccessSource, statistics::ZeroSharedCacheSetStatistics,
        },
        mmu::NoMMU,
    },
    parameter,
};

use super::ParallelMemoryHierarchy;

const PCACHE_SET: usize = 64;

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

#[derive(Debug, PartialEq, Eq)]
enum BlockPosition {
    InPrivateCache(Vec<u32>),
    InSharedCache,
    NotInCache,
}

#[derive(Debug, PartialEq, Eq)]
enum BlockState {
    Shared,
    Modified,
}

impl MH {
    fn where_is_the_block(&mut self, block_id: u64) -> BlockPosition {
        let private_owner = self.private_caches.in_which_cores(block_id);

        if !private_owner.is_empty() {
            return BlockPosition::InPrivateCache(private_owner);
        }

        if self.shared_cache.peek(&SharedCacheAccessRequest {
            source: SharedCacheAccessSource::Core(0),
            block_id,
            access_type: CacheAccessType::DataRead,
            is_os: false,
        }) {
            return BlockPosition::InSharedCache;
        }
        BlockPosition::NotInCache
    }

    fn get_all_private_replicas(&mut self, block_id: u64) -> HashMap<u32, BlockState> {
        let result = self.private_caches.query_replica_state(block_id);
        result
            .into_iter()
            .map(|(k, v)| {
                (
                    k,
                    if v {
                        BlockState::Modified
                    } else {
                        BlockState::Shared
                    },
                )
            })
            .collect()
    }
}

#[test]
#[should_panic(expected = "assertion failed: self.lines[idx_of_slot_to_fill].ts <= ts")]
fn reversed_timestamp_from_the_same_core() {
    // let mh = MH::new();
    let mh = MH::new();
    // Fill one cache set with some data.
    let mut ts = 100;
    // Fill one cache set with some data.
    for l in 0..parameter::UNIFIED_PRI_CACHE_ASSO {
        let block_id: u64 = (l * PCACHE_SET) as u64;
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id,
                access_type: CacheAccessType::DataRead,
                is_os: false,
            },
            l as u64 + ts + 1,
        );
    }

    ts += 100;

    // If I access any touched block, it should be hit.
    for l in 0..parameter::UNIFIED_PRI_CACHE_ASSO {
        let block_id: u64 = (l * PCACHE_SET) as u64;
        assert_eq!(
            mh.access_memory_pblock_id(
                &CacheBlockRequest {
                    core_id: 0,
                    block_id,
                    access_type: CacheAccessType::DataRead,
                    is_os: false,
                },
                l as u64 + ts + 1,
            ),
            CacheHierarchyAccessResult::HitInSelfPrivateCache
        );
    }

    // OK, now there is an access with a reversed timestamp.
    let eval_block_id = (128 * PCACHE_SET) as u64;
    // The following line should trigger an assertion failure.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id: eval_block_id,
                access_type: CacheAccessType::DataRead,
                is_os: false,
            },
            1,
        ),
        CacheHierarchyAccessResult::Miss
    );
}

#[test]
fn write_invalidation_coherence() {
    let mut mh = MH::new();

    let block_id = 1024;
    // Core 0 gets a read permission at 1.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id,
                access_type: CacheAccessType::DataRead,
                is_os: false,
            },
            1,
        ),
        CacheHierarchyAccessResult::Miss
    );
    // Core 1 get a read permission at 10.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 1,
                block_id,
                access_type: CacheAccessType::DataRead,
                is_os: false,
            },
            10,
        ),
        CacheHierarchyAccessResult::HitInOtherPrivateCache
    );
    // Core 2 get a write permission at 50.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 2,
                block_id,
                access_type: CacheAccessType::DataWrite,
                is_os: false,
            },
            50,
        ),
        CacheHierarchyAccessResult::HitInOtherPrivateCache
    );
    // Now, core 0 and core 1 should have invalid the cache.
    let sharers = mh.get_all_private_replicas(block_id);
    assert_eq!(sharers.len(), 1);
    assert_eq!(sharers[&2], BlockState::Modified);

    // Now core 0 gets a read permission at 100.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id,
                access_type: CacheAccessType::DataRead,
                is_os: false,
            },
            100,
        ),
        CacheHierarchyAccessResult::HitInOtherPrivateCache
    );
    let sharers = mh.get_all_private_replicas(block_id);
    assert_eq!(sharers.len(), 2);
    assert_eq!(sharers[&2], BlockState::Shared);
    assert_eq!(sharers[&0], BlockState::Shared);
}

#[test]
fn raw_and_war() {
    let mut mh = MH::new();

    let block_id = 1024;
    // Core 0 gets a read permission at 0.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id,
                access_type: CacheAccessType::DataRead,
                is_os: false,
            },
            1,
        ),
        CacheHierarchyAccessResult::Miss
    );
    // Core 1 get a read permission at 10.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 1,
                block_id,
                access_type: CacheAccessType::DataRead,
                is_os: false,
            },
            10,
        ),
        CacheHierarchyAccessResult::HitInOtherPrivateCache
    );
    // Core 2 get a write permission at 5.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 2,
                block_id,
                access_type: CacheAccessType::DataWrite,
                is_os: false,
            },
            5,
        ),
        CacheHierarchyAccessResult::Unknown
    );
    // Now, core 0 should have invalid the cache.
    let sharers = mh.get_all_private_replicas(block_id);
    assert_eq!(sharers.len(), 1);
    assert_eq!(sharers[&2], BlockState::Modified);
}

#[test]
fn rarw() {
    let mut mh = MH::new();

    let block_id = 1024;
    // Core 0 gets a read permission at 10.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id,
                access_type: CacheAccessType::DataRead,
                is_os: false,
            },
            10,
        ),
        CacheHierarchyAccessResult::Miss
    );
    // Core 0 get a write permission at 20.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id,
                access_type: CacheAccessType::DataWrite,
                is_os: false,
            },
            20,
        ),
        if parameter::ENABLE_EXCLUSIVE_CACHE_STATE {
            CacheHierarchyAccessResult::HitInSelfPrivateCache
        } else {
            CacheHierarchyAccessResult::MissDueToPermission
        }
    );

    // Core 1 get a read permission at 1.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 1,
                block_id,
                access_type: CacheAccessType::DataRead,
                is_os: false,
            },
            1,
        ),
        CacheHierarchyAccessResult::Unknown
    );

    let sharers = mh.get_all_private_replicas(block_id);
    assert_eq!(sharers.len(), 2); // core 0 and core 1 should have the replica.
    assert_eq!(sharers[&0], BlockState::Shared); // core 0 should be in the shared state.
    assert_eq!(sharers[&1], BlockState::Shared); // core 1 should be in the shared state as well
}

#[test]
fn waw() {
    let mut mh = MH::new();
    let block_id = 1024;
    // Core 0 gets a write permission at timestamp 10
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id,
                access_type: CacheAccessType::DataWrite,
                is_os: false,
            },
            10,
        ),
        CacheHierarchyAccessResult::Miss
    );

    // Core 1 gets a write permission at timestamp 5.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 1,
                block_id,
                access_type: CacheAccessType::DataWrite,
                is_os: false,
            },
            5,
        ),
        CacheHierarchyAccessResult::Unknown
    );

    let sharers = mh.get_all_private_replicas(block_id);
    assert_eq!(sharers.len(), 1);
    assert_eq!(sharers[&1], BlockState::Modified);
}

#[test]
fn wwaw() {
    let mut mh = MH::new();
    let block_id = 1024;
    // Core 0 gets a write permission at timestamp 10
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id,
                access_type: CacheAccessType::DataWrite,
                is_os: false,
            },
            10,
        ),
        CacheHierarchyAccessResult::Miss
    );

    // Core 0 gets a write permission at timestamp 20. It should be a bit and no broadcast to the directory.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id,
                access_type: CacheAccessType::DataWrite,
                is_os: false,
            },
            20,
        ),
        CacheHierarchyAccessResult::HitInSelfPrivateCache
    );

    // Core 1 gets a write permission at timestamp 15.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 1,
                block_id,
                access_type: CacheAccessType::DataWrite,
                is_os: false,
            },
            15,
        ),
        CacheHierarchyAccessResult::Unknown
    );

    // Now, the only owner of the data should be core 0.
    let sharers = mh.get_all_private_replicas(block_id);
    assert_eq!(sharers.len(), 1);
    assert_eq!(sharers[&1], BlockState::Modified);
}

#[test]
fn rae() {
    let mut mh = MH::new();
    let block_id = 1024;
    // Core 0 writes to this block at timestamp 10.
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

    // This block is evicted due to contention.
    for i in 0..parameter::UNIFIED_PRI_CACHE_ASSO {
        let block_id: u64 = (10 * (i + 1) * PCACHE_SET + block_id as usize) as u64;
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id,
                access_type: CacheAccessType::DataRead,
                is_os: false,
            },
            (100 + i) as u64,
        );
    }

    // Now, the line is not in the memory hierarchy anymore.
    let sharers = mh.get_all_private_replicas(block_id);
    assert_eq!(sharers.len(), 0);

    // Then, there is a reader replica which is created before core 0 writes to the position.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 1,
                block_id,
                access_type: CacheAccessType::DataRead,
                is_os: false,
            },
            5,
        ),
        CacheHierarchyAccessResult::Unknown
    );

    let sharers = mh.get_all_private_replicas(block_id);
    assert_eq!(sharers.len(), 1); // only core 1 has the replica based on the host time order.
}

#[test]
fn eae() {
    let mut mh = MH::new();
    // Core 0 accesses the core at 200 and evicts the block at 216 with the dirty permission.
    let block_id = 1024;
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id,
                access_type: CacheAccessType::DataWrite,
                is_os: false,
            },
            200,
        ),
        CacheHierarchyAccessResult::Miss
    );

    for i in 0..parameter::UNIFIED_PRI_CACHE_ASSO {
        let block_id: u64 = (100 * (i + 1) * PCACHE_SET + block_id as usize) as u64;
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id,
                access_type: CacheAccessType::DataRead,
                is_os: false,
            },
            (200 + i) as u64,
        );
    }

    // That block should be evicted from the memory hierarchy.
    let sharers = mh.get_all_private_replicas(block_id);
    assert_eq!(sharers.len(), 0);

    // Core 1 accesses the core at 10 and evict the block
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 1,
                block_id,
                access_type: CacheAccessType::DataWrite,
                is_os: false,
            },
            10,
        ),
        CacheHierarchyAccessResult::Unknown
    );

    for i in 0..parameter::UNIFIED_PRI_CACHE_ASSO {
        let block_id: u64 = (255 * (i + 1) * PCACHE_SET + block_id as usize) as u64;
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 1,
                block_id,
                access_type: CacheAccessType::DataRead,
                is_os: false,
            },
            (10 + i) as u64,
        );
    }

    // Now there should be nothing in the private cache.
    let sharers = mh.get_all_private_replicas(block_id);
    assert_eq!(sharers.len(), 0);

    // Now, the block should be in the last-level cache.
    assert_eq!(
        mh.where_is_the_block(block_id),
        BlockPosition::InSharedCache
    );
}

#[test]
fn wae() {
    let mut mh = MH::new();
    let block_id = 1024;
    // Core 0 writes the block at 200 and evicts from the 217.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id,
                access_type: CacheAccessType::DataWrite,
                is_os: false,
            },
            200,
        ),
        CacheHierarchyAccessResult::Miss
    );

    for i in 0..parameter::UNIFIED_PRI_CACHE_ASSO {
        let block_id: u64 = (100 * (i + 1) * PCACHE_SET + block_id as usize) as u64;
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id,
                access_type: CacheAccessType::DataRead,
                is_os: false,
            },
            (200 + i) as u64,
        );
    }

    // That block should be evicted from the memory hierarchy.
    let sharers = mh.get_all_private_replicas(block_id);
    assert_eq!(sharers.len(), 0);

    // Core 1 writes to the block at 10.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 1,
                block_id,
                access_type: CacheAccessType::DataWrite,
                is_os: false,
            },
            10,
        ),
        CacheHierarchyAccessResult::Unknown
    );

    // Based on the host time order, core 1 should have the replica.
    let sharers = mh.get_all_private_replicas(block_id);
    assert_eq!(sharers.len(), 1);
    assert_eq!(sharers[&1], BlockState::Shared); // no way to get the modified state, because there is a later copy in the LLC.

    // assert_eq!(
    //     mh.where_is_the_block(block_id),
    //     BlockPosition::InSharedCache
    // );
}

#[test]
fn eaw() {
    let mut mh = MH::new();
    let block_id = 1024;

    // Core 0 writes to the block at 200.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id,
                access_type: CacheAccessType::DataWrite,
                is_os: false,
            },
            200,
        ),
        CacheHierarchyAccessResult::Miss
    );

    // Core 1 evicts the block at 100 with the dirty permission. the write happens at 10.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 1,
                block_id,
                access_type: CacheAccessType::DataWrite,
                is_os: false,
            },
            10,
        ),
        CacheHierarchyAccessResult::Unknown
    );

    for i in 0..parameter::UNIFIED_PRI_CACHE_ASSO {
        let block_id: u64 = (100 * (i + 1) * PCACHE_SET + block_id as usize) as u64;
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 1,
                block_id,
                access_type: CacheAccessType::DataRead,
                is_os: false,
            },
            (10 + i) as u64,
        );
    }

    // Because we are processing the request based on the order of the host time, we should not see any replica at this stage.
    let sharers = mh.get_all_private_replicas(block_id);
    assert_eq!(sharers.len(), 0);
}

#[test]
fn ear() {
    let mut mh = MH::new();
    let block_id = 1024;

    // Core 0 reads the block at 100.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id,
                access_type: CacheAccessType::DataRead,
                is_os: false,
            },
            100,
        ),
        CacheHierarchyAccessResult::Miss
    );

    // Then, core 1 evicts the block before 50. It creates a write access at 10.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 1,
                block_id,
                access_type: CacheAccessType::DataWrite,
                is_os: false,
            },
            10,
        ),
        CacheHierarchyAccessResult::Unknown
    );

    for i in 0..parameter::UNIFIED_PRI_CACHE_ASSO {
        let block_id: u64 = (100 * (i + 1) * PCACHE_SET + block_id as usize) as u64;
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 1,
                block_id,
                access_type: CacheAccessType::DataRead,
                is_os: false,
            },
            (10 + i) as u64,
        );
    }

    let sharers = mh.get_all_private_replicas(block_id);
    assert_eq!(sharers.len(), 0); // no replica anymore based on the host time order.
}

#[test]
fn rar() {
    let mut mh = MH::new();
    let block_id = 1024;

    // Core 0 reads the block at 10.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id,
                access_type: CacheAccessType::DataRead,
                is_os: false,
            },
            10,
        ),
        CacheHierarchyAccessResult::Miss
    );

    // Core 1 reads the block at 5.
    assert_eq!(
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 1,
                block_id,
                access_type: CacheAccessType::DataRead,
                is_os: false,
            },
            5,
        ),
        CacheHierarchyAccessResult::Unknown
    );

    // Now there should be two replicas of core 0 and core 1 in the private cache.
    let sharers = mh.get_all_private_replicas(block_id);
    assert_eq!(sharers.len(), 2);
    assert_eq!(sharers[&0], BlockState::Shared);
    assert_eq!(sharers[&1], BlockState::Shared);
}
