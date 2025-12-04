use rustc_hash::FxHashMap;
use serde::Serialize;
use serde_json::json;

use crate::{
    checkpoint::{FlexusDirectoryType, FlexusParameter},
    components::cache_hierarchy::common::{
        HarvardPerCorePrivateCacheSerdeHelper, PrivateCacheLine, PrivateCacheSet,
        UnifiedPerCorePrivateCacheSerdeHelper,
    },
};

pub struct BackReferencedEntry {
    sharers: [bool; 256], // For flexus, (0) is data cache, (1) is instruction cache.
    ts: u64,
    anti_reference: Vec<(usize, usize, usize, usize)>,
}

impl BackReferencedEntry {
    fn sharer_count(&self) -> usize {
        self.sharers.iter().filter(|b| **b).count()
    }
}

pub struct ExportedDirectoryEntry {
    tag: u64,
    sharers: Vec<bool>,
    ts: u64,
}

pub struct FlexusPrivateCacheCheckpointHelper {
    pub caches: Vec<HarvardPerCorePrivateCacheSerdeHelper>,
    pub directory: Vec<Vec<ExportedDirectoryEntry>>,
    pub evicted_cache_lines: FxHashMap<u64, (PrivateCacheLine, u32)>,
    pub flexus_configuration: FlexusParameter,
}

fn resize_private_cache(
    private_cache: Vec<PrivateCacheSet>,
    set: usize,
    asso: usize,
    no_resizing: bool,
) -> (Vec<PrivateCacheSet>, Vec<PrivateCacheLine>) {
    assert!(private_cache.len() % set == 0);

    if no_resizing {
        assert!(private_cache.len() == set);
    }

    let mut new_cache = vec![vec![]; set];
    let mut evicted_lines = vec![];

    // Step 1: merge sets.
    for (set_idx, mut p_set) in private_cache.into_iter().enumerate() {
        p_set.lines.retain(|line| line.is_valid());
        new_cache[set_idx % set].append(&mut p_set.lines);
    }

    // Step 2: Keep the first asso lines in each set.
    for set in new_cache.iter_mut() {
        set.sort_by(|a, b| b.ts.cmp(&a.ts));
        if set.len() > asso {
            if no_resizing {
                panic!("No resizing is enabled, but the cache is too large.");
            }
            evicted_lines.append(&mut set.drain(asso..).collect::<Vec<_>>());
        }
    }

    // Step 3: Compose the new cache.
    let new_cache = new_cache
        .into_iter()
        .map(|set| PrivateCacheSet {
            touched_count: set.len(),
            lines: set,
            recent_invalid_slot_index: None,
            hit_time: 0,
            hit_index_acc: 0,
        })
        .collect::<Vec<_>>();

    (new_cache, evicted_lines)
}

#[allow(dead_code)]
pub fn resize_directory(
    infinite_directory: FxHashMap<u64, BackReferencedEntry>,
    directory_set: usize,
    directory_associativity: usize,
    harvard: &mut [HarvardPerCorePrivateCacheSerdeHelper],
    evicted_cache_line: &mut FxHashMap<u64, (PrivateCacheLine, u32)>,
) -> Vec<Vec<ExportedDirectoryEntry>> {
    let mut res: Vec<Vec<(u64, BackReferencedEntry)>> =
        (0..directory_set).map(|_| vec![]).collect();

    // Step 1: collect the directory entries.
    for (tag, entry) in infinite_directory {
        let directory_set_idx = tag as usize % directory_set;
        let directory_set = &mut res[directory_set_idx];
        directory_set.push((tag, entry));
    }

    let mut number_of_evicted_directory_entry = 0;

    // Step 2: Apply LRUs.
    let res = res
        .into_iter()
        .map(|mut set| {
            // Flexus uses a different replacement policy. Instead of LRU, it uses the one with the least number of sharers.
            set.sort_by(|a, b| b.1.sharer_count().cmp(&a.1.sharer_count()));

            if directory_associativity <= set.len() {
                number_of_evicted_directory_entry += set.len() - directory_associativity;
                for evicted_entry in set.drain(directory_associativity..) {
                    let mut line_to_evict = PrivateCacheLine {
                        block_id_with_v: (evicted_entry.0 << 1 | 1),
                        ts: 0,
                        is_instruction: false,
                        writeable: false,
                        modified: false,
                    };
                    let mut owner = 0;

                    for (owner_id, cache_id, set_idx, way_idx) in evicted_entry.1.anti_reference {
                        let cache = if cache_id == 0 {
                            &mut harvard[owner_id].i_cache
                        } else {
                            &mut harvard[owner_id].d_cache
                        };

                        let line = &mut cache[set_idx].lines[way_idx];

                        // aggregate the line information to the evicted_line.
                        assert!(line_to_evict.block_id() == evicted_entry.0);
                        assert!(line_to_evict.block_id() == line.block_id());

                        if line.ts > line_to_evict.ts {
                            owner = owner_id as u32;
                        }

                        line_to_evict.ts = line_to_evict.ts.max(line.ts);
                        line_to_evict.writeable |= line.writeable;
                        line_to_evict.modified |= line.modified;

                        // invalid the line
                        line.ts = 0;
                        line.block_id_with_v = 0;
                    }

                    match evicted_cache_line.get_mut(&line_to_evict.block_id()) {
                        Some(line) => {
                            // keep the one that has the latest ts.
                            if line.0.ts < line_to_evict.ts {
                                *line = (line_to_evict, owner);
                            }
                        }
                        None => {
                            evicted_cache_line
                                .insert(line_to_evict.block_id(), (line_to_evict, owner));
                        }
                    }
                }
            }

            set.into_iter()
                .map(|entry| ExportedDirectoryEntry {
                    tag: entry.0,
                    sharers: entry.1.sharers.to_vec(),
                    ts: entry.1.ts,
                })
                .collect()
        })
        .collect();

    println!(
        "Number of evicted directory entries: {}, in {}%",
        number_of_evicted_directory_entry,
        number_of_evicted_directory_entry as f64 / (directory_set * directory_associativity) as f64
            * 100.0
    );

    res
}

