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

use strum::{EnumCount, IntoEnumIterator};
use strum_macros::{Display, EnumCount, EnumIter};

use std::sync::LazyLock;
use std::{cell::UnsafeCell, io::Write};
use std::{ffi, thread};

use crate::parameter::{CORE_COUNT, ENABLE_STATISTICS};
use crate::qemu_api;
use crate::util::get_monotonic_ts;

#[derive(EnumCount, EnumIter, Display, Debug, Clone, Copy)]
pub enum EventType {
    Instruction,
    TargetLocalCycle,

    MemoryAccess,
    InstructionAccess,
    DataAccess,

    PrivateICacheMiss,
    PrivateDCacheMiss,
    PrivateCacheMiss,
    PrivateCacheMissDueToPTW,

    PrivateCacheMissTriggerCoherenceDueToFetch, // All misses that involve the coherence activity (GetS, GetX)
    PrivateCacheMissTriggerCoherenceDueToRead, // All misses that involve the coherence activity (GetS, GetX)
    PrivateCacheMissTriggerCoherenceDueToWrite, // All misses that involve the coherence activity (GetS, GetX)
    PrivateCacheMissTriggerInvalidation,        // All misses that invalid other copies (GetX)
    PrivateCacheInvalidation, // All invalidations that invalidate other copies (GetX)

    SharedCacheAccess,
    SharedCacheMiss,
    SharedCacheMissDueToPTW,
    SharedCacheMissDueToInstructionFetch,
    SharedCacheMissDueToDataRead,
    SharedCacheMissDueToDataWrite,

    SharedCacheColdMiss, // The cache miss is caused due to the cold start of the shared cache.

    PrivateCacheInvalidationCausailityViolation, // The cache line is invalidated by a access with a smaller timestamp.
    PrivateCacheDowngradeCausalityViolation, // The cache line is downgraded by a access with a smaller timestamp.
    SharedCacheAccessCausalityViolation, // The cache line (in LLC) is accessed by a access with a smaller timestamp.
    SharedCacheEvictionCausalityViolation, // The cache line (in LLC) is evicted by a access with a smaller timestamp.

    // the key problem is still how I convert the previous two counters' value into the miss rate impact.
    TLBMiss,
    TLBMissDueToInstruction,
    TLBMissDueToData,

    TLBAccess,
    TLBAccessDueToInstruction,
    TLBAccessDueToData,

    HugeTLBHit,
    HugeTLBHitDueToInstruction,
    HugeTLBHitDueToData,

    BranchCount,
    BTBMiss,
    RASMiss,
    TageMiss,
    BPMiss, // this is different from summing the previous one.
    // It includes the following logic to judge:
    // - For directional branch, it is a miss if the direction prediction is wrong, or
    // - For directional branch, it is a miss if the direction prediction is right but the target prediction is wrong.
    // - For indirect branch, it is a miss if the target prediction is wrong.
    // - For return, it is a miss if the target prediction (provided by the RAS) is wrong.
    WaitForInterrupt,
}

#[repr(align(64))]
struct PerCoreStatistics {
    counters: [u64; EventType::COUNT * 2],
}

impl PerCoreStatistics {
    pub fn new() -> Self {
        Self {
            counters: [0; EventType::COUNT * 2],
        }
    }

    #[inline]
    pub fn record(&mut self, event: EventType, is_os: bool) {
        self.record_by(event, is_os, 1);
    }

    #[inline]
    pub fn record_by(&mut self, event: EventType, is_os: bool, increment: u64) {
        if ENABLE_STATISTICS {
            let index = (event as usize) * 2;
            if is_os {
                self.counters[index + 1] += increment;
            } else {
                self.counters[index] += increment;
            }
        }
    }

    #[inline]
    pub fn set(&mut self, event: EventType, is_os: bool, value: u64) {
        if ENABLE_STATISTICS {
            let index = (event as usize) * 2;
            if is_os {
                self.counters[index + 1] = value;
            } else {
                self.counters[index] = value;
            }
        }
    }

    #[inline]
    pub fn get_line(&self, ts: u64, core_id: u32) -> String {
        let mut line = format!("{},{}", ts, core_id);
        for event in 0..EventType::COUNT {
            let u = self.counters[2 * event];
            let k = self.counters[2 * event + 1];
            line.push_str(&format!(",{}", u + k));
            line.push_str(&format!(",{}", u));
            line.push_str(&format!(",{}", k));
        }
        // Add dirty page count if enabled (same value for all cores since it's global)
        if crate::parameter::ENABLE_DIRTY_PAGE_TRACKER {
            let dirty_count = crate::components::dirty_page_tracker::get_host_dirty_page_count();
            line.push_str(&format!(",{}", dirty_count));
        }
        line
    }
}

pub struct Statistics {
    per_core: [UnsafeCell<PerCoreStatistics>; CORE_COUNT],
}

unsafe impl Sync for Statistics {}

impl Default for Statistics {
    fn default() -> Self {
        Self::new()
    }
}

