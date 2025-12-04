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

use std::{
    io::prelude::*,
    sync::atomic::{AtomicUsize, Ordering},
};

use super::{SharedCacheAccessRequest, SharedCacheAccessSource};

use super::{
    SharedCacheLookupAndInsertResult, SharedCacheLookupResult, SharedCacheSet,
    statistics::{SharedCacheSetStatistics, ZeroSharedCacheSetStatistics},
};
use serde::{Deserialize, Serialize};
use spin::mutex::SpinMutex;

use zstd::{Decoder, Encoder};

pub struct LRUSharedCache<
    S: SharedCacheSetStatistics,
    const SET: usize,
    const WAY: usize,
    const EXCLUSIVE: bool,
> {
    blocks: Box<[SpinMutex<SharedCacheSet<WAY, SET, EXCLUSIVE, S>>; SET]>,
    warmed_sets: AtomicUsize,
    _phantom: std::marker::PhantomData<S>,
}

#[derive(Serialize, Deserialize)]
pub struct SingleSharedCacheSerdeHelper<const SET: usize, const WAY: usize, const EXCLUSIVE: bool> {
    blocks: Vec<SharedCacheSet<WAY, SET, EXCLUSIVE, ZeroSharedCacheSetStatistics>>,
    warmed_sets: usize,
}

impl<S: SharedCacheSetStatistics, const SET: usize, const WAY: usize, const EXCLUSIVE: bool>
    LRUSharedCache<S, SET, WAY, EXCLUSIVE>
{
    pub fn from_serialize_helper(
        helper: SingleSharedCacheSerdeHelper<SET, WAY, EXCLUSIVE>,
    ) -> Self {
        let mut blocks = Vec::with_capacity(SET);
        for block in helper.blocks {
            blocks.push(SpinMutex::new(SharedCacheSet::from_without_statistics(
                block,
            )));
        }
        Self {
            blocks: blocks.into_boxed_slice().try_into().unwrap(),
            warmed_sets: AtomicUsize::new(helper.warmed_sets),
            _phantom: std::marker::PhantomData,
        }
    }

    pub fn to_serialize_helper(&self) -> SingleSharedCacheSerdeHelper<SET, WAY, EXCLUSIVE> {
        SingleSharedCacheSerdeHelper {
            blocks: self
                .blocks
                .iter()
                .map(|entry| entry.lock().without_statistics())
                .collect(),
            warmed_sets: self.warmed_sets.load(Ordering::Relaxed),
        }
    }
}

