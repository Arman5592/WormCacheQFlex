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
use spin::mutex::SpinMutexGuard;

use bitvec::prelude::*;

use crate::parameter;

pub mod finite;
pub mod infinite;

const SHARED_LIST_LENGTH: usize = if parameter::USE_UNIFIED_CACHE {
    parameter::CORE_COUNT
} else {
    parameter::CORE_COUNT * 2
};

pub type SharerList = BitArr!(for SHARED_LIST_LENGTH, in u64, Lsb0);

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DirectoryEntry {
    pub lru_ts: u64,
    pub sharers: SharerList,
    pub in_shared_cache: bool,
    pub shared: bool,
}

impl DirectoryEntry {
    #[inline]
    pub fn update_lru_ts(&mut self, ts: u64) {
        if ts > self.lru_ts {
            self.lru_ts = ts;
        }
    }
}

pub trait DirectorySet: Sized + Send + Sync + Clone {
    fn new(index: usize) -> Self;
    fn from(raw: HashMap<u64, DirectoryEntry>, index: usize) -> Self;
    fn get_or_create(
        &mut self,
        block_id: u64,
    ) -> (&mut DirectoryEntry, Option<(u64, DirectoryEntry)>);

    fn get(&mut self, block_id: u64) -> Option<&mut DirectoryEntry>;

    fn erase(&mut self, block_id: u64);
    fn run_gc(&mut self);

    fn raw(&self) -> HashMap<u64, DirectoryEntry>;
}

#[derive(Serialize, Deserialize)]
pub struct DirectorySerdeHelper<const SET: usize> {
    entries: Vec<HashMap<u64, DirectoryEntry>>,
}
pub trait Directory: Send + Sync {
    type TSet: DirectorySet;

    fn new() -> Self;

    fn fetch_one_entry(&self, block_id: u64) -> SpinMutexGuard<'_, Self::TSet>;

    fn fetch_two_entries(
        &self,
        block_id_0: u64,
        block_id_1: u64,
    ) -> (
        SpinMutexGuard<'_, Self::TSet>,
        Option<SpinMutexGuard<'_, Self::TSet>>,
    );

    fn run_gc(&self);

    fn serialize(&self, name: &str, numa_node_id: usize);

    fn deserialize(&mut self, name: &str, numa_node_id: usize);

    fn information() -> String;
}