impl Statistics {
    pub fn new() -> Self {
        Self {
            per_core: std::array::from_fn(|_| UnsafeCell::new(PerCoreStatistics::new())),
        }
    }

    #[inline]
    pub fn record(&self, core_id: u32, event: EventType, is_os: bool) {
        if ENABLE_STATISTICS {
            unsafe {
                (*self.per_core[core_id as usize].get()).record(event, is_os);
            }
        }
    }

    #[inline]
    pub fn record_by(&self, core_id: u32, event: EventType, is_os: bool, increment: u64) {
        if ENABLE_STATISTICS {
            unsafe {
                (*self.per_core[core_id as usize].get()).record_by(event, is_os, increment);
            }
        }
    }

    #[inline]
    pub fn set(&self, core_id: u32, event: EventType, is_os: bool, value: u64) {
        if ENABLE_STATISTICS {
            unsafe {
                (*self.per_core[core_id as usize].get()).set(event, is_os, value);
            }
        }
    }

    pub fn get_header() -> String {
        // generate all event names.
        let headers = EventType::iter()
            .map(|event| {
                let event_name = event.to_string();
                format!("{},{}:u,{}:k", event_name, event_name, event_name)
            })
            .collect::<Vec<String>>()
            .join(",");

        // Add dirty page count column if enabled
        let dirty_pages_header = if crate::parameter::ENABLE_DIRTY_PAGE_TRACKER {
            ",DirtyPages"
        } else {
            ""
        };

        format!("ts,core_id,{}{}", headers, dirty_pages_header)
    }

    pub fn get_line_for_all_cores(&self, ts: u64) -> Vec<String> {
        let mut lines = Vec::new();
        for core_id in 0..CORE_COUNT as u32 {
            unsafe {
                lines.push((*self.per_core[core_id as usize].get()).get_line(ts, core_id));
            }
        }
        lines
    }
}

static GLOBAL_STATISTICS: LazyLock<Statistics> = LazyLock::new(Statistics::new);

impl Statistics {
    #[inline]
    pub fn global_record(core_id: u32, event: EventType, is_os: bool) {
        GLOBAL_STATISTICS.record(core_id, event, is_os);
    }

    #[inline]
    pub fn global_record_by(core_id: u32, event: EventType, is_os: bool, increment: u64) {
        GLOBAL_STATISTICS.record_by(core_id, event, is_os, increment);
    }

    #[inline]
    pub fn global_set(core_id: u32, event: EventType, is_os: bool, value: u64) {
        GLOBAL_STATISTICS.set(core_id, event, is_os, value);
    }

    pub fn global_query_record(core_id: u32, event: EventType) -> (u64, u64, u64) {
        unsafe {
            let cnt = (*GLOBAL_STATISTICS.per_core[core_id as usize].get()).counters;
            let index = (event as usize) * 2;
            let u = cnt[index];
            let k = cnt[index + 1];
            (u + k, u, k)
        }
    }

    /// Get the sum of an event across all cores (total, user, kernel)
    pub fn global_query_record_all_cores(event: EventType) -> (u64, u64, u64) {
        let mut total = 0u64;
        let mut total_u = 0u64;
        let mut total_k = 0u64;
        for core_id in 0..CORE_COUNT {
            let (sum, u, k) = Statistics::global_query_record(core_id as u32, event);
            total += sum;
            total_u += u;
            total_k += k;
        }
        (total, total_u, total_k)
    }

    pub fn global_get_line_for_all_cores(ts: u64) -> Vec<String> {
        GLOBAL_STATISTICS.get_line_for_all_cores(ts)
    }

    pub fn global_one_line_statistics() -> String {
        let mut lines = vec![];
        lines.push(Self::get_header());

        for stat in Self::global_get_line_for_all_cores(0) {
            lines.push(stat);
        }

        lines.join("\n")
    }

    #[inline]
    pub fn save_to_csv(file_name: &str, ts: u64) {
        // save statistics.
        let mut file = std::fs::File::create(file_name).unwrap();
        // write header.
        file.write_fmt(format_args!("{}\n", Statistics::get_header()))
            .unwrap();
        // write content
        for stat in Statistics::global_get_line_for_all_cores(ts) {
            file.write_all(stat.as_bytes()).unwrap();
            file.write_all(b"\n").unwrap();
        }

        file.flush().unwrap();
        drop(file);
    }
}

