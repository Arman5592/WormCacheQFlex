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

use crate::components::cache_hierarchy::common::{
    CacheAccessType, CacheHierarchyAccessResult, InfiniteDirectory, PrivateCache,
};
use crate::components::cache_hierarchy::mmu::NoMMU;
use crate::components::cache_hierarchy::{CacheBlockRequest, MemoryHierarchy};
use crate::parameter;
use crate::util::get_monotonic_ts;

use super::super::super::common::{
    ParallelHarvardPrivateCache, ParallelLRUSharedCache, statistics::ZeroSharedCacheSetStatistics,
};
use super::ParallelMemoryHierarchy;

const PCACHE_SET: usize = 64;

type MH = ParallelMemoryHierarchy<
    NoMMU,
    ParallelHarvardPrivateCache<
        32,
        { PCACHE_SET },
        { parameter::HARVARD_PRI_I_CACHE_ASSO },
        { PCACHE_SET },
        { parameter::HARVARD_PRI_D_CACHE_ASSO },
    >,
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
fn i_create_sharer_from_clean_d() {
    let mh = MH::new();

    let block_id = 203;

    // First, generate a read request to the core 0 data cache.
    assert_eq!(
        // mh.access_memory_pblock_id_with_the_same_ts_and_vts(
        //     0,
        //     block_id,
        //     get_monotonic_ts(),
        //     CacheAccessType::DataRead
        // ),
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id,
                access_type: CacheAccessType::DataRead,
                is_os: false
            },
            get_monotonic_ts()
        ),
        CacheHierarchyAccessResult::Miss
    );

    // Second, generate a read request to the core 0 instruction cache.
    assert_eq!(
        // mh.access_memory_pblock_id_with_the_same_ts_and_vts(
        //     0,
        //     block_id,
        //     get_monotonic_ts(),
        //     CacheAccessType::InstructionFetch
        // ),
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id,
                access_type: CacheAccessType::InstructionFetch,
                is_os: false
            },
            get_monotonic_ts()
        ),
        CacheHierarchyAccessResult::HitInOtherPrivateCache
    );

    // Now, we get the share information.
    let replicas = mh.private_caches.query_replica_state(block_id);
    assert_eq!(replicas.len(), 1);
    assert!(!replicas[&0]);
}

#[test]
fn i_create_sharer_from_dirty_d() {
    let mh = MH::new();

    let block_id = 203;

    // First, generate a read request to the core 0 data cache.
    assert_eq!(
        // mh.access_memory_pblock_id_with_the_same_ts_and_vts(
        //     0,
        //     block_id,
        //     get_monotonic_ts(),
        //     CacheAccessType::DataRead
        // ),
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id,
                access_type: CacheAccessType::DataRead,
                is_os: false
            },
            get_monotonic_ts()
        ),
        CacheHierarchyAccessResult::Miss
    );

    // Second, generate a write request to the core 0 data cache.
    assert_eq!(
        // mh.access_memory_pblock_id_with_the_same_ts_and_vts(
        //     0,
        //     block_id,
        //     get_monotonic_ts(),
        //     CacheAccessType::DataWrite
        // ),
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id,
                access_type: CacheAccessType::DataWrite,
                is_os: false
            },
            get_monotonic_ts()
        ),
        if parameter::ENABLE_EXCLUSIVE_CACHE_STATE {
            CacheHierarchyAccessResult::HitInSelfPrivateCache
        } else {
            CacheHierarchyAccessResult::MissDueToPermission
        }
    );

    // Third, generate a read request to the core 0 instruction cache.
    assert_eq!(
        // mh.access_memory_pblock_id_with_the_same_ts_and_vts(
        //     0,
        //     block_id,
        //     get_monotonic_ts(),
        //     CacheAccessType::InstructionFetch
        // ),
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id,
                access_type: CacheAccessType::InstructionFetch,
                is_os: false
            },
            get_monotonic_ts()
        ),
        CacheHierarchyAccessResult::HitInOtherPrivateCache
    );

    // Now, we get the share information.
    let replicas = mh.private_caches.query_replica_state(block_id);
    assert_eq!(replicas.len(), 1);
    assert!(!replicas[&0]);
}

#[test]
fn d_create_sharer_from_clean_i() {
    let mh = MH::new();

    let block_id = 203;

    // First, generate a read request to the core 0 instruction cache.
    assert_eq!(
        // mh.access_memory_pblock_id_with_the_same_ts_and_vts(
        //     0,
        //     block_id,
        //     get_monotonic_ts(),
        //     CacheAccessType::InstructionFetch
        // ),
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id,
                access_type: CacheAccessType::InstructionFetch,
                is_os: false
            },
            get_monotonic_ts()
        ),
        CacheHierarchyAccessResult::Miss
    );

    // Second, generate a read request to the core 0 data cache.
    assert_eq!(
        // mh.access_memory_pblock_id_with_the_same_ts_and_vts(
        //     0,
        //     block_id,
        //     get_monotonic_ts(),
        //     CacheAccessType::DataRead
        // ),
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id,
                access_type: CacheAccessType::DataRead,
                is_os: false
            },
            get_monotonic_ts()
        ),
        CacheHierarchyAccessResult::HitInOtherPrivateCache
    );

    // Now, we get the share information.
    let replicas = mh.private_caches.query_replica_state(block_id);
    assert_eq!(replicas.len(), 1);
    assert!(!replicas[&0]);
}

#[test]
fn d_dirty_create_sharer_from_clean_i() {
    let mh = MH::new();

    let block_id = 203;

    // First, generate a read request to the core 0 instruction cache.
    assert_eq!(
        // mh.access_memory_pblock_id_with_the_same_ts_and_vts(
        //     0,
        //     block_id,
        //     get_monotonic_ts(),
        //     CacheAccessType::InstructionFetch
        // ),
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id,
                access_type: CacheAccessType::InstructionFetch,
                is_os: false
            },
            get_monotonic_ts()
        ),
        CacheHierarchyAccessResult::Miss
    );

    // Second, generate a write request to the core 0 data cache.
    assert_eq!(
        // mh.access_memory_pblock_id_with_the_same_ts_and_vts(
        //     0,
        //     block_id,
        //     get_monotonic_ts(),
        //     CacheAccessType::DataWrite
        // ),
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id,
                access_type: CacheAccessType::DataWrite,
                is_os: false
            },
            get_monotonic_ts()
        ),
        CacheHierarchyAccessResult::HitInOtherPrivateCache
    );

    // Third, generate a read request to the core 0 data cache.
    assert_eq!(
        // mh.access_memory_pblock_id_with_the_same_ts_and_vts(
        //     0,
        //     block_id,
        //     get_monotonic_ts(),
        //     CacheAccessType::DataRead
        // ),
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id,
                access_type: CacheAccessType::DataRead,
                is_os: false
            },
            get_monotonic_ts()
        ),
        CacheHierarchyAccessResult::HitInSelfPrivateCache
    );

    // Now, we get the share information.
    let replicas = mh.private_caches.query_replica_state(block_id);
    assert_eq!(replicas.len(), 1);
    assert!(replicas[&0]);
}
