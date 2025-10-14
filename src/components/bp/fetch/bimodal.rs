#![allow(dead_code)]
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

use crate::components::bp::{BranchResolutionResult, BranchType};

use serde::{Deserialize, Serialize};

use super::BranchPredictorResult;

#[derive(Deserialize, Serialize)]
struct BimodalPredictor<const S: usize> {
    array: Vec<u8>,
    // 0, 1 -> NT
    // 2, 3 -> T
}

impl<const S: usize> BimodalPredictor<S> {
    pub fn new() -> BimodalPredictor<S> {
        BimodalPredictor {
            array: Vec::from_iter(std::iter::repeat(0).take(S)),
        }
    }

    fn saturaing_add(&mut self, index: usize) {
        let v = self.array[index];
        if v < 3 {
            self.array[index] = v + 1;
        }
    }

    fn saturating_sub(&mut self, index: usize) {
        let v = self.array[index];
        if v > 0 {
            self.array[index] = v - 1;
        }
    }

    // True for taken, false for not taken
    fn get_prediction(&self, index: usize) -> bool {
        self.array[index] >= 2
    }

    pub fn train(
        &mut self,
        pc: u64,
        result: BranchResolutionResult,
        _target: u64,
    ) -> BranchPredictorResult {
        if result.branch_type != BranchType::Conditional {
            return BranchPredictorResult::NotActive;
        }

        let index = (pc % S as u64) as usize;
        let prediction = self.get_prediction(index);
        if result.is_taken {
            self.saturaing_add(index);
        } else {
            self.saturating_sub(index)
        }

        if prediction == result.is_taken {
            BranchPredictorResult::Match
        } else {
            BranchPredictorResult::Mispredict
        }
    }

    pub fn serialize(&self) -> Vec<u8> {
        unimplemented!("BimodalPredictor::serialize")
    }
}
