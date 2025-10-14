use rand::Rng;
use worm_cache::components::cache_hierarchy::common::ParallelLRUSharedCache;
use worm_cache::components::cache_hierarchy::common::SharedCache;

struct WorkingSet {
    starting_addr: u64,
    size: u64,
}

struct TraceGenerator {
    working_set: Vec<(WorkingSet, u64)>, // (working set, weight)
}

impl TraceGenerator {
    fn new() -> Self {
        TraceGenerator {
            working_set: Vec::new(),
        }
    }

    fn add_working_set(&mut self, working_set: WorkingSet, weight: u64) {
        self.working_set.push((working_set, weight));
    }

    fn generate(&mut self, rng: &mut impl rand::Rng) -> u64 {
        let mut total_weight = 0;
        for (_, weight) in &self.working_set {
            total_weight += weight;
        }
        let mut weight = rng.gen_range(0..total_weight);
        for (working_set, w) in &self.working_set {
            if weight < *w {
                let addr = working_set.starting_addr + rng.gen_range(0..working_set.size);
                return addr;
            }
            weight -= w;
        }
        unreachable!();
    }
}

impl Default for TraceGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
fn default_trace(bias: u64) -> TraceGenerator {
    // A trace generator with 5 workingset.
    // 1: 0, 1000, 100
    // 2: 1000, 2000, 50
    // 3: 10000, 10000, 30
    // 4: 100000, 100000, 10
    // 5: 1000000, 1000000, 5
    let mut trace_generator = TraceGenerator::new();

    trace_generator.add_working_set(
        WorkingSet {
            starting_addr: 0 + bias,
            size: 1000,
        },
        100,
    );
    trace_generator.add_working_set(
        WorkingSet {
            starting_addr: 1000 + bias,
            size: 2000,
        },
        50,
    );
    trace_generator.add_working_set(
        WorkingSet {
            starting_addr: 10000 + bias,
            size: 10000,
        },
        30,
    );
    trace_generator.add_working_set(
        WorkingSet {
            starting_addr: 100000 + bias,
            size: 100000,
        },
        10,
    );
    trace_generator.add_working_set(
        WorkingSet {
            starting_addr: 1000000 + bias,
            size: 1000000,
        },
        5,
    );
    trace_generator
}

// Possible tests:
// - Resizable LLC
// - Resizable Cache hierarchy
// - Resizable TLB

#[cfg(test)]
#[test]
#[ignore]
fn test_resizable_llc() {
    use worm_cache::{
        checkpoint::FlexusParameter,
        components::cache_hierarchy::common::{
            CacheAccessType, SharedCacheAccessRequest, SharedCacheAccessSource,
            statistics::ZeroSharedCacheSetStatistics,
        },
    };
    // Fix the seed.
    let mut trace_generator = default_trace(0);

    let mut rng = rand::thread_rng();

    // create an LLC.
    let big_cache = ParallelLRUSharedCache::<ZeroSharedCacheSetStatistics, 128, 16, false>::new();
    let small_cache = ParallelLRUSharedCache::<ZeroSharedCacheSetStatistics, 32, 8, false>::new();

    let special = 778;

    // Generate 1M requests.
    for ts in 1..10001u64 {
        let addr = trace_generator.generate(&mut rng);

        // Three functions.
        // lookup: 50%
        // insert: 20%
        let r = rng.gen_range(0..70);

        // 50% read and 50% write
        let is_write = rng.gen_bool(0.5);

        if r < 50 {
            let request = SharedCacheAccessRequest {
                source: SharedCacheAccessSource::Core(0),
                block_id: addr,
                access_type: if is_write {
                    CacheAccessType::DataWrite
                } else {
                    CacheAccessType::DataRead
                },
                is_os: false,
            };
            big_cache.lookup_and_insert_on_miss(&request, ts, true);
            small_cache.lookup_and_insert_on_miss(&request, ts, true);

            if addr == special {
                eprintln!("lookup ts: {}, addr: {}, is_write: {}", ts, addr, is_write);
            }
        } else if r < 70 {
            big_cache.insert(SharedCacheAccessSource::Core(0), addr, ts, false, true);
            small_cache.insert(SharedCacheAccessSource::Core(0), addr, ts, false, true);

            if addr == special {
                eprintln!("insert ts: {}, addr: {}, is_write: {}", ts, addr, is_write);
            }
        } else {
            let request = SharedCacheAccessRequest {
                source: SharedCacheAccessSource::Core(0),
                block_id: addr,
                access_type: if is_write {
                    CacheAccessType::DataWrite
                } else {
                    CacheAccessType::DataRead
                },
                is_os: false,
            };
            big_cache.lookup_and_insert_on_miss(&request, ts, true);
            small_cache.lookup_and_insert_on_miss(&request, ts, true);

            if addr == special {
                eprintln!(
                    "lookup_and_insert_on_miss ts: {}, addr: {}, is_write: {}",
                    ts, addr, is_write
                );
            }
        }
    }

    // Now, it is time to resize the cache.
    let big_cache_serial_helper = big_cache.to_serialize_helper();
    let small_cache_serial_helper = small_cache.to_serialize_helper();

    // convert these helpers to json Value.
    let big_cache_json = serde_json::to_value(&big_cache_serial_helper).unwrap();
    let small_cache_json = serde_json::to_value(&small_cache_serial_helper).unwrap();

    use worm_cache::checkpoint::cache_hierarchy::SingleSharedCacheSerdeHelper;
    let mut big_cache_serial_helper: SingleSharedCacheSerdeHelper =
        serde_json::from_value(big_cache_json).unwrap();
    let mut small_cache_serial_helper: SingleSharedCacheSerdeHelper =
        serde_json::from_value(small_cache_json).unwrap();

    // Resize the big cache to the small cache.
    let small_cache_flexus_configuration = FlexusParameter {
        l1i_sets: 1,
        l1i_associativity: 1,
        l1d_sets: 1,
        l1d_associativity: 1,
        l2_sets: 32,
        l2_associativity: 8,
        l2_slice_count: 1,
        itlb_sets: 1,
        itlb_associativity: 1,
        dtlb_sets: 1,
        dtlb_associativity: 1,
        stlb_sets: 1,
        stlb_associativity: 1,
        stlb_inclusion: worm_cache::checkpoint::FlexusSTLBInclusion::Inclusive,
        directory: worm_cache::checkpoint::FlexusDirectoryType::Infinite,
        directory_slice_count: 1,
        btb_sets: 1,
        btb_associativity: 1,
        no_resizing: false,
    };

    big_cache_serial_helper.resize(&small_cache_flexus_configuration);
    small_cache_serial_helper.resize(&small_cache_flexus_configuration);

    big_cache_serial_helper.assert_eq(&small_cache_serial_helper);
}

