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

use std::collections::HashSet;

pub struct TouchedCacheSet {
    set: HashSet<usize>, // The block ID is in the cache.
    capacity: usize,     // Associativity
    fully_touched: bool, // All blocks are touched.
}

impl TouchedCacheSet {
    pub fn new(capacity: usize) -> Self {
        Self {
            set: HashSet::new(),
            capacity,
            fully_touched: false,
        }
    }

    // Returns true if the set is fully touched by the current touch.
    pub fn touch(&mut self, block_id: usize) -> bool {
        if self.fully_touched {
            return false;
        }
        let previous_size = self.set.len();
        self.set.insert(block_id);
        if previous_size == (self.capacity - 1) && self.set.len() == self.capacity {
            self.fully_touched = true;
            return true;
        }
        false
    }
}

pub struct TouchedCache {
    sets: Vec<TouchedCacheSet>,
    fully_touched_sets: usize,
}

impl TouchedCache {
    pub fn new(sets: usize, associativity: usize) -> Self {
        Self {
            sets: Vec::from_iter((0..sets).map(|_| TouchedCacheSet::new(associativity))),
            fully_touched_sets: 0,
        }
    }

    fn touch(&mut self, set_id: usize, block_id: usize) -> bool {
        self.sets[set_id].touch(block_id)
    }

    pub fn access(&mut self, pa: usize) -> bool {
        let block_id = pa >> (crate::parameter::CACHE_LINE_SIZE.trailing_zeros());
        let set_index = block_id & (self.sets.len() - 1);
        let res = self.touch(set_index, block_id);
        if res {
            self.fully_touched_sets += 1;
        }
        res
    }

    pub fn is_fully_touched(&self) -> bool {
        self.fully_touched_sets == self.sets.len()
    }

    pub fn get_fully_touched_set_count(&self) -> usize {
        self.fully_touched_sets
    }

    pub fn reset(&mut self) {
        self.sets.iter_mut().for_each(|set| {
            set.fully_touched = false;
            set.set.clear();
        });
        self.fully_touched_sets = 0;
    }
}
