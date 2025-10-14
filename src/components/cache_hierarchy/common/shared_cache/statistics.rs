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

use serde::{Deserialize, Serialize};

use super::super::CacheAccessType;
use crate::parameter;
use std::fmt::Debug;

pub trait SharedCacheSetStatistics: Default + Debug + Clone + Serialize {
    // print statistics
    fn get_header() -> String;
    fn render_line(&self) -> String;

    // record
    fn record(&mut self, access_type: CacheAccessType, is_os: bool, is_hit: bool);
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ZeroSharedCacheSetStatistics {}

impl SharedCacheSetStatistics for ZeroSharedCacheSetStatistics {
    fn get_header() -> String {
        "".to_string()
    }

    fn render_line(&self) -> String {
        "".to_string()
    }

    fn record(&mut self, _access_type: CacheAccessType, _is_os: bool, _is_hit: bool) {}
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct SharedCacheSetMissStatistics {
    // statistics
    pub miss_count: u64,
    pub miss_count_u: u64,
    pub miss_count_k: u64,

    pub fetch_miss_count: u64,
    pub fetch_miss_count_u: u64,
    pub fetch_miss_count_k: u64,

    pub read_miss_count: u64,
    pub read_miss_count_u: u64,
    pub read_miss_count_k: u64,

    pub write_miss_count: u64,
    pub write_miss_count_u: u64,
    pub write_miss_count_k: u64,

    pub ptw_miss_count: u64,
    pub ptw_miss_count_u: u64,
    pub ptw_miss_count_k: u64,
}

impl SharedCacheSetStatistics for SharedCacheSetMissStatistics {
    fn get_header() -> String {
        let mut result = vec![];
        for basic_string in [
            "miss_count",
            "fetch_miss_count",
            "read_miss_count",
            "write_miss_count",
            "ptw_miss_count",
        ] {
            for suffix in ["", ":u", ":k"] {
                result.push(format!("{}{}", basic_string, suffix));
            }
        }

        result.join(",")
    }

    fn render_line(&self) -> String {
        let res = vec![
            self.miss_count.to_string(),
            self.miss_count_u.to_string(),
            self.miss_count_k.to_string(),
            self.fetch_miss_count.to_string(),
            self.fetch_miss_count_u.to_string(),
            self.fetch_miss_count_k.to_string(),
            self.read_miss_count.to_string(),
            self.read_miss_count_u.to_string(),
            self.read_miss_count_k.to_string(),
            self.write_miss_count.to_string(),
            self.write_miss_count_u.to_string(),
            self.write_miss_count_k.to_string(),
            self.ptw_miss_count.to_string(),
            self.ptw_miss_count_u.to_string(),
            self.ptw_miss_count_k.to_string(),
        ];

        res.join(",")
    }

    #[inline]
    fn record(&mut self, access_type: CacheAccessType, is_os: bool, _is_hit: bool) {
        if parameter::ENABLE_STATISTICS {
            self.miss_count += 1;

            match access_type {
                CacheAccessType::InstructionFetch => {
                    self.fetch_miss_count += 1;
                }
                CacheAccessType::DataRead => {
                    self.read_miss_count += 1;
                }
                CacheAccessType::DataWrite => {
                    self.write_miss_count += 1;
                }
                CacheAccessType::PageWalkRead => {
                    self.ptw_miss_count += 1;
                }

                _ => panic!("Error: unsupported access type."),
            }

            if is_os {
                self.miss_count_k += 1;
                match access_type {
                    CacheAccessType::InstructionFetch => {
                        self.fetch_miss_count_k += 1;
                    }
                    CacheAccessType::DataRead => {
                        self.read_miss_count_k += 1;
                    }
                    CacheAccessType::DataWrite => {
                        self.write_miss_count_k += 1;
                    }
                    CacheAccessType::PageWalkRead => {
                        self.ptw_miss_count_k += 1;
                    }
                    _ => panic!("Error: unsupported access type."),
                }
            } else {
                self.miss_count_u += 1;
                match access_type {
                    CacheAccessType::InstructionFetch => {
                        self.fetch_miss_count_u += 1;
                    }
                    CacheAccessType::DataRead => {
                        self.read_miss_count_u += 1;
                    }
                    CacheAccessType::DataWrite => {
                        self.write_miss_count_u += 1;
                    }
                    CacheAccessType::PageWalkRead => {
                        self.ptw_miss_count_u += 1;
                    }
                    _ => panic!("Error: unsupported access type."),
                }
            }
        }
    }
}
