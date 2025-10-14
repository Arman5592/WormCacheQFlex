use serde_json;

use worm_cache::checkpoint::FlexusParameter;
use worm_cache::checkpoint::process_cache_hierarchy;
use worm_cache::checkpoint::process_frontend;
use worm_cache::checkpoint::process_mmus;

fn main() {
    let args = std::env::args().collect::<Vec<String>>();

    // usage: <pf_checkpoint_folder> <flexus_configuration> <output_folder>
    if args.len() != 4 && args.len() != 5 {
        println!(
            "Usage: {} <pf_checkpoint_folder> <flexus_configuration> <output_folder> <resizing=true>",
            args[0]
        );
        std::process::exit(1);
    }

    // check the folder of the checkpoint.
    // If there are files with name like "(.*)-harvard.json.zstd", it is a harvard cache.

    let check_point_folder = &args[1];
    let flexus_file = std::fs::File::open(&args[2]).unwrap();
    let mut flexus: FlexusParameter = serde_json::from_reader(flexus_file).unwrap();
    let output_folder = &args[3];
    let enable_resizing = if args.len() == 5 {
        args[4].parse::<bool>().unwrap()
    } else {
        true
    };

    flexus.no_resizing = !enable_resizing;

    process_cache_hierarchy(check_point_folder, &flexus, output_folder);
    process_frontend(check_point_folder, &flexus, output_folder);
    process_mmus(check_point_folder, &flexus, output_folder);
}
