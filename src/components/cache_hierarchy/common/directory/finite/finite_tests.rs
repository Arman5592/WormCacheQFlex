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

use crate::components::cache_hierarchy::common::directory::{
    Directory, DirectorySet, finite::FiniteDirectory, finite::FiniteDirectorySet,
};

const SET: usize = 4;
const WAY: usize = 2;
type TestDirectory = FiniteDirectory<SET, WAY>;
type TestDirectorySet = FiniteDirectorySet<SET, WAY>;

#[test]
fn test_finite_directory_set_get_or_create_and_lookup() {
    let mut set = TestDirectorySet::new(0);
    let block_id = 100;

    // First access, create a new entry
    let (entry, evicted) = set.get_or_create(block_id);
    assert_eq!(entry.lru_ts, 0);
    assert!(!entry.sharers.any());
    assert!(evicted.is_none());

    // Update the entry
    entry.lru_ts = 123;
    entry.sharers.set(1, true);

    // Second access, should get the same entry
    let (entry, evicted) = set.get_or_create(block_id);
    assert_eq!(entry.lru_ts, 123);
    assert!(entry.sharers.get(1).unwrap());
    assert!(evicted.is_none());

    // Lookup with get()
    let entry = set.get(block_id).unwrap();
    assert_eq!(entry.lru_ts, 123);
}

#[test]
fn test_finite_directory_set_eviction() {
    let mut set = TestDirectorySet::new(0);
    let block_id_0 = 100;
    let block_id_1 = 200;
    let block_id_2 = 300;

    // Fill the set
    let (entry0, _) = set.get_or_create(block_id_0);
    entry0.lru_ts = 10;
    let (entry1, _) = set.get_or_create(block_id_1);
    entry1.lru_ts = 20;

    assert_eq!(set.entries.len(), WAY);

    // This should evict block_id_0 (LRU)
    let (entry2, evicted) = set.get_or_create(block_id_2);
    entry2.lru_ts = 30;

    assert!(evicted.is_some());
    let (evicted_id, _) = evicted.unwrap();
    assert_eq!(evicted_id, block_id_0);
    assert_eq!(set.entries.len(), WAY);
    assert!(set.get(block_id_0).is_none());
    assert!(set.get(block_id_1).is_some());
    assert!(set.get(block_id_2).is_some());
}

#[test]
fn test_finite_directory_lookup_and_allocation() {
    let dir = TestDirectory::new();
    let block_id = 123;

    // First access, should allocate
    let mut set_guard = dir.fetch_one_entry(block_id);
    let (entry, evicted) = set_guard.get_or_create(block_id);
    assert!(evicted.is_none());
    entry.lru_ts = 100;
    entry.sharers.set(0, true);

    drop(set_guard);

    // Second access, should find the entry
    let mut set_guard = dir.fetch_one_entry(block_id);
    let (entry, evicted) = set_guard.get_or_create(block_id);
    assert!(evicted.is_none());
    assert_eq!(entry.lru_ts, 100);
    assert!(entry.sharers.get(0).unwrap());
}

#[test]
fn test_finite_directory_eviction() {
    let dir = TestDirectory::new();
    let set_id = 0;
    let block_id_0 = set_id;
    let block_id_1 = set_id + SET as u64;
    let block_id_2 = set_id + 2 * SET as u64;

    // Fill the set
    let mut set_guard_0 = dir.fetch_one_entry(block_id_0);
    let (entry_0, _) = set_guard_0.get_or_create(block_id_0);
    entry_0.lru_ts = 10;
    drop(set_guard_0);

    let mut set_guard_1 = dir.fetch_one_entry(block_id_1);
    let (entry_1, _) = set_guard_1.get_or_create(block_id_1);
    entry_1.lru_ts = 20;
    drop(set_guard_1);

    // This access should trigger an eviction of block_id_0
    let mut set_guard_2 = dir.fetch_one_entry(block_id_2);
    let (_, evicted) = set_guard_2.get_or_create(block_id_2);

    assert!(evicted.is_some());
    let (evicted_id, _) = evicted.unwrap();
    assert_eq!(evicted_id, block_id_0);
}