fn render_infinite_directory(
    infinite_directory: FxHashMap<u64, BackReferencedEntry>,
) -> Vec<ExportedDirectoryEntry> {
    infinite_directory
        .into_iter()
        .map(|(tag, entry)| ExportedDirectoryEntry {
            tag,
            sharers: entry.sharers.to_vec(),
            ts: entry.ts,
        })
        .collect()
}

impl FlexusPrivateCacheCheckpointHelper {
    pub fn from_harvard_caches(
        caches: Vec<HarvardPerCorePrivateCacheSerdeHelper>,
        flexus_configuration: &FlexusParameter,
    ) -> Self {
        let mut result = FlexusPrivateCacheCheckpointHelper {
            caches: vec![],
            directory: vec![],
            evicted_cache_lines: FxHashMap::default(),
            flexus_configuration: flexus_configuration.clone(),
        };

        let mut infinite_directory = FxHashMap::default();

        // Scan all caches, shrink them, and insert the lines to the directory.
        for (owner_id, cache) in caches.into_iter().enumerate() {
            assert!(cache.i_cache.len() % flexus_configuration.l1i_sets == 0);
            assert!(cache.d_cache.len() % flexus_configuration.l1d_sets == 0);

            let (i_cache, d_cache) = (cache.i_cache, cache.d_cache);

            let mut evicted_cache_line = vec![vec![]];

            let (new_icache, i_rem) = resize_private_cache(
                i_cache,
                flexus_configuration.l1i_sets,
                flexus_configuration.l1i_associativity,
                flexus_configuration.no_resizing,
            );
            let (new_dcache, d_rem) = resize_private_cache(
                d_cache,
                flexus_configuration.l1d_sets,
                flexus_configuration.l1d_associativity,
                flexus_configuration.no_resizing,
            );

            // This is the way to create an empty data cache.
            // let new_dcache = d_cache
            //     .into_iter()
            //     .map(|_| PrivateCacheSet::new(flexus_configuration.l1d_associativity))
            //     .collect::<Vec<_>>();
            // let d_rem = vec![];

            evicted_cache_line.push(i_rem);
            evicted_cache_line.push(d_rem);

            for (cache_idx, cache) in [&new_icache, &new_dcache].iter().enumerate() {
                // insert to the directory.
                for (set_idx, set) in cache.iter().enumerate() {
                    for (way_idx, line) in set.lines.iter().enumerate() {
                        if line.is_valid() {
                            let directory_entry = infinite_directory
                                .entry(line.block_id())
                                .or_insert_with(|| BackReferencedEntry {
                                    sharers: [false; 256],
                                    ts: line.ts,
                                    anti_reference: vec![],
                                });

                            let sharer_position = owner_id * 2 + (1 - cache_idx);
                            directory_entry.sharers[sharer_position] = true;

                            if directory_entry.ts < line.ts {
                                directory_entry.ts = line.ts;
                            }

                            directory_entry
                                .anti_reference
                                .push((owner_id, cache_idx, set_idx, way_idx));
                        }
                    }
                }
            }

            let target_harvard = HarvardPerCorePrivateCacheSerdeHelper {
                i_cache: new_icache,
                d_cache: new_dcache,
            };

            result.caches.push(target_harvard);

            for (core_id, lines) in evicted_cache_line.into_iter().enumerate() {
                for line in lines {
                    match result.evicted_cache_lines.get_mut(&line.block_id()) {
                        Some(evicted_line) => {
                            if evicted_line.0.ts < line.ts {
                                *evicted_line = (line, core_id as u32);
                            }
                        }
                        None => {
                            result
                                .evicted_cache_lines
                                .insert(line.block_id(), (line, core_id as u32));
                        }
                    }
                }
            }
        }

        result.directory = match flexus_configuration.directory {
            FlexusDirectoryType::Infinite => vec![render_infinite_directory(infinite_directory)],
            FlexusDirectoryType::Standard {
                sets,
                associativity,
            } => resize_directory(
                infinite_directory,
                sets,
                associativity,
                &mut result.caches,
                &mut result.evicted_cache_lines,
            ),
        };

        result
    }

