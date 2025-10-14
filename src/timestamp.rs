// generate timestamp.

use std::io::{Read, Write};

use crate::util::get_monotonic_ts;

static mut QEMU_INITIALIZING_TIMESTAMP: u64 = 0;
static mut CHECKPOINT_TIMESTAMP: u64 = 0;


pub fn initialize() {
    unsafe {
        QEMU_INITIALIZING_TIMESTAMP = get_monotonic_ts();
    }
}

#[inline(always)]
pub fn get_ts() -> u64 {
    let current_ts = get_monotonic_ts();
    return current_ts - unsafe { QEMU_INITIALIZING_TIMESTAMP } + unsafe { CHECKPOINT_TIMESTAMP };
}


// Save the starting timestamp.
pub fn serialize(name: &str) {
    let current_ts = get_ts();
    // dump the current_ts to a file.
    let mut file = std::fs::File::create(format!("{}/timestamp", name)).unwrap();
    file.write_all(current_ts.to_string().as_bytes()).unwrap();
    file.flush().unwrap();    
}

// Load the starting timestamp.
pub fn deserialize(name: &str) {
    // load the timestamp from the file.
    // If the file does not exist, ignore and set the timestamp to 0.
    if !std::path::Path::new(&format!("{}/timestamp", name)).exists() {
        println!("Checkpoint timestamp file does not exist.");
        unsafe {
            CHECKPOINT_TIMESTAMP = 0;
        }
        return;
    }

    let mut file = std::fs::File::open(format!("{}/timestamp", name)).unwrap();
    let mut buffer = String::new();
    file.read_to_string(&mut buffer).unwrap();
    let current_ts = buffer.trim().parse::<u64>().unwrap();
    unsafe {
        CHECKPOINT_TIMESTAMP = current_ts;
    }
    // print the timestamp.
    println!("Checkpoint timestamp: {}", unsafe { CHECKPOINT_TIMESTAMP });
}

