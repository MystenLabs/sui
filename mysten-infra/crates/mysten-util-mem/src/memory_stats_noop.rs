// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Copyright 2021 Parity Technologies
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#[derive(Clone, Debug)]
pub struct Unimplemented;
pub use Unimplemented as Error;

#[cfg(feature = "std")]
impl std::fmt::Display for Unimplemented {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        fmt.write_str("unimplemented")
    }
}

#[derive(Clone)]
pub struct MemoryAllocationTracker {}

impl MemoryAllocationTracker {
    pub fn new() -> Result<Self, Error> {
        Err(Error)
    }

    pub fn snapshot(&self) -> Result<crate::MemoryAllocationSnapshot, Error> {
        unimplemented!();
    }
}