pub fn create_thread_for_periodic_log() {
    thread::spawn(move || {
        let mut miss_file = std::fs::File::create("statistics.csv").unwrap();

        miss_file
            .write_fmt(format_args!("{}\n", Statistics::get_header()))
            .unwrap();

        // Host Time CSV: dirty pages and SharedCacheMissDueToDataWrite (every 10s Host Time)
        let mut host_summary_file: Option<std::fs::File> = if crate::parameter::ENABLE_DIRTY_PAGE_TRACKER {
            let mut file = std::fs::File::create("dirty_pages_and_cache_misses.csv").unwrap();
            file.write_all(b"timestamp,DirtyPages,SharedCacheMissDueToDataWrite\n").unwrap();
            Some(file)
        } else {
            None
        };

        // Guest Time CSV: dirty pages (every 10s Guest Time)
        let mut guest_summary_file: Option<std::fs::File> = if crate::parameter::ENABLE_DIRTY_PAGE_TRACKER {
            let mut file = std::fs::File::create("dirty_pages_guest.csv").unwrap();
            file.write_all(b"guest_timestamp_ns,DirtyPages,SharedCacheMissDueToDataWrite\n").unwrap();
            Some(file)
        } else {
            None
        };

        let mut next_host_log_time = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut next_guest_log_time_ns = unsafe { qemu_api::qemu_plugin_get_vcpu_vtime(0) } + 100_000_000;

        loop {
            // 1. Host Time Logging (Every 10s Real Time)
            if std::time::Instant::now() >= next_host_log_time {
                // Swap Host dirty page buffers
                let dirty_page_count: usize = if crate::parameter::ENABLE_DIRTY_PAGE_TRACKER {
                    crate::components::dirty_page_tracker::swap_host_buffers()
                } else {
                    0
                };

                // update the local target time before writing the statistics
                for core_id in 0..CORE_COUNT {
                    Statistics::global_set(
                        core_id as u32,
                        EventType::TargetLocalCycle,
                        false,
                        unsafe { qemu_api::qemu_plugin_get_vcpu_vtime(core_id as u32) },
                    );
                }

                let ts = get_monotonic_ts();
                // Write detailed statistics
                for stat in Statistics::global_get_line_for_all_cores(ts) {
                    miss_file.write_all(stat.as_bytes()).unwrap();
                    miss_file.write_all(b"\n").unwrap();
                }
                miss_file.flush().unwrap();

                // Write to Host summary CSV
                if let Some(ref mut file) = host_summary_file {
                    let (cache_misses_total, _, _) = Statistics::global_query_record_all_cores(
                        EventType::SharedCacheMissDueToDataWrite
                    );
                    file.write_fmt(format_args!("{},{},{}\n", ts, dirty_page_count, cache_misses_total))
                        .unwrap();
                    file.flush().unwrap();
                }

                next_host_log_time = std::time::Instant::now() + std::time::Duration::from_secs(10);
            }

            // 2. Guest Time Logging (Every 1s Virtual Time)
            if crate::parameter::ENABLE_DIRTY_PAGE_TRACKER {
                // Use vCP0 time as reference
                let current_guest_time_ns = unsafe { qemu_api::qemu_plugin_get_vcpu_vtime(0) };
                
                if current_guest_time_ns >= next_guest_log_time_ns {
                    let dirty_page_count = crate::components::dirty_page_tracker::swap_guest_buffers();
                    
                    if let Some(ref mut file) = guest_summary_file {
                        let (cache_misses_total, _, _) = Statistics::global_query_record_all_cores(
                            EventType::SharedCacheMissDueToDataWrite
                        );
                        file.write_fmt(format_args!(
                            "{},{},{}\n",
                            current_guest_time_ns, dirty_page_count, cache_misses_total
                        ))
                        .unwrap();
                        file.flush().unwrap();
                    }

                    // Advance target. If we are behind (e.g. after a jump), leap forward 
                    // to the next 1s boundary to avoid a "log storm".
                    while next_guest_log_time_ns <= current_guest_time_ns {
                        next_guest_log_time_ns += 100_000_000;
                    }
                }
            }

            // Sleep for 100ms to avoid busy waiting
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    });
}

unsafe extern "C" fn user_vcpu_insn_exec(
    vcpu_idx: u32,
    size: *mut ffi::c_void, // the size of the basic block
) {
    Statistics::global_record_by(vcpu_idx, EventType::Instruction, false, size as u64);
}

unsafe extern "C" fn kernel_vcpu_insn_exec(
    vcpu_idx: u32,
    size: *mut ffi::c_void, // the size of the basic block
) {
    Statistics::global_record_by(vcpu_idx, EventType::Instruction, true, size as u64);
}

pub unsafe extern "C" fn on_translation_instructions(tb: *mut qemu_api::qemu_plugin_tb) {
    unsafe {
        let first_instruction = qemu_api::qemu_plugin_tb_get_insn(tb, 0);
        let size = qemu_api::qemu_plugin_tb_n_insns(tb);
        // I need to get the first instruction's PC to see if it is a user or kernel space.
        let pc = qemu_api::qemu_plugin_insn_vaddr(first_instruction);
        if pc & 0x8000_0000_0000_0000 == 0 {
            // user space
            qemu_api::qemu_plugin_register_vcpu_insn_exec_cb(
                first_instruction,
                Some(user_vcpu_insn_exec),
                qemu_api::qemu_plugin_cb_flags_QEMU_PLUGIN_CB_NO_REGS,
                size as *mut ffi::c_void,
            );
        } else {
            qemu_api::qemu_plugin_register_vcpu_insn_exec_cb(
                first_instruction,
                Some(kernel_vcpu_insn_exec),
                qemu_api::qemu_plugin_cb_flags_QEMU_PLUGIN_CB_NO_REGS,
                size as *mut ffi::c_void,
            );
        }
    }
}
