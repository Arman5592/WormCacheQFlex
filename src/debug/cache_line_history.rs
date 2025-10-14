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

use dashmap::DashMap;
use dashmap::mapref::one::Ref;
use std::sync::LazyLock;

use crate::components::cache_hierarchy::common::SharerList;

#[derive(Debug)]
pub enum CacheOperationType {
    GetM,
    GetR,
    Drop,
    Invalidate(usize),
}

#[derive(Debug)]
pub struct SingleCacheLineCoherenceHistory {
    history: Vec<(CacheOperationType, usize, u64, bool, SharerList, u32)>, // operation, cache_id, timestamp, is_refilled, sharers, line number
}

impl Default for SingleCacheLineCoherenceHistory {
    fn default() -> Self {
        Self::new()
    }
}

impl SingleCacheLineCoherenceHistory {
    pub fn new() -> Self {
        Self {
            history: Vec::new(),
        }
    }

    pub fn record(
        &mut self,
        operation: CacheOperationType,
        cache_id: usize,
        timestamp: u64,
        refilled: bool,
        sharers: SharerList,
        line_number: u32,
    ) {
        self.history.push((
            operation,
            cache_id,
            timestamp,
            refilled,
            sharers,
            line_number,
        ));
    }

    pub fn print_history(&self) {
        for (operation, core_id, timestamp, refilled, share_list, line_number) in &self.history {
            println!(
                "Operation: {:?}, Cache ID: {}, Timestamp: {}, Refilled: {}, Share List: {:?}, line: {}",
                operation,
                core_id,
                timestamp,
                refilled,
                share_list.iter_ones().collect::<Vec<usize>>(),
                line_number
            );
        }
    }

    pub fn print_last_n_history(&self, n: usize) {
        // for (operation, core_id, timestamp) in self.history.iter().rev().take(n) {
        //     println!(
        //         "Operation: {:?}, Core ID: {}, Timestamp: {}",
        //         operation, core_id, timestamp
        //     );
        // }

        if self.history.len() < n {
            self.print_history();
        } else {
            let middle = self.history.len() - n;

            for i in middle..self.history.len() {
                let (operation, core_id, timestamp, refilled, share_list, line_number) =
                    &self.history[i];
                println!(
                    "Operation: {:?}, Cache ID: {}, Timestamp: {}, Refilled: {}, Share List: {:?}, line: {}",
                    operation,
                    core_id,
                    timestamp,
                    refilled,
                    share_list.iter_ones().collect::<Vec<usize>>(),
                    line_number
                );
            }
        }
    }
}

pub struct CacheLineCoherenceHistory {
    history: DashMap<u64, SingleCacheLineCoherenceHistory>,
}

static GLOBAL_HISTORY: LazyLock<CacheLineCoherenceHistory> =
    LazyLock::new(CacheLineCoherenceHistory::new);

impl Default for CacheLineCoherenceHistory {
    fn default() -> Self {
        Self::new()
    }
}

impl CacheLineCoherenceHistory {
    pub fn new() -> Self {
        Self {
            history: DashMap::new(),
        }
    }

    fn record(
        &self,
        block_id: u64,
        operation: CacheOperationType,
        cache_id: usize,
        timestamp: u64,
        refilled: bool,
        sharers: SharerList,
        line_number: u32,
    ) {
        let mut history = self.history.entry(block_id).or_default();
        history.record(
            operation,
            cache_id,
            timestamp,
            refilled,
            sharers,
            line_number,
        );
    }

    #[inline]
    pub fn global_record_history(
        block_id: u64,
        operation: CacheOperationType,
        cache_id: usize,
        timestamp: u64,
        refilled: bool,
        sharers: SharerList,
        line: u32,
    ) {
        if !crate::parameter::ENABLE_CACHE_LINE_HISTORY {
            // I believe the compiler will optimize this function out.
            return;
        }
        GLOBAL_HISTORY.record(
            block_id, operation, cache_id, timestamp, refilled, sharers, line,
        );
    }

    #[inline]
    pub fn global_get_block_history(
        block_id: u64,
    ) -> Option<Ref<'static, u64, SingleCacheLineCoherenceHistory>> {
        GLOBAL_HISTORY.history.get(&block_id)
    }
}
