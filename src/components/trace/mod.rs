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

use std::{
    ffi,
    fs::File,
    io::Write,
    process::exit,
    sync::atomic::{AtomicU64, Ordering},
};

use rustc_hash::FxHashMap;
use zstd::Encoder;

static mut TRACE_FILE: *mut Encoder<File> = std::ptr::null_mut();

static C0_COUNTER: AtomicU64 = AtomicU64::new(0);

use crate::qemu_api;

unsafe extern "C" fn vcpu_insn_exec(vcpu_idx: u32, host_va: *mut ffi::c_void) {
    unsafe {
        if vcpu_idx != 0 {
            return;
        }

        let host_va_u64 = host_va as u64;
        let pc = qemu_api::qemu_plugin_read_pc_vpn() << 12 | host_va_u64 & 0xfff;
        let instruction_literal = *(host_va as *mut u32);

        // write the instruction to the trace file.
        //writeln!(*TRACE_FILE, "{:x} {:x}\n", pc, instruction_literal).unwrap();
        (*TRACE_FILE)
            .write_all(&format!("i {:x} {:x}\n", pc, instruction_literal).into_bytes())
            .unwrap();

        // increment the counter.
        if C0_COUNTER.fetch_add(1, Ordering::Relaxed) == 20000000 {
            let owned_trace_file = Box::from_raw(TRACE_FILE);
            owned_trace_file.finish().unwrap();
            exit(0);
        }
    }
}

unsafe extern "C" fn _vcpu_mem_access(
    _vcpu_idx: u32,
    info: qemu_api::qemu_plugin_meminfo_t,
    vaddr: u64,
    _: *mut ffi::c_void, // should be NULL.
) {
    unsafe {
        let hw_handler = qemu_api::qemu_plugin_get_hwaddr(info, vaddr);
        let is_device = qemu_api::qemu_plugin_hwaddr_is_io(hw_handler);

        if !is_device {
            let _is_store = qemu_api::qemu_plugin_mem_is_store(info);
            let _paddr = qemu_api::qemu_plugin_hwaddr_phys_addr(hw_handler) as usize;
        } else {
            // TODO: check the I/O event
        }
    }
}

// This structure is just a wrapper of for the plugin system to register. Plugin is believed to be globally singleton.
pub struct TracePlugin {}

impl super::Plugin for TracePlugin {
    #[inline]
    fn init(_plugin: u64, _options: &FxHashMap<String, String>) {
        unsafe {
            TRACE_FILE = Box::into_raw(Box::new(
                Encoder::new(File::create("worm_cache.c0.trace.zst").unwrap(), 3).unwrap(),
            ));
        }
    }

    unsafe fn on_translation(tb: *mut crate::qemu_api::qemu_plugin_tb) {
        unsafe {
            let n_instruction = qemu_api::qemu_plugin_tb_n_insns(tb);

            if n_instruction == 0 {
                return;
            }

            // bind the memory callback.
            for i in 0..n_instruction {
                let inst = qemu_api::qemu_plugin_tb_get_insn(tb, i);
                let host_va_instruction = qemu_api::qemu_plugin_insn_haddr(inst);
                qemu_api::qemu_plugin_register_vcpu_insn_exec_cb(
                    inst,
                    Some(vcpu_insn_exec),
                    qemu_api::qemu_plugin_cb_flags_QEMU_PLUGIN_CB_NO_REGS,
                    host_va_instruction,
                );
                // qemu_api::qemu_plugin_register_vcpu_mem_cb(
                //     inst,
                //     Some(vcpu_mem_access),
                //     qemu_api::qemu_plugin_cb_flags_QEMU_PLUGIN_CB_NO_REGS,
                //     qemu_api::qemu_plugin_mem_rw_QEMU_PLUGIN_MEM_RW,
                //     std::ptr::null_mut(),
                // );
            }
        }
    }

    fn serialize(_: &str) {}

    fn deserialize(_: &str) {}
}
