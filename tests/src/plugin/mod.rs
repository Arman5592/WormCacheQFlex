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

pub mod qemu_wrapper;

// Re-export the QEMU API
pub use qemu_wrapper::QEMUPluginBasicBlock;
pub use qemu_wrapper::QEMUPluginBasicBlockIterator;
pub use qemu_wrapper::QEMUPluginInstruction;
pub use qemu_wrapper::QEMUMemoryInfo;


#[derive(Debug)]
pub struct PerInstructionInstrumentation {
    pub instruction_execution: Option<*mut std::ffi::c_void>,
    pub memory_access: Option<*mut std::ffi::c_void>
}

pub unsafe trait QEMUPluginPerCoreActor {

    type PluginType: QEMUPlugin<PerCorePlugin = Self>;

    unsafe fn on_instruction_execution(
        &mut self, 
        cpu_idx: u32, 
        user_data: *mut ffi::c_void
    );
    
    unsafe fn on_memory_access(
        &mut self,
        cpu_idx: u32,
        info: &QEMUMemoryInfo,
        vaddr: u64,
        user_data: *mut ffi::c_void
    );
}

// This one should have access to the Quantum server for synchronization and private data submission.
pub unsafe trait QEMUPlugin {

    type PerCorePlugin: QEMUPluginPerCoreActor;

    // Please use concurrency hashmap if possible.
    unsafe fn on_translation(&self, tb: &QEMUPluginBasicBlock)
        -> Vec<PerInstructionInstrumentation>;
    
    unsafe fn on_qemu_exit(&self);
}

