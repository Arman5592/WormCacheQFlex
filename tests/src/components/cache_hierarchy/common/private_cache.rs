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

use std::collections::HashMap;
use std::ops::DerefMut;

mod havard;
mod set_and_line;
mod unified;

pub use set_and_line::{
    PrivateCacheEvictedSlot, PrivateCacheLine, PrivateCachePokeResult, PrivateCacheSet,
};

pub trait PrivateCache {
    // This function is for creating all new private caches.
    fn new() -> Self;

    // This function is for checking the whether the private cache hits or miss.
    fn poke_and_update(&self, request: &CacheBlockRequest, ts: u64) -> PrivateCachePokeResult;

    // This function is for filling the cache line from the shared LLC.
    fn get_set_for_fill(
        &self,
        request: &CacheBlockRequest,
    ) -> impl DerefMut<Target = PrivateCacheSet>;

    // This function is for coherence messages and refill.
    // The rest of the function of coherence logic is handled outside.
    fn get_set_guard_by_sharer_list(
        &self,
        block_id: u64,
        sharers: SharerList,
    ) -> Vec<(
        usize,
        impl DerefMut<Target = PrivateCacheSet>,
        Option<usize>,
    )>; // (sharer_index, guard, index)

    // This function is for debugging. It gets the ids of all cores that have the cache line.
    fn in_which_cores(&self, block_id: u64) -> Vec<u32>;

    // This function is for debugging. It gets the state of the cache line in all cores.
    fn query_replica_state(&self, block_id: u64) -> HashMap<u32, bool>; // (core_id, is_modified)

    // These functions are for maintaining the shared list in the directory.
    // For Harvard architecture, each core takes two bits.
    // For unified architecture, each core takes one bit.
    fn find_cache_info_by_cache_id(id: usize) -> (u32, bool); // (core_id, is_instruction_cache)
    fn get_cache_id_by_cache_info(core_id: u32, is_instruction_cache: bool) -> usize;

    fn information() -> String;

    // This function is for printing diagnose information. It is used for debugging.
    fn print_debug_info(&self);

    const DIRECTORY_SET: usize;

    fn serialize(&self, name: &str, numa_node_id: usize);
    fn deserialize(&mut self, name: &str, numa_node_id: usize); // this is in-place deserialization.
}

pub use havard::HarvardPerCorePrivateCacheSerdeHelper;
pub use havard::ParallelHarvardPrivateCache;

pub use unified::ParallelUnifiedPrivateCache;
pub use unified::UnifiedPerCorePrivateCacheSerdeHelper;

use crate::components::cache_hierarchy::CacheBlockRequest;

use super::directory::SharerList;
