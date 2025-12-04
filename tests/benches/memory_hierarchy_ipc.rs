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

use worm_cache::components::cache_hierarchy::CacheBlockRequest;
use worm_cache::components::cache_hierarchy::MemoryHierarchy;
use worm_cache::components::cache_hierarchy::common::CacheAccessType;
use worm_cache::components::cache_hierarchy::common::InfiniteDirectory;
use worm_cache::components::cache_hierarchy::common::ParallelLRUSharedCache;
use worm_cache::components::cache_hierarchy::common::ParallelUnifiedPrivateCache;
use worm_cache::components::cache_hierarchy::common::statistics::ZeroSharedCacheSetStatistics;
use worm_cache::components::cache_hierarchy::hierarchy::ParallelMemoryHierarchy;
use worm_cache::components::cache_hierarchy::mmu::NoMMU;

use worm_cache::parameter;

use divan;

type MH = ParallelMemoryHierarchy<
    NoMMU,
    ParallelUnifiedPrivateCache<
        1,
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
    1,
>;

#[divan::bench]
fn test_hit_last() {
    let mh = MH::new();

    let set_idx = 1;
    for i in 0..parameter::UNIFIED_PRI_CACHE_ASSO {
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id: (i as u64) * (parameter::UNIFIED_PRI_CACHE_SET as u64) + set_idx,
                access_type: CacheAccessType::DataRead,
                is_os: false,
            },
            (i + 1) as u64,
        );
    }

    // start testing. Access the same set and the last way.
    let addr = (parameter::UNIFIED_PRI_CACHE_ASSO as u64 - 1)
        * (parameter::UNIFIED_PRI_CACHE_SET as u64)
        + set_idx;

    let mut ts = parameter::UNIFIED_PRI_CACHE_ASSO as u64;
    loop {
        mh.access_memory_pblock_id(
            &CacheBlockRequest {
                core_id: 0,
                block_id: addr,
                access_type: CacheAccessType::DataRead,
                is_os: false,
            },
            ts,
        );
        ts += 1;

        if ts > 1024 * 1024 {
            break;
        }
    }
}

#[divan::bench]
fn testing_pcache_always_miss() {
    let mh = MH::new();

    // What I need to do is just to access the block id belonging to a specific shared cache set.
    // The block id is calculated as follows:
    // block_id = set_id * associativity + way_id

    let mut ts: u64 = 1;
    let mut block_id = 42;

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

        if ts > 1024 * 1024 {
            break;
        }
    }
}

#[divan::bench]
fn testing_always_miss() {
    let mh = MH::new();

    // What I need to do is just to access the block id belonging to a specific shared cache set.
    // The block id is calculated as follows:
    // block_id = set_id * associativity + way_id

    let mut ts: u64 = 1;
    let mut block_id = 37;

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
            block_id += parameter::SHARED_CACHE_SET as u64;
        }
        block_id = 37;

        if ts > 1024 * 1024 {
            break;
        }
    }
}

pub fn main() {
    // let mut counter = Builder::new().build().unwrap();

    // counter.enable().unwrap();
    // testing_pcache_always_miss();
    // counter.disable().unwrap();

    // println!("Instructions: {}", counter.read().unwrap());

    divan::main();
}
