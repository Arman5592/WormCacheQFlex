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
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::sync::LazyLock;

use crate::qemu_api;
// use crate::util::get_monotonic_ts;
use crate::parameter;
// use dashmap::DashMap;
use rustc_hash::FxHashMap;

const PAGE_SIZE: usize = 4096; // 4KB pages
const DIRTY_BYTE: u8 = 0xFF; // 11111111 in binary
const MAX_RAM_SIZE_GB: usize = 64;
// Calculate total pages for 64GB RAM
// 64 * 1024 * 1024 * 1024 / 4096 = 16,777,216 pages
const MAX_PAGES: usize = (MAX_RAM_SIZE_GB * 1024 * 1024 * 1024) / PAGE_SIZE;

// Encapsulated tracker for a single time domain (Host or Guest)
struct DirtyPageSet {
    // Active buffer index (0 or 1) - atomically swapped every period
    active_buffer: AtomicUsize,
    // Double-buffered sparse bytemaps
    buffer_0: Vec<AtomicU8>,
    buffer_1: Vec<AtomicU8>,
    // Count for the last completed period
    period_count: AtomicUsize,
}

impl DirtyPageSet {
    fn new() -> Self {
        Self {
            active_buffer: AtomicUsize::new(0),
            buffer_0: (0..MAX_PAGES).map(|_| AtomicU8::new(0)).collect(),
            buffer_1: (0..MAX_PAGES).map(|_| AtomicU8::new(0)).collect(),
            period_count: AtomicUsize::new(0),
        }
    }

    #[inline]
    fn mark_dirty(&self, page_num: usize) {
        if page_num < MAX_PAGES {
            let active_idx = self.active_buffer.load(Ordering::Relaxed);
            
            if active_idx == 0 {
                self.buffer_0[page_num].store(DIRTY_BYTE, Ordering::Relaxed);
            } else {
                self.buffer_1[page_num].store(DIRTY_BYTE, Ordering::Relaxed);
            }
        }
    }

    fn swap_and_count(&self) -> usize {
        // Atomically swap the active buffer (0 <-> 1)
        let old_active = self.active_buffer.fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
            Some(1 - current)
        }).unwrap_or_else(|x| x);
        
        let buffer = if old_active == 0 {
            &self.buffer_0
        } else {
            &self.buffer_1
        };

        let mut dirty_count = 0;
        for byte in buffer.iter() {
            if byte.load(Ordering::Relaxed) != 0 {
                dirty_count += 1;
                byte.store(0, Ordering::Relaxed);
            }
        }
        
        self.period_count.store(dirty_count, Ordering::Release);
        dirty_count
    }

    fn get_count(&self) -> usize {
        self.period_count.load(Ordering::Acquire)
    }
}

// Instantiate two independent trackers
static HOST_TRACKER: LazyLock<DirtyPageSet> = LazyLock::new(DirtyPageSet::new);
static GUEST_TRACKER: LazyLock<DirtyPageSet> = LazyLock::new(DirtyPageSet::new);

// Wrapper for existing statistics API (Host Time)
pub fn get_host_dirty_page_count() -> usize {
    HOST_TRACKER.get_count()
}

pub fn swap_host_buffers() -> usize {
    HOST_TRACKER.swap_and_count()
}

// New API for Guest Time
pub fn get_guest_dirty_page_count() -> usize {
    GUEST_TRACKER.get_count()
}

pub fn swap_guest_buffers() -> usize {
    GUEST_TRACKER.swap_and_count()
}

unsafe extern "C" fn vcpu_mem_access(
    vcpu_idx: u32,
    info: qemu_api::qemu_plugin_meminfo_t,
    vaddr: u64,
    _: *mut ffi::c_void,
) {
    unsafe {
        if parameter::MEASURE_HALF_OF_CORES && vcpu_idx >= parameter::CORE_COUNT as u32 / 2 {
            return;
        }

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
            let page_num = (pa as usize) / PAGE_SIZE;

            // Update both trackers independently
            HOST_TRACKER.mark_dirty(page_num);
            GUEST_TRACKER.mark_dirty(page_num);
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
        // Note: Dirty page counts are logged to dirty_pages_and_cache_misses.csv via the statistics thread
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

