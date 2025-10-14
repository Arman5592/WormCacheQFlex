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

use std::cell::UnsafeCell;

use crate::parameter::CACHE_LINE_SIZE;

#[repr(align(64))]
struct PerCoreL0 {
    v_block_id: u64,
}

impl PerCoreL0 {
    pub fn new() -> Self {
        Self { v_block_id: 0 }
    }
}

impl Default for PerCoreL0 {
    fn default() -> Self {
        Self::new()
    }
}

pub struct L0InstructionCache<const CORE_COUNT: usize> {
    content: [UnsafeCell<PerCoreL0>; CORE_COUNT],
}

impl<const CORE_COUNT: usize> L0InstructionCache<CORE_COUNT> {
    pub fn new() -> Self {
        Self {
            content: std::array::from_fn(|_| UnsafeCell::new(PerCoreL0::new())),
        }
    }

    // return true if the vcache line is the same as the previous one.
    pub fn check_and_update(&self, core_id: u32, vaddr: u64) -> bool {
        let vcache_line = vaddr >> CACHE_LINE_SIZE.trailing_zeros();
        let core_l0 = unsafe { &mut *self.content[core_id as usize].get() };

        let res = core_l0.v_block_id == vcache_line;

        core_l0.v_block_id = vcache_line;

        res
    }
}

impl<const CORE_COUNT: usize> Default for L0InstructionCache<CORE_COUNT> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_l0_instruction_cache_check_and_update() {
        let l0_cache: L0InstructionCache<4> = L0InstructionCache::new();
        assert!(!l0_cache.check_and_update(0, 1 << CACHE_LINE_SIZE.trailing_zeros()));
        assert!(l0_cache.check_and_update(0, 1 << CACHE_LINE_SIZE.trailing_zeros()));
        assert!(!l0_cache.check_and_update(0, 2 << CACHE_LINE_SIZE.trailing_zeros()));
    }
}
