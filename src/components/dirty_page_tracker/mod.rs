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

use std::ffi;
use std::fs::File;
use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::sync::LazyLock;

use crate::qemu_api;
use crate::util::get_monotonic_ts;
use crate::parameter;
use dashmap::DashMap;
use rustc_hash::FxHashMap;

const PAGE_SIZE: usize = 4096; // 4KB pages
const DIRTY_BYTE: u8 = 0xFF; // 11111111 in binary

// Double-buffered sparse bytemaps for lock-free concurrent access
// Using DashMap for lock-free concurrent access - safe since we only ever set to 0xFF and never reset
// Active buffer index (0 or 1) - atomically swapped every period
static ACTIVE_BUFFER: AtomicUsize = AtomicUsize::new(0);
static DIRTY_PAGE_BYTEMAP_0: LazyLock<DashMap<u64, u8>> = LazyLock::new(|| {
    DashMap::new()
});
static DIRTY_PAGE_BYTEMAP_1: LazyLock<DashMap<u64, u8>> = LazyLock::new(|| {
    DashMap::new()
});

// Current period's dirty page count (pages dirtied in the current 10s period)
// This is updated when buffers are swapped and cleared
static CURRENT_PERIOD_COUNT: AtomicUsize = AtomicUsize::new(0);

static LOG_FILE: LazyLock<Mutex<Option<File>>> = LazyLock::new(|| Mutex::new(None));

unsafe extern "C" fn vcpu_mem_access(
    _vcpu_idx: u32,
    info: qemu_api::qemu_plugin_meminfo_t,
    vaddr: u64,
    _: *mut ffi::c_void,
) {
    unsafe {
        // Early return if feature is disabled
        if !parameter::ENABLE_DIRTY_PAGE_TRACKER {
            return;
        }

        // Only track stores (writes)
        if !qemu_api::qemu_plugin_mem_is_store(info) {
            return;
        }

        let hw_handler = qemu_api::qemu_plugin_get_hwaddr(info, vaddr);
        let is_device = qemu_api::qemu_plugin_hwaddr_is_io(hw_handler);

        if !is_device {
            let pa = qemu_api::qemu_plugin_hwaddr_phys_addr(hw_handler);
            let page_num = pa >> 12; // Divide by PAGE_SIZE (4096)

            // Get the active buffer index atomically (no mutex needed for reads)
            let active_idx = ACTIVE_BUFFER.load(Ordering::Acquire);
            
            // Set the byte for this page to 0xFF (dirty) in the active buffer
            // DashMap is lock-free and safe for concurrent access since we only ever set to 0xFF
            if active_idx == 0 {
                DIRTY_PAGE_BYTEMAP_0.insert(page_num, DIRTY_BYTE);
            } else {
                DIRTY_PAGE_BYTEMAP_1.insert(page_num, DIRTY_BYTE);
            }
        }
    }
}

/// Get the number of pages dirtied in the current period (per 10s, matching statistics.csv interval)
/// This should be called when statistics are written to ensure we get the count for the period that just ended
pub fn get_dirty_page_count() -> usize {
    // Return the count for the current period (updated when buffers are swapped)
    CURRENT_PERIOD_COUNT.load(Ordering::Acquire)
}

/// Swap buffers and update the count - call this when statistics are written to ensure synchronization
/// Returns the number of pages dirtied in the period that just ended
pub fn swap_buffers_for_statistics() -> usize {
    swap_and_count_buffers()
}

fn swap_and_count_buffers() -> usize {
    // Atomically swap the active buffer (0 <-> 1)
    // Use fetch_update to atomically toggle between 0 and 1
    let old_active = ACTIVE_BUFFER.fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
        Some(1 - current)
    }).unwrap_or_else(|x| x);
    
    // Count dirty pages in the buffer that was just swapped out (now inactive)
    // After the swap, all new writes go to the new buffer, so old_active is safe to read
    let dirty_count = if old_active == 0 {
        DIRTY_PAGE_BYTEMAP_0.len()
    } else {
        DIRTY_PAGE_BYTEMAP_1.len()
    };
    
    // Clear the buffer that was just counted (prepare it for next period)
    // This is safe because it's now inactive, so no writes are happening to it
    if old_active == 0 {
        DIRTY_PAGE_BYTEMAP_0.clear();
    } else {
        DIRTY_PAGE_BYTEMAP_1.clear();
    }
    
    // Update the current period count atomically
    CURRENT_PERIOD_COUNT.store(dirty_count, Ordering::Release);
    
    dirty_count
}

