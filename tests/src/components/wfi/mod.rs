use std::ffi;

use crate::qemu_api;

use crate::debug::statistics::*;

unsafe extern "C" fn vcpu_exec_wfi(vcpu_idx: u32, _: *mut ffi::c_void) {
    Statistics::global_record(vcpu_idx, EventType::WaitForInterrupt, true);
}

pub struct WaitForInterruptCounterPlugin {}

impl super::Plugin for WaitForInterruptCounterPlugin {
    fn init(_plugin_id: u64, _options: &rustc_hash::FxHashMap<String, String>) {}

    unsafe fn on_translation(tb: *mut qemu_api::qemu_plugin_tb) {
        unsafe {
            let n_instruction = qemu_api::qemu_plugin_tb_n_insns(tb);

            // go over all instructions and check which one is WFI.

            for i in 0..n_instruction {
                let inst = qemu_api::qemu_plugin_tb_get_insn(tb, i);
                let literal = qemu_api::qemu_plugin_insn_data(inst) as *const u32;
                let literal = *literal;

                if literal == 0b_1101_0101_0000_0011_0010_0000_0111_1111 {
                    qemu_api::qemu_plugin_register_vcpu_insn_exec_cb(
                        inst,
                        Some(vcpu_exec_wfi),
                        qemu_api::qemu_plugin_cb_flags_QEMU_PLUGIN_CB_NO_REGS,
                        std::ptr::null_mut(),
                    );
                }
            }
        }
    }

    fn serialize(_name: &str) {}

    fn deserialize(_name: &str) {}
}
