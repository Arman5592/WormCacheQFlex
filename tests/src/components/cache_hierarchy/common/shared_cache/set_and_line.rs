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

use core::panic;

use serde::{Deserialize, Serialize};
use serde_with::serde_as;

use crate::debug::cache_line_history::CacheLineCoherenceHistory;

use super::{
    SharedCacheAccessRequest, SharedCacheAccessSource, SharedCacheLookupAndInsertResult,
    SharedCacheLookupResult,
    statistics::{SharedCacheSetStatistics, ZeroSharedCacheSetStatistics},
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SharedCacheBlock {
    pub block_id_with_v: u64, // the last bit is the valid bit.
    pub ts: u64,
    pub modified: bool,
    pub last_accessor: SharedCacheAccessSource, // the last accessor of this cache line.
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SharedCacheSet<
    const WAY: usize,
    const SET: usize,
    const EXCLUSIVE: bool,
    S: SharedCacheSetStatistics,
> {
    #[serde_as(as = "[_; WAY]")]
    pub blocks: [SharedCacheBlock; WAY],
    pub touched_count: usize,
    pub recent_evict_ts: u64, // if a cache access has a timestamp less than this one, its result might be unknown if there is a hit.

    pub access_count: u64,

    pub statistics: S,
}

impl<const WAY: usize, const SET: usize, const EXCLUSIVE: bool, S: SharedCacheSetStatistics> Default
    for SharedCacheSet<WAY, SET, EXCLUSIVE, S>
{
    fn default() -> Self {
        Self::new()
    }
}

impl<const WAY: usize, const SET: usize, const EXCLUSIVE: bool, S: SharedCacheSetStatistics>
    SharedCacheSet<WAY, SET, EXCLUSIVE, S>
{
    pub fn new() -> Self {
        Self {
            blocks: std::array::from_fn(|_| SharedCacheBlock {
                block_id_with_v: 0,
                ts: 0,
                modified: false,
                last_accessor: SharedCacheAccessSource::Core(0),
            }),

            touched_count: 0,
            recent_evict_ts: 0,

            access_count: 0,

            // modifying_history: vec![],
            statistics: S::default(),
        }
    }

    pub fn from_without_statistics(
        other: SharedCacheSet<WAY, SET, EXCLUSIVE, ZeroSharedCacheSetStatistics>,
    ) -> Self {
        Self {
            blocks: other.blocks,
            touched_count: other.touched_count,
            recent_evict_ts: other.recent_evict_ts,
            access_count: other.access_count,
            // modifying_history: vec![],
            statistics: S::default(),
        }
    }

    pub fn without_statistics(
        &self,
    ) -> SharedCacheSet<WAY, SET, EXCLUSIVE, ZeroSharedCacheSetStatistics> {
        SharedCacheSet {
            blocks: self.blocks.clone(),
            touched_count: self.touched_count,
            recent_evict_ts: self.recent_evict_ts,
            access_count: self.access_count,
            // modifying_history: vec![],
            statistics: ZeroSharedCacheSetStatistics {},
        }
    }

    // Return whether the block is a hit.
    #[inline]
    pub fn index_of(&self, block_id: u64) -> Option<usize> {
        let internal_block_id = block_id << 1 | 1;
        self.blocks
            .iter()
            .position(|p| p.block_id_with_v == internal_block_id)
    }

    pub fn invalidate(&mut self, block_id: u64, ts: u64) -> SharedCacheLookupResult {
        // if it is a hit, we remove this block from the cache
        if let Some(hit_block) = self.index_of(block_id) {
            if self.recent_evict_ts < ts {
                self.recent_evict_ts = ts;
            }

            let hit_block = &mut self.blocks[hit_block];
            let hit_ts = hit_block.ts;
            hit_block.block_id_with_v = 0;
            hit_block.ts = 0;

            return if hit_ts <= ts {
                SharedCacheLookupResult::Hit(hit_block.modified)
            } else {
                SharedCacheLookupResult::LookupLate(hit_ts as u32 - ts as u32, hit_block.modified)
            };
        }

        // otherwise, it is a miss.
        if self.touched_count < WAY {
            SharedCacheLookupResult::ColdMiss
        } else {
            SharedCacheLookupResult::Miss
        }
    }

    #[inline]
    pub fn lookup(&mut self, r: &SharedCacheAccessRequest, ts: u64) -> SharedCacheLookupResult {
        self.access_count += 1;

        let block_id = r.block_id;
        let access_type = r.access_type.clone();
        let is_os = r.is_os;

        if let Some(hit_block) = self.index_of(block_id) {
            let hit_block = &mut self.blocks[hit_block];
            let hit_ts = hit_block.ts;
            if r.is_store() || EXCLUSIVE {
                // In any case, the invaildation should be recorded.
                if self.recent_evict_ts < ts {
                    self.recent_evict_ts = ts;
                }

                hit_block.block_id_with_v = 0;
                hit_block.ts = 0;

                return if hit_ts > ts {
                    SharedCacheLookupResult::LookupLate(
                        hit_ts as u32 - ts as u32,
                        hit_block.modified,
                    )
                } else {
                    SharedCacheLookupResult::Hit(hit_block.modified)
                };
            } else {
                if hit_ts > ts {
                    return SharedCacheLookupResult::LookupLate(
                        hit_ts as u32 - ts as u32,
                        hit_block.modified,
                    );
                }

                // the equal case is only about page walk, which enables touching multiple cache lines with the same timestamp.
                hit_block.ts = ts;

                hit_block.last_accessor = r.source;

                // Accessed by a higher-level, meaning that the block is not dirty anymore.
                let was_modified = hit_block.modified;
                hit_block.modified = false;

                // self.modifying_history
                //     .push((block_id, ts, core_id, false, true));

                self.statistics.record(access_type, is_os, true);
                return SharedCacheLookupResult::Hit(was_modified);
            }
        }

        self.statistics.record(access_type, is_os, false);

        if self.touched_count < WAY {
            SharedCacheLookupResult::ColdMiss
        } else {
            SharedCacheLookupResult::Miss
        }
    }

    // return whether this cache set is just warmed.
    #[inline]
    pub fn insert(
        &mut self,
        block_id: u64,
        source: SharedCacheAccessSource,
        ts: u64,
        is_modified: bool,
        increase_touched_count: bool,
    ) -> (bool, bool) // (was_just_warmed, causality violation?)
    {
        assert!(ts != 0); // ts should not be 0. 0 is reserved for invalid blocks.

        if let Some(hit_block) = self.index_of(block_id) {
            let hit_block = &mut self.blocks[hit_block];
            if EXCLUSIVE {
                // this should not happen for exclusive caches
                CacheLineCoherenceHistory::global_get_block_history(block_id)
                    .unwrap()
                    .print_history();
                panic!("Error: the incoming block is already in the shared cache.");
            } else {
                // update the timestamp and the modified bit.
                if ts >= hit_block.ts {
                    hit_block.ts = ts;
                    if is_modified {
                        // This cache line's permission should have been taken by the private cache.
                        // So it should not be modified in the shared cache.
                        if hit_block.modified {
                            panic!();
                        }
                        hit_block.modified = is_modified;
                    }
                    hit_block.last_accessor = source;
                }
                return (false, false);
            }
        }

        let result = if increase_touched_count && self.touched_count < WAY {
            self.touched_count += 1;
            self.touched_count == WAY
        } else {
            false
        };

        let block_id_with_v = block_id << 1 | 1;

        // TODO: This part can be accelerated using SIMD instructions.
        let mut minimal_ts = u64::MAX;
        let mut minimal_index = 0;

        for (index, line) in self.blocks.iter_mut().enumerate() {
            if line.ts < minimal_ts {
                minimal_ts = line.ts;
                minimal_index = index;
            }
        }

        let oldest_block = &mut self.blocks[minimal_index];
        let causality_violation = oldest_block.ts > ts;

        // otherwise, we replace the oldest block.
        oldest_block.block_id_with_v = block_id_with_v;
        oldest_block.modified = is_modified;
        oldest_block.ts = ts;
        oldest_block.last_accessor = source;

        // update the eviction counter.
        if self.recent_evict_ts < ts {
            self.recent_evict_ts = ts;
        }

        // push the evicted line to the history.
        (result, causality_violation)
    }

    #[inline]
    pub fn lookup_and_insert(
        &mut self,
        r: &SharedCacheAccessRequest,
        ts: u64,
        increase_touched_count: bool,
    ) -> SharedCacheLookupAndInsertResult {
        self.access_count += 1;

        let block_id = r.block_id;
        let block_id_with_v = block_id << 1 | 1;
        let access_type = r.access_type.clone();
        let is_os = r.is_os;

        let mut hit_block = None;
        let mut eviction_ts: u64 = 0;
        let mut evicted_index: usize = 0;

        for (index, line) in self.blocks.iter_mut().enumerate() {
            if line.block_id_with_v == block_id_with_v {
                hit_block = Some(index);
                break;
            }

            if line.ts <= eviction_ts {
                eviction_ts = line.ts;
                evicted_index = index;
            }
        }

        if let Some(hit_block) = hit_block {
            let hit_block = &mut self.blocks[hit_block];
            let hit_ts = hit_block.ts;
            if hit_ts > ts {
                // Reversed access order. We cannot do anything but return unknown.

                // self.modifying_history
                //     .push((block_id, ts, core_id, false, false));

                return SharedCacheLookupAndInsertResult::LookupLate(
                    hit_ts as u32 - ts as u32,
                    hit_block.modified,
                );
            } else {
                if r.is_store() || EXCLUSIVE {
                    // In any case, the invaildation should be recorded.
                    if self.recent_evict_ts < ts {
                        self.recent_evict_ts = ts;
                    }

                    hit_block.block_id_with_v = 0;
                    hit_block.ts = 0;

                    return if hit_ts > ts {
                        SharedCacheLookupAndInsertResult::LookupLate(
                            hit_ts as u32 - ts as u32,
                            hit_block.modified,
                        )
                    } else {
                        SharedCacheLookupAndInsertResult::Hit(hit_block.modified)
                    };
                } else {
                    // the equal case is only about page walk, which enables touching multiple cache lines with the same timestamp.
                    hit_block.ts = ts;

                    hit_block.last_accessor = r.source;

                    // Accessed by a higher-level, meaning that the block is not dirty anymore.
                    let was_modified = hit_block.modified;
                    hit_block.modified = false;

                    // self.modifying_history
                    //     .push((block_id, ts, core_id, false, true));

                    self.statistics.record(access_type, is_os, true);
                    return SharedCacheLookupAndInsertResult::Hit(was_modified);
                }
            }
        }

        self.statistics.record(access_type, is_os, false);

        // we need to insert the block.
        if self.recent_evict_ts < ts {
            self.recent_evict_ts = ts;
        }

        let mut just_warmed = false;

        let index_to_replace = if self.touched_count < WAY {
            self.touched_count
        } else {
            evicted_index
        };

        if increase_touched_count && self.touched_count < WAY {
            self.touched_count += 1;
            if self.touched_count == WAY {
                just_warmed = true;
            }
        }

        let causality_violation = self.blocks[index_to_replace].ts > ts;
        let ts_diff = if causality_violation {
            self.blocks[index_to_replace].ts as u32 - ts as u32
        } else {
            0
        };

        let oldest_block = &mut self.blocks[index_to_replace];
        oldest_block.block_id_with_v = block_id_with_v;
        oldest_block.modified = r.is_store();
        oldest_block.ts = ts;
        oldest_block.last_accessor = r.source;

        if self.touched_count < WAY || just_warmed {
            SharedCacheLookupAndInsertResult::InsertedAndCold(just_warmed)
        } else {
            if !causality_violation {
                SharedCacheLookupAndInsertResult::Inserted
            } else {
                SharedCacheLookupAndInsertResult::EvictedLate(ts_diff)
            }
        }
    }
}

#[test]
fn minimum_can_find_invalid() {
    use super::statistics::ZeroSharedCacheSetStatistics;

    let mut set = SharedCacheSet::<8, 1, false, ZeroSharedCacheSetStatistics>::new();
    let mut ts = 1;

    // push 8 elements inside.
    for i in 0..8 {
        assert_eq!(
            set.insert(i, SharedCacheAccessSource::Core(0), ts, false, true),
            (i == 7, false)
        );
        ts += 1;
    }

    // now, we invalid set 0.
    assert_eq!(set.invalidate(0, ts), SharedCacheLookupResult::Hit(false));
    ts += 1;

    // Now if we refill, we will hit the first place.
    set.insert(9, SharedCacheAccessSource::Core(0), ts, false, true);

    // And the cache line 0 should be replaced.
    assert_eq!(
        set.blocks[0],
        SharedCacheBlock {
            block_id_with_v: 9 << 1 | 1,
            ts: ts,
            modified: false,
            last_accessor: SharedCacheAccessSource::Core(0)
        }
    );

    ts += 1;

    // If we now insert another one, line[1] will be replaced.
    assert_eq!(
        set.insert(10, SharedCacheAccessSource::Core(0), ts, false, true),
        (false, false)
    );

    assert_eq!(
        set.blocks[1],
        SharedCacheBlock {
            block_id_with_v: 10 << 1 | 1,
            ts: ts,
            modified: false,
            last_accessor: SharedCacheAccessSource::Core(0)
        }
    )
}

#[test]
fn cold_miss_exist() {
    use super::statistics::ZeroSharedCacheSetStatistics;
    use crate::components::cache_hierarchy::common::CacheAccessType;

    let mut set = SharedCacheSet::<8, 1, false, ZeroSharedCacheSetStatistics>::new();

    let mut ts = 1;

    // do one lookup. It will return the cold.
    assert_eq!(
        set.lookup(
            &SharedCacheAccessRequest {
                is_os: false,
                source: SharedCacheAccessSource::Core(0),
                block_id: 10,
                access_type: CacheAccessType::DataRead
            },
            ts,
        ),
        SharedCacheLookupResult::ColdMiss,
    );

    ts += 1;

    // fill the cache.
    for i in 0..8 {
        assert_eq!(
            set.insert(i + 10, SharedCacheAccessSource::Core(0), ts, false, true),
            (i == 7, false)
        );
        ts += 1;
    }

    // do a lookup that triggers a miss. Now it should be a miss instead of a cold miss.
    assert_eq!(
        set.lookup(
            &SharedCacheAccessRequest {
                is_os: false,
                source: SharedCacheAccessSource::Core(0),
                block_id: 20,
                access_type: CacheAccessType::DataRead
            },
            ts,
        ),
        SharedCacheLookupResult::Miss,
    );
}

#[test]
fn test_lookup_and_insert() {
    let mut set =
        SharedCacheSet::<4, 1, true, super::statistics::ZeroSharedCacheSetStatistics>::new();
    let mut ts = 1;
    use crate::components::cache_hierarchy::common::CacheAccessType;

    // - When it is cold, look up and insert will return a miss
    // - Then, lookup and insert with the same cache line will return a hit.
    // - If we create another four cache lines through lookup and insert, we will
    //  - See an eviction of the first one
    // - Then, a lookup and insert of the first one will return a miss.

    assert_eq!(
        set.lookup_and_insert(
            &SharedCacheAccessRequest {
                is_os: false,
                source: SharedCacheAccessSource::Core(0),
                block_id: 1,
                access_type: CacheAccessType::DataWrite
            },
            ts,
            true,
        ),
        SharedCacheLookupAndInsertResult::InsertedAndCold(false),
    );

    ts += 1;

    assert_eq!(
        set.lookup_and_insert(
            &SharedCacheAccessRequest {
                is_os: false,
                source: SharedCacheAccessSource::Core(0),
                block_id: 1,
                access_type: CacheAccessType::DataRead
            },
            ts,
            true,
        ),
        SharedCacheLookupAndInsertResult::Hit(true),
    );

    ts += 1;

    for i in 0..4 {
        // insert 4 new cache lines.
        assert_eq!(
            set.lookup_and_insert(
                &SharedCacheAccessRequest {
                    is_os: false,
                    source: SharedCacheAccessSource::Core(0),
                    block_id: i + 2,
                    access_type: CacheAccessType::DataRead
                },
                ts,
                true,
            ),
            if i < 2 {
                SharedCacheLookupAndInsertResult::InsertedAndCold(false)
            } else if i == 2 {
                SharedCacheLookupAndInsertResult::InsertedAndCold(true)
            } else {
                SharedCacheLookupAndInsertResult::Inserted
            },
        );

        ts += 1;
    }

    // now, if we lookup and insert the first cache line, it should be a miss.
    ts += 1;

    // do a lookup. It should be a miss.
    assert_eq!(
        set.lookup(
            &SharedCacheAccessRequest {
                is_os: false,
                source: SharedCacheAccessSource::Core(0),
                block_id: 1,
                access_type: CacheAccessType::DataRead
            },
            ts
        ),
        SharedCacheLookupResult::Miss,
    );

    ts += 1;

    assert_eq!(
        set.lookup_and_insert(
            &SharedCacheAccessRequest {
                is_os: false,
                source: SharedCacheAccessSource::Core(0),
                block_id: 1,
                access_type: CacheAccessType::DataRead
            },
            ts,
            true,
        ),
        SharedCacheLookupAndInsertResult::Inserted,
    );
}
