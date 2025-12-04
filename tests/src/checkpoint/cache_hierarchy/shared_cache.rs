use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    checkpoint::FlexusParameter,
    components::cache_hierarchy::common::{
        PrivateCacheLine, SharedCacheAccessSource, SharedCacheBlock,
    },
};

#[derive(Serialize, Deserialize)]
struct SharedCacheSet {
    blocks: Vec<SharedCacheBlock>,
    touched_count: usize,
    recent_evict_ts: u64,
    access_count: u64,
}

#[derive(Serialize, Deserialize)]
pub struct SingleSharedCacheSerdeHelper {
    // This block is different from the one from worm_cache.
    blocks: Vec<SharedCacheSet>,
    warmed_sets: usize,
}

#[derive(Serialize)]
pub struct FlexusSerializedSharedCacheBlock {
    pub tag: u64,
    pub dirty: bool,
    pub writable: bool,
    pub ts: u64,
}

fn serialize_a_share_cache_set(
    set: &SharedCacheSet,
    _: usize,
) -> Vec<FlexusSerializedSharedCacheBlock> {
    let mut result = vec![];

    for block in set.blocks.iter().rev() {
        result.push(FlexusSerializedSharedCacheBlock {
            tag: (block.block_id_with_v >> 1) << crate::parameter::CACHE_LINE_SIZE.trailing_zeros(), // well, complete PA (with offset as 0) is used as a tag in the LLC.
            dirty: block.modified,
            writable: true,
            ts: block.ts,
        });
    }

    result
}

impl SingleSharedCacheSerdeHelper {
    pub fn process_evicted_cache_line(
        &mut self,
        line: &PrivateCacheLine,
        accessor: SharedCacheAccessSource,
    ) {
        let block_id = line.block_id();
        let set_idx = block_id as usize % self.blocks.len();
        let set = &mut self.blocks[set_idx];

        for block in set.blocks.iter_mut() {
            let is_valid = block.block_id_with_v & 1 == 1;

            if !is_valid {
                assert!(block.ts == 0);
            }

            let block_id = block.block_id_with_v >> 1;
            if block_id == line.block_id() {
                if !is_valid {
                    block.block_id_with_v = (line.block_id() << 1) | 1;
                    block.ts = line.ts;
                    block.modified = line.modified;
                    block.last_accessor = accessor;
                } else if is_valid && line.ts > block.ts {
                    block.ts = line.ts;
                    block.modified = line.modified;
                    block.last_accessor = accessor;
                }

                return;
            }
        }

        set.blocks.push(SharedCacheBlock {
            block_id_with_v: (line.block_id() << 1) | 1,
            ts: line.ts,
            modified: line.modified,
            last_accessor: accessor,
        });
    }

    pub fn resize(&mut self, flexus_configuration: &FlexusParameter) {
        assert!(self.blocks.len() % flexus_configuration.l2_sets == 0);

        if flexus_configuration.no_resizing {
            assert!(self.blocks.len() == flexus_configuration.l2_sets);
        }

        let mut new_blocks = vec![];

        for _ in 0..flexus_configuration.l2_sets {
            new_blocks.push(SharedCacheSet {
                blocks: vec![],
                touched_count: 0,
                recent_evict_ts: 0,
                access_count: 0,
            });
        }

        // Step 1: Combine sets.
        for (set_idx, set) in self.blocks.iter().enumerate() {
            let new_idx = set_idx % flexus_configuration.l2_sets;

            new_blocks[new_idx]
                .blocks
                .extend(set.blocks.iter().cloned());
        }

        // Step 2: Filter each set and keep the most recent blocks.
        for set in new_blocks.iter_mut() {
            set.blocks.retain(|block| block.block_id_with_v & 1 == 1); // keep valid blocks.
            set.blocks.sort_by_key(|block| block.ts);
            set.blocks.reverse();

            if flexus_configuration.no_resizing {
                assert!(
                    set.blocks.len() <= flexus_configuration.l2_associativity,
                    "Shared cache set is too large",
                );
            }

            set.blocks.truncate(flexus_configuration.l2_associativity);
        }

        self.blocks = new_blocks;
    }