impl<S: SharedCacheSetStatistics, const SET: usize, const WAY: usize, const EXCLUSIVE: bool>
    super::SharedCache for LRUSharedCache<S, SET, WAY, EXCLUSIVE>
{
    fn new() -> Self {
        Self {
            blocks: crate::util::init_heap_array(|_| SpinMutex::new(SharedCacheSet::new())),
            warmed_sets: AtomicUsize::new(0),
            _phantom: std::marker::PhantomData,
        }
    }

    fn invalidate(
        &self,
        _source: SharedCacheAccessSource,
        block_id: u64,
        ts: u64,
    ) -> SharedCacheLookupResult {
        let set_idx = (block_id % SET as u64) as usize;
        self.blocks[set_idx].lock().invalidate(block_id, ts);
        return self.blocks[set_idx].lock().invalidate(block_id, ts);
    }

    fn peek(&self, r: &SharedCacheAccessRequest) -> bool {
        let set_idx = (r.block_id % SET as u64) as usize;
        self.blocks[set_idx].lock().index_of(r.block_id).is_some()
    }

    fn lookup(&self, r: &SharedCacheAccessRequest, ts: u64) -> SharedCacheLookupResult {
        let set_idx = (r.block_id % SET as u64) as usize;

        self.blocks[set_idx].lock().lookup(r, ts)
    }

    fn insert(
        &self,
        source: SharedCacheAccessSource,
        block_id: u64,
        ts: u64,
        is_modified: bool,
        increase_touched_count: bool,
    ) -> (bool, bool) {
        let set_idx = (block_id % SET as u64) as usize;
        let insertion_result = self.blocks[set_idx].lock().insert(
            block_id,
            source,
            ts,
            is_modified,
            increase_touched_count,
        );

        if insertion_result.0 {
            self.warmed_sets.fetch_add(1, Ordering::Relaxed);
        }

        return insertion_result;
    }

    fn lookup_and_insert_on_miss(
        &self,
        r: &SharedCacheAccessRequest,
        ts: u64,
        increase_touched_count: bool,
    ) -> SharedCacheLookupResult {
        let set_idx = (r.block_id % SET as u64) as usize;
        let result = self.blocks[set_idx]
            .lock()
            .lookup_and_insert(r, ts, increase_touched_count);

        match result {
            SharedCacheLookupAndInsertResult::Hit(modified) => {
                SharedCacheLookupResult::Hit(modified)
            }
            SharedCacheLookupAndInsertResult::Miss => SharedCacheLookupResult::Miss,
            SharedCacheLookupAndInsertResult::InsertedAndCold(just_warmed) => {
                if just_warmed {
                    self.warmed_sets.fetch_add(1, Ordering::Relaxed);
                }
                SharedCacheLookupResult::ColdMiss
            }
            SharedCacheLookupAndInsertResult::Inserted => SharedCacheLookupResult::Miss,
            SharedCacheLookupAndInsertResult::LookupLate(diff, is_modified) => {
                SharedCacheLookupResult::LookupLate(diff, is_modified)
            }
            SharedCacheLookupAndInsertResult::EvictedLate(diff) => {
                SharedCacheLookupResult::EvictedLate(diff)
            }
        }
    }

    fn warmed_sets_count(&self) -> usize {
        self.warmed_sets.load(Ordering::Relaxed)
    }

    fn warmed_slots_count(&self) -> usize {
        self.blocks
            .iter()
            .map(|entry| {
                let entry = entry.lock();
                entry.touched_count
            })
            .sum()
    }

    fn information() -> String {
        format!(
            "SharedCache: SET={}, WAY={}, EXCLUSIVE={}",
            SET, WAY, EXCLUSIVE
        )
    }

    fn dump_access_frequency(&self, file_name: &str) {
        let mut file = std::fs::File::create(file_name).unwrap();
        writeln!(file, "idx,{}\n", S::get_header()).unwrap();
        for (idx, entry) in self.blocks.iter().enumerate() {
            let entry = entry.lock();
            writeln!(file, "{},{}\n", idx, entry.statistics.render_line()).unwrap();
        }
    }

    fn serialize(&self, name: &str, numa_node_id: usize) {
        let helper = self.to_serialize_helper();
        let mut file =
            std::fs::File::create(format!("{}/llc-{}.json.zstd", name, numa_node_id)).unwrap();

        let mut file = Encoder::new(&mut file, 0).unwrap();
        serde_json::to_writer(&mut file, &helper).unwrap();

        file.finish().unwrap();
    }

    fn deserialize(&mut self, name: &str, numa_node_id: usize) {
        let file = std::fs::File::open(format!("{}/llc-{}.json.zstd", name, numa_node_id));

        if file.is_err() {
            println!("Cannot load the shared cache. Error: {:?}", file.err());
            return;
        }

        let file = file.unwrap();
        let file = Decoder::new(file).unwrap();

        let helper: SingleSharedCacheSerdeHelper<SET, WAY, EXCLUSIVE> =
            serde_json::from_reader(file).unwrap();
        *self = LRUSharedCache::from_serialize_helper(helper);
    }
}

pub type ParallelLRUSharedCache<S, const SET: usize, const WAY: usize, const EXCLUSIVE: bool> =
    LRUSharedCache<S, SET, WAY, EXCLUSIVE>;
