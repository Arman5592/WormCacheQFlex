use super::FlexusParameter;
use serde_json;
pub use shared_cache::SingleSharedCacheSerdeHelper;
use zstd::stream::read::Decoder;

use crate::components::cache_hierarchy::common::{
    HarvardPerCorePrivateCacheSerdeHelper, SharedCacheAccessSource,
    UnifiedPerCorePrivateCacheSerdeHelper,
};

mod private_cache;
mod shared_cache;

pub fn process_cache_hierarchy(
    checkpoint_folder: &String,
    flexus_configuration: &FlexusParameter,
    output_folder: &String,
) {
    let harvard_cache_checkpoints = std::fs::read_dir(checkpoint_folder)
        .unwrap()
        .filter_map(|entry| {
            let entry = entry.unwrap();
            let file_name = entry.file_name().into_string().unwrap();
            if file_name.contains("harvard") && file_name.ends_with(".json.zstd") {
                Some(file_name)
            } else {
                None
            }
        })
        .collect::<Vec<String>>();

    let unified_cache_checkpoints = std::fs::read_dir(checkpoint_folder)
        .unwrap()
        .filter_map(|entry| {
            let entry = entry.unwrap();
            let file_name = entry.file_name().into_string().unwrap();
            if file_name.contains("unified") && file_name.ends_with(".json.zstd") {
                Some(file_name)
            } else {
                None
            }
        })
        .collect::<Vec<String>>();

    let is_harvard_cache = !harvard_cache_checkpoints.is_empty();

    let is_unified_cache = !unified_cache_checkpoints.is_empty();

    assert!((is_harvard_cache || is_unified_cache) && !(is_harvard_cache && is_unified_cache));

    let private_cache = if is_harvard_cache {
        assert!(harvard_cache_checkpoints.len() == 1); // only one harvard cache is reserved.
        println!(
            "Harvard cache is detected. Filename: {}",
            harvard_cache_checkpoints[0]
        );
        let file = std::fs::File::open(format!(
            "{}/{}",
            checkpoint_folder, harvard_cache_checkpoints[0]
        ))
        .unwrap();

        let decoder = Decoder::new(file).unwrap();

        let cache: Vec<HarvardPerCorePrivateCacheSerdeHelper> =
            serde_json::from_reader(decoder).unwrap();

        private_cache::FlexusPrivateCacheCheckpointHelper::from_harvard_caches(
            cache,
            flexus_configuration,
        )
    } else {
        assert!(unified_cache_checkpoints.len() == 1); // only one unified cache is reserved.
        println!(
            "Unified cache is detected. Filename: {}",
            unified_cache_checkpoints[0]
        );
        let file = std::fs::File::open(format!(
            "{}/{}",
            checkpoint_folder, unified_cache_checkpoints[0]
        ))
        .unwrap();

        let decoder = Decoder::new(file).unwrap();

        let cache: Vec<UnifiedPerCorePrivateCacheSerdeHelper> =
            serde_json::from_reader(decoder).unwrap();

        private_cache::FlexusPrivateCacheCheckpointHelper::from_unified_caches(
            cache,
            flexus_configuration,
        )
    };

    private_cache.export(output_folder.clone());

    // Now, we need to process the shared cache.
    let shared_cache_checkpoints = std::fs::read_dir(checkpoint_folder)
        .unwrap()
        .filter_map(|entry| {
            let entry = entry.unwrap();
            let file_name = entry.file_name().into_string().unwrap();
            if file_name.contains("json.zstd") && file_name.contains("llc") {
                Some(file_name)
            } else {
                None
            }
        })
        .collect::<Vec<String>>();

    assert!(shared_cache_checkpoints.len() == 1); // only one shared cache is reserved.
    println!(
        "Shared cache is detected. Filename: {}",
        shared_cache_checkpoints[0]
    );

    let file = std::fs::File::open(format!(
        "{}/{}",
        checkpoint_folder, shared_cache_checkpoints[0]
    ))
    .unwrap();

    let decoder = Decoder::new(file).unwrap();

    let mut shared_cache: SingleSharedCacheSerdeHelper = serde_json::from_reader(decoder).unwrap();

    if flexus_configuration.no_resizing {
        assert!(private_cache.get_evicted_lines().is_empty());
    }

    for (_, (line, accessor)) in private_cache.get_evicted_lines().iter() {
        shared_cache.process_evicted_cache_line(line, SharedCacheAccessSource::Core(*accessor));
    }

    shared_cache.resize(flexus_configuration);

    shared_cache.export_slices(output_folder, flexus_configuration);
}