    #[allow(dead_code)]
    pub fn export(&self, folder_name: &String, flexus_configuration: &FlexusParameter) {
        let mut file = std::fs::File::create(format!("{}/sys-L2-cache.json", folder_name)).unwrap();

        serde_json::to_writer(
            &mut file,
            &json!({
                "associativity": flexus_configuration.l2_associativity,
                "tags": self.blocks.iter().map(|set| {
                    serialize_a_share_cache_set(set, flexus_configuration.l2_sets)
                }).collect::<Vec<_>>(),
            }),
        )
        .unwrap();

        println!(
            "Shared cache is exported to {}/sys-L2-cache.json",
            folder_name
        );
    }

    pub fn export_slices(&self, folder_name: &String, flexus_configuration: &FlexusParameter) {
        // Now, we export each slice. Slices are set-interleaved at this stage.
        assert!(
            self.blocks.len() % flexus_configuration.l2_slice_count == 0,
            "The number of sets in the shared cache should be a multiple of the slice count."
        );

        // Step 1: Calculate the number of sets in each slice.
        let sets_per_slice = self.blocks.len() / flexus_configuration.l2_slice_count;

        // Step 2: Export each slice.
        for slice_idx in 0..flexus_configuration.l2_slice_count {
            let mut file = std::fs::File::create(format!(
                "{}/{:03}-L2-cache-slice.json",
                folder_name, slice_idx
            ))
            .unwrap();

            let sets: Vec<_> = (0..sets_per_slice)
                .map(|set_idx| {
                    &self.blocks[set_idx * flexus_configuration.l2_slice_count + slice_idx]
                })
                .collect();

            serde_json::to_writer(
                &mut file,
                &json!({
                    "associativity": flexus_configuration.l2_associativity,
                    "tags": sets.iter().map(|set| {
                        serialize_a_share_cache_set(set, flexus_configuration.l2_sets)
                    }).collect::<Vec<_>>(),
                }),
            )
            .unwrap();

            println!(
                "Shared cache slice {} is exported to {}/sys-L2-cache-slice-{}.json",
                slice_idx, folder_name, slice_idx
            );
        }
    }
}

impl SingleSharedCacheSerdeHelper {
    pub fn assert_eq(&self, other: &Self) {
        // set count should be the same.
        if self.blocks.len() != other.blocks.len() {
            panic!("Size mismatch");
        }

        let mut set_idx = 0;

        // Now comparing each set.
        for (a, b) in self.blocks.iter().zip(other.blocks.iter()) {
            if a.blocks.len() != b.blocks.len() {
                panic!("Set {} size mishatch", set_idx);
            }

            let mut way_idx = 0;

            for (a_block, b_block) in a.blocks.iter().zip(b.blocks.iter()) {
                if a_block.block_id_with_v != b_block.block_id_with_v
                    || a_block.ts != b_block.ts
                    || a_block.modified != b_block.modified
                    || a_block.last_accessor != b_block.last_accessor
                {
                    panic!("Set {} way {} mismatch", set_idx, way_idx);
                }

                way_idx += 1;
            }

            set_idx += 1;
        }
    }
}