fn log_dirty_pages() {
    // Swap buffers and get the count for the period that just ended
    let dirty_count = swap_and_count_buffers();
    let timestamp = get_monotonic_ts();

    // Log the count for this period (optional - main output is now in statistics.csv)
    if let Ok(mut file_guard) = LOG_FILE.lock() {
        if let Some(file) = file_guard.as_mut() {
            if let Err(e) = file.write_fmt(format_args!("{},{}\n", timestamp, dirty_count)) {
                eprintln!("Error writing to dirty page log: {}", e);
            }
            if let Err(e) = file.flush() {
                eprintln!("Error flushing dirty page log: {}", e);
            }
        }
    }
}

pub struct DirtyPageTrackerPlugin {}

impl super::Plugin for DirtyPageTrackerPlugin {
    #[inline]
    fn init(_plugin_id: u64, _options: &FxHashMap<String, String>) {
        if !parameter::ENABLE_DIRTY_PAGE_TRACKER {
            return;
        }

        println!("Dirty page tracker plugin initialized.");

        // Initialize the log file
        let file_path = "/mnt/ssd4t/home/arman/qflex/m/dirty_pages.csv";
        let file = File::create(file_path).expect(&format!("Failed to create {}", file_path));
        *LOG_FILE.lock().unwrap() = Some(file);

        // Write CSV header
        if let Ok(mut file_guard) = LOG_FILE.lock() {
            if let Some(file) = file_guard.as_mut() {
                file.write_all(b"timestamp,dirty_page_count\n")
                    .expect("Failed to write CSV header");
            }
        }

        // Spawn a thread to periodically swap buffers and log dirty page count
        // Note: The main swap happens when statistics are written (every 10s) via swap_buffers_for_statistics()
        // This thread is just for the backup log file, and will swap less frequently to avoid double-swapping
        // We'll swap every 10 seconds but the statistics thread will handle the actual swap
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(std::time::Duration::from_secs(10));
                // Just log the current count without swapping (swap is handled by statistics thread)
                let dirty_count = get_dirty_page_count();
                let timestamp = get_monotonic_ts();
                if let Ok(mut file_guard) = LOG_FILE.lock() {
                    if let Some(file) = file_guard.as_mut() {
                        if let Err(e) = file.write_fmt(format_args!("{},{}\n", timestamp, dirty_count)) {
                            eprintln!("Error writing to dirty page log: {}", e);
                        }
                        if let Err(e) = file.flush() {
                            eprintln!("Error flushing dirty page log: {}", e);
                        }
                    }
                }
            }
        });
    }

    #[inline]
    unsafe fn on_translation(tb: *mut crate::qemu_api::qemu_plugin_tb) {
        if !parameter::ENABLE_DIRTY_PAGE_TRACKER {
            return;
        }

        unsafe {
            let n_instruction = qemu_api::qemu_plugin_tb_n_insns(tb);

            if n_instruction == 0 {
                return;
            }

            // Register memory callback for all instructions (we filter stores in the callback)
            for i in 0..n_instruction {
                let inst = qemu_api::qemu_plugin_tb_get_insn(tb, i);
                qemu_api::qemu_plugin_register_vcpu_mem_cb(
                    inst,
                    Some(vcpu_mem_access),
                    qemu_api::qemu_plugin_cb_flags_QEMU_PLUGIN_CB_NO_REGS,
                    qemu_api::qemu_plugin_mem_rw_QEMU_PLUGIN_MEM_RW,
                    std::ptr::null_mut(),
                );
            }
        }
    }

    fn serialize(_: &str) {}

    fn deserialize(_: &str) {}
}

