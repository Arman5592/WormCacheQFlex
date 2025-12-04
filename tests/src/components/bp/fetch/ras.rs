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

use std::collections::LinkedList;

use crate::components::bp::BranchResolutionResult;
use serde::{Deserialize, Serialize};

use super::BranchPredictorResult;

#[derive(Deserialize, Serialize)]
pub struct ReturnAddressStacle<const S: usize> {
    stack: LinkedList<u64>,
}

impl<const S: usize> ReturnAddressStacle<S> {
    pub fn new() -> ReturnAddressStacle<S> {
        ReturnAddressStacle {
            stack: LinkedList::new(),
        }
    }

    pub fn train(
        &mut self,
        pc: u64,
        result: BranchResolutionResult,
        target: u64,
    ) -> BranchPredictorResult {
        if !result.branch_type.is_call() && !result.branch_type.is_return() {
            return BranchPredictorResult::NotActive;
        }
        if result.branch_type.is_call() {
            self.push_and_evict(pc + 4);
            return BranchPredictorResult::NotActive;
        } else if result.branch_type.is_return() {
            let miss = self.stack.back() != Some(&target);
            self.stack.pop_back();
            return if miss {
                BranchPredictorResult::Mispredict
            } else {
                BranchPredictorResult::Match
            };
        }

        BranchPredictorResult::NotActive
    }

    fn push_and_evict(&mut self, pc: u64) {
        if self.stack.len() == S {
            self.stack.pop_front();
        }
        self.stack.push_back(pc);
    }
}