#[test]
fn test_process_evicted_cache_line() {
    // There are two cases:
    // - The shared cahce already has the cache block.
    // - The shared cache does not have the cache block.

    let mut shared_cache_containing_block = SingleSharedCacheSerdeHelper {
        blocks: vec![
            SharedCacheSet {
                blocks: vec![SharedCacheBlock {
                    block_id_with_v: 0b1,
                    ts: 1,
                    modified: false,
                    last_accessor: SharedCacheAccessSource::Core(0),
                }],
                touched_count: 0,
                recent_evict_ts: 0,
                access_count: 0,
            },
            SharedCacheSet {
                blocks: vec![SharedCacheBlock {
                    block_id_with_v: 0b11,
                    ts: 2,
                    modified: false,
                    last_accessor: SharedCacheAccessSource::Core(0),
                }],
                touched_count: 0,
                recent_evict_ts: 0,
                access_count: 0,
            },
        ],
        warmed_sets: 0,
    };

    shared_cache_containing_block.process_evicted_cache_line(
        &PrivateCacheLine {
            block_id_with_v: 0b1,
            ts: 3,
            is_instruction: false,
            writeable: false,
            modified: false,
        },
        SharedCacheAccessSource::Core(0),
    );

    // Now, this block should be updated.
    assert_eq!(shared_cache_containing_block.blocks[0].blocks.len(), 1);
    assert_eq!(shared_cache_containing_block.blocks[0].blocks[0].ts, 3);

    // The shared cache does not have the cache block.
    let mut shared_cache_not_containing_block = SingleSharedCacheSerdeHelper {
        blocks: vec![
            SharedCacheSet {
                blocks: vec![SharedCacheBlock {
                    block_id_with_v: 0b1,
                    ts: 1,
                    modified: false,
                    last_accessor: SharedCacheAccessSource::Core(0),
                }],
                touched_count: 0,
                recent_evict_ts: 0,
                access_count: 0,
            },
            SharedCacheSet {
                blocks: vec![SharedCacheBlock {
                    block_id_with_v: 0b11,
                    ts: 2,
                    modified: false,
                    last_accessor: SharedCacheAccessSource::Core(0),
                }],
                touched_count: 0,
                recent_evict_ts: 0,
                access_count: 0,
            },
        ],
        warmed_sets: 0,
    };

    shared_cache_not_containing_block.process_evicted_cache_line(
        &PrivateCacheLine {
            block_id_with_v: 0b101,
            ts: 3,
            is_instruction: false,
            writeable: false,
            modified: false,
        },
        SharedCacheAccessSource::Core(0),
    );

    // Now, this block should be added.
    assert_eq!(shared_cache_not_containing_block.blocks[0].blocks.len(), 2);
    assert_eq!(shared_cache_not_containing_block.blocks[0].blocks[0].ts, 1);
    assert_eq!(
        shared_cache_not_containing_block.blocks[0].blocks[0].block_id_with_v,
        0b1
    );
    assert_eq!(shared_cache_not_containing_block.blocks[0].blocks[1].ts, 3);
    assert_eq!(
        shared_cache_not_containing_block.blocks[0].blocks[1].block_id_with_v,
        0b101
    );
}

