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

// Return value: Map[first_index, size]
pub fn find_fetch_block_from_block_id_sequence(i: Vec<usize>) -> HashMap<usize, usize> {
    let mut res = HashMap::new();
    let mut last_fb_block_id: Option<usize> = None;
    let mut last_fb_first_instruction_index: usize = 0;
    let total = i.len();

    // traverse the array and find the inconsistent position.
    for (idx, block_id) in i.into_iter().enumerate() {
        match last_fb_block_id {
            Some(last_fb_id) => {
                if block_id != last_fb_id {
                    // We start a new block.
                    // First, keep the old block.
                    let last_fb_size = idx - last_fb_first_instruction_index;
                    res.insert(last_fb_first_instruction_index, last_fb_size);
                    // Then, adjust the information of the last block
                    last_fb_first_instruction_index = idx;
                    last_fb_block_id = Some(block_id);
                }
            }
            None => {
                last_fb_block_id = Some(block_id);
            }
        }
    }

    // insert the last element as well.
    res.insert(
        last_fb_first_instruction_index,
        total - last_fb_first_instruction_index,
    );

    res
}

pub fn init_heap_array<T: Sized + std::fmt::Debug, const N: usize>(
    f: impl Fn(usize) -> T,
) -> Box<[T; N]> {
    let res = Vec::from_iter((0..N).map(f));
    res.into_boxed_slice().try_into().unwrap()
}

use libc::{CLOCK_MONOTONIC_RAW, clock_gettime, timespec};

pub fn get_monotonic_ts() -> u64 {
    let mut ts = timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    unsafe {
        assert!(clock_gettime(CLOCK_MONOTONIC_RAW, &mut ts) == 0);
    }
    ts.tv_sec as u64 * 1_000_000_000 + ts.tv_nsec as u64
}

#[test]
fn test_get_monotonic_ts() {
    let ts = (0..100).map(|_| get_monotonic_ts()).collect::<Vec<u64>>();
    for i in 0..ts.len() - 1 {
        assert!(ts[i] < ts[i + 1]);
    }
}

#[test]
fn test_find_fetch_block_from_pa_sequence() {
    let example = vec![0, 0, 1, 1, 2, 2, 3, 3, 3];
    let res = find_fetch_block_from_block_id_sequence(example);
    assert_eq!(res.len(), 4);
    assert_eq!(res.get(&0), Some(&2));
    assert_eq!(res.get(&2), Some(&2));
    assert_eq!(res.get(&4), Some(&2));
    assert_eq!(res.get(&6), Some(&3));

    let example = vec![0];
    let res = find_fetch_block_from_block_id_sequence(example);
    assert_eq!(res.len(), 1);
    assert_eq!(res.get(&0), Some(&1));

    let example = vec![1];
    let res = find_fetch_block_from_block_id_sequence(example);
    assert_eq!(res.len(), 1);
    assert_eq!(res.get(&0), Some(&1));

    let example = vec![1, 2, 2, 10, 10];
    let res = find_fetch_block_from_block_id_sequence(example);
    assert_eq!(res.len(), 3);
    assert_eq!(res.get(&0), Some(&1));
    assert_eq!(res.get(&1), Some(&2));
    assert_eq!(res.get(&3), Some(&2));
}