#[cfg(test)]
#[test]
#[ignore]
fn test_resize_cache_hierarchy() {
    use rand::SeedableRng;
    use worm_cache::{
        checkpoint::{FlexusParameter, process_cache_hierarchy},
        components::cache_hierarchy::{
            CacheBlockRequest, MemoryHierarchy,
            common::{
                CacheAccessType, InfiniteDirectory, ParallelHarvardPrivateCache,
                statistics::ZeroSharedCacheSetStatistics,
            },
            hierarchy::ParallelMemoryHierarchy,
            mmu::NoMMU,
        },
    };

    const CORE_COUNT: usize = 8;

    let mut data_trace_generator = default_trace(0);
    let mut instriction_trace_generator = default_trace(1000000000);
    let mut rng = rand::rngs::StdRng::seed_from_u64(0);

    // Create a small cache hierarchy and a large cache hierarchy.

    type SmallHierarchy = ParallelMemoryHierarchy<
        NoMMU,
        ParallelHarvardPrivateCache<{ CORE_COUNT }, 32, 4, 32, 4>,
        ParallelLRUSharedCache<ZeroSharedCacheSetStatistics, 32, 8, true>,
        InfiniteDirectory<65536>,
        true,
        true,
        true,
        true,
        { CORE_COUNT },
    >;

    type LargeHierarchy = ParallelMemoryHierarchy<
        NoMMU,
        ParallelHarvardPrivateCache<{ CORE_COUNT }, 128, 8, 128, 8>,
        ParallelLRUSharedCache<ZeroSharedCacheSetStatistics, 128, 16, true>,
        InfiniteDirectory<65536>,
        true,
        true,
        true,
        true,
        { CORE_COUNT },
    >;

    let special_cache_line = 490;

    let small_hierarchy = SmallHierarchy::new();
    let large_hierarchy = LargeHierarchy::new();

    for ts in 1..10001u64 {
        let core_id = rng.gen_range(0..CORE_COUNT as u32);

        let is_read = rng.gen_bool(0.7);
        let is_instruction = rng.gen_bool(0.5);
        let is_os = false;

        let access_type = if is_read {
            if is_instruction {
                CacheAccessType::InstructionFetch
            } else {
                CacheAccessType::DataRead
            }
        } else {
            CacheAccessType::DataWrite
        };

        let addr = if matches!(access_type, CacheAccessType::InstructionFetch) {
            instriction_trace_generator.generate(&mut rng)
        } else {
            data_trace_generator.generate(&mut rng)
        };

        if addr == special_cache_line {
            eprintln!(
                "ts: {}, core_id: {}, addr: {}, access_type: {:#?}, is_os: {}",
                ts, core_id, addr, access_type, is_os
            );
        }

        let request = CacheBlockRequest {
            core_id,
            block_id: addr,
            access_type,
            is_os,
        };

        small_hierarchy.access_memory_pblock_id(&request, ts);
        large_hierarchy.access_memory_pblock_id(&request, ts);
    }

    // get a temporal folder
    let temp_dir = tempfile::tempdir().unwrap();

    println!("temp_dir: {:?}", temp_dir.path());

    // make them
    std::fs::create_dir(temp_dir.path().join("small")).unwrap();
    std::fs::create_dir(temp_dir.path().join("small-flexus")).unwrap();
    std::fs::create_dir(temp_dir.path().join("large")).unwrap();
    std::fs::create_dir(temp_dir.path().join("large-flexus")).unwrap();

    // serialize the small hierarchy and large hierarchy.
    small_hierarchy.serialize(&format!("{}/small", temp_dir.path().to_str().unwrap()), 0);
    large_hierarchy.serialize(&format!("{}/large", temp_dir.path().to_str().unwrap()), 0);

    // Now, we need to parse them back and resize it.
    let flexus_parameter = FlexusParameter {
        l1i_sets: 32,
        l1i_associativity: 4,
        l1d_sets: 32,
        l1d_associativity: 4,
        l2_sets: 32,
        l2_associativity: 8,
        l2_slice_count: 1,
        itlb_sets: 1,
        itlb_associativity: 1,
        dtlb_sets: 1,
        dtlb_associativity: 1,
        stlb_sets: 1,
        stlb_associativity: 1,
        stlb_inclusion: worm_cache::checkpoint::FlexusSTLBInclusion::Inclusive,
        directory: worm_cache::checkpoint::FlexusDirectoryType::Infinite,
        directory_slice_count: 1,
        btb_sets: 1,
        btb_associativity: 1,
        no_resizing: false,
    };

    process_cache_hierarchy(
        &format!("{}/small", temp_dir.path().to_str().unwrap()),
        &flexus_parameter,
        &format!("{}/small-flexus", temp_dir.path().to_str().unwrap()),
    );

    process_cache_hierarchy(
        &format!("{}/large", temp_dir.path().to_str().unwrap()),
        &flexus_parameter,
        &format!("{}/large-flexus", temp_dir.path().to_str().unwrap()),
    );

    // First compare L1i.
    for i in 0..CORE_COUNT {
        // Read the JSON file and compare them.
        let small_l1i: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(
                temp_dir
                    .path()
                    .join("small-flexus")
                    .join(format!("{:03}-ufetch-L1i.json", i)),
            )
            .unwrap(),
        )
        .unwrap();

        let large_l1i: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(
                temp_dir
                    .path()
                    .join("large-flexus")
                    .join(format!("{:03}-ufetch-L1i.json", i)),
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(small_l1i, large_l1i);
    }

    // Then compare L1d.
    for i in 0..CORE_COUNT {
        let small_l1d: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(
                temp_dir
                    .path()
                    .join("small-flexus")
                    .join(format!("{:03}-L1d.json", i)),
            )
            .unwrap(),
        )
        .unwrap();

        let large_l1d: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(
                temp_dir
                    .path()
                    .join("large-flexus")
                    .join(format!("{:03}-L1d.json", i)),
            )
            .unwrap(),
        )
        .unwrap();

        if small_l1d != large_l1d {
            // copy two json file to the current directory.
            std::fs::copy(
                temp_dir
                    .path()
                    .join("small-flexus")
                    .join(format!("{:03}-L1d.json", i)),
                std::path::Path::new("small-L1d.json"),
            )
            .unwrap();

            std::fs::copy(
                temp_dir
                    .path()
                    .join("large-flexus")
                    .join(format!("{:03}-L1d.json", i)),
                std::path::Path::new("large-L1d.json"),
            )
            .unwrap();

            panic!();
        }

        assert_eq!(small_l1d, large_l1d);
    }

    // Then compare the L2.
    let small_l2: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            temp_dir
                .path()
                .join("small-flexus")
                .join("000-L2-cache-slice.json"),
        )
        .unwrap(),
    )
    .unwrap();

    let large_l2: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            temp_dir
                .path()
                .join("large-flexus")
                .join("000-L2-cache-slice.json"),
        )
        .unwrap(),
    )
    .unwrap();

    assert_eq!(small_l2, large_l2);

    // delete the temporal folder.
    temp_dir.close().unwrap();
}
