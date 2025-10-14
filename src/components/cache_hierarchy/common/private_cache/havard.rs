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

use serde::{Deserialize, Serialize};

use crate::components::cache_hierarchy::CacheBlockRequest;

use super::PrivateCache;
use super::{PrivateCachePokeResult, PrivateCacheSet};
use spin::mutex::SpinMutex;

use std::collections::HashMap;
use std::ops::DerefMut;
use zstd::{Decoder, Encoder};

#[repr(align(64))]
#[derive(Debug)]
pub struct HarvardPerCorePrivateCache<
    const I_SET: usize,
    const I_ASSO: usize,
    const D_SET: usize,
    const D_ASSO: usize,
> {
    i_cache: Box<[SpinMutex<PrivateCacheSet>; I_SET]>,
    d_cache: Box<[SpinMutex<PrivateCacheSet>; D_SET]>,
}

#[derive(Serialize, Deserialize)]
pub struct HarvardPerCorePrivateCacheSerdeHelper {
    pub i_cache: Vec<PrivateCacheSet>,
    pub d_cache: Vec<PrivateCacheSet>,
}

impl<const I_SET: usize, const I_ASSO: usize, const D_SET: usize, const D_ASSO: usize>
    HarvardPerCorePrivateCache<I_SET, I_ASSO, D_SET, D_ASSO>
{
    pub fn new() -> Self {
        Self {
            i_cache: crate::util::init_heap_array(|_| SpinMutex::new(PrivateCacheSet::new(I_ASSO))),
            d_cache: crate::util::init_heap_array(|_| SpinMutex::new(PrivateCacheSet::new(D_ASSO))),
        }
    }

    fn from_serialize_helper(helper: HarvardPerCorePrivateCacheSerdeHelper) -> Self {
        let mut i_cache = Vec::with_capacity(I_SET);
        for set in helper.i_cache {
            i_cache.push(SpinMutex::new(set));
        }

        let mut d_cache = Vec::with_capacity(D_SET);
        for set in helper.d_cache {
            d_cache.push(SpinMutex::new(set));
        }

        Self {
            i_cache: i_cache.into_boxed_slice().try_into().unwrap(),
            d_cache: d_cache.into_boxed_slice().try_into().unwrap(),
        }
    }

    fn to_serialize_helper(&self) -> HarvardPerCorePrivateCacheSerdeHelper {
        HarvardPerCorePrivateCacheSerdeHelper {
            i_cache: self
                .i_cache
                .iter()
                .map(|entry| entry.lock().clone())
                .collect(),
            d_cache: self
                .d_cache
                .iter()
                .map(|entry| entry.lock().clone())
                .collect(),
        }
    }
}

pub struct HarvardPrivateCaches<
    const CORE_COUNT: usize,
    const I_SET: usize,
    const I_ASSO: usize,
    const D_SET: usize,
    const D_ASSO: usize,
> {
    caches: Box<[HarvardPerCorePrivateCache<I_SET, I_ASSO, D_SET, D_ASSO>; CORE_COUNT]>,
}

impl<
    const CORE_COUNT: usize,
    const I_SET: usize,
    const I_ASSO: usize,
    const D_SET: usize,
    const D_ASSO: usize,
