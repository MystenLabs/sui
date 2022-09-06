// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use duration_str::parse;
use serde::{Deserialize, Serialize};

pub mod bench_driver;
pub mod driver;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Interval {
    Count(u64),
    Time(tokio::time::Duration),
}

impl Interval {
    pub fn is_unbounded(&self) -> bool {
        matches!(self, Interval::Time(tokio::time::Duration::MAX))
    }
}

impl FromStr for Interval {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Ok(i) = s.parse() {
            Ok(Interval::Count(i))
        } else if let Ok(d) = parse(s) {
            Ok(Interval::Time(d))
        } else if "unbounded" == s {
            Ok(Interval::Time(tokio::time::Duration::MAX))
        } else {
            Err("Required integer number of cycles or time duration".to_string())
        }
    }
}
