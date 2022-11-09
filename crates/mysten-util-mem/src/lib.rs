// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Copyright 2020 Parity Technologies
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Crate for parity memory management related utilities.
//! It includes global allocator choice, heap measurement and
//! memory erasure.

#![allow(clippy::all)]
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;

// default allocator used
mod memory_stats_noop;
use memory_stats_noop as memory_stats;

pub mod allocators;

#[cfg(any(
    any(target_os = "macos", target_os = "ios"),
    feature = "estimate-heapsize"
))]
pub mod sizeof;

/// This is a copy of patched crate `malloc_size_of` as a module.
/// We need to have it as an inner module to be able to define our own traits implementation,
/// if at some point the trait become standard enough we could use the right way of doing it
/// by implementing it in our type traits crates. At this time moving this trait to the primitive
/// types level would impact too much of the dependencies to be easily manageable.
#[macro_use]
mod malloc_size;

pub mod external_impls;

pub use allocators::MallocSizeOfExt;
pub use malloc_size::{MallocShallowSizeOf, MallocSizeOf, MallocSizeOfOps};

pub use mysten_util_mem_derive::*;

/// Heap size of structure.
///
/// Structure can be anything that implements MallocSizeOf.
pub fn malloc_size<T: MallocSizeOf + ?Sized>(t: &T) -> usize {
    MallocSizeOf::size_of(t, &mut allocators::new_malloc_size_ops())
}

/// An error related to the memory stats gathering.
#[derive(Clone, Debug)]
pub struct MemoryStatsError(memory_stats::Error);

#[cfg(feature = "std")]
impl std::fmt::Display for MemoryStatsError {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.0.fmt(fmt)
    }
}

#[cfg(feature = "std")]
impl std::error::Error for MemoryStatsError {}

/// Snapshot of collected memory metrics.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct MemoryAllocationSnapshot {
    /// Total resident memory, in bytes.
    pub resident: u64,
    /// Total allocated memory, in bytes.
    pub allocated: u64,
}

/// Accessor to the allocator internals.
#[derive(Clone)]
pub struct MemoryAllocationTracker(self::memory_stats::MemoryAllocationTracker);

impl MemoryAllocationTracker {
    /// Create an instance of an allocation tracker.
    pub fn new() -> Result<Self, MemoryStatsError> {
        self::memory_stats::MemoryAllocationTracker::new()
            .map(MemoryAllocationTracker)
            .map_err(MemoryStatsError)
    }

    /// Create an allocation snapshot.
    pub fn snapshot(&self) -> Result<MemoryAllocationSnapshot, MemoryStatsError> {
        self.0.snapshot().map_err(MemoryStatsError)
    }
}

#[cfg(feature = "std")]
#[cfg(test)]
mod test {
    use super::{malloc_size, MallocSizeOf, MallocSizeOfExt};
    use std::sync::Arc;

    #[test]
    fn test_arc() {
        let val = Arc::new("test".to_string());
        let s = val.malloc_size_of();
        assert!(s > 0);
    }

    #[test]
    fn test_dyn() {
        trait Augmented: MallocSizeOf {}
        impl Augmented for Vec<u8> {}
        let val: Arc<dyn Augmented> = Arc::new(vec![0u8; 1024]);
        assert!(malloc_size(&*val) > 1000);
    }
}
