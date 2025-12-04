use core::ffi;

use crate::{parameter::CORE_COUNT, qemu_api};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;

#[serde_as]
#[derive(Serialize, Deserialize)]
pub struct InstructionFrequency {
    #[serde_as(as = "[_; CORE_COUNT]")]
    pub frequencies: [FxHashMap<u64, u64>; CORE_COUNT], // PC -> frequency
}

static mut PLUGIN: *mut InstructionFrequency = std::ptr::null_mut();

unsafe extern "C" fn vcpu_insn_exec(vcpu_idx: u32, inst_virtual_addr: *mut ffi::c_void) {
    let vpn = unsafe { qemu_api::qemu_plugin_read_pc_vpn() };
    let vaddr = vpn << 12 | (inst_virtual_addr as u64 & 0xfff);

    let plugin = unsafe { &mut *PLUGIN };
    let freq = plugin.frequencies[vcpu_idx as usize]
        .entry(vaddr)
        .or_insert(0);
    *freq += 1;
}

pub struct InstructionFrequencyPlugin {}

impl super::super::Plugin for InstructionFrequencyPlugin {
    fn init(_plugin_id: u64, _options: &FxHashMap<String, String>) {
        let plugin = InstructionFrequency {
            frequencies: std::array::from_fn(|_| FxHashMap::default()),
        };

        unsafe {
            PLUGIN = Box::into_raw(Box::new(plugin));
        }

        println!("InstructionFrequencyPlugin initialized");
    }

    unsafe fn on_translation(tb: *mut qemu_api::qemu_plugin_tb) {
        unsafe {
            let n_instruction = qemu_api::qemu_plugin_tb_n_insns(tb);

            if n_instruction == 0 {
                return;
            }

            for i in 0..n_instruction {
                let insn = qemu_api::qemu_plugin_tb_get_insn(tb, i);
                let insn_addr = qemu_api::qemu_plugin_insn_vaddr(insn);

                qemu_api::qemu_plugin_register_vcpu_insn_exec_cb(
                    insn,
                    Some(vcpu_insn_exec),
                    qemu_api::qemu_plugin_cb_flags_QEMU_PLUGIN_CB_NO_REGS,
                    insn_addr as *mut ffi::c_void,
                );
            }
        }
    }

    fn serialize(name: &str) {
        let file = std::fs::File::create(format!("{}/inst_frequency.json.zstd", name)).unwrap();
        let mut file = zstd::Encoder::new(file, 0).unwrap();

        let plugin = unsafe { &*PLUGIN };

        serde_json::to_writer(&mut file, plugin).unwrap();

        file.finish().unwrap();
    }

    fn deserialize(_name: &str) {}
}
