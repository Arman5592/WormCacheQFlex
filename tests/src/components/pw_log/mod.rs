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

use rustc_hash::FxHashMap;

use super::Plugin;
use crate::arch::{aarch64, ISA};
use crate::qemu_api;
use std::ffi::{self, c_void};

unsafe extern "C" fn vcpu_mem_access(
    _cpu_idx: u32,
    info: qemu_api::qemu_plugin_meminfo_t,
    vaddr: u64,
    _: *mut ffi::c_void, // should be NULL.
) {
    unsafe {
        let hw_handler = qemu_api::qemu_plugin_get_hwaddr(info, vaddr);
        let is_device = qemu_api::qemu_plugin_hwaddr_is_io(hw_handler);

        if !is_device {
            // let is_store = qemu_api::qemu_plugin_mem_is_store(info);
            let paddr = qemu_api::qemu_plugin_hwaddr_phys_addr(hw_handler) as usize;

            let traces = std::slice::from_raw_parts(
                qemu_api::qemu_plugin_hwaddr_translate_walk_trace(hw_handler),
                4,
            );

            // println!("Walk trace for {:x}: ", paddr);

            for t in traces.iter() {
                if *t == u64::MAX {
                    break;
                }
                let mut buf: u64 = 0;
                qemu_api::qemu_plugin_read_physical_memory(
                    *t,
                    8,
                    &mut buf as *mut u64 as *mut c_void,
                );
                // println!("- {:x} -> {:x}", *t, buf);
            }

            let va = vaddr;
            // let is_kernel = (va & 0xFFFF000000000000) != 0;
            let _ttbr0 = qemu_api::qemu_plugin_read_ttbr_el1(0);
            let _ttbr1 = qemu_api::qemu_plugin_read_ttbr_el1(1);
            // let ttbr = qemu_api::qemu_plugin_read_ttbr_el1(if is_kernel { 1 } else { 0 });
            // let tcr = qemu_api::qemu_plugin_read_tcr_el1();


            let res = aarch64::AArch64::ptw(va);

            assert!(
                res.paddr == paddr as u64,
                "The physical address is not matched! {:x} vs {:x}",
                res.paddr,
                paddr
            );
            assert!(res.traces == traces, "The traces are not matched!");
        } else {
            // TODO: check the I/O event
        }
    }
}

unsafe extern "C" fn vcpu_insn_exec(
    _: u32,
    _va: *mut ffi::c_void, // it is basically its physical address.
) {
    // let va = va as u64;
    // let is_kernel = (va & 0xFFFF000000000000) != 0;
    // let ttbr0 = qemu_api::qemu_plugin_read_ttbr_el1(0);
    // let ttbr1 = qemu_api::qemu_plugin_read_ttbr_el1(1);
    // let ttbr = qemu_api::qemu_plugin_read_ttbr_el1(if is_kernel { 1 } else { 0 });
    // let tcr = qemu_api::qemu_plugin_read_tcr_el1();

    // println!("Instruction at {:x}, TTBR0: {:x}, TTBR1: {:x}, selected TTBR: {:x}, TCR: {:x}", va, ttbr0, ttbr1, ttbr, tcr);

    // aarch64::ptw(ttbr, tcr, va, paddr_reader);
}

pub struct PageWalkLoggerPlugin {}

impl Plugin for PageWalkLoggerPlugin {
    fn init(_plugin: u64, _options: &FxHashMap<String, String>) {
        println!("PageWalkLoggerPlugin initialized.");
    }

    unsafe fn on_translation(tb: *mut crate::qemu_api::qemu_plugin_tb) {
        unsafe {
            let n_instruction = qemu_api::qemu_plugin_tb_n_insns(tb);

            if n_instruction == 0 {
                return;
            }

            let mut block_id = vec![];
            for i in 0..n_instruction {
                let inst = qemu_api::qemu_plugin_tb_get_insn(tb, i);
                block_id.push(
                    qemu_api::qemu_plugin_insn_haddr(inst) as usize
                        >> crate::parameter::CACHE_LINE_SIZE.trailing_zeros(),
                );
            }

            let fb_info = crate::util::find_fetch_block_from_block_id_sequence(block_id);

            // bind the instruction call back.
            for (idx, _) in fb_info.into_iter() {
                let i = qemu_api::qemu_plugin_tb_get_insn(tb, idx);
                qemu_api::qemu_plugin_register_vcpu_insn_exec_cb(
                    i,
                    Some(vcpu_insn_exec),
                    qemu_api::qemu_plugin_cb_flags_QEMU_PLUGIN_CB_NO_REGS,
                    qemu_api::qemu_plugin_insn_vaddr(i) as *mut ffi::c_void,
                );
            }

            // bind the memory callback.
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
