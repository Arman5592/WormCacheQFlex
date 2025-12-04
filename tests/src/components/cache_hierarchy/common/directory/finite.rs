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

use rustc_hash::FxHashMap as HashMap;
use serde::{Deserialize, Serialize};
use spin::mutex::{SpinMutex, SpinMutexGuard};
use zstd::{Decoder, Encoder};

#[cfg(test)]
mod finite_tests;

use crate::util;

use super::{Directory, DirectoryEntry, DirectorySet, SharerList};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[repr(align(64))]
pub struct FiniteDirectorySet<const SET: usize, const WAY: usize> {
    entries: HashMap<u64, DirectoryEntry>,
    pub index: usize,
}

impl<const SET: usize, const WAY: usize> DirectorySet for FiniteDirectorySet<SET, WAY> {
    fn new(index: usize) -> Self {
        Self {
            entries: HashMap::<u64, DirectoryEntry>::default(),
            index,
        }
    }

    fn from(raw: HashMap<u64, DirectoryEntry>, index: usize) -> Self {
        Self {
            entries: raw,
            index,
        }
    }

    fn get_or_create(
        &mut self,
        block_id: u64,
    ) -> (&mut DirectoryEntry, Option<(u64, DirectoryEntry)>) {
        // if the block is not found, create a new entry.
        if !self.entries.contains_key(&block_id) {
            // if the set is full, evict the LRU block.
            let evicted = if self.entries.len() == WAY {
                let lru_block = *self
                    .entries
                    .iter()
                    .min_by_key(|(_, entry)| entry.lru_ts)
                    .unwrap()
                    .0;

                self.entries
                    .remove(&lru_block)
                    .map(|entry| (lru_block, entry))
            } else {
                None
            };

            self.entries.insert(
                block_id,
                DirectoryEntry {
                    lru_ts: 0,
                    sharers: SharerList::ZERO,
                    in_shared_cache: false,
                    shared: false,
                },
            );

            return (self.entries.get_mut(&block_id).unwrap(), evicted);
        }

        (self.entries.get_mut(&block_id).unwrap(), None)
    }

    fn get(&mut self, block_id: u64) -> Option<&mut DirectoryEntry> {
        self.entries.get_mut(&block_id)
    }

    fn erase(&mut self, block_id: u64) {
        self.entries.remove(&block_id);
    }

    fn run_gc(&mut self) {
        // clean all entries that has zero sharers.
        self.entries.retain(|_, entry| entry.sharers.any());
    }

    fn raw(&self) -> HashMap<u64, DirectoryEntry> {
        self.entries.clone()
    }
}

pub struct FiniteDirectory<const SET: usize, const WAY: usize> {
    entries: Box<[SpinMutex<FiniteDirectorySet<SET, WAY>>; SET]>,
}

impl<const SET: usize, const WAY: usize> FiniteDirectory<SET, WAY> {
    fn to_serialize_helper(&self) -> super::DirectorySerdeHelper<SET> {
        let entries: Vec<HashMap<u64, DirectoryEntry>> = self
            .entries
            .iter()
            .map(|set| set.lock().clone().raw())
            .collect::<Vec<_>>();

        super::DirectorySerdeHelper { entries }
    }

    fn from_serialize_helper(helper: super::DirectorySerdeHelper<SET>) -> Self {
        let entries = helper
            .entries
            .into_iter()
            .enumerate()
            .map(|(idx, set)| {
                SpinMutex::new(<FiniteDirectorySet<SET, WAY> as DirectorySet>::from(
                    set, idx,
                ))
            })
            .collect::<Vec<_>>();

        Self {
            entries: entries.try_into().unwrap(),
        }
    }
}

impl<const SET: usize, const WAY: usize> Default for FiniteDirectory<SET, WAY> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const SET: usize, const WAY: usize> Directory for FiniteDirectory<SET, WAY> {
    type TSet = FiniteDirectorySet<SET, WAY>;

    fn new() -> Self {
        Self {
            entries: util::init_heap_array(|idx| SpinMutex::new(FiniteDirectorySet::new(idx))),
        }
    }

    fn fetch_one_entry(&self, block_id: u64) -> SpinMutexGuard<'_, Self::TSet> {
        let set_id = (block_id as usize) % SET;
        self.entries[set_id].lock()
    }

    fn fetch_two_entries(
        &self,
        block_id_0: u64,
        block_id_1: u64,
    ) -> (
        SpinMutexGuard<'_, Self::TSet>,
        Option<SpinMutexGuard<'_, Self::TSet>>,
    ) {
        let index_0 = (block_id_0 as usize) % SET;
        let index_1 = (block_id_1 as usize) % SET;

        match index_0.cmp(&index_1) {
            std::cmp::Ordering::Equal => (self.fetch_one_entry(block_id_0), None),
            std::cmp::Ordering::Less => {
                let g0 = self.fetch_one_entry(block_id_0);
                let g1 = self.fetch_one_entry(block_id_1);
                (g0, Some(g1))
            }
            std::cmp::Ordering::Greater => {
                let g1 = self.fetch_one_entry(block_id_1);
                let g0 = self.fetch_one_entry(block_id_0);
                (g0, Some(g1))
            }
        }
    }

    fn run_gc(&self) {
        // clean all entries that has zero sharers.
        for set in self.entries.iter() {
            let mut set = set.lock();
            set.run_gc();
        }
    }

    fn serialize(&self, name: &str, numa_node_id: usize) {
        self.run_gc();
        let file = std::fs::File::create(format!("{}/directory-{}.json.zstd", name, numa_node_id))
            .unwrap();

        let mut file = Encoder::new(file, 0).unwrap();

        let helper = self.to_serialize_helper();
        serde_json::to_writer(&mut file, &helper).unwrap();

        file.finish().unwrap();
    }

    fn deserialize(&mut self, name: &str, numa_node_id: usize) {
        let file = std::fs::File::open(format!("{}/directory-{}.json.zstd", name, numa_node_id));

        if file.is_err() {
            println!("Cannot load the directory state. Error: {:?}", file.err());
            return;
        }

        let file = file.unwrap();

        let file = Decoder::new(file).unwrap();

        let helper: super::DirectorySerdeHelper<SET> = serde_json::from_reader(file).unwrap();
        *self = Self::from_serialize_helper(helper);
    }

    fn information() -> String {
        format!("Finite Directory ({} sets, {} ways)", SET, WAY)
    }
}
