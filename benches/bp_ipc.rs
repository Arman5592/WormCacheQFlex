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

use worm_cache::components::bp::{BranchResolutionResult, BranchType, fetch::FetchUnit};

use rand::prelude::*;

use divan;

type FetchUnitType = FetchUnit<1>;

/*
 * What to test?
 *
 * There are three components in the FetchUnit:
 * 1. btb: btb::BTB<{ parameter::BTB_SET }, { parameter::BTB_ASSO }>
 * 2. ras: ras::ReturnAddressStacle<BP_RAS_COUNT>
 * 3. tage: tage::TAGEPredictor
 *
 * BTB is similar to a cache, so its extreme case is either a hit or a miss.
 * RAS should have the same speed whenever it is a hit or a miss.
 * TAGE has two branches inside:
 * - Whether its prediction is correct (it requires an allocation, allocation)
 * - Whether the prediction is from the btable or the gtable
 *
 *  It is really hard to
 *
*/

#[divan::bench]
fn all_hits_btb() {
    let mut fetch_unit = FetchUnitType::new();
    let indirect_branch_result = BranchResolutionResult {
        branch_type: BranchType::IndirectBranch,
        is_taken: true,
    };

    fetch_unit.train(0, 4, indirect_branch_result, 8);

    for _ in 0..(1000 * 1000) {
        fetch_unit.train(0, 4, indirect_branch_result, 8);
    }
}

#[divan::bench]
fn all_misses_btb_same_branch() {
    let mut fetch_unit = FetchUnitType::new();
    let indirect_branch_result = BranchResolutionResult {
        branch_type: BranchType::IndirectBranch,
        is_taken: true,
    };

    fetch_unit.train(0, 4, indirect_branch_result, 8);

    for idx in 0..(1000) {
        fetch_unit.train(0, 4, indirect_branch_result, 8 + idx * 4);
    }
}

#[divan::bench]
fn all_misses_btb_different_branches() {
    // This test can trigger the slow path of the BTB due to the eviction.
    let mut fetch_unit = FetchUnitType::new();
    let indirect_branch_result = BranchResolutionResult {
        branch_type: BranchType::IndirectBranch,
        is_taken: true,
    };

    for pc in 0..(1000) {
        let pc = pc * worm_cache::parameter::BTB_SET as u64 + 8;
        fetch_unit.train(0, pc, indirect_branch_result, pc * 4);
    }
}

#[divan::bench]
fn exercise_ras_best() {
    let mut fetch_unit = FetchUnitType::new();
    // it should be a pair of call and the return.
    let caller_pc = 0x4;
    let callee_pc = 0x0;

    let call_result = BranchResolutionResult {
        branch_type: BranchType::DirectCall,
        is_taken: true,
    };

    let return_result = BranchResolutionResult {
        branch_type: BranchType::Return,
        is_taken: true,
    };

    for idx in 0..(1000) {
        let is_call = idx % 2 == 0;
        if is_call {
            fetch_unit.train(0, caller_pc, call_result, callee_pc);
        } else {
            fetch_unit.train(0, callee_pc, return_result, caller_pc + 4);
        }
    }
}

#[divan::bench]
fn tage_best_case() {
    let mut fetch_unit = FetchUnitType::new();

    let taken = BranchResolutionResult {
        branch_type: BranchType::Conditional,
        is_taken: true,
    };

    for _ in 0..(100 * 1000) {
        fetch_unit.train(0, 4, taken, 16);
    }
}

#[divan::bench]
fn tage_worst_case() {
    // generate 1000 branches, and their directions are random.
    let branches = (0..100 * 100)
        .map(|_| {
            let pc = random::<u64>();
            let direction = random::<bool>();
            (pc, direction)
        })
        .collect::<Vec<_>>();

    let mut fetch_unit = FetchUnitType::new();

    let taken = BranchResolutionResult {
        branch_type: BranchType::Conditional,
        is_taken: true,
    };

    let not_taken = BranchResolutionResult {
        branch_type: BranchType::Conditional,
        is_taken: false,
    };

    for (pc, direction) in branches.iter() {
        fetch_unit.train(0, *pc, if *direction { taken } else { not_taken }, 0);
    }
}

fn main() {
    divan::main();
}