> PrivateCache for HarvardPrivateCaches<CORE_COUNT, I_SET, I_ASSO, D_SET, D_ASSO>
{
    const DIRECTORY_SET: usize = gcd::binary_usize(I_SET, D_SET);

    fn new() -> Self {
        Self {
            caches: crate::util::init_heap_array(|_| HarvardPerCorePrivateCache::new()),
        }
    }

    #[inline]
    fn poke_and_update(&self, request: &CacheBlockRequest, ts: u64) -> PrivateCachePokeResult {
        let is_instruction = request.is_instruction();
        let core_id = request.core_id;
        let block_id = request.block_id;
        let is_store = request.is_store();
        if is_instruction {
            self.caches[core_id as usize].i_cache[block_id as usize % I_SET]
                .lock()
                .poke_and_update(block_id, ts, is_store, is_instruction)
        } else {
            self.caches[core_id as usize].d_cache[block_id as usize % D_SET]
                .lock()
                .poke_and_update(block_id, ts, is_store, is_instruction)
        }
    }

    #[inline]
    fn get_set_guard_by_sharer_list(
        &self,
        block_id: u64,
        sharers: super::super::SharerList,
    ) -> Vec<(
        usize,
        impl DerefMut<Target = PrivateCacheSet>,
        Option<usize>,
    )> {
        let mut res = Vec::new();

        for sharer_index in sharers.iter_ones() {
            let core_id = sharer_index / 2;
            let is_instruction = sharer_index % 2 == 0;

            if is_instruction {
                let set = &self.caches[core_id].i_cache[block_id as usize % I_SET];
                let guard = set.lock();
                let index = guard.index_of(block_id);
                res.push((sharer_index, guard, index));
            } else {
                let set = &self.caches[core_id].d_cache[block_id as usize % D_SET];
                let guard = set.lock();
                let index = guard.index_of(block_id);
                res.push((sharer_index, guard, index));
            }
        }

        res
    }

    #[inline]
    fn in_which_cores(&self, block_id: u64) -> Vec<u32> {
        let mut res: Vec<u32> = Vec::new();

        for core_id in 0..CORE_COUNT {
            if self.caches[core_id].i_cache[block_id as usize % I_SET]
                .lock()
                .poke(block_id)
                .is_some()
                || self.caches[core_id].d_cache[block_id as usize % D_SET]
                    .lock()
                    .poke(block_id)
                    .is_some()
            {
                res.push(core_id as u32);
            }
        }

        res
    }

    #[inline]
    fn query_replica_state(&self, block_id: u64) -> HashMap<u32, bool> {
        let mut res = HashMap::new();

        // If the i cache has the block, it is considered as shared. Just double check the d cache and make sure it is shared if exists.

        for core_id in 0..CORE_COUNT {
            let is_i = self.caches[core_id].i_cache[block_id as usize % I_SET]
                .lock()
                .poke(block_id);

            let is_d = self.caches[core_id].d_cache[block_id as usize % D_SET]
                .lock()
                .poke(block_id);

            if let Some(i_line) = is_i {
                assert!(i_line.block_id() == block_id);
                assert!(i_line.is_instruction());
                assert!(!i_line.is_modified());

                // alright, we check the data cache and make sure there is no modified copy.
                if let Some(d_line) = is_d {
                    assert!(d_line.block_id() == block_id);
                    assert!(!d_line.is_instruction());
                    assert!(!d_line.is_modified());
                }

                res.insert(core_id as u32, false);
            } else if let Some(d_line) = is_d {
                assert!(d_line.block_id() == block_id);
                assert!(!d_line.is_instruction());
                res.insert(core_id as u32, d_line.is_modified());

                continue;
            }
        }

        res
    }
    #[inline]
    fn find_cache_info_by_cache_id(index: usize) -> (u32, bool) {
        let core_id = index / 2;
        let is_instruction_cache = index % 2 == 0;
        (core_id as u32, is_instruction_cache)
    }
    #[inline]
    fn get_cache_id_by_cache_info(core_id: u32, is_instruction_cache: bool) -> usize {
        core_id as usize * 2 + if is_instruction_cache { 0 } else { 1 }
    }

    fn information() -> String {
        format!(
            "Type: HarvardPrivateCache, Core Count: {}, ICache Set: {}, ICache Associativity: {}, DCache Set: {}, DCache Associativity: {}",
            CORE_COUNT, I_SET, I_ASSO, D_SET, D_ASSO,
        )
    }

    fn get_set_for_fill(
        &self,
        request: &CacheBlockRequest,
    ) -> impl DerefMut<Target = PrivateCacheSet> {
        let core_id = request.core_id;
        let block_id = request.block_id;
        let is_instruction = request.is_instruction();

        if is_instruction {
            self.caches[core_id as usize].i_cache[block_id as usize % I_SET].lock()
        } else {
            self.caches[core_id as usize].d_cache[block_id as usize % D_SET].lock()
        }
    }

    #[inline]
    fn print_debug_info(&self) {}

    fn serialize(&self, name: &str, numa_node_id: usize) {
        let helper = self
            .caches
            .iter()
            .map(|cache| cache.to_serialize_helper())
            .collect::<Vec<_>>();

        let file =
            std::fs::File::create(format!("{}/{}-{}.json.zstd", name, "harvard", numa_node_id))
                .unwrap();

        let mut file = Encoder::new(file, 0).unwrap();

        serde_json::to_writer(&mut file, &helper).unwrap();

        file.finish().unwrap();
    }
    fn deserialize(&mut self, name: &str, numa_node_id: usize) {
        let file =
            std::fs::File::open(format!("{}/{}-{}.json.zstd", name, "harvard", numa_node_id));

        if file.is_err() {
            println!(
                "Cannot load the harvard private cache state. Error: {:?}",
                file.err()
            );
            return;
        }

        let file = file.unwrap();
        let file = Decoder::new(file).unwrap();

        let helper: Vec<HarvardPerCorePrivateCacheSerdeHelper> =
            serde_json::from_reader(file).unwrap();

        for (cache, helper) in self.caches.iter_mut().zip(helper.into_iter()) {
            *cache = HarvardPerCorePrivateCache::from_serialize_helper(helper);
        }
    }
}

pub type ParallelHarvardPrivateCache<
    const CORE_COUNT: usize,
    const I_SET: usize,
    const I_ASSO: usize,
    const D_SET: usize,
    const D_ASSO: usize,
> = HarvardPrivateCaches<CORE_COUNT, I_SET, I_ASSO, D_SET, D_ASSO>;
