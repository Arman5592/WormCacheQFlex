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

use spin::Spin;
use spin::mutex::SpinMutex;
use std::arch::asm;
use std::sync::Arc;
use std::time::Instant;
/// This test is critical to examine whether the timer is atomic and monotonic.
/// The high-level idea is very simple: There is a FIFO protected by a lock, and each thread acquires the lock, pushes the timestamp it reads, and releases the lock.
/// Finally the main threads pops the timestamps and checks whether they are monotonic.
use std::time::SystemTime;

fn normal_time_function() -> u64 {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap();
    now.as_nanos() as u64
}

// This function is only available on x86 and x86_64 platform.
#[cfg(target_arch = "x86_64")]
#[inline]
fn retsc_time_function() -> u64 {
    let time: u64;
    unsafe {
        asm!("rdtsc", out("rax") time);
    }
    time
}

#[test]
#[ignore]
#[cfg(target_arch = "x86_64")]
fn test_wiether_tsc_is_atomic() {
    let mut time_list: Vec<u64> = Vec::new();
    for _ in 0..1000 {
        time_list.push(retsc_time_function());
    }
    for i in 0..time_list.len() - 1 {
        assert!(time_list[i] < time_list[i + 1]);
        println!("{} {}", time_list[i], time_list[i + 1]);
    }
}

#[test]
#[ignore]
fn test_whether_timer_atomic() {
    // let me try whether the rdstic timer on the single thread is monotonic.

    const TOTAL_TEST_COUNT: usize = 1_000_000;
    const TOTAL_THREAD_COUNT: usize = 16;

    let time_list = Arc::new(SpinMutex::<Vec<u64>, Spin>::new(Vec::<u64>::with_capacity(
        TOTAL_TEST_COUNT * TOTAL_THREAD_COUNT,
    )));

    // spawn 16 threads
    let mut thread_list = Vec::new();
    for _ in 0..TOTAL_THREAD_COUNT {
        let time_list = time_list.clone();
        thread_list.push(std::thread::spawn(move || {
            for _ in 0..TOTAL_TEST_COUNT {
                let mut time_list = time_list.lock();
                time_list.push(normal_time_function());
            }
        }));
    }

    // wait for all threads to finish
    for thread in thread_list {
        thread.join().unwrap();
    }

    // verify whether value is monotonic
    let time_list = time_list.lock();
    for i in 0..time_list.len() - 1 {
        assert!(time_list[i] <= time_list[i + 1]);
        // println!("{} {}", time_list[i], time_list[i + 1]);
    }
}

#[test]
#[ignore]
fn the_cost_of_timer() {
    // Get the average latency of calling the timer function.
    const TOTAL_TEST_COUNT: usize = 100_000_000;
    const THREAD_COUNT: usize = 16;

    let handlers: Vec<_> = (0..THREAD_COUNT)
        .map(|_| {
            std::thread::spawn(|| {
                let mut fake_number: u64 = 0;
                let start = Instant::now();
                for _ in 0..TOTAL_TEST_COUNT {
                    fake_number |= normal_time_function();
                }
                let end = start.elapsed();
                if fake_number == 0 {
                    println!("This is a fake number: {}", fake_number);
                }

                end
            })
        })
        .collect();

    // sum all durations from each thread
    let acc = handlers.into_iter().fold(0, |acc, handler| {
        let end = handler.join().unwrap();
        acc + end.as_nanos()
    });

    println!(
        "Average time: {} ns",
        acc / (TOTAL_TEST_COUNT * THREAD_COUNT) as u128
    );
}