    pub fn get_evicted_lines(&self) -> &FxHashMap<u64, (PrivateCacheLine, u32)> {
        &self.evicted_cache_lines
    }
}

impl UnifiedPerCorePrivateCacheSerdeHelper {
    pub fn to_harvard_cache(self) -> HarvardPerCorePrivateCacheSerdeHelper {
        // Create a harvard cache with the same size.
        let asso = self.cache[0].lines.len();
        let set = self.cache.len();
        let mut harvard_cache = HarvardPerCorePrivateCacheSerdeHelper {
            i_cache: vec![],
            d_cache: vec![],
        };

        for _ in 0..set {
            harvard_cache.i_cache.push(PrivateCacheSet::new(asso));
            harvard_cache.d_cache.push(PrivateCacheSet::new(asso));
        }

        // scan the private cache and fill the harvard cache.
        for (set_index, set) in self.cache.into_iter().enumerate() {
            for line in set.lines {
                if line.is_valid() {
                    if line.is_instruction() {
                        let position = harvard_cache.i_cache[set_index].touched_count;
                        harvard_cache.i_cache[set_index].lines[position] = line;
                        harvard_cache.i_cache[set_index].touched_count += 1;
                    } else {
                        let position = harvard_cache.d_cache[set_index].touched_count;
                        harvard_cache.d_cache[set_index].lines[position] = line;
                        harvard_cache.d_cache[set_index].touched_count += 1;
                    }
                }
            }
        }

        harvard_cache
    }
}

impl FlexusPrivateCacheCheckpointHelper {
    pub fn from_unified_caches(
        caches: Vec<UnifiedPerCorePrivateCacheSerdeHelper>,
        flexus_configuration: &FlexusParameter,
    ) -> Self {
        let harvard_caches = caches
            .into_iter()
            .map(|cache| cache.to_harvard_cache())
            .collect::<Vec<_>>();

        FlexusPrivateCacheCheckpointHelper::from_harvard_caches(
            harvard_caches,
            flexus_configuration,
        )
    }
}

#[derive(Serialize)]
pub struct FlexusCacheLine {
    tag: u64,
    writable: bool,
    dirty: bool,
    ts: u64,
}

fn serialize_a_set(set: &PrivateCacheSet, number_of_set: usize) -> Vec<FlexusCacheLine> {
    // 1. sort the line by its timestamp. smaller timestamp goes first
    // 2. filter out the invalid lines.
    // 3. tag should be removed with the valid bit and the index bit.

    let mut sorted_lines = set.lines.clone();
    sorted_lines.sort_by(|a, b| a.ts.cmp(&b.ts));

    let set_bits = (number_of_set as u64).trailing_zeros();

    return sorted_lines
        .iter()
        .filter(|line| line.block_id_with_v & 0x1 == 1)
        .map(|line| FlexusCacheLine {
            tag: (line.block_id_with_v >> 1) >> set_bits,
            writable: line.modified,
            dirty: line.modified,
            ts: line.ts,
        })
        .collect();
}

fn serialize_a_cache(
    cache: &[PrivateCacheSet],
    set_number: usize,
    asso: usize,
) -> serde_json::Value {
    json!({
        "associativity": asso,
        "tags": cache
            .iter()
            .map(|set| serialize_a_set(set, set_number))
            .collect::<Vec<_>>()
        }
    )
}

#[derive(Serialize)]
pub struct FlexusDirectoryEntry {
    tag: u64, // block id.
    sharers: String,
    ts: u64,
}

impl ExportedDirectoryEntry {
    pub fn to_flexus_directory_entry(&self) -> FlexusDirectoryEntry {
        FlexusDirectoryEntry {
            tag: self.tag << crate::parameter::CACHE_LINE_SIZE.trailing_zeros(), // the directory tag is the address of the block with offset to be 0.
            sharers: self
                .sharers
                .iter()
                .rev()
                .map(|b| if *b { "1" } else { "0" })
                .collect::<String>(),
            ts: self.ts,
        }
    }
}

