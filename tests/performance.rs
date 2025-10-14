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

// This file contains the performance test of various configurations.

use perf_event::Builder;

use worm_cache::components::cache_hierarchy::CacheBlockRequest;
use worm_cache::components::cache_hierarchy::MemoryHierarchy;
use worm_cache::components::cache_hierarchy::common::CacheAccessType;
use worm_cache::components::cache_hierarchy::common::InfiniteDirectory;
use worm_cache::components::cache_hierarchy::common::ParallelLRUSharedCache;
use worm_cache::components::cache_hierarchy::common::ParallelUnifiedPrivateCache;
use worm_cache::components::cache_hierarchy::common::statistics::ZeroSharedCacheSetStatistics;
use worm_cache::components::cache_hierarchy::hierarchy::ParallelMemoryHierarchy;
use worm_cache::components::cache_hierarchy::mmu::FullyAssociativeTLB;
use worm_cache::components::cache_hierarchy::mmu::NoMMU;
use worm_cache::components::cache_hierarchy::mmu::tlb::AddressSpaceID::NonGlobal;
use worm_cache::debug::statistics::Statistics;

use worm_cache::parameter;

type MH = ParallelMemoryHierarchy<
    NoMMU,
    ParallelUnifiedPrivateCache<
        64,
        { parameter::UNIFIED_PRI_CACHE_SET },
        { parameter::UNIFIED_PRI_CACHE_ASSO },
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
    64,
>;

#[test]
fn testing_pcache_always_miss() {
    let mh = MH::new();

    // What I need to do is just to access the block id belonging to a specific shared cache set.
    // The block id is calculated as follows:
    // block_id = set_id * associativity + way_id

    let mut ts: u64 = 1;
    let mut block_id = 42;

    let mut counter = Builder::new().build().unwrap();

    counter.enable().unwrap();
    loop {
        for _ in 0..64 {
            mh.access_memory_pblock_id(
                &CacheBlockRequest {
                    core_id: 0,
                    block_id,
                    access_type: CacheAccessType::DataRead,
                    is_os: false,
                },
                ts,
            );
            ts += 1;
            block_id += parameter::UNIFIED_PRI_CACHE_SET as u64;
        }
        block_id = 42;

        if ts > 1024 * 1024 * 10 {
            break;
        }
    }
    counter.disable().unwrap();

    let count = counter.read().unwrap();
    let ave_count = count / 1024 / 1024 / 10;

    println!("{}", ave_count);
    assert!(ave_count < 1200);

    // print the miss rate of the data cache and shared cache from core 0. They should be 100%.
    println!("{}", Statistics::global_get_line_for_all_cores(0)[0]);
}

#[test]
fn testing_pcache_always_hit() {
    let mh = MH::new();

    // What I need to do is just to access the block id belonging to a specific shared cache set.
    // The block id is calculated as follows:
    // block_id = set_id * associativity + way_id

    let mut ts: u64 = 1;
    let block_id = 42;

    let mut counter = Builder::new().build().unwrap();

    counter.enable().unwrap();
    loop {
        for _ in 0..64 {
            mh.access_memory_pblock_id(
                &CacheBlockRequest {
                    core_id: 0,
                    block_id,
                    access_type: CacheAccessType::DataRead,
                    is_os: false,
                },
                ts,
            );
            ts += 1;
        }

        if ts > 1024 * 1024 * 10 {
            break;
        }
    }
    counter.disable().unwrap();

    let count = counter.read().unwrap();
    let ave_count = count / 1024 / 1024 / 10;

    println!("{}", ave_count);
    assert!(ave_count < 130);

    // print the miss rate of the data cache and shared cache from core 0. They should be 100%.
    println!("{}", Statistics::global_get_line_for_all_cores(0)[0]);
}

#[test]
fn read_shared_cache_line() {
    let mh = MH::new();

    // What I need to do is just to access the block id belonging to a specific shared cache set.
    // The block id is calculated as follows:
    // block_id = set_id * associativity + way_id

    let mut ts: u64 = 1;
    let block_id = 42;

    // all cores except the last core read the shared cache line.
    for core_id in 0..62 {
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: core_id as u32,
                block_id,
                access_type: CacheAccessType::DataRead,
                is_os: false,
            },
            ts,
        );
        ts += 1;
    }

    let mut counter = Builder::new().build().unwrap();
    counter.enable().unwrap();

    // core 128 reads the shared cache line.
    mh.access_memory_pblock_id(
        &CacheBlockRequest {
            core_id: 63,
            block_id,
            access_type: CacheAccessType::DataRead,
            is_os: false,
        },
        ts,
    );

    counter.disable().unwrap();

    // print the counter value
    let count = counter.read().unwrap();
    assert!(count < 700);
    println!("{}", count);
}

#[test]
fn test_64_way_tlb_access_time() {
    let mut counter = Builder::new().build().unwrap();

    let mut ts: u64 = 1;
    let mut vpn = 42;
    let mut hit = 0;

    let mut tlb_set = FullyAssociativeTLB::new(64);

    // populate elements
    for _ in 0..64 {
        tlb_set.insert(vpn, NonGlobal(1), ts, vpn);
        ts += 1;
        vpn += 1;
    }

    counter.enable().unwrap();

    for _ in 0..2048 {
        for _ in 0..64 {
            let result = tlb_set.lookup(vpn, 1, ts);
            ts += 1;
            vpn += 1;
            vpn %= 64;

            if result.is_some() {
                hit += 1;
            }
        }
    }

    counter.disable().unwrap();

    let count = counter.read().unwrap();
    let ave_count = count / (64 * 2048);

    println!("FA TLB: {}", ave_count);
    println!("FA TLB: {}", hit);
    assert!(ave_count < 90);
}

#[test]
fn test_64_way_tlb_defer_insertion_time() {
    let mut counter = Builder::new().build().unwrap();

    let mut ts: u64 = 1;
    let mut vpn = 42;

    let mut tlb_set = FullyAssociativeTLB::new(64);

    // populate elements
    for _ in 0..64 {
        tlb_set.insert(vpn, NonGlobal(1), ts, vpn);
        ts += 1;
        vpn += 1;
    }

    counter.enable().unwrap();

    for _ in 0..2048 {
        for _ in 0..64 {
            tlb_set.deferred_insert(vpn, NonGlobal(1), ts, vpn * 12);
            ts += 1;
            vpn += 1;
            vpn %= 64;
        }
    }

    counter.disable().unwrap();

    let count = counter.read().unwrap();
    let ave_count = count / (64 * 2048);

    println!("FA TLB Insertion: {}", ave_count);
    assert!(ave_count < 80);
}