#[test]
fn test_resize() {
    let mut shared_cache = SingleSharedCacheSerdeHelper {
        blocks: vec![
            SharedCacheSet {
                blocks: vec![
                    SharedCacheBlock {
                        block_id_with_v: 0b1,
                        ts: 1,
                        modified: false,
                        last_accessor: SharedCacheAccessSource::Core(0),
                    },
                    SharedCacheBlock {
                        block_id_with_v: 0b101,
                        ts: 2,
                        modified: false,
                        last_accessor: SharedCacheAccessSource::Core(0),
                    },
                    SharedCacheBlock {
                        block_id_with_v: 0b1101,
                        ts: 3,
                        modified: false,
                        last_accessor: SharedCacheAccessSource::Core(0),
                    },
                    SharedCacheBlock {
                        block_id_with_v: 0b11101,
                        ts: 5,
                        modified: false,
                        last_accessor: SharedCacheAccessSource::Core(0),
                    },
                ],
                touched_count: 0,
                recent_evict_ts: 0,
                access_count: 0,
            },
            SharedCacheSet {
                blocks: vec![
                    SharedCacheBlock {
                        block_id_with_v: 0b11,
                        ts: 1,
                        modified: false,
                        last_accessor: SharedCacheAccessSource::Core(0),
                    },
                    SharedCacheBlock {
                        block_id_with_v: 0b111,
                        ts: 2,
                        modified: false,
                        last_accessor: SharedCacheAccessSource::Core(0),
                    },
                    SharedCacheBlock {
                        block_id_with_v: 0b1111,
                        ts: 3,
                        modified: false,
                        last_accessor: SharedCacheAccessSource::Core(0),
                    },
                    SharedCacheBlock {
                        block_id_with_v: 0b11111,
                        ts: 4,
                        modified: false,
                        last_accessor: SharedCacheAccessSource::Core(0),
                    },
                ],
                touched_count: 0,
                recent_evict_ts: 0,
                access_count: 0,
            },
        ],
        warmed_sets: 0,
    };

    shared_cache.resize(&FlexusParameter {
        l2_sets: 1,
        l2_associativity: 2,
        l2_slice_count: 1,
        l1i_sets: 1,
        l1i_associativity: 1,
        l1d_sets: 1,
        l1d_associativity: 1,
        itlb_sets: 1,
        itlb_associativity: 1,
        dtlb_sets: 1,
        dtlb_associativity: 1,
        stlb_sets: 1,
        stlb_associativity: 1,
        stlb_inclusion: crate::checkpoint::FlexusSTLBInclusion::Inclusive,
        directory: crate::checkpoint::FlexusDirectoryType::Infinite,
        directory_slice_count: 1,
        btb_sets: 1,
        btb_associativity: 1,
        no_resizing: false,
    });

    assert_eq!(shared_cache.blocks.len(), 1);
    assert_eq!(shared_cache.blocks[0].blocks.len(), 2);

    // The left two blocks should be kept.
    assert_eq!(shared_cache.blocks[0].blocks[0].ts, 5);
    assert_eq!(shared_cache.blocks[0].blocks[0].block_id_with_v, 0b11101);
    assert_eq!(shared_cache.blocks[0].blocks[1].ts, 4);
    assert_eq!(shared_cache.blocks[0].blocks[1].block_id_with_v, 0b11111);
}

#[test]
fn insert_a_cache_line_that_is_invalid_in_shared_cache() {
    let mut shared_cache = SingleSharedCacheSerdeHelper {
        blocks: vec![
            SharedCacheSet {
                blocks: vec![SharedCacheBlock {
                    block_id_with_v: 0b100,
                    ts: 0,
                    modified: false,
                    last_accessor: SharedCacheAccessSource::Core(0),
                }],
                touched_count: 0,
                recent_evict_ts: 0,
                access_count: 0,
            },
            SharedCacheSet {
                blocks: vec![],
                touched_count: 0,
                recent_evict_ts: 0,
                access_count: 0,
            },
        ],
        warmed_sets: 0,
    };

    shared_cache.process_evicted_cache_line(
        &PrivateCacheLine {
            block_id_with_v: 0b101,
            ts: 3,
            is_instruction: false,
            writeable: false,
            modified: false,
        },
        SharedCacheAccessSource::Core(0),
    );

    // Do a resize, and we should expect that block 0b101 is kept.
    shared_cache.resize(&FlexusParameter {
        l2_sets: 1,
        l2_associativity: 1,
        l2_slice_count: 1,
        l1i_sets: 1,
        l1i_associativity: 1,
        l1d_sets: 1,
        l1d_associativity: 1,
        itlb_sets: 1,
        itlb_associativity: 1,
        dtlb_sets: 1,
        dtlb_associativity: 1,
        stlb_sets: 1,
        stlb_associativity: 1,
        stlb_inclusion: crate::checkpoint::FlexusSTLBInclusion::Inclusive,
        directory: crate::checkpoint::FlexusDirectoryType::Infinite,
        directory_slice_count: 1,
        btb_sets: 1,
        btb_associativity: 1,
        no_resizing: false,
    });

    assert_eq!(shared_cache.blocks.len(), 1);
    assert_eq!(shared_cache.blocks[0].blocks.len(), 1);
    assert_eq!(shared_cache.blocks[0].blocks[0].ts, 3);
    assert_eq!(shared_cache.blocks[0].blocks[0].block_id_with_v, 0b101);
}