#[allow(dead_code)]
fn serialize_a_directory(
    directory: &[Vec<ExportedDirectoryEntry>],
    directory_type: FlexusDirectoryType,
) -> serde_json::Value {
    match directory_type {
        FlexusDirectoryType::Infinite => serde_json::to_value(
            directory[0]
                .iter()
                .map(|entry| entry.to_flexus_directory_entry())
                .collect::<Vec<_>>(),
        )
        .unwrap(),
        FlexusDirectoryType::Standard {
            sets: _,
            associativity: _,
        } => serde_json::to_value(
            directory
                .iter()
                .map(|set| {
                    set.iter()
                        .map(|entry| entry.to_flexus_directory_entry())
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>(),
        )
        .unwrap(),
    }
}

fn serialize_directory_slices(
    directory: &[Vec<ExportedDirectoryEntry>],
    directory_type: FlexusDirectoryType,
    slice_count: usize,
) -> Vec<serde_json::Value> {
    match directory_type {
        FlexusDirectoryType::Infinite => {
            let mut slices = Vec::from_iter(std::iter::repeat_with(Vec::new).take(slice_count));

            // Now, classify the directory entries into slices.
            for entry in directory[0].iter() {
                let slice_idx = (entry.tag as usize) % slice_count;
                slices[slice_idx].push(entry.to_flexus_directory_entry());
            }

            slices
                .into_iter()
                .map(|slice| serde_json::to_value(slice).unwrap())
                .collect()
        }
        FlexusDirectoryType::Standard {
            sets,
            associativity,
        } => {
            assert!(sets % slice_count == 0);
            assert!(sets == directory.len()); // The directory must be resized ahead of time.

            let set_per_slice = sets / slice_count;

            let mut slices = Vec::from_iter(
                std::iter::repeat_with(|| {
                    Vec::from_iter(std::iter::repeat_with(Vec::new).take(set_per_slice))
                })
                .take(slice_count),
            );

            for (set_idx, set) in directory.iter().enumerate() {
                assert!(set.len() <= associativity);

                let slice_idx = set_idx % slice_count;
                let new_set_index = set_idx / slice_count;

                slices[slice_idx][new_set_index] = set
                    .iter()
                    .map(|entry| entry.to_flexus_directory_entry())
                    .collect();
            }

            slices
                .into_iter()
                .map(|slice| serde_json::to_value(slice).unwrap())
                .collect()
        }
    }
}

impl FlexusPrivateCacheCheckpointHelper {
    pub fn export(&self, folder_name: String) {
        // Export the caches.
        for (core_id, cache) in self.caches.iter().enumerate() {
            let icache_path = format!("{}/{:03}-ufetch-L1i.json", folder_name, core_id);
            std::fs::write(
                &icache_path,
                serde_json::to_string(&serialize_a_cache(
                    &cache.i_cache,
                    self.flexus_configuration.l1i_sets,
                    self.flexus_configuration.l1i_associativity,
                ))
                .unwrap(),
            )
            .unwrap();
            println!(
                "Core {}'s L1i cache is exported to {}",
                core_id, icache_path
            );

            let dcache_path = format!("{}/{:03}-L1d.json", folder_name, core_id);
            std::fs::write(
                &dcache_path,
                serde_json::to_string(&serialize_a_cache(
                    &cache.d_cache,
                    self.flexus_configuration.l1d_sets,
                    self.flexus_configuration.l1d_associativity,
                ))
                .unwrap(),
            )
            .unwrap();
            println!("Core {}'s L1d is exported to {}", core_id, dcache_path);
        }

        // Export the directory slices.
        let directory_slices = serialize_directory_slices(
            &self.directory,
            self.flexus_configuration.directory.clone(),
            self.flexus_configuration.directory_slice_count,
        );

        for slice_idx in 0..directory_slices.len() {
            let directory_path = format!("{}/{:03}-L2-dir-slice.json", folder_name, slice_idx);
            std::fs::write(
                &directory_path,
                serde_json::to_string(&directory_slices[slice_idx]).unwrap(),
            )
            .unwrap();
            println!(
                "Directory slice {} is exported to {}",
                slice_idx, directory_path
            );
        }

        // // Export the directory.
        // let directory_path = format!("{}/sys-L2-dir.json", folder_name);
        // std::fs::write(
        //     &directory_path,
        //     serde_json::to_string(&serialize_a_directory(
        //         &self.directory,
        //         self.flexus_configuration.directory.clone(),
        //     ))
        //     .unwrap(),
        // )
        // .unwrap();

        // println!("Directory is exported to {}", directory_path);
    }
}
